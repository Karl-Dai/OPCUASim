//! Events: DemoEvents source object, event notification and RaiseEvent method.

use std::sync::Arc;
use std::time::Duration;

use log::info;
use opcua_core::sync::RwLock;
use opcua_nodes::{BaseEventType, EventNotifier, ObjectBuilder};
use opcua_server::address_space::AddressSpace;
use opcua_server::node_manager::memory::InMemoryNodeManager;
use opcua_server::SubscriptionCache;
use opcua_types::{
    AttributeId, DataEncoding, DataTypeId, DateTime, LocalizedText, NodeId, NumericRange, ObjectId,
    ObjectTypeId, StatusCode, TimestampsToReturn, UAString, Variant, VariableId,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::event_store::EventStore;
use super::history_node_manager::HistoryNodeManagerImpl;
use super::methods::{arg, register_method};
use crate::error::OpcUaSimError;

/// Browse-name / string identifier of the DemoEvents source object.
pub const DEMO_EVENTS_ID: &str = "DemoEvents";

/// Create the DemoEvents event source object under ObjectsFolder with
/// `SUBSCRIBE_TO_EVENTS | HISTORY_READ` event notifier and BaseObjectType.
pub fn build_events_object(
    address_space: &mut AddressSpace,
    ns: u16,
) -> Result<NodeId, OpcUaSimError> {
    let id = NodeId::new(ns, DEMO_EVENTS_ID);
    let inserted = ObjectBuilder::new(&id, DEMO_EVENTS_ID, DEMO_EVENTS_ID)
        .event_notifier(EventNotifier::SUBSCRIBE_TO_EVENTS | EventNotifier::HISTORY_READ)
        .organized_by(ObjectId::ObjectsFolder)
        .has_type_definition(ObjectTypeId::BaseObjectType)
        .insert(address_space);
    if inserted {
        info!("Created DemoEvents object: {:?}", id);
        Ok(id)
    } else {
        Err(OpcUaSimError::ServerError(
            "Failed to insert DemoEvents object into address space".into(),
        ))
    }
}

/// Construct a `BaseEventType`, live-notify subscribers and
/// asynchronously record the event fields in `event_store` (if provided).
///
/// Note: must be called from within a tokio runtime context (method
/// callbacks run on the server's tokio worker threads).
pub fn notify_event(
    subscriptions: &SubscriptionCache,
    event_store: &Option<Arc<EventStore>>,
    source: &NodeId,
    message: &str,
    severity: u16,
) {
    let now = DateTime::now();
    let event_id = opcua_crypto::random::byte_string(6);
    let event_id_for_store = event_id.clone();

    let event = BaseEventType::new(
        ObjectTypeId::BaseEventType,
        event_id,
        message,
        now,
    )
    .set_source_node(source.clone())
    .set_source_name(UAString::from(DEMO_EVENTS_ID))
    .set_severity(severity);

    // Live-notify listeners subscribed to events on this source (single-event push).
    subscriptions.notify_events(std::iter::once((&event as &dyn opcua_nodes::Event, source)));

    // Asynchronously record fields to the EventStore ring buffer.
    if let Some(store) = event_store {
        let store = Arc::clone(store);
        let source_clone = source.clone();
        let fields = vec![
            Variant::ByteString(event_id_for_store),
            Variant::NodeId(Box::new(event.event_type.clone())),
            Variant::NodeId(Box::new(source.clone())),
            Variant::String(UAString::from(DEMO_EVENTS_ID)),
            Variant::DateTime(Box::new(now)),
            Variant::DateTime(Box::new(now)),
            Variant::LocalizedText(Box::new(LocalizedText::from(message))),
            Variant::UInt16(severity),
        ];
        tokio::spawn(async move {
            store.record(&source_clone, now, fields).await;
        });
    }
}

/// Register the `Demo.RaiseEvent(severity: UInt16, message: String)`
/// preset method under ObjectsFolder. Calling the method constructs a
/// BaseEventType, notifies subscribers and records it to the EventStore.
pub fn register_raise_event_method(
    nm: &Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>,
    ns: u16,
    subscriptions: Arc<SubscriptionCache>,
    event_store: Option<Arc<EventStore>>,
    events_source: NodeId,
) -> NodeId {
    let subs_capture = Arc::clone(&subscriptions);
    let store_capture = event_store.clone();
    let source_capture = events_source.clone();

    register_method(
        nm,
        ns,
        "Demo.RaiseEvent",
        "RaiseEvent",
        &[
            arg("severity", DataTypeId::UInt16),
            arg("message", DataTypeId::String),
        ],
        &[],
        move |inputs: &[Variant]| {
            let severity = match inputs.first() {
                Some(Variant::UInt16(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let message = match inputs.get(1) {
                Some(Variant::String(s)) => s.to_string(),
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            notify_event(&subs_capture, &store_capture, &source_capture, &message, severity);
            Ok(vec![])
        },
    )
}

/// Spawn a background task that emits a heartbeat event at the specified
/// interval with an incrementing sequence number. The task exits when
/// `cancel` is triggered.
pub fn spawn_heartbeat_task(
    subscriptions: Arc<SubscriptionCache>,
    event_store: Option<Arc<EventStore>>,
    source: NodeId,
    interval: Duration,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut seq: u64 = 0;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {
                    let message = format!("Heartbeat {}", seq);
                    notify_event(&subscriptions, &event_store, &source, &message, 100);
                    seq += 1;
                }
            }
        }
        info!("Heartbeat task stopped");
    })
}

/// Spawn a background task that polls the server's current session count
/// at the specified interval and emits an event when the count changes.
///
/// Reads `CurrentSessionCount` from the standard OPC UA address space
/// (`VariableId::Server_ServerDiagnostics_ServerDiagnosticsSummary_CurrentSessionCount`).
/// If the node cannot be located or the value cannot be extracted the
/// iteration is silently skipped.
pub fn spawn_connection_monitor_task(
    subscriptions: Arc<SubscriptionCache>,
    event_store: Option<Arc<EventStore>>,
    source: NodeId,
    address_space: Arc<RwLock<AddressSpace>>,
    interval: Duration,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let session_node_id: NodeId =
            VariableId::Server_ServerDiagnostics_ServerDiagnosticsSummary_CurrentSessionCount
                .into();
        let mut last_count: Option<i32> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {
                    // Read session count from address space (synchronous lock,
                    // brief and cheap — no async work while holding the guard).
                    let current: Option<i32> = {
                        let space = address_space.read();
                        space
                            .find_node(&session_node_id)
                            .and_then(|node| {
                                node.as_node().get_attribute(
                                    TimestampsToReturn::Neither,
                                    AttributeId::Value,
                                    &NumericRange::None,
                                    &DataEncoding::default(),
                                )
                            })
                            .and_then(|dv| dv.value)
                            .and_then(variant_to_i32)
                    };

                    if let Some(count) = current {
                        match last_count {
                            None => {
                                // First observation — record baseline; no event.
                                last_count = Some(count);
                            }
                            Some(prev) if prev == count => { /* unchanged */ }
                            Some(prev) => {
                                let (message, severity) = if count > prev {
                                    (format!("Client connected ({} sessions)", count), 200u16)
                                } else {
                                    (format!("Client disconnected ({} sessions)", count), 300u16)
                                };
                                notify_event(&subscriptions, &event_store, &source, &message, severity);
                                last_count = Some(count);
                            }
                        }
                    }
                }
            }
        }
        info!("Connection monitor task stopped");
    })
}

fn variant_to_i32(v: Variant) -> Option<i32> {
    match v {
        Variant::Int32(n) => Some(n),
        Variant::UInt32(n) => n.try_into().ok(),
        Variant::Int16(n) => Some(i32::from(n)),
        Variant::UInt16(n) => Some(i32::from(n)),
        Variant::Byte(n) => Some(i32::from(n)),
        _ => None,
    }
}
