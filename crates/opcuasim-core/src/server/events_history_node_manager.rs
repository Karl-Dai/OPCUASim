//! A thin wrapper around [`InMemoryNodeManager<HistoryNodeManagerImpl>`] that
//! bypasses the library's incorrect validation in `history_read_events`.
//!
//! The upstream `InMemoryNodeManager` wraps `history_read_events` with a call
//! to `validate_history_read_nodes(context, nodes, false)`. The hardcoded
//! `false` flag forces the "variable history" validation branch, which rejects
//! `Object` nodes (event sources like `DemoEvents`) with
//! `BadHistoryOperationUnsupported`.  Our [`HistoryNodeManagerImpl`]
//! already validates and serves event history correctly; this wrapper simply
//! skips the library's faulty gate and calls it directly.
//!
//! Every other OPC UA service is forwarded unchanged to the inner
//! [`InMemoryNodeManager`].

use std::sync::Arc;

use async_trait::async_trait;

use opcua_nodes::DefaultTypeTree;
use opcua_server::diagnostics::NamespaceMetadata;
use opcua_server::node_manager::memory::{InMemoryNodeManager, InMemoryNodeManagerImpl};
use opcua_server::node_manager::{
    AddNodeItem, AddReferenceItem, BrowseNode, BrowsePathItem, DeleteNodeItem, DeleteReferenceItem,
    DynNodeManager, ExternalReferenceRequest, HistoryNode, HistoryUpdateNode, MethodCall,
    MonitoredItemRef, MonitoredItemUpdateRef, NodeManager, QueryRequest, ReadNode,
    RegisterNodeItem, RequestContext, ServerContext, WriteNode,
};
use opcua_server::CreateMonitoredItem;
use opcua_types::{
    ExpandedNodeId, MonitoringMode, NodeId, ReadAnnotationDataDetails, ReadAtTimeDetails,
    ReadEventDetails, ReadProcessedDetails, ReadRawModifiedDetails, StatusCode, TimestampsToReturn,
};

use super::history_node_manager::HistoryNodeManagerImpl;

/// Wrapper node manager that fixes event-history reads while keeping all other
/// OPC UA services identical to [`InMemoryNodeManager<HistoryNodeManagerImpl>`].
pub struct EventsHistoryNodeManager {
    inner: Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>,
}

impl EventsHistoryNodeManager {
    /// Wrap an already-constructed [`InMemoryNodeManager`].
    pub fn new(inner: Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>) -> Self {
        Self { inner }
    }

    /// Access the underlying [`InMemoryNodeManager`].
    pub fn inner(&self) -> &Arc<InMemoryNodeManager<HistoryNodeManagerImpl>> {
        &self.inner
    }

    /// Build an [`EventsHistoryNodeManager`] from the library's builder,
    /// downcasting the opaque `Arc<DynNodeManager>` produced by
    /// [`InMemoryNodeManagerBuilder::build`].
    pub fn from_dyn(dyn_nm: Arc<DynNodeManager>) -> Option<Arc<Self>> {
        dyn_nm
            .into_any_arc()
            .downcast::<InMemoryNodeManager<HistoryNodeManagerImpl>>()
            .ok()
            .map(|inner| Arc::new(Self { inner }))
    }
}

#[async_trait]
impl NodeManager for EventsHistoryNodeManager {
    fn owns_node(&self, id: &NodeId) -> bool {
        self.inner.owns_node(id)
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    /// Claim ownership of server-wide events so that history reads for
    /// `ObjectId::Server` are also routed here.
    fn owns_server_events(&self) -> bool {
        true
    }

    fn handle_new_node(&self, parent_id: &ExpandedNodeId) -> bool {
        self.inner.handle_new_node(parent_id)
    }

    fn namespaces_for_user(&self, context: &RequestContext) -> Vec<NamespaceMetadata> {
        self.inner.namespaces_for_user(context)
    }

    async fn init(&self, type_tree: &mut DefaultTypeTree, context: ServerContext) {
        self.inner.init(type_tree, context).await;
    }

    async fn resolve_external_references(
        &self,
        context: &RequestContext,
        items: &mut [&mut ExternalReferenceRequest],
    ) {
        self.inner.resolve_external_references(context, items).await;
    }

    async fn read(
        &self,
        context: &RequestContext,
        max_age: f64,
        timestamps_to_return: TimestampsToReturn,
        nodes_to_read: &mut [&mut ReadNode],
    ) -> Result<(), StatusCode> {
        self.inner
            .read(context, max_age, timestamps_to_return, nodes_to_read)
            .await
    }

    async fn history_read_raw_modified(
        &self,
        context: &RequestContext,
        details: &ReadRawModifiedDetails,
        nodes: &mut [&mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        self.inner
            .history_read_raw_modified(context, details, nodes, timestamps_to_return)
            .await
    }

    async fn history_read_processed(
        &self,
        context: &RequestContext,
        details: &ReadProcessedDetails,
        nodes: &mut [&mut HistoryNode],
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
        nodes: &mut [&mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        self.inner
            .history_read_at_time(context, details, nodes, timestamps_to_return)
            .await
    }

    /// Bypass the library's hard-coded `validate_history_read_nodes(..., false)`
    /// gate and call [`HistoryNodeManagerImpl::history_read_events`] directly.
    async fn history_read_events(
        &self,
        context: &RequestContext,
        details: &ReadEventDetails,
        nodes: &mut [&mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        // Convert `&mut [&mut HistoryNode]` into the shape expected by
        // `InMemoryNodeManagerImpl::history_read_events` (a `&mut [&mut &mut HistoryNode]`).
        let mut nodes_for_inner: Vec<&mut &mut HistoryNode> = nodes.iter_mut().collect();
        self.inner
            .inner()
            .history_read_events(context, details, &mut nodes_for_inner, timestamps_to_return)
            .await
    }

    async fn history_read_annotations(
        &self,
        context: &RequestContext,
        details: &ReadAnnotationDataDetails,
        nodes: &mut [&mut HistoryNode],
        timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        self.inner
            .history_read_annotations(context, details, nodes, timestamps_to_return)
            .await
    }

    async fn write(
        &self,
        context: &RequestContext,
        nodes_to_write: &mut [&mut WriteNode],
    ) -> Result<(), StatusCode> {
        self.inner.write(context, nodes_to_write).await
    }

    async fn history_update(
        &self,
        context: &RequestContext,
        nodes: &mut [&mut HistoryUpdateNode],
    ) -> Result<(), StatusCode> {
        self.inner.history_update(context, nodes).await
    }

    async fn browse(
        &self,
        context: &RequestContext,
        nodes_to_browse: &mut [BrowseNode],
    ) -> Result<(), StatusCode> {
        self.inner.browse(context, nodes_to_browse).await
    }

    async fn translate_browse_paths_to_node_ids(
        &self,
        context: &RequestContext,
        nodes: &mut [&mut BrowsePathItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .translate_browse_paths_to_node_ids(context, nodes)
            .await
    }

    async fn register_nodes(
        &self,
        context: &RequestContext,
        nodes: &mut [&mut RegisterNodeItem],
    ) -> Result<(), StatusCode> {
        self.inner.register_nodes(context, nodes).await
    }

    async fn unregister_nodes(
        &self,
        context: &RequestContext,
        nodes: &[&NodeId],
    ) -> Result<(), StatusCode> {
        self.inner.unregister_nodes(context, nodes).await
    }

    async fn create_monitored_items(
        &self,
        context: &RequestContext,
        items: &mut [&mut CreateMonitoredItem],
    ) -> Result<(), StatusCode> {
        self.inner.create_monitored_items(context, items).await
    }

    async fn modify_monitored_items(
        &self,
        context: &RequestContext,
        items: &[&MonitoredItemUpdateRef],
    ) {
        self.inner.modify_monitored_items(context, items).await;
    }

    async fn set_monitoring_mode(
        &self,
        context: &RequestContext,
        mode: MonitoringMode,
        items: &[&MonitoredItemRef],
    ) {
        self.inner.set_monitoring_mode(context, mode, items).await;
    }

    async fn delete_monitored_items(&self, context: &RequestContext, items: &[&MonitoredItemRef]) {
        self.inner.delete_monitored_items(context, items).await;
    }

    async fn query(
        &self,
        context: &RequestContext,
        request: &mut QueryRequest,
    ) -> Result<(), StatusCode> {
        // `InMemoryNodeManager` uses the trait default which returns
        // `BadServiceUnsupported`; forward explicitly for symmetry.
        self.inner.query(context, request).await
    }

    async fn call(
        &self,
        context: &RequestContext,
        methods_to_call: &mut [&mut MethodCall],
    ) -> Result<(), StatusCode> {
        self.inner.call(context, methods_to_call).await
    }

    async fn add_nodes(
        &self,
        context: &RequestContext,
        nodes_to_add: &mut [&mut AddNodeItem],
    ) -> Result<(), StatusCode> {
        self.inner.add_nodes(context, nodes_to_add).await
    }

    async fn add_references(
        &self,
        context: &RequestContext,
        references_to_add: &mut [&mut AddReferenceItem],
    ) -> Result<(), StatusCode> {
        self.inner.add_references(context, references_to_add).await
    }

    async fn delete_nodes(
        &self,
        context: &RequestContext,
        nodes_to_delete: &mut [&mut DeleteNodeItem],
    ) -> Result<(), StatusCode> {
        self.inner.delete_nodes(context, nodes_to_delete).await
    }

    async fn delete_node_references(
        &self,
        context: &RequestContext,
        to_delete: &[&DeleteNodeItem],
    ) {
        self.inner.delete_node_references(context, to_delete).await;
    }

    async fn delete_references(
        &self,
        context: &RequestContext,
        references_to_delete: &mut [&mut DeleteReferenceItem],
    ) -> Result<(), StatusCode> {
        self.inner
            .delete_references(context, references_to_delete)
            .await
    }
}
