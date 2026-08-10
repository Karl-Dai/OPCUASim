use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use opcua_client::Session;
use opcua_types::{AttributeId, NodeId, ReadValueId, TimestampsToReturn};

use crate::error::OpcUaSimError;
use crate::node::MonitoredNode;

pub struct PollingManager {
    polling_tasks: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
    monitored_items: Arc<RwLock<HashMap<String, MonitoredNode>>>,
    session_holder: Arc<RwLock<Option<Arc<Session>>>>,
}

impl PollingManager {
    pub fn new(session_holder: Arc<RwLock<Option<Arc<Session>>>>) -> Self {
        Self {
            polling_tasks: Arc::new(RwLock::new(HashMap::new())),
            monitored_items: Arc::new(RwLock::new(HashMap::new())),
            session_holder,
        }
    }

    pub async fn add_polling_node(
        &self,
        node: MonitoredNode,
        interval_ms: u64,
    ) -> Result<(), OpcUaSimError> {
        let node_id = node.node_id.clone();
        info!(
            "Adding polling for node: {} (interval: {}ms)",
            node_id, interval_ms
        );

        {
            let mut items = self.monitored_items.write().await;
            items.insert(node_id.clone(), node);
        }

        let items = self.monitored_items.clone();
        let session_holder = self.session_holder.clone();
        let nid = node_id.clone();

        let handle = tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                // Stop if the node was removed
                {
                    let items = items.read().await;
                    if !items.contains_key(&nid) {
                        break;
                    }
                }
                let Ok(nid_parsed) = nid.parse::<NodeId>() else {
                    continue;
                };
                let session = {
                    let guard = session_holder.read().await;
                    guard.clone()
                };
                let Some(session) = session else {
                    continue; // disconnected; try again next tick
                };
                let read_ids = vec![ReadValueId::new(nid_parsed, AttributeId::Value)];
                match session.read(&read_ids, TimestampsToReturn::Both, 0.0).await {
                    Ok(values) => {
                        if let Some(dv) = values.first() {
                            let value_str = dv
                                .value
                                .as_ref()
                                .map(|v| crate::server::address_space::variant_to_display_string(v))
                                .unwrap_or_else(|| "null".to_string());
                            let quality_str = dv
                                .status
                                .as_ref()
                                .map(|s| format!("{s}"))
                                .unwrap_or_else(|| "Good".to_string());
                            let source_ts = dv
                                .source_timestamp
                                .as_ref()
                                .map(|t| t.to_string())
                                .unwrap_or_default();
                            let server_ts = dv
                                .server_timestamp
                                .as_ref()
                                .map(|t| t.to_string())
                                .unwrap_or_default();
                            let mut items = items.write().await;
                            if let Some(node) = items.get_mut(&nid) {
                                node.value = Some(value_str);
                                node.quality = Some(quality_str);
                                node.timestamp = Some(source_ts);
                                node.server_timestamp = Some(server_ts);
                                node.update_seq = node.update_seq.wrapping_add(1);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Polling read failed for {}: {}", nid, e);
                    }
                }
            }
        });

        let mut tasks = self.polling_tasks.write().await;
        if let Some(old_handle) = tasks.insert(node_id, handle) {
            old_handle.abort();
        }
        Ok(())
    }

    pub async fn remove_polling_node(&self, node_id: &str) {
        let mut tasks = self.polling_tasks.write().await;
        if let Some(handle) = tasks.remove(node_id) {
            handle.abort();
        }
        let mut items = self.monitored_items.write().await;
        items.remove(node_id);
    }

    pub async fn stop_all(&self) {
        let mut tasks = self.polling_tasks.write().await;
        for (_, handle) in tasks.drain() {
            handle.abort();
        }
    }

    pub async fn get_polling_nodes(&self) -> Vec<MonitoredNode> {
        self.monitored_items
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }
}
