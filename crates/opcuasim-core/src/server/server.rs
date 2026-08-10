use std::sync::Arc;
use std::time::Duration;

use log::info;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use opcua_crypto::SecurityPolicy;
use opcua_server::address_space::AddressSpace;
use opcua_server::diagnostics::NamespaceMetadata;
use opcua_server::node_manager::memory::{
    InMemoryNodeManager, InMemoryNodeManagerBuilder, InMemoryNodeManagerImplBuilder,
    SimpleNodeManagerBuilder,
};
use opcua_server::node_manager::ServerContext;
use opcua_server::{
    Server, ServerBuilder, ServerHandle, ServerUserToken, SubscriptionCache,
    ANONYMOUS_USER_TOKEN_ID,
};
use opcua_types::MessageSecurityMode;

use super::address_space::populate_address_space;
use super::event_store::EventStore;
use super::events::DEMO_EVENTS_ID;
use super::history_node_manager::HistoryNodeManagerImpl;
use super::history_store::HistoryStore;
use super::models::{ServerConfig, ServerFolder, ServerNode, ServerState};
use super::simulation::SimulationEngine;
use crate::error::OpcUaSimError;

const APPLICATION_URI: &str = "urn:opcuasim:server";
const NAMESPACE_URI: &str = "urn:opcuasim:server:nodes";

/// The OPC UA simulation server.
pub struct OpcUaServer {
    state: Arc<RwLock<ServerState>>,
    handle: Arc<RwLock<Option<ServerHandle>>>,
    node_manager: Arc<RwLock<Option<Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>>>>,
    simulation_engine: Arc<RwLock<Option<Arc<SimulationEngine>>>>,
    namespace_index: Arc<RwLock<u16>>,
    event_store: Arc<RwLock<Option<Arc<EventStore>>>>,
    heartbeat_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    connection_monitor_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

/// Result of building the server (all sync, no async).
struct BuildResult {
    server: Server,
    handle: ServerHandle,
    node_manager: Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>,
    history: Arc<HistoryStore>,
    namespace_index: u16,
    subscriptions: Arc<SubscriptionCache>,
}

/// Build the OPC UA server synchronously (ServerBuilder is not Send).
fn build_server(
    config: &ServerConfig,
    folders: &[ServerFolder],
    nodes: &[ServerNode],
) -> Result<BuildResult, OpcUaSimError> {
    // Build user tokens
    let mut user_token_ids: Vec<String> = Vec::new();
    let history = Arc::new(HistoryStore::new(config.history_buffer_size));
    let ns_meta = NamespaceMetadata {
        namespace_uri: NAMESPACE_URI.to_string(),
        ..Default::default()
    };
    let history_for_impl = history.clone();
    let nm_builder = move |context: ServerContext, address_space: &mut AddressSpace| {
        let inner = SimpleNodeManagerBuilder::new(ns_meta.clone(), "SimNodeManager")
            .build(context, address_space);
        HistoryNodeManagerImpl::new(inner, history_for_impl.clone())
    };

    let mut builder = ServerBuilder::new()
        .application_name(&config.name)
        .application_uri(APPLICATION_URI)
        .product_uri("urn:opcuasim")
        .create_sample_keypair(true)
        .pki_dir("./pki-server")
        .host("0.0.0.0")
        .port(config.port)
        .trust_client_certs(true)
        .with_node_manager(InMemoryNodeManagerBuilder::new(nm_builder));

    if config.anonymous_enabled {
        user_token_ids.push(ANONYMOUS_USER_TOKEN_ID.to_string());
    }

    for user in &config.users {
        let token_id = format!("user_{}", user.username);
        builder = builder.add_user_token(
            &token_id,
            ServerUserToken {
                user: user.username.clone(),
                pass: Some(user.password.clone()),
                ..Default::default()
            },
        );
        user_token_ids.push(token_id);
    }

    let endpoint_path = "/";
    let token_ids_ref: Vec<&str> = user_token_ids.iter().map(|s| s.as_str()).collect();

    builder = builder.add_endpoint(
        "none",
        (
            endpoint_path,
            SecurityPolicy::None,
            MessageSecurityMode::None,
            &token_ids_ref as &[&str],
        ),
    );

    for policy in &config.security_policies {
        for mode in &config.security_modes {
            if policy == "None" && mode == "None" {
                continue;
            }
            let sec_policy = match policy.as_str() {
                "Basic128Rsa15" => SecurityPolicy::Basic128Rsa15,
                "Basic256" => SecurityPolicy::Basic256,
                "Basic256Sha256" => SecurityPolicy::Basic256Sha256,
                "Aes128Sha256RsaOaep" => SecurityPolicy::Aes128Sha256RsaOaep,
                "Aes256Sha256RsaPss" => SecurityPolicy::Aes256Sha256RsaPss,
                _ => continue,
            };
            let sec_mode = match mode.as_str() {
                "Sign" => MessageSecurityMode::Sign,
                "SignAndEncrypt" => MessageSecurityMode::SignAndEncrypt,
                _ => continue,
            };
            let id = format!("{}_{}", policy.to_lowercase(), mode.to_lowercase());
            builder = builder.add_endpoint(
                &id,
                (
                    endpoint_path,
                    sec_policy,
                    sec_mode,
                    &token_ids_ref as &[&str],
                ),
            );
        }
    }

    builder = builder.discovery_urls(vec![endpoint_path.to_string()]);

    let (server, handle) = builder
        .build()
        .map_err(|e| OpcUaSimError::ServerError(format!("Server build failed: {}", e)))?;

    let node_managers = handle.node_managers();
    let sim_nm = node_managers
        .get_of_type::<InMemoryNodeManager<HistoryNodeManagerImpl>>()
        .ok_or_else(|| {
            OpcUaSimError::ServerError(
                "InMemoryNodeManager<HistoryNodeManagerImpl> not found".into(),
            )
        })?;

    let ns_index = {
        let ns = sim_nm.namespaces();
        ns.iter()
            .find(|(_, uri)| uri.as_str() == NAMESPACE_URI)
            .map(|(k, _)| *k)
            .ok_or_else(|| {
                OpcUaSimError::ServerError(format!(
                    "Custom namespace '{}' not registered; got: {:?}",
                    NAMESPACE_URI, ns
                ))
            })?
    };

    // Populate address space (sync)
    {
        let mut address_space = sim_nm.address_space().write();
        populate_address_space(&mut address_space, ns_index, folders, nodes);
    }
    info!(
        "Address space populated: {} folders, {} nodes",
        folders.len(),
        nodes.len()
    );

    let subscriptions = server.subscriptions();

    Ok(BuildResult {
        server,
        handle,
        node_manager: sim_nm,
        history,
        namespace_index: ns_index,
        subscriptions,
    })
}

impl OpcUaServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ServerState::Stopped)),
            handle: Arc::new(RwLock::new(None)),
            node_manager: Arc::new(RwLock::new(None)),
            simulation_engine: Arc::new(RwLock::new(None)),
            namespace_index: Arc::new(RwLock::new(2)),
            event_store: Arc::new(RwLock::new(None)),
            heartbeat_task: Arc::new(RwLock::new(None)),
            connection_monitor_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the OPC UA server with the given configuration.
    pub async fn start(
        &self,
        config: &ServerConfig,
        folders: &[ServerFolder],
        nodes: &[ServerNode],
    ) -> Result<(), OpcUaSimError> {
        {
            let state = self.state.read().await;
            if *state != ServerState::Stopped {
                return Err(OpcUaSimError::ServerError("Server is not stopped".into()));
            }
        }
        *self.state.write().await = ServerState::Starting;

        info!("Starting OPC UA server on port {}", config.port);

        // Build server synchronously (ServerBuilder is not Send)
        let config_clone = config.clone();
        let folders_clone = folders.to_vec();
        let nodes_clone = nodes.to_vec();

        let build_result = tokio::task::spawn_blocking(move || {
            build_server(&config_clone, &folders_clone, &nodes_clone)
        })
        .await
        .map_err(|e| OpcUaSimError::ServerError(format!("Build task failed: {}", e)))??;

        let BuildResult {
            server,
            handle,
            node_manager: sim_nm,
            history,
            namespace_index: ns_index,
            subscriptions,
        } = build_result;

        *self.namespace_index.write().await = ns_index;
        *self.handle.write().await = Some(handle);
        *self.node_manager.write().await = Some(sim_nm.clone());

        // EventStore capacity is hardcoded until Task 5 adds config.event_history_size
        let event_store = Arc::new(EventStore::new(10_000));
        *self.event_store.write().await = Some(event_store.clone());

        {
            let mut addr = sim_nm.address_space().write();
        super::events::build_events_object(&mut *addr, ns_index)
            .expect("failed to create DemoEvents object");
        }

        let events_source = opcua_types::NodeId::new(ns_index, DEMO_EVENTS_ID);
        super::events::register_raise_event_method(
            &sim_nm,
            ns_index,
            subscriptions.clone(),
            Some(event_store.clone()),
            events_source.clone(),
        );

        // Start simulation engine
        let sim_engine = Arc::new(SimulationEngine::new());
        sim_engine.register_nodes(nodes, ns_index).await;
        sim_engine.set_history_store(history.clone()).await;
        sim_engine.start(sim_nm.clone(), subscriptions.clone());
        *self.simulation_engine.write().await = Some(sim_engine.clone());

        // Register preset demo methods
        if let Err(e) = super::methods::register_demo_methods(self, subscriptions.clone()).await {
            log::warn!("Failed to register preset demo methods: {e}");
        }

        // Spawn heartbeat and connection monitor tasks
        {
            let handle_guard = self.handle.read().await;
            let server_handle = handle_guard.as_ref().expect("handle set above");
            let cancel = server_handle.token().clone();
            let address_space = sim_nm.address_space().clone();

            let heartbeat = super::events::spawn_heartbeat_task(
                subscriptions.clone(),
                Some(event_store.clone()),
                events_source.clone(),
                Duration::from_secs(5),
                cancel.clone(),
            );
            *self.heartbeat_task.write().await = Some(heartbeat);

            let conn_mon = super::events::spawn_connection_monitor_task(
                subscriptions.clone(),
                Some(event_store.clone()),
                events_source.clone(),
                address_space,
                Duration::from_secs(1),
                cancel,
            );
            *self.connection_monitor_task.write().await = Some(conn_mon);
        }

        // Run server in background task
        let state = self.state.clone();
        let sim_engine_bg = sim_engine.clone();

        tokio::spawn(async move {
            *state.write().await = ServerState::Running;
            info!("OPC UA server is running");

            let result = server.run().await;

            sim_engine_bg.stop();
            *state.write().await = ServerState::Stopped;

            match result {
                Ok(_) => info!("OPC UA server stopped normally"),
                Err(e) => info!("OPC UA server stopped with error: {}", e),
            }
        });

        // Wait briefly for server to start
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        Ok(())
    }

    /// Stop the server.
    pub async fn stop(&self) -> Result<(), OpcUaSimError> {
        let handle = self.handle.read().await;
        if let Some(ref h) = *handle {
            *self.state.write().await = ServerState::Stopping;
            info!("Stopping OPC UA server");
            h.cancel();

            // Abort background event tasks (cancel token already triggered
            // above; abort as a belt-and-braces guarantee).
            if let Some(hb) = self.heartbeat_task.write().await.take() {
                hb.abort();
            }
            if let Some(cm) = self.connection_monitor_task.write().await.take() {
                cm.abort();
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(())
        } else {
            Err(OpcUaSimError::ServerError("Server is not running".into()))
        }
    }

    /// Get the current server state.
    pub async fn state(&self) -> ServerState {
        self.state.read().await.clone()
    }

    /// Get the current namespace index.
    pub async fn namespace_index(&self) -> u16 {
        *self.namespace_index.read().await
    }

    /// Get a reference to the node manager (if server is running).
    pub async fn node_manager(&self) -> Option<Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>> {
        self.node_manager.read().await.clone()
    }

    /// Get a reference to the simulation engine (if server is running).
    pub async fn simulation_engine(&self) -> Option<Arc<SimulationEngine>> {
        self.simulation_engine.read().await.clone()
    }

    pub async fn event_store(&self) -> Option<Arc<EventStore>> {
        self.event_store.read().await.clone()
    }
}

impl Default for OpcUaServer {
    fn default() -> Self {
        Self::new()
    }
}
