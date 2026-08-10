use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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
use opcua_types::{MessageSecurityMode, NodeId};

use super::address_space::{populate_address_space, register_custom_types_in_address_space};
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
    /// Registered custom type IDs (`type_name -> NodeId`). Populated by
    /// [`super::address_space::register_custom_types_in_address_space`] at
    /// server start time.
    custom_types: Arc<RwLock<HashMap<String, NodeId>>>,
    heartbeat_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    connection_monitor_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

/// Result of building the server (all sync, no async).
struct BuildResult {
    server: Server,
    handle: ServerHandle,
    node_manager: Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>,
    history: Arc<HistoryStore>,
    event_store: Arc<EventStore>,
    namespace_index: u16,
    subscriptions: Arc<SubscriptionCache>,
    custom_types: HashMap<String, NodeId>,
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
    let event_store = Arc::new(EventStore::new(config.event_history_size));
    let ns_meta = NamespaceMetadata {
        namespace_uri: NAMESPACE_URI.to_string(),
        ..Default::default()
    };
    let history_for_impl = history.clone();
    let event_store_for_impl = event_store.clone();
    let custom_types_share: Arc<Mutex<HashMap<String, NodeId>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let custom_for_closure = custom_types_share.clone();
    let folders_captured: Arc<Mutex<Vec<ServerFolder>>> = Arc::new(Mutex::new(folders.to_vec()));
    let nodes_captured: Arc<Mutex<Vec<ServerNode>>> = Arc::new(Mutex::new(nodes.to_vec()));
    let ns_uri_for_closure = NAMESPACE_URI.to_string();
    let nm_builder = move |context: ServerContext, address_space: &mut AddressSpace| {
        let type_tree_clone = context.type_tree.clone();
        let inner = SimpleNodeManagerBuilder::new(ns_meta.clone(), "SimNodeManager")
            .build(context, address_space);

        let ns_index = {
            let tree = type_tree_clone.read();
            tree.namespaces()
                .get_index(&ns_uri_for_closure)
                .unwrap_or(2)
        };

        let folders_ref = folders_captured.lock().unwrap();
        let nodes_ref = nodes_captured.lock().unwrap();
        let mut custom_ref = custom_for_closure.lock().unwrap();
        let registered =
            register_custom_types_in_address_space(address_space, ns_index, &nodes_ref);
        *custom_ref = registered;

        populate_address_space(
            address_space,
            ns_index,
            &folders_ref,
            &nodes_ref,
            &custom_ref,
        );
        HistoryNodeManagerImpl::new(
            inner,
            history_for_impl.clone(),
            event_store_for_impl.clone(),
        )
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

    let subscriptions = server.subscriptions();

    let custom_types: HashMap<String, NodeId> = {
        let guard = custom_types_share.lock().unwrap();
        guard.clone()
    };

    Ok(BuildResult {
        server,
        handle,
        node_manager: sim_nm,
        history,
        event_store,
        namespace_index: ns_index,
        subscriptions,
        custom_types,
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
            custom_types: Arc::new(RwLock::new(HashMap::new())),
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
            event_store,
            namespace_index: ns_index,
            subscriptions,
            custom_types,
        } = build_result;

        *self.namespace_index.write().await = ns_index;
        *self.handle.write().await = Some(handle.clone());
        *self.node_manager.write().await = Some(sim_nm.clone());
        *self.event_store.write().await = Some(event_store.clone());
        *self.custom_types.write().await = custom_types.clone();

        // Register custom types in the server's shared type tree (async-safe).
        // The address-space DataType nodes already exist (populated in the
        // builder closure); here we add the corresponding type-tree entries so
        // clients can resolve the types when browsing.
        super::address_space::register_custom_types_in_type_tree(
            &mut *handle.type_tree().write(),
            &custom_types,
            nodes,
        );

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
        sim_engine.set_custom_types(custom_types.clone()).await;
        {
            let subs_for_alarm = subscriptions.clone();
            let store_for_alarm: Option<Arc<EventStore>> = Some(event_store.clone());
            let source_for_alarm = events_source.clone();
            let notifier: Arc<dyn Fn(&str, u16) + Send + Sync> =
                Arc::new(move |message: &str, severity: u16| {
                    super::events::notify_event(
                        &subs_for_alarm,
                        &store_for_alarm,
                        &source_for_alarm,
                        message,
                        severity,
                    );
                });
            sim_engine.set_event_notifier(notifier).await;
        }
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

    /// Retrieve the map of registered custom type names to their NodeIds.
    /// Empty if the server has not yet started or no custom types were
    /// encountered in the configuration.
    pub async fn custom_types(&self) -> HashMap<String, NodeId> {
        self.custom_types.read().await.clone()
    }
}

impl Default for OpcUaServer {
    fn default() -> Self {
        Self::new()
    }
}
