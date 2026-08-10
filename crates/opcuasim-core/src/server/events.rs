//! Events: DemoEvents source object, event notification and RaiseEvent method.

use std::sync::Arc;

use log::info;
use opcua_nodes::{BaseEventType, EventNotifier, ObjectBuilder};
use opcua_server::address_space::AddressSpace;
use opcua_server::node_manager::memory::InMemoryNodeManager;
use opcua_server::SubscriptionCache;
use opcua_types::{
    DataTypeId, DateTime, LocalizedText, NodeId, ObjectId, ObjectTypeId, StatusCode, UAString,
    Variant,
};

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

    // Live-notify listeners subscribed to events on this source.
    subscriptions.notify_events([(&event as &dyn opcua_nodes::Event, source)].into_iter());

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
