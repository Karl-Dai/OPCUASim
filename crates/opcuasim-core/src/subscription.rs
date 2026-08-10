use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use opcua_client::{DataChangeCallback, EventCallback, Session};
use opcua_types::{
    AttributeId, ContentFilter, DataChangeFilter, DataChangeTrigger, EventFilter, ExtensionObject,
    MonitoredItemCreateRequest, MonitoringMode, MonitoringParameters, NodeId, NumericRange,
    ObjectTypeId, QualifiedName, ReadValueId, SimpleAttributeOperand, TimestampsToReturn,
};

use crate::error::OpcUaSimError;
use crate::events::{EventItem, EventLog};
use crate::node::{DataChangeFilterCfg, DataChangeTriggerKind, DeadbandKind, MonitoredNode};
use crate::output::DataChangeItem;

#[derive(Clone)]
pub struct SubscriptionManager {
    monitored_items: Arc<RwLock<HashMap<String, MonitoredNode>>>,
    update_seq: Arc<RwLock<u64>>,
    subscription_id: Arc<RwLock<Option<u32>>>,
    event_subscription_id: Arc<RwLock<Option<u32>>>,
    event_log: Arc<RwLock<Option<Arc<EventLog>>>>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            monitored_items: Arc::new(RwLock::new(HashMap::new())),
            update_seq: Arc::new(RwLock::new(0)),
            subscription_id: Arc::new(RwLock::new(None)),
            event_subscription_id: Arc::new(RwLock::new(None)),
            event_log: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_event_log(&self) -> Option<Arc<EventLog>> {
        self.event_log.read().await.clone()
    }

    pub async fn subscribe_to_events(
        &self,
        session: &Arc<Session>,
        source_id: &NodeId,
    ) -> Result<(), OpcUaSimError> {
        {
            let sub = self.event_subscription_id.read().await;
            if sub.is_some() {
                return Err(OpcUaSimError::SubscriptionError(
                    "Event subscription already active".into(),
                ));
            }
        }

        let event_log = Arc::new(EventLog::new(500));
        let log_for_cb = event_log.clone_shared();

        let base_event_type_id: NodeId = ObjectTypeId::BaseEventType.into();
        let select_clauses = vec![
            make_select_clause(&base_event_type_id, "Time"),
            make_select_clause(&base_event_type_id, "Severity"),
            make_select_clause(&base_event_type_id, "SourceNode"),
            make_select_clause(&base_event_type_id, "SourceName"),
            make_select_clause(&base_event_type_id, "Message"),
            make_select_clause(&base_event_type_id, "EventId"),
            make_select_clause(&base_event_type_id, "EventType"),
        ];

        let callback = EventCallback::new(move |event_fields, _item| {
            let fields = match event_fields {
                Some(f) => f,
                None => return,
            };
            if fields.len() < 7 {
                return;
            }
            let time = variant_to_string(&fields[0]);
            let severity = variant_to_u16(&fields[1]);
            let source = variant_to_string(&fields[2]) + ":" + &variant_to_string(&fields[3]);
            let message = variant_to_string(&fields[4]);
            let event_type = variant_to_string(&fields[6]);
            let item = EventItem {
                time,
                severity,
                source,
                message,
                event_type,
            };
            log_for_cb.add_sync(item);
        });

        let sub_id = session
            .create_subscription(Duration::from_millis(500), 300, 10, 0, 0, true, callback)
            .await
            .map_err(|e| {
                OpcUaSimError::SubscriptionError(format!("Create event subscription failed: {}", e))
            })?;

        let event_filter = EventFilter {
            select_clauses: Some(select_clauses),
            where_clause: ContentFilter::default(),
        };
        let filter_obj = ExtensionObject::from_message(event_filter);
        let create_req = MonitoredItemCreateRequest {
            item_to_monitor: ReadValueId {
                node_id: source_id.clone(),
                attribute_id: AttributeId::EventNotifier as u32,
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
            },
            monitoring_mode: MonitoringMode::Reporting,
            requested_parameters: MonitoringParameters {
                client_handle: 0,
                sampling_interval: 0.0,
                filter: filter_obj,
                queue_size: 10,
                discard_oldest: true,
            },
        };

        session
            .create_monitored_items(sub_id, TimestampsToReturn::Both, vec![create_req])
            .await
            .map_err(|e| {
                OpcUaSimError::SubscriptionError(format!(
                    "Create event monitored item failed: {}",
                    e
                ))
            })?;

        {
            let mut sub_slot = self.event_subscription_id.write().await;
            *sub_slot = Some(sub_id);
        }
        {
            let mut log_slot = self.event_log.write().await;
            *log_slot = Some(event_log);
        }
        info!("Event subscription created: sub_id={}", sub_id);
        Ok(())
    }

    pub async fn unsubscribe_events(
        &self,
        session: Option<&Arc<Session>>,
    ) -> Result<(), OpcUaSimError> {
        let sub_id = {
            let mut sub_slot = self.event_subscription_id.write().await;
            sub_slot.take()
        };
        if let Some(id) = sub_id {
            if let Some(s) = session {
                if let Err(e) = s.delete_subscription(id).await {
                    info!(
                        "delete event subscription {} failed (session may be gone): {}",
                        id, e
                    );
                }
            }
            let mut log_slot = self.event_log.write().await;
            *log_slot = None;
        }
        Ok(())
    }

    /// Reset subscription slot state after `Session::disconnect()` has
    /// already deleted the server-side subscriptions — prevents
    /// stale-id early returns on reconnect.
    pub async fn on_disconnect(&self) {
        {
            let mut sid = self.subscription_id.write().await;
            *sid = None;
        }
        {
            let mut eid = self.event_subscription_id.write().await;
            *eid = None;
        }
        {
            let mut log_slot = self.event_log.write().await;
            *log_slot = None;
        }
    }

    pub async fn get_events(&self) -> Vec<EventItem> {
        self.event_log
            .read()
            .await
            .as_ref()
            .map(|l| l.items_sync())
            .unwrap_or_default()
    }

    pub async fn clear_events(&self) {
        if let Some(log) = self.event_log.read().await.as_ref() {
            log.clear_sync();
        }
    }

    pub async fn add_nodes(
        &self,
        nodes: Vec<MonitoredNode>,
        session: Option<&Arc<Session>>,
    ) -> Result<(), OpcUaSimError> {
        // Insert into local tracking
        {
            let mut items = self.monitored_items.write().await;
            for node in &nodes {
                if matches!(node.access_mode, crate::node::AccessMode::Polling { .. }) {
                    continue;
                }
                info!("Adding subscription for node: {}", node.node_id);
                items.insert(node.node_id.clone(), node.clone());
            }
        }

        // If we have a session, create actual OPC UA monitored items
        if let Some(session) = session {
            let sub_id = self.ensure_subscription(session).await?;

            let items_to_create: Vec<MonitoredItemCreateRequest> = nodes
                .iter()
                .filter_map(|n| {
                    let nid: NodeId = n.node_id.parse().ok()?;
                    let interval_ms = match &n.access_mode {
                        crate::node::AccessMode::Subscription { interval_ms } => *interval_ms,
                        crate::node::AccessMode::Polling { .. } => return None, // polling handled by PollingManager
                    };
                    let filter_obj = n
                        .filter
                        .as_ref()
                        .map(filter_cfg_to_extension_object)
                        .unwrap_or_else(ExtensionObject::null);
                    Some(MonitoredItemCreateRequest {
                        item_to_monitor: ReadValueId {
                            node_id: nid,
                            attribute_id: AttributeId::Value as u32,
                            index_range: NumericRange::None,
                            data_encoding: QualifiedName::null(),
                        },
                        monitoring_mode: MonitoringMode::Reporting,
                        requested_parameters: MonitoringParameters {
                            client_handle: 0,
                            sampling_interval: interval_ms,
                            filter: filter_obj,
                            queue_size: 1,
                            discard_oldest: true,
                        },
                    })
                })
                .collect();

            if !items_to_create.is_empty() {
                match session
                    .create_monitored_items(
                        sub_id,
                        TimestampsToReturn::Both,
                        items_to_create.clone(),
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        // If subscription ID is invalid (e.g. after reconnect), recreate it
                        let err_str = format!("{}", e);
                        if err_str.contains("BadSubscriptionIdInvalid") {
                            info!("Subscription {} invalid, recreating...", sub_id);
                            self.reset_subscription_id().await;
                            let new_sub_id = self.ensure_subscription(session).await?;
                            session
                                .create_monitored_items(
                                    new_sub_id,
                                    TimestampsToReturn::Both,
                                    items_to_create,
                                )
                                .await
                                .map_err(|e2| {
                                    OpcUaSimError::SubscriptionError(format!(
                                        "Retry create monitored items failed: {}",
                                        e2
                                    ))
                                })?;
                        } else {
                            return Err(OpcUaSimError::SubscriptionError(format!(
                                "Create monitored items failed: {}",
                                e
                            )));
                        }
                    }
                }
            }

            // Do an initial read to populate values immediately (don't wait for data change)
            self.initial_read(session, &nodes).await;
        }

        Ok(())
    }

    /// Ensure a subscription exists, creating one if needed.
    async fn ensure_subscription(&self, session: &Arc<Session>) -> Result<u32, OpcUaSimError> {
        {
            let sub_id = self.subscription_id.read().await;
            if let Some(id) = *sub_id {
                return Ok(id);
            }
        }

        // Create the subscription with a DataChangeCallback that feeds into our apply_data_changes
        let monitored_items = self.monitored_items.clone();
        let update_seq = self.update_seq.clone();

        let callback = DataChangeCallback::new(move |data_value, monitored_item| {
            let raw_node_id = &monitored_item.item_to_monitor().node_id;
            let node_id_str = format!("{}", raw_node_id);
            info!("DataChange callback for node: {}", node_id_str);
            let value_str = data_value
                .value
                .as_ref()
                .map(|v| crate::server::address_space::variant_to_display_string(v))
                .unwrap_or_else(|| "null".to_string());
            let data_type_str = data_value.value.as_ref().map(|v| match v.type_id() {
                opcua_types::variant::VariantTypeId::Empty => "Empty".to_string(),
                opcua_types::variant::VariantTypeId::Scalar(s) => format!("{}", s),
                opcua_types::variant::VariantTypeId::Array(s, _) => format!("Array<{}>", s),
            });
            let quality_str = data_value
                .status
                .as_ref()
                .map(|s| format!("{}", s))
                .unwrap_or_else(|| "Good".to_string());
            let source_ts = data_value
                .source_timestamp
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_default();
            let server_ts = data_value
                .server_timestamp
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_default();

            let items = monitored_items.clone();
            let seq = update_seq.clone();
            tokio::spawn(async move {
                let mut monitored = items.write().await;
                let mut seq_val = seq.write().await;
                if let Some(node) = monitored.get_mut(&node_id_str) {
                    *seq_val += 1;
                    node.value = Some(value_str);
                    node.quality = Some(quality_str);
                    node.timestamp = Some(source_ts);
                    node.server_timestamp = Some(server_ts);
                    if let Some(dt) = data_type_str {
                        node.data_type = dt;
                    }
                    node.update_seq = *seq_val;
                }
            });
        });

        let sub_id = session
            .create_subscription(
                Duration::from_millis(1000), // publishing interval
                300,                         // lifetime count (must be >= 3 * max_keep_alive_count)
                10,                          // max keep alive count
                0,                           // max notifications per publish (0 = unlimited)
                0,                           // priority
                true,                        // publishing enabled
                callback,
            )
            .await
            .map_err(|e| {
                OpcUaSimError::SubscriptionError(format!("Create subscription failed: {}", e))
            })?;

        {
            let mut sid = self.subscription_id.write().await;
            *sid = Some(sub_id);
        }

        info!("Created OPC UA subscription with id: {}", sub_id);
        Ok(sub_id)
    }

    /// Batch read current values for all nodes in one OPC UA request (per batch of 200).
    async fn initial_read(&self, session: &Arc<Session>, nodes: &[MonitoredNode]) {
        const BATCH_SIZE: usize = 200;

        // Build all ReadValueIds upfront: 4 attributes per node
        const ATTRS_PER_NODE: usize = 4;
        let mut valid_nodes: Vec<(usize, NodeId)> = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            if let Ok(nid) = node.node_id.parse::<NodeId>() {
                valid_nodes.push((i, nid));
            }
        }

        let mut items = self.monitored_items.write().await;
        let mut seq = self.update_seq.write().await;

        for batch in valid_nodes.chunks(BATCH_SIZE) {
            let read_ids: Vec<ReadValueId> = batch
                .iter()
                .flat_map(|(_, nid)| {
                    vec![
                        ReadValueId::new(nid.clone(), opcua_types::AttributeId::DataType),
                        ReadValueId::new(nid.clone(), opcua_types::AttributeId::Value),
                        ReadValueId::new(nid.clone(), opcua_types::AttributeId::AccessLevel),
                        ReadValueId::new(nid.clone(), opcua_types::AttributeId::UserAccessLevel),
                    ]
                })
                .collect();

            match session.read(&read_ids, TimestampsToReturn::Both, 0.0).await {
                Ok(values) => {
                    for (batch_idx, (node_idx, _)) in batch.iter().enumerate() {
                        let dt_dv = values.get(batch_idx * ATTRS_PER_NODE);
                        let val_dv = values.get(batch_idx * ATTRS_PER_NODE + 1);
                        let al_dv = values.get(batch_idx * ATTRS_PER_NODE + 2);
                        let ual_dv = values.get(batch_idx * ATTRS_PER_NODE + 3);

                        let data_type = dt_dv
                            .and_then(|dv| dv.value.as_ref())
                            .map(|v| resolve_data_type(&format!("{}", v)))
                            .unwrap_or_else(|| "Unknown".to_string());

                        let value = val_dv
                            .and_then(|dv| dv.value.as_ref())
                            .map(|v| crate::server::address_space::variant_to_display_string(v));
                        let quality = val_dv
                            .and_then(|dv| dv.status.as_ref())
                            .map(|s| format!("{}", s));
                        let source_ts = val_dv
                            .and_then(|dv| dv.source_timestamp.as_ref())
                            .map(|t| t.to_string());
                        let server_ts = val_dv
                            .and_then(|dv| dv.server_timestamp.as_ref())
                            .map(|t| t.to_string());
                        let is_value_ok = quality.as_deref() != Some("BadAttributeIdInvalid");

                        // Extract access level byte from Variant, handling multiple numeric types
                        let extract_byte = |dv: Option<&opcua_types::DataValue>| -> Option<u8> {
                            let v = dv?.value.as_ref()?;
                            match v {
                                opcua_types::Variant::Byte(b) => Some(*b),
                                opcua_types::Variant::UInt16(u) => Some(*u as u8),
                                opcua_types::Variant::Int16(i) => Some(*i as u8),
                                opcua_types::Variant::UInt32(u) => Some(*u as u8),
                                opcua_types::Variant::Int32(i) => Some(*i as u8),
                                _ => None,
                            }
                        };
                        // Prefer UserAccessLevel; fall back to AccessLevel if unavailable
                        let user_access_level = extract_byte(ual_dv)
                            .or_else(|| extract_byte(al_dv))
                            .unwrap_or(0);

                        if let Some(n) = items.get_mut(&nodes[*node_idx].node_id) {
                            *seq += 1;
                            n.data_type = data_type;
                            n.timestamp = source_ts;
                            n.server_timestamp = server_ts;
                            n.user_access_level = user_access_level;
                            if is_value_ok {
                                n.value = value;
                                n.quality = Some(quality.unwrap_or_else(|| "Good".to_string()));
                            } else {
                                n.value = None;
                                n.quality = Some("N/A".to_string());
                            }
                            n.update_seq = *seq;
                        }
                    }
                    info!("Batch read completed: {} nodes", batch.len());
                }
                Err(e) => {
                    // Mark all nodes in this batch as failed
                    for (node_idx, _) in batch {
                        if let Some(n) = items.get_mut(&nodes[*node_idx].node_id) {
                            *seq += 1;
                            n.quality = Some(format!("ReadError: {}", e));
                            n.update_seq = *seq;
                        }
                    }
                    info!("Batch read failed: {}", e);
                }
            }
        }
        info!(
            "Initial read completed for {} nodes ({} batches)",
            nodes.len(),
            valid_nodes.len().div_ceil(BATCH_SIZE)
        );
    }

    /// Reset the subscription ID (e.g. after reconnect)
    async fn reset_subscription_id(&self) {
        let mut sid = self.subscription_id.write().await;
        *sid = None;
    }

    pub async fn remove_nodes(&self, node_ids: &[String]) -> Result<(), OpcUaSimError> {
        let mut items = self.monitored_items.write().await;
        for id in node_ids {
            items.remove(id);
        }
        Ok(())
    }

    pub async fn get_monitored_nodes(&self) -> Vec<MonitoredNode> {
        self.monitored_items
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn get_monitored_nodes_since(&self, since_seq: u64) -> Vec<MonitoredNode> {
        self.monitored_items
            .read()
            .await
            .values()
            .filter(|n| n.update_seq > since_seq)
            .cloned()
            .collect()
    }

    pub async fn apply_data_changes(&self, items: &[DataChangeItem]) {
        let mut monitored = self.monitored_items.write().await;
        let mut seq = self.update_seq.write().await;
        for item in items {
            if let Some(node) = monitored.get_mut(&item.node_id) {
                *seq += 1;
                node.value = Some(item.value.clone());
                node.quality = Some(item.quality.clone());
                node.timestamp = Some(item.timestamp.clone());
                node.update_seq = *seq;
            }
        }
    }

    pub async fn get_update_seq(&self) -> u64 {
        *self.update_seq.read().await
    }
}

/// Resolve OPC UA DataType NodeId to human-readable name
fn resolve_data_type(node_id_str: &str) -> String {
    // OPC UA built-in type NodeIds (namespace 0, numeric identifiers)
    match node_id_str {
        "i=1" => "Boolean".to_string(),
        "i=2" => "SByte".to_string(),
        "i=3" => "Byte".to_string(),
        "i=4" => "Int16".to_string(),
        "i=5" => "UInt16".to_string(),
        "i=6" => "Int32".to_string(),
        "i=7" => "UInt32".to_string(),
        "i=8" => "Int64".to_string(),
        "i=9" => "UInt64".to_string(),
        "i=10" => "Float".to_string(),
        "i=11" => "Double".to_string(),
        "i=12" => "String".to_string(),
        "i=13" => "DateTime".to_string(),
        "i=14" => "Guid".to_string(),
        "i=15" => "ByteString".to_string(),
        "i=16" => "XmlElement".to_string(),
        "i=17" => "NodeId".to_string(),
        "i=19" => "StatusCode".to_string(),
        "i=20" => "QualifiedName".to_string(),
        "i=21" => "LocalizedText".to_string(),
        "i=22" => "ExtensionObject".to_string(),
        "i=24" => "BaseDataType".to_string(),
        "i=26" => "Number".to_string(),
        "i=27" => "Integer".to_string(),
        "i=28" => "UInteger".to_string(),
        "i=29" => "Enumeration".to_string(),
        other => other.to_string(),
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

fn make_select_clause(type_def_id: &NodeId, field_name: &str) -> SimpleAttributeOperand {
    SimpleAttributeOperand {
        type_definition_id: type_def_id.clone(),
        browse_path: Some(vec![QualifiedName::from(field_name)]),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
    }
}

fn variant_to_string(v: &opcua_types::Variant) -> String {
    match v {
        opcua_types::Variant::String(s) => s.to_string(),
        opcua_types::Variant::LocalizedText(lt) => lt.text.to_string(),
        opcua_types::Variant::NodeId(n) => format!("{}", n),
        opcua_types::Variant::ByteString(b) => format!("{:?}", b),
        opcua_types::Variant::DateTime(dt) => format!("{}", dt),
        opcua_types::Variant::Empty => String::new(),
        other => format!("{}", other),
    }
}

fn variant_to_u16(v: &opcua_types::Variant) -> u16 {
    match v {
        opcua_types::Variant::UInt16(u) => *u,
        opcua_types::Variant::UInt32(u) => (*u).min(u16::MAX as u32) as u16,
        opcua_types::Variant::Int16(i) => (*i).max(0) as u16,
        opcua_types::Variant::Int32(i) => (*i).clamp(0, u16::MAX as i32) as u16,
        opcua_types::Variant::Byte(b) => *b as u16,
        _ => 0,
    }
}

fn filter_cfg_to_extension_object(cfg: &DataChangeFilterCfg) -> ExtensionObject {
    let trigger = match cfg.trigger {
        DataChangeTriggerKind::Status => DataChangeTrigger::Status,
        DataChangeTriggerKind::StatusValue => DataChangeTrigger::StatusValue,
        DataChangeTriggerKind::StatusValueTimestamp => DataChangeTrigger::StatusValueTimestamp,
    };
    let deadband_type: u32 = match cfg.deadband_kind {
        DeadbandKind::None => 0,
        DeadbandKind::Absolute => 1,
        DeadbandKind::Percent => 2,
    };
    ExtensionObject::from_message(DataChangeFilter {
        trigger,
        deadband_type,
        deadband_value: cfg.deadband_value,
    })
}
