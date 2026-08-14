use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::log_entry::DetailEvent;
use opcuasim_core::node::{MonitoredNode, NodeGroup};
use opcuasim_core::polling::PollingManager;
use opcuasim_core::subscription::SubscriptionManager;
use serde::{Deserialize, Serialize};

/// Per-connection runtime state. Mirrors the legacy egui `ConnectionEntry`.
pub struct ConnectionEntry {
    pub connection: Arc<OpcUaConnection>,
    pub subscription_mgr: SubscriptionManager,
    pub polling_mgr: Arc<PollingManager>,
    /// Subscription-mode nodes to re-create after a reconnect.
    pub pending_subscriptions: Vec<MonitoredNode>,
    /// Polling-mode nodes to restart after a reconnect.
    pub pending_polling: Vec<MonitoredNode>,
}

/// Application state for the OPC UA master client.
///
/// The connections / groups maps use `std::sync::RwLock`, matching the legacy
/// `BackendState`. The managers themselves hold tokio locks internally, so the
/// async command bodies always clone the needed `Arc`/manager out of the map
/// before the first `.await` and never hold a `std::sync` guard across it.
#[derive(Default)]
pub struct AppState {
    pub connections: RwLock<HashMap<String, ConnectionEntry>>,
    pub groups: RwLock<Vec<NodeGroup>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// DTOs for API responses
// ---------------------------------------------------------------------------

/// Certificate role discriminator shared by requests and responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertRoleDto {
    Trusted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub security_policy: String,
    pub security_mode: String,
    pub auth_type: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowseItem {
    pub node_id: String,
    pub display_name: String,
    pub node_class: String,
    pub data_type: Option<String>,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeAttrsDto {
    pub node_id: String,
    pub display_name: String,
    pub description: String,
    pub data_type: String,
    pub access_level: String,
    pub value: Option<String>,
    pub quality: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MonitoredRow {
    pub node_id: String,
    pub display_name: String,
    pub data_type: String,
    pub value: Option<String>,
    pub quality: Option<String>,
    pub source_timestamp: Option<String>,
    pub server_timestamp: Option<String>,
    pub access_mode: String,
    pub interval_ms: f64,
    pub update_seq: u64,
    pub user_access_level: u8,
}

/// Incremental subscription-monitoring snapshot (mirrors 104's
/// `get_received_data_since`). `full` is true on the first poll (`seq == 0`)
/// and false for deltas. `seq` is the new cursor to pass on the next poll.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MonitoredSnapshot {
    pub seq: u64,
    pub full: bool,
    pub nodes: Vec<MonitoredRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DiscoveredEndpointDto {
    pub endpoint_url: String,
    pub security_policy: String,
    pub security_mode: String,
    pub security_level: u8,
    pub server_cert_thumbprint: String,
    pub user_token_policy_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CertSummaryDto {
    pub path: String,
    pub file_name: String,
    pub role: CertRoleDto,
    pub thumbprint: String,
    pub subject_cn: String,
    pub issuer_cn: String,
    pub valid_from: String,
    pub valid_to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeGroupDto {
    pub id: String,
    pub name: String,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogRow {
    pub seq: u64,
    /// UTC milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    /// "Request" | "Response".
    pub direction: String,
    pub service: String,
    pub detail: String,
    pub status: Option<String>,
    /// Structured payload for frontend i18n. Present on connection-lifecycle
    /// entries recorded by this backend; per-request/response entries use
    /// plain `detail` text for now.
    pub detail_event: Option<DetailEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EventItemDto {
    pub time: String,
    pub severity: u16,
    pub source: String,
    pub message: String,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MethodArgInfo {
    pub name: String,
    pub data_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MethodArgValue {
    pub data_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MethodArgsDto {
    pub inputs: Vec<MethodArgInfo>,
    pub outputs: Vec<MethodArgInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MethodCallResultDto {
    pub status: String,
    pub outputs: Vec<MethodArgValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HistoryPointDto {
    pub source_timestamp: String,
    pub server_timestamp: String,
    pub value: String,
    pub numeric: Option<f64>,
    pub status: String,
}

/// Non-fatal subscription result: event subscription reports success/failure
/// through this payload instead of failing the whole command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SubscribeResult {
    pub ok: bool,
    pub detail: Option<String>,
}

/// Payload of the `connection-state` event emitted on connect / disconnect /
/// reconnect transitions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionStateEvent {
    pub id: String,
    pub state: String,
}
