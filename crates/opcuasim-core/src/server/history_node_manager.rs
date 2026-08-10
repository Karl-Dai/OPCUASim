//! Custom in-memory node manager: delegates all standard services to an inner
//! [`SimpleNodeManagerImpl`], adds real history read (ring buffer) and records
//! client writes into the history store.

use std::sync::Arc;

use async_trait::async_trait;
use opcua_core::sync::RwLock;
use opcua_server::address_space::AddressSpace;
use opcua_server::diagnostics::NamespaceMetadata;
use opcua_server::node_manager::memory::{InMemoryNodeManagerImpl, SimpleNodeManagerImpl};
use opcua_server::node_manager::{
    AddNodeItem, AddReferenceItem, DeleteNodeItem, DeleteReferenceItem, HistoryNode,
    HistoryUpdateNode, MethodCall, MonitoredItemRef, MonitoredItemUpdateRef, ParsedReadValueId,
    RegisterNodeItem, RequestContext, ServerContext, WriteNode,
};
use opcua_server::{ContinuationPoint, CreateMonitoredItem};
use opcua_types::{
    DataValue, DateTime, FilterOperator, HistoryData, HistoryEvent, HistoryEventFieldList,
    MonitoringMode, NodeId, NumericRange, ReadAnnotationDataDetails, ReadAtTimeDetails,
    ReadEventDetails, ReadProcessedDetails, ReadRawModifiedDetails, StatusCode, TimestampsToReturn,
    Variant,
};

use super::event_store::EventStore;
use super::history_store::HistoryStore;

/// In-memory node manager with history support.
///
/// Wraps a [`SimpleNodeManagerImpl`] and delegates every OPC UA service to it,
/// except:
/// * `history_read_raw_modified` — serves samples from the in-memory ring
///   buffer with paginated continuation points.
/// * `history_read_events` — serves events from the in-memory ring buffer with
///   field selection and continuation-point paging.
/// * `write` — delegates first, then records every successful external write
///   into the [`HistoryStore`].
pub struct HistoryNodeManagerImpl {
    inner: SimpleNodeManagerImpl,
    history: Arc<HistoryStore>,
    event_store: Arc<EventStore>,
}

impl HistoryNodeManagerImpl {
    pub fn new(
        inner: SimpleNodeManagerImpl,
        history: Arc<HistoryStore>,
        event_store: Arc<EventStore>,
    ) -> Self {
        Self {
            inner,
            history,
            event_store,
        }
    }

    /// Forward a method callback registration to the inner manager.
    pub fn add_method_callback(
        &self,
        id: NodeId,
        cb: impl Fn(&[Variant]) -> Result<Vec<Variant>, StatusCode> + Send + Sync + 'static,
    ) {
        self.inner.add_method_callback(id, cb);
    }

    /// Forward a write callback registration to the inner manager.
    pub fn add_write_callback(
        &self,
        id: NodeId,
        cb: impl Fn(DataValue, &NumericRange) -> StatusCode + Send + Sync + 'static,
    ) {
        self.inner.add_write_callback(id, cb);
    }

    /// Forward a read callback registration to the inner manager.
    pub fn add_read_callback(
        &self,
        id: NodeId,
        cb: impl Fn(&NumericRange, TimestampsToReturn, f64) -> Result<DataValue, StatusCode>
            + Send
            + Sync
            + 'static,
    ) {
        self.inner.add_read_callback(id, cb);
    }
}

#[async_trait]
impl InMemoryNodeManagerImpl for HistoryNodeManagerImpl {
    // ------------------------------------------------------------------
    // Delegated lifecycle / metadata
    // ------------------------------------------------------------------

    async fn init(&self, address_space: &mut AddressSpace, context: ServerContext) {
        self.inner.init(address_space, context).await;
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn namespaces(&self) -> Vec<NamespaceMetadata> {
        self.inner.namespaces()
    }

    // ------------------------------------------------------------------
    // Delegated Read services
    // ------------------------------------------------------------------

    async fn read_values(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes: &[&ParsedReadValueId],
        max_age: f64,
        timestamps_to_return: TimestampsToReturn,
    ) -> Vec<DataValue> {
        self.inner
            .read_values(context, address_space, nodes, max_age, timestamps_to_return)
            .await
    }

    // ------------------------------------------------------------------
    // Delegated Subscription / Monitored Item services
    // ------------------------------------------------------------------

    async fn create_value_monitored_items(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        items: &mut [&mut &mut CreateMonitoredItem],
    ) {
        self.inner
            .create_value_monitored_items(context, address_space, items)
            .await;
    }

    async fn create_event_monitored_items(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        items: &mut [&mut &mut CreateMonitoredItem],
    ) {
        self.inner
            .create_event_monitored_items(context, address_space, items)
            .await;
    }

    async fn set_monitoring_mode(
        &self,
        context: &RequestContext,
        mode: MonitoringMode,
        items: &[&MonitoredItemRef],
    ) {
        self.inner.set_monitoring_mode(context, mode, items).await;
    }

    async fn modify_monitored_items(
        &self,
        context: &RequestContext,
        items: &[&MonitoredItemUpdateRef],
    ) {
        self.inner.modify_monitored_items(context, items).await;
    }

    async fn delete_monitored_items(&self, context: &RequestContext, items: &[&MonitoredItemRef]) {
        self.inner.delete_monitored_items(context, items).await;
    }

    // ------------------------------------------------------------------
    // Delegated Register/Unregister
    // ------------------------------------------------------------------

    async fn register_nodes(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes: &mut [&mut RegisterNodeItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .register_nodes(context, address_space, nodes)
            .await
    }

    async fn unregister_nodes(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes: &[&NodeId],
    ) -> Result<(), StatusCode> {
        self.inner
            .unregister_nodes(context, address_space, nodes)
            .await
    }

    // ------------------------------------------------------------------
    // Write: delegate, then record successful external writes to history
    // ------------------------------------------------------------------

    async fn write(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes_to_write: &mut [&mut WriteNode],
    ) -> Result<(), StatusCode> {
        let result = self
            .inner
            .write(context, address_space, nodes_to_write)
            .await;
        if result.is_ok() {
            for node in nodes_to_write.iter() {
                if node.status().is_good() {
                    let pv = node.value();
                    let mut dv = pv.value.clone();
                    let now = DateTime::now();
                    if dv.source_timestamp.is_none() {
                        dv.source_timestamp = Some(now);
                    }
                    if dv.server_timestamp.is_none() {
                        dv.server_timestamp = Some(now);
                    }
                    self.history.record(&pv.node_id, dv).await;
                }
            }
        }
        result
    }

    // ------------------------------------------------------------------
    // Delegated Call
    // ------------------------------------------------------------------

    async fn call(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        methods_to_call: &mut [&mut &mut MethodCall],
    ) -> Result<(), StatusCode> {
        self.inner
            .call(context, address_space, methods_to_call)
            .await
    }

    // ------------------------------------------------------------------
    // History read, raw only. Serves samples from the in-memory ring
    // buffer with continuation-point paging (CP wraps a `usize` skip count).
    // ------------------------------------------------------------------

    async fn history_read_raw_modified(
        &self,
        _context: &RequestContext,
        details: &ReadRawModifiedDetails,
        nodes: &mut [&mut &mut HistoryNode],
        _timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        if details.is_read_modified {
            return Err(StatusCode::BadHistoryOperationUnsupported);
        }
        for node in nodes.iter_mut() {
            let node_id = node.node_id().clone();
            let skip = match node.continuation_point() {
                Some(cp) => cp
                    .get::<usize>()
                    .copied()
                    .ok_or(StatusCode::BadContinuationPointInvalid)?,
                None => 0,
            };
            let (values, next_skip) = self
                .history
                .query(
                    &node_id,
                    details.start_time,
                    details.end_time,
                    details.num_values_per_node,
                    skip,
                )
                .await;
            node.set_result(HistoryData {
                data_values: Some(values),
            });
            node.set_next_continuation_point(
                next_skip.map(|s| ContinuationPoint::new(Box::new(s))),
            );
            node.set_status(StatusCode::Good);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Other history reads: delegate — SimpleNodeManagerImpl doesn't
    // override them, so the default returns BadHistoryOperationUnsupported.
    // ------------------------------------------------------------------

    async fn history_read_processed(
        &self,
        context: &RequestContext,
        details: &ReadProcessedDetails,
        nodes: &mut [&mut &mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        self.inner
            .history_read_processed(context, details, nodes, timestamps_to_return)
            .await
    }

    async fn history_read_at_time(
        &self,
        context: &RequestContext,
        details: &ReadAtTimeDetails,
        nodes: &mut [&mut &mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        self.inner
            .history_read_at_time(context, details, nodes, timestamps_to_return)
            .await
    }

    async fn history_read_events(
        &self,
        _context: &RequestContext,
        details: &ReadEventDetails,
        nodes: &mut [&mut &mut HistoryNode],
        _timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        let field_names: &[&str] = &[
            "EventId",
            "EventType",
            "SourceNode",
            "SourceName",
            "Time",
            "ReceiveTime",
            "Message",
            "Severity",
        ];
        let select_indices: Option<Vec<usize>> =
            details.filter.select_clauses.as_ref().map(|clauses| {
                clauses
                    .iter()
                    .map(|clause| {
                        clause
                            .browse_path
                            .as_ref()
                            .and_then(|path| {
                                path.last()
                                    .map(|qn| qn.name.to_string())
                                    .and_then(|name| field_names.iter().position(|n| *n == name))
                            })
                            .unwrap_or(usize::MAX)
                    })
                    .collect()
            });

        let filter_eq: Option<(usize, Variant)> = details
            .filter
            .where_clause
            .elements
            .as_ref()
            .and_then(|elements| {
                if elements.len() != 1 {
                    return None;
                }
                let el = &elements[0];
                if el.filter_operator != FilterOperator::Equals {
                    return None;
                }
                let operands = el.filter_operands.as_ref()?;
                if operands.len() != 2 {
                    return None;
                }
                let sao = operands[0].inner_as::<opcua_types::SimpleAttributeOperand>()?;
                let lit = operands[1].inner_as::<opcua_types::LiteralOperand>()?;
                let field_idx = sao
                    .browse_path
                    .as_ref()
                    .and_then(|path| path.last().map(|qn| qn.name.to_string()))
                    .and_then(|name| field_names.iter().position(|n| *n == name))?;
                Some((field_idx, lit.value.clone()))
            });

        for node in nodes.iter_mut() {
            let node_id = node.node_id().clone();
            let skip = match node.continuation_point() {
                Some(cp) => cp
                    .get::<usize>()
                    .copied()
                    .ok_or(StatusCode::BadContinuationPointInvalid)?,
                None => 0,
            };
            let (mut events, next_skip) = self
                .event_store
                .query(
                    &node_id,
                    details.start_time,
                    details.end_time,
                    details.num_values_per_node,
                    skip,
                )
                .await;

            if let Some((idx, ref expected)) = filter_eq {
                events.retain(|(_, fields)| {
                    fields.get(idx).is_some_and(|v| variants_match(v, expected))
                });
            }

            let field_lists: Vec<HistoryEventFieldList> = events
                .into_iter()
                .map(|(_time, fields)| {
                    let event_fields = match &select_indices {
                        Some(indices) => indices
                            .iter()
                            .map(|&i| {
                                if i < fields.len() {
                                    fields[i].clone()
                                } else {
                                    Variant::Empty
                                }
                            })
                            .collect(),
                        None => fields,
                    };
                    HistoryEventFieldList {
                        event_fields: Some(event_fields),
                    }
                })
                .collect();

            node.set_result(HistoryEvent {
                events: if field_lists.is_empty() {
                    None
                } else {
                    Some(field_lists)
                },
            });
            node.set_next_continuation_point(
                next_skip.map(|s| ContinuationPoint::new(Box::new(s))),
            );
            node.set_status(StatusCode::Good);
        }
        Ok(())
    }

    async fn history_read_annotations(
        &self,
        context: &RequestContext,
        details: &ReadAnnotationDataDetails,
        nodes: &mut [&mut &mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        self.inner
            .history_read_annotations(context, details, nodes, timestamps_to_return)
            .await
    }

    // ------------------------------------------------------------------
    // history_update: delegate.
    // ------------------------------------------------------------------

    async fn history_update(
        &self,
        context: &RequestContext,
        nodes: &mut [&mut &mut HistoryUpdateNode],
    ) -> Result<(), StatusCode> {
        self.inner.history_update(context, nodes).await
    }

    // ------------------------------------------------------------------
    // Node-management services: delegate.
    // ------------------------------------------------------------------

    async fn add_nodes(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes_to_add: &mut [&mut AddNodeItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .add_nodes(context, address_space, nodes_to_add)
            .await
    }

    async fn add_references(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        references_to_add: &mut [&mut AddReferenceItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .add_references(context, address_space, references_to_add)
            .await
    }

    async fn delete_nodes(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes_to_delete: &mut [&mut DeleteNodeItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .delete_nodes(context, address_space, nodes_to_delete)
            .await
    }

    async fn delete_node_references(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        to_delete: &[&DeleteNodeItem],
    ) {
        self.inner
            .delete_node_references(context, address_space, to_delete)
            .await;
    }

    async fn delete_references(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        references_to_delete: &mut [&mut DeleteReferenceItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .delete_references(context, address_space, references_to_delete)
            .await
    }
}

fn variants_match(a: &Variant, b: &Variant) -> bool {
    match (a, b) {
        (Variant::Boolean(x), Variant::Boolean(y)) => x == y,
        (Variant::Byte(x), Variant::Byte(y)) => x == y,
        (Variant::SByte(x), Variant::SByte(y)) => x == y,
        (Variant::Int16(x), Variant::Int16(y)) => x == y,
        (Variant::UInt16(x), Variant::UInt16(y)) => x == y,
        (Variant::Int32(x), Variant::Int32(y)) => x == y,
        (Variant::UInt32(x), Variant::UInt32(y)) => x == y,
        (Variant::Int64(x), Variant::Int64(y)) => x == y,
        (Variant::UInt64(x), Variant::UInt64(y)) => x == y,
        (Variant::Float(x), Variant::Float(y)) => x == y,
        (Variant::Double(x), Variant::Double(y)) => x == y,
        (Variant::String(x), Variant::String(y)) => x == y,
        (Variant::ByteString(x), Variant::ByteString(y)) => x == y,
        (Variant::NodeId(x), Variant::NodeId(y)) => x == y,
        _ => format!("{a}") == format!("{b}"),
    }
}
