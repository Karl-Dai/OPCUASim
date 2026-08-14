use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::info;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use opcua_server::node_manager::memory::{InMemoryNodeManager, InMemoryNodeManagerImpl};
use opcua_server::SubscriptionCache;
use opcua_types::{
    AttributeId, DataEncoding, DataValue, DateTime, NodeId, NumericRange, TimestampsToReturn,
};

use super::address_space::{f64_to_variant, variant_to_display_string};
use super::generator::generate_value;
use super::history_store::HistoryStore;
use super::models::{DataType, ServerNode, SimulationMode};

/// Callback invoked by the engine to raise an event (message, severity).
pub type EventNotifier = Arc<dyn Fn(&str, u16) + Send + Sync>;

/// State for a single simulated node.
#[derive(Clone)]
struct NodeSimState {
    node_id_str: String,
    display_name: String,
    opcua_node_id: NodeId,
    data_type: DataType,
    simulation: SimulationMode,
    iteration: u64,
    eu_range_low: f64,
    eu_range_high: f64,
}

/// The simulation engine drives value generation for all non-Static nodes.
/// Nodes are grouped by interval_ms; one tokio task per group.
pub struct SimulationEngine {
    cancel_token: CancellationToken,
    node_states: Arc<RwLock<HashMap<String, NodeSimState>>>,
    update_seq: Arc<RwLock<u64>>,
    current_values: Arc<RwLock<HashMap<String, (String, u64)>>>,
    history_store: Arc<RwLock<Option<Arc<HistoryStore>>>>,
    alarm_states: Arc<RwLock<HashMap<String, bool>>>,
    event_notifier: Arc<RwLock<Option<EventNotifier>>>,
    custom_types: Arc<RwLock<HashMap<String, NodeId>>>,
    /// Static nodes (no auto-generation) tracked for address-space polling so
    /// that client writes are reflected in the frontend.
    static_node_ids: Arc<RwLock<Vec<(String, NodeId)>>>,
}

impl SimulationEngine {
    pub fn new() -> Self {
        Self {
            cancel_token: CancellationToken::new(),
            node_states: Arc::new(RwLock::new(HashMap::new())),
            update_seq: Arc::new(RwLock::new(0)),
            current_values: Arc::new(RwLock::new(HashMap::new())),
            history_store: Arc::new(RwLock::new(None)),
            alarm_states: Arc::new(RwLock::new(HashMap::new())),
            event_notifier: Arc::new(RwLock::new(None)),
            custom_types: Arc::new(RwLock::new(HashMap::new())),
            static_node_ids: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Attach the history store; simulation updates will be recorded there.
    pub async fn set_history_store(&self, store: Arc<HistoryStore>) {
        *self.history_store.write().await = Some(store);
    }

    pub async fn set_event_notifier(&self, notifier: EventNotifier) {
        *self.event_notifier.write().await = Some(notifier);
    }

    pub async fn set_custom_types(&self, custom: HashMap<String, NodeId>) {
        *self.custom_types.write().await = custom;
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
            states.insert(
                node.node_id.clone(),
                NodeSimState {
                    node_id_str: node.node_id.clone(),
                    display_name: node.display_name.clone(),
                    data_type: node.data_type.clone(),
                    simulation: node.simulation.clone(),
                    opcua_node_id,
                    iteration: 0,
                    eu_range_low: node.eu_range_low,
                    eu_range_high: node.eu_range_high,
                },
            );
        }
    }

    /// Start the simulation engine. Spawns one tokio task per interval group.
    pub fn start<T>(
        &self,
        node_manager: Arc<InMemoryNodeManager<T>>,
        subscriptions: Arc<SubscriptionCache>,
    ) where
        T: InMemoryNodeManagerImpl,
    {
        let cancel_token = self.cancel_token.clone();
        let node_states = self.node_states.clone();
        let update_seq = self.update_seq.clone();
        let current_values = self.current_values.clone();
        let static_node_ids = self.static_node_ids.clone();
        let history_store = self.history_store.clone();
        let alarm_states = self.alarm_states.clone();
        let event_notifier = self.event_notifier.clone();
        let custom_types = self.custom_types.clone();

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

            info!(
                "SimulationEngine starting: {} interval groups",
                groups.len()
            );

            // Spawn a polling task for Static nodes so client writes are
            // reflected in current_values (and thus the frontend). Static nodes
            // have no auto-generation, so the engine only learns of new values
            // by reading the address space.
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
                let hs = history_store.clone();
                let as_ = alarm_states.clone();
                let en = event_notifier.clone();
                let ct = custom_types.clone();

                let handle = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                    loop {
                        tokio::select! {
                            _ = token.cancelled() => break,
                            _ = interval.tick() => {
                                let elapsed = start_time.elapsed().as_secs_f64();
                                let now = DateTime::now();
                                let custom = ct.read().await.clone();

                                let mut updates: Vec<(&NodeId, Option<&NumericRange>, DataValue)> = Vec::new();
                                let mut value_strings: Vec<(String, String)> = Vec::new();
                                let mut alarm_events: Vec<(String, String, u16)> = Vec::new();

                                for node_state in &mut group_nodes {
                                    if let Some(raw_value) = generate_value(
                                        &node_state.simulation,
                                        elapsed,
                                        node_state.iteration,
                                    ) {
                                        let variant = f64_to_variant(raw_value, &node_state.data_type, &custom);
                                        let value_str = variant_to_display_string(&variant);
                                        value_strings.push((node_state.node_id_str.clone(), value_str));

                                        let mut dv = DataValue::new_now(variant);
                                        dv.source_timestamp = Some(now);
                                        dv.server_timestamp = Some(now);

                                        if let Some(store) = {
                                            let guard = hs.read().await;
                                            guard.as_ref().cloned()
                                        } {
                                            store.record(&node_state.opcua_node_id, dv.clone()).await;
                                        }

                                        // Threshold alarm detection (numeric nodes only).
                                        if node_state.data_type.is_numeric() {
                                            let is_out =
                                                raw_value < node_state.eu_range_low
                                                    || raw_value > node_state.eu_range_high;
                                            let was_active = {
                                                let guard = as_.read().await;
                                                *guard.get(&node_state.node_id_str).unwrap_or(&false)
                                            };
                                            match (is_out, was_active) {
                                                (true, false) => {
                                                    let msg = format!(
                                                        "{} exceeded limit ({}..{})",
                                                        node_state.display_name,
                                                        node_state.eu_range_low,
                                                        node_state.eu_range_high,
                                                    );
                                                    alarm_events.push((
                                                        node_state.node_id_str.clone(),
                                                        msg,
                                                        500,
                                                    ));
                                                    as_.write()
                                                        .await
                                                        .insert(node_state.node_id_str.clone(), true);
                                                }
                                                (false, true) => {
                                                    let msg = format!(
                                                        "{} back to normal",
                                                        node_state.display_name,
                                                    );
                                                    alarm_events.push((
                                                        node_state.node_id_str.clone(),
                                                        msg,
                                                        100,
                                                    ));
                                                    as_.write()
                                                        .await
                                                        .insert(node_state.node_id_str.clone(), false);
                                                }
                                                _ => {}
                                            }
                                        }

                                        updates.push((
                                            &node_state.opcua_node_id,
                                            None,
                                            dv,
                                        ));
                                        node_state.iteration += 1;
                                    }
                                }

                                // Emit alarm events (after dropping all per-node locks).
                                if !alarm_events.is_empty() {
                                    if let Some(notifier) = {
                                        let guard = en.read().await;
                                        guard.clone()
                                    } {
                                        for (_nid, msg, severity) in &alarm_events {
                                            notifier(msg.as_str(), *severity);
                                        }
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
        let changed: Vec<(String, String)> = cv
            .iter()
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
