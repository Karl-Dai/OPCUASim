use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::info;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use opcua_server::node_manager::memory::SimpleNodeManager;
use opcua_server::SubscriptionCache;
use opcua_types::{
    AttributeId, DataEncoding, DataValue, DateTime, NodeId, NumericRange, TimestampsToReturn,
};

use super::address_space::f64_to_variant;
use super::generator::generate_value;
use super::models::{DataType, ServerNode, SimulationMode};

/// State for a single simulated node.
#[derive(Clone)]
struct NodeSimState {
    node_id_str: String,
    opcua_node_id: NodeId,
    data_type: DataType,
    simulation: SimulationMode,
    iteration: u64,
}

/// The simulation engine drives value generation for all non-Static nodes.
/// Nodes are grouped by interval_ms; one tokio task per group.
pub struct SimulationEngine {
    cancel_token: CancellationToken,
    node_states: Arc<RwLock<HashMap<String, NodeSimState>>>,
    update_seq: Arc<RwLock<u64>>,
    /// Map of node_id -> current_value for incremental polling from frontend.
    current_values: Arc<RwLock<HashMap<String, (String, u64)>>>,
    /// Static nodes (no auto-generation) tracked for address-space polling so that
    /// client writes are reflected in the frontend.
    static_node_ids: Arc<RwLock<Vec<(String, NodeId)>>>,
}

impl SimulationEngine {
    pub fn new() -> Self {
        Self {
            cancel_token: CancellationToken::new(),
            node_states: Arc::new(RwLock::new(HashMap::new())),
            update_seq: Arc::new(RwLock::new(0)),
            current_values: Arc::new(RwLock::new(HashMap::new())),
            static_node_ids: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register nodes for simulation. Must be called before start().
    pub async fn register_nodes(&self, nodes: &[ServerNode], namespace_index: u16) {
        let mut states = self.node_states.write().await;
        let mut static_ids = self.static_node_ids.write().await;
        for node in nodes {
            let opcua_node_id = super::address_space::parse_node_id(&node.node_id)
                .unwrap_or_else(|_| NodeId::new(namespace_index, node.node_id.as_str()));
            if node.simulation.interval_ms().is_none() {
                // Static node: track for address-space polling (reflects client writes).
                static_ids.push((node.node_id.clone(), opcua_node_id));
                continue;
            }
            states.insert(node.node_id.clone(), NodeSimState {
                node_id_str: node.node_id.clone(),
                data_type: node.data_type.clone(),
                simulation: node.simulation.clone(),
                opcua_node_id,
                iteration: 0,
            });
        }
    }

    /// Start the simulation engine. Spawns one tokio task per interval group.
    pub fn start(
        &self,
        node_manager: Arc<SimpleNodeManager>,
        subscriptions: Arc<SubscriptionCache>,
    ) {
        let cancel_token = self.cancel_token.clone();
        let node_states = self.node_states.clone();
        let update_seq = self.update_seq.clone();
        let current_values = self.current_values.clone();
        let static_node_ids = self.static_node_ids.clone();

        tokio::spawn(async move {
            // Group nodes by interval
            let states = node_states.read().await;
            let mut groups: HashMap<u64, Vec<NodeSimState>> = HashMap::new();
            for state in states.values() {
                if let Some(interval) = state.simulation.interval_ms() {
                    groups.entry(interval).or_default().push(state.clone());
                }
            }
            drop(states);

            info!("SimulationEngine starting: {} interval groups", groups.len());

            // Spawn a polling task for Static nodes so client writes are reflected
            // in current_values (and thus the frontend). Static nodes have no
            // auto-generation, so the engine only learns of new values by reading
            // the address space.
            let nm_static = node_manager.clone();
            let vals_static = current_values.clone();
            let seq_static = update_seq.clone();
            let static_ids = static_node_ids.read().await.clone();
            let token_static = cancel_token.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = token_static.cancelled() => break,
                        _ = interval.tick() => {
                            let changed: Vec<(String, String)> = {
                                let guard = nm_static.address_space().read();
                                let mut c = Vec::new();
                                for (nid_str, nid) in &static_ids {
                                    if let Some(node) = guard.find(nid) {
                                        if let Some(dv) = node.as_node().get_attribute_max_age(
                                            TimestampsToReturn::Server,
                                            AttributeId::Value,
                                            &NumericRange::None,
                                            &DataEncoding::Binary,
                                            0.0,
                                        ) {
                                            if let Some(v) = &dv.value {
                                                c.push((nid_str.clone(), format!("{}", v)));
                                            }
                                        }
                                    }
                                }
                                c
                            };
                            if !changed.is_empty() {
                                let mut cv = vals_static.write().await;
                                let mut s = seq_static.write().await;
                                for (nid, val) in changed {
                                    *s += 1;
                                    cv.insert(nid, (val, *s));
                                }
                            }
                        }
                    }
                }
            });

            let mut handles = Vec::new();
            let start_time = Instant::now();

            for (interval_ms, mut group_nodes) in groups {
                let token = cancel_token.clone();
                let nm = node_manager.clone();
                let subs = subscriptions.clone();
                let seq = update_seq.clone();
                let vals = current_values.clone();

                let handle = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            _ = token.cancelled() => break,
                            _ = interval.tick() => {
                                let elapsed = start_time.elapsed().as_secs_f64();
                                let now = DateTime::now();

                                // Generate values for all nodes in this group
                                let mut updates: Vec<(&NodeId, Option<&NumericRange>, DataValue)> = Vec::new();
                                let mut value_strings: Vec<(String, String)> = Vec::new();

                                for node_state in &mut group_nodes {
                                    if let Some(raw_value) = generate_value(
                                        &node_state.simulation,
                                        elapsed,
                                        node_state.iteration,
                                    ) {
                                        let variant = f64_to_variant(raw_value, &node_state.data_type);
                                        let value_str = format!("{}", variant);
                                        value_strings.push((node_state.node_id_str.clone(), value_str));

                                        let mut dv = DataValue::new_now(variant);
                                        dv.source_timestamp = Some(now);
                                        dv.server_timestamp = Some(now);

                                        updates.push((
                                            &node_state.opcua_node_id,
                                            None,
                                            dv,
                                        ));
                                        node_state.iteration += 1;
                                    }
                                }

                                // Batch write to address space
                                if !updates.is_empty() {
                                    if let Err(e) = nm.set_values(&subs, updates.into_iter()) {
                        log::warn!("Failed to set simulation values: {:?}", e);
                    }

                                    // Update current_values for frontend polling
                                    let mut cv = vals.write().await;
                                    let mut s = seq.write().await;
                                    for (nid, val) in value_strings {
                                        *s += 1;
                                        cv.insert(nid, (val, *s));
                                    }
                                }
                            }
                        }
                    }
                });
                handles.push(handle);
            }

            // Wait for all group tasks to complete (i.e. cancellation)
            for h in handles {
                let _ = h.await;
            }
            info!("SimulationEngine stopped");
        });
    }

    /// Stop the simulation engine.
    pub fn stop(&self) {
        self.cancel_token.cancel();
    }

    /// Get current values that changed since `since_seq`.
    pub async fn get_values_since(&self, since_seq: u64) -> (Vec<(String, String)>, u64) {
        let cv = self.current_values.read().await;
        let seq = *self.update_seq.read().await;
        let changed: Vec<(String, String)> = cv.iter()
            .filter(|(_, (_, s))| *s > since_seq)
            .map(|(nid, (val, _))| (nid.clone(), val.clone()))
            .collect();
        (changed, seq)
    }

    /// Get the current update sequence number.
    pub async fn get_update_seq(&self) -> u64 {
        *self.update_seq.read().await
    }

    /// Check if the engine is running.
    pub fn is_running(&self) -> bool {
        !self.cancel_token.is_cancelled()
    }
}

impl Default for SimulationEngine {
    fn default() -> Self {
        Self::new()
    }
}
