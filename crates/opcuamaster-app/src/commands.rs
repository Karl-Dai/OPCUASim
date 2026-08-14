use std::sync::Arc;

use opcuasim_core::browse;
use opcuasim_core::cert_manager::{self, CertRole};
use opcuasim_core::client::{ConnectionState, OpcUaConnection};
use opcuasim_core::config::{AuthConfig, ConnectionConfig, ConnectionProjectEntry, ProjectFile};
use opcuasim_core::discovery;
use opcuasim_core::events::EventItem as CoreEventItem;
use opcuasim_core::history::{self, HistoryDataPoint};
use opcuasim_core::log_entry::{Direction, LogEntry};
use opcuasim_core::method;
use opcuasim_core::node::{
    AccessMode, BrowseResultItem, DataChangeFilterCfg, DataChangeTriggerKind, DeadbandKind,
    MonitoredNode, NodeGroup,
};
use opcuasim_core::polling::PollingManager;
use opcuasim_core::subscription::SubscriptionManager;
use opcuasim_core::OpcUaSession;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::state::{
    AppState, BrowseItem, CertRoleDto, CertSummaryDto, ConnectionEntry, ConnectionInfo,
    ConnectionStateEvent, DiscoveredEndpointDto, EventItemDto, HistoryPointDto, LogRow,
    MethodArgInfo, MethodArgValue, MethodArgsDto, MethodCallResultDto, MonitoredRow,
    MonitoredSnapshot, NodeAttrsDto, NodeGroupDto, SubscribeResult,
};

/// Health-check command used by the frontend shell to prove the IPC bridge is
/// wired up end to end.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthRequest {
    Anonymous,
    UserPassword { username: String, password: String },
    Certificate { cert_path: String, key_path: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateConnectionRequest {
    pub name: String,
    pub endpoint_url: String,
    pub security_policy: String,
    pub security_mode: String,
    pub auth: AuthRequest,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataChangeTriggerKindReq {
    Status,
    StatusValue,
    StatusValueTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadbandKindReq {
    None,
    Absolute,
    Percent,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DataChangeFilterReq {
    pub trigger: DataChangeTriggerKindReq,
    pub deadband_kind: DeadbandKindReq,
    pub deadband_value: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MonitoredNodeReq {
    pub node_id: String,
    pub display_name: String,
    pub data_type: Option<String>,
    /// "Subscription" | "Polling".
    pub access_mode: String,
    pub interval_ms: f64,
    pub filter: Option<DataChangeFilterReq>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddVariablesUnderNodeRequest {
    pub node_id: String,
    /// "Subscription" | "Polling".
    pub access_mode: String,
    pub interval_ms: f64,
    pub max_depth: u32,
    pub filter: Option<DataChangeFilterReq>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryMode {
    #[default]
    Raw,
    Processed,
    Events,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReadHistoryRequest {
    pub node_id: String,
    pub start_iso: String,
    pub end_iso: String,
    pub max_values: u32,
    pub mode: HistoryMode,
    pub agg_type: Option<String>,
    pub processing_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CallMethodRequest {
    pub object_id: String,
    pub method_id: String,
    pub inputs: Vec<MethodArgValue>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn auth_from_req(auth: AuthRequest) -> AuthConfig {
    match auth {
        AuthRequest::Anonymous => AuthConfig::Anonymous,
        AuthRequest::UserPassword { username, password } => {
            AuthConfig::UserPassword { username, password }
        }
        AuthRequest::Certificate {
            cert_path,
            key_path,
        } => AuthConfig::Certificate {
            cert_path,
            key_path,
        },
    }
}

fn auth_label(a: &AuthConfig) -> &'static str {
    match a {
        AuthConfig::Anonymous => "Anonymous",
        AuthConfig::UserPassword { .. } => "UserPassword",
        AuthConfig::Certificate { .. } => "Certificate",
    }
}

fn filter_req_to_core(req: &DataChangeFilterReq) -> DataChangeFilterCfg {
    DataChangeFilterCfg {
        trigger: match req.trigger {
            DataChangeTriggerKindReq::Status => DataChangeTriggerKind::Status,
            DataChangeTriggerKindReq::StatusValue => DataChangeTriggerKind::StatusValue,
            DataChangeTriggerKindReq::StatusValueTimestamp => {
                DataChangeTriggerKind::StatusValueTimestamp
            }
        },
        deadband_kind: match req.deadband_kind {
            DeadbandKindReq::None => DeadbandKind::None,
            DeadbandKindReq::Absolute => DeadbandKind::Absolute,
            DeadbandKindReq::Percent => DeadbandKind::Percent,
        },
        deadband_value: req.deadband_value,
    }
}

fn nodes_to_monitored(nodes: Vec<MonitoredNodeReq>) -> Vec<MonitoredNode> {
    nodes
        .into_iter()
        .map(|n| {
            let access_mode = match n.access_mode.as_str() {
                "Polling" => AccessMode::Polling {
                    interval_ms: n.interval_ms as u64,
                },
                _ => AccessMode::Subscription {
                    interval_ms: n.interval_ms,
                },
            };
            MonitoredNode {
                node_id: n.node_id,
                display_name: n.display_name,
                browse_path: String::new(),
                data_type: n.data_type.unwrap_or_else(|| "Unknown".to_string()),
                value: None,
                quality: None,
                timestamp: None,
                server_timestamp: None,
                access_mode,
                group_id: None,
                update_seq: 0,
                user_access_level: 0,
                filter: n.filter.as_ref().map(filter_req_to_core),
            }
        })
        .collect()
}

fn monitored_node_to_row(n: MonitoredNode) -> MonitoredRow {
    let (access_mode, interval_ms) = match &n.access_mode {
        AccessMode::Subscription { interval_ms } => ("Subscription".to_string(), *interval_ms),
        AccessMode::Polling { interval_ms } => ("Polling".to_string(), *interval_ms as f64),
    };
    MonitoredRow {
        node_id: n.node_id,
        display_name: n.display_name,
        data_type: n.data_type,
        value: n.value,
        quality: n.quality,
        source_timestamp: n.timestamp,
        server_timestamp: n.server_timestamp,
        access_mode,
        interval_ms,
        update_seq: n.update_seq,
        user_access_level: n.user_access_level,
    }
}

fn browse_item_to_dto(item: BrowseResultItem) -> BrowseItem {
    BrowseItem {
        node_id: item.node_id,
        display_name: item.display_name,
        node_class: item.node_class,
        data_type: item.data_type,
        has_children: item.has_children,
    }
}

fn history_datapoint_to_dto(p: HistoryDataPoint) -> HistoryPointDto {
    HistoryPointDto {
        source_timestamp: p.source_timestamp,
        server_timestamp: p.server_timestamp,
        value: p.value,
        numeric: p.numeric,
        status: p.status,
    }
}

fn event_history_point_to_dto(ep: opcuasim_core::history::EventHistoryPoint) -> HistoryPointDto {
    HistoryPointDto {
        source_timestamp: ep.time,
        server_timestamp: String::new(),
        value: ep.fields.get(6).cloned().unwrap_or_default(),
        numeric: None,
        status: ep.fields.get(7).cloned().unwrap_or_default(),
    }
}

fn arg_info_to_dto(a: opcuasim_core::method::ArgumentInfo) -> MethodArgInfo {
    MethodArgInfo {
        name: a.name,
        data_type: a.data_type,
        description: a.description,
    }
}

fn event_item_to_dto(e: CoreEventItem) -> EventItemDto {
    EventItemDto {
        time: e.time,
        severity: e.severity,
        source: e.source,
        message: e.message,
        event_type: e.event_type,
    }
}

fn role_to_core(r: CertRoleDto) -> CertRole {
    match r {
        CertRoleDto::Trusted => CertRole::Trusted,
        CertRoleDto::Rejected => CertRole::Rejected,
    }
}

fn role_to_dto(r: CertRole) -> CertRoleDto {
    match r {
        CertRole::Trusted => CertRoleDto::Trusted,
        CertRole::Rejected => CertRoleDto::Rejected,
    }
}

fn cert_summary_to_dto(c: opcuasim_core::cert_manager::CertSummary) -> CertSummaryDto {
    CertSummaryDto {
        path: c.path.to_string_lossy().into_owned(),
        file_name: c.file_name,
        role: role_to_dto(c.role),
        thumbprint: c.thumbprint,
        subject_cn: c.subject_cn,
        issuer_cn: c.issuer_cn,
        valid_from: c.valid_from,
        valid_to: c.valid_to,
    }
}

fn group_to_dto(g: &NodeGroup) -> NodeGroupDto {
    NodeGroupDto {
        id: g.id.clone(),
        name: g.name.clone(),
        node_ids: g.node_ids.clone(),
    }
}

fn log_entry_to_row(e: LogEntry) -> LogRow {
    LogRow {
        seq: e.seq,
        timestamp_ms: e.timestamp.timestamp_millis(),
        direction: e.direction.to_string(),
        service: e.service,
        detail: e.detail,
        status: e.status,
        detail_event: e.detail_event,
    }
}

fn list_groups_impl(state: &AppState) -> Result<Vec<NodeGroupDto>, String> {
    let groups = state.groups.read().map_err(|e| e.to_string())?;
    Ok(groups.iter().map(group_to_dto).collect())
}

/// Record a structured connection-lifecycle entry on the connection's own log
/// collector so the frontend can localize it via `detail_event.kind`.
fn log_lifecycle(
    connection: &OpcUaConnection,
    direction: Direction,
    kind: &str,
    payload: JsonValue,
    detail: &str,
) {
    let seq = connection.log_collector.next_seq();
    connection.log_collector.add(
        LogEntry::new(
            seq,
            connection.config.id.clone(),
            direction,
            "Session".to_string(),
            detail.to_string(),
            None,
        )
        .with_detail_event(kind, payload),
    );
}

/// Clone a live session out of the connection's session holder. The tokio
/// guard is dropped before the returned `Arc` is used across `.await` points.
async fn get_session(state: &AppState, conn_id: &str) -> Result<Arc<OpcUaSession>, String> {
    let holder = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(conn_id).ok_or("Connection not found")?;
        entry.connection.get_session_holder()
    };
    let guard = holder.read().await;
    guard
        .clone()
        .ok_or_else(|| "Not connected — no active session".to_string())
}

/// Store pending subscription/polling nodes, then start the subscription
/// monitored items and polling tasks against the live session.
async fn add_monitored_core(
    state: &AppState,
    conn_id: &str,
    nodes: Vec<MonitoredNode>,
) -> Result<(), String> {
    let (sub_nodes, poll_nodes): (Vec<MonitoredNode>, Vec<MonitoredNode>) = nodes
        .into_iter()
        .partition(|n| matches!(n.access_mode, AccessMode::Subscription { .. }));

    {
        let mut conns = state.connections.write().map_err(|e| e.to_string())?;
        let entry = conns.get_mut(conn_id).ok_or("Connection not found")?;
        entry.pending_subscriptions.extend(sub_nodes.clone());
        entry.pending_polling.extend(poll_nodes.clone());
    }

    let (sub_mgr, poll_mgr, session_holder) = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(conn_id).ok_or("Connection not found")?;
        (
            entry.subscription_mgr.clone(),
            entry.polling_mgr.clone(),
            entry.connection.get_session_holder(),
        )
    };

    let session = {
        let guard = session_holder.read().await;
        guard.clone()
    };
    if !sub_nodes.is_empty() {
        sub_mgr
            .add_nodes(sub_nodes, session.as_ref())
            .await
            .map_err(|e| e.to_string())?;
    }
    for node in poll_nodes {
        let interval_ms = match node.access_mode {
            AccessMode::Polling { interval_ms } => interval_ms,
            AccessMode::Subscription { .. } => 1000,
        };
        poll_mgr
            .add_polling_node(node, interval_ms)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Re-create subscription monitored items and restart polling tasks after a
/// (re)connect. Idempotent: remove_nodes clears stale local tracking first,
/// then add_nodes re-inserts; add_polling_node aborts+replaces existing tasks.
async fn restore_monitoring(conn_id: &str, state: &AppState) {
    let (sub_nodes, poll_nodes, sub_mgr, poll_mgr, session_holder) = {
        let conns = match state.connections.read() {
            Ok(c) => c,
            Err(_) => return,
        };
        let Some(entry) = conns.get(conn_id) else {
            return;
        };
        (
            entry.pending_subscriptions.clone(),
            entry.pending_polling.clone(),
            entry.subscription_mgr.clone(),
            entry.polling_mgr.clone(),
            entry.connection.get_session_holder(),
        )
    };
    if sub_nodes.is_empty() && poll_nodes.is_empty() {
        return;
    }
    log::info!(
        "Restoring {} subscription + {} polling nodes for {}",
        sub_nodes.len(),
        poll_nodes.len(),
        conn_id
    );

    let ids: Vec<String> = sub_nodes.iter().map(|n| n.node_id.clone()).collect();
    let _ = sub_mgr.remove_nodes(&ids).await;

    let session = {
        let guard = session_holder.read().await;
        guard.clone()
    };
    if !sub_nodes.is_empty() {
        if let Err(e) = sub_mgr.add_nodes(sub_nodes, session.as_ref()).await {
            log::warn!("Restore subscriptions failed for {}: {}", conn_id, e);
        }
    }

    for node in poll_nodes {
        let interval_ms = match node.access_mode {
            AccessMode::Polling { interval_ms } => interval_ms,
            AccessMode::Subscription { .. } => 1000,
        };
        if let Err(e) = poll_mgr.add_polling_node(node, interval_ms).await {
            log::warn!("Restore polling failed for {}: {}", conn_id, e);
        }
    }
}

fn string_to_variant(data_type: &str, value: &str) -> Result<opcua_types::Variant, String> {
    use opcua_types::Variant;
    match data_type {
        "Boolean" => value
            .parse::<bool>()
            .map(Variant::Boolean)
            .map_err(|e| e.to_string()),
        "SByte" => value
            .parse::<i8>()
            .map(Variant::SByte)
            .map_err(|e| e.to_string()),
        "Byte" => value
            .parse::<u8>()
            .map(Variant::Byte)
            .map_err(|e| e.to_string()),
        "Int16" => value
            .parse::<i16>()
            .map(Variant::Int16)
            .map_err(|e| e.to_string()),
        "UInt16" => value
            .parse::<u16>()
            .map(Variant::UInt16)
            .map_err(|e| e.to_string()),
        "Int32" => value
            .parse::<i32>()
            .map(Variant::Int32)
            .map_err(|e| e.to_string()),
        "UInt32" => value
            .parse::<u32>()
            .map(Variant::UInt32)
            .map_err(|e| e.to_string()),
        "Int64" => value
            .parse::<i64>()
            .map(Variant::Int64)
            .map_err(|e| e.to_string()),
        "UInt64" => value
            .parse::<u64>()
            .map(Variant::UInt64)
            .map_err(|e| e.to_string()),
        "Float" => value
            .parse::<f32>()
            .map(Variant::Float)
            .map_err(|e| e.to_string()),
        "Double" => value
            .parse::<f64>()
            .map(Variant::Double)
            .map_err(|e| e.to_string()),
        "String" => Ok(Variant::String(value.into())),
        other => Err(format!("unsupported method arg type: {other}")),
    }
}

fn variant_type_label(v: &opcua_types::Variant) -> String {
    use opcua_types::variant::VariantTypeId;
    match v.type_id() {
        VariantTypeId::Empty => "Empty".to_string(),
        VariantTypeId::Scalar(s) => format!("{s}"),
        VariantTypeId::Array(s, _) => format!("Array<{s}>"),
    }
}

fn agg_name_to_node_id(name: &str) -> Option<opcua_types::NodeId> {
    use opcua_types::ObjectId as O;
    let id = match name {
        "平均" => O::AggregateFunction_Average,
        "最小" => O::AggregateFunction_Minimum,
        "最大" => O::AggregateFunction_Maximum,
        "计数" => O::AggregateFunction_Count,
        "TimeAvg" => O::AggregateFunction_TimeAverage,
        "总计" => O::AggregateFunction_Total,
        "Delta" => O::AggregateFunction_Delta,
        "PercentGood" => O::AggregateFunction_PercentGood,
        _ => return None,
    };
    Some(id.into())
}

fn parse_iso_to_datetime(s: &str) -> Result<opcua_types::DateTime, String> {
    let parsed = chrono::DateTime::parse_from_rfc3339(s.trim())
        .map_err(|e| format!("invalid time '{s}': {e}"))?;
    let utc: chrono::DateTime<chrono::Utc> = parsed.with_timezone(&chrono::Utc);
    Ok(opcua_types::DateTime::from(utc))
}

// ---------------------------------------------------------------------------
// Connection commands
// ---------------------------------------------------------------------------

pub(crate) fn create_connection_impl(
    state: &AppState,
    request: CreateConnectionRequest,
) -> Result<ConnectionInfo, String> {
    let id = Uuid::new_v4().to_string();
    let config = ConnectionConfig {
        id: id.clone(),
        name: request.name,
        endpoint_url: request.endpoint_url,
        security_policy: request.security_policy,
        security_mode: request.security_mode,
        auth: auth_from_req(request.auth),
        timeout_ms: request.timeout_ms,
    };
    let connection = Arc::new(OpcUaConnection::new(config.clone()));
    let session_holder = connection.get_session_holder();

    {
        let mut conns = state.connections.write().map_err(|e| e.to_string())?;
        conns.insert(
            id.clone(),
            ConnectionEntry {
                connection,
                subscription_mgr: SubscriptionManager::new(),
                polling_mgr: Arc::new(PollingManager::new(session_holder)),
                pending_subscriptions: Vec::new(),
                pending_polling: Vec::new(),
            },
        );
    }

    Ok(ConnectionInfo {
        id,
        name: config.name,
        endpoint_url: config.endpoint_url,
        security_policy: config.security_policy,
        security_mode: config.security_mode,
        auth_type: auth_label(&config.auth).to_string(),
        state: "Disconnected".to_string(),
    })
}

#[tauri::command]
pub fn create_connection(
    state: State<'_, AppState>,
    request: CreateConnectionRequest,
) -> Result<ConnectionInfo, String> {
    create_connection_impl(state.inner(), request)
}

pub(crate) async fn connect_impl(
    state: &AppState,
    connection_id: &str,
    emit: impl Fn(&str),
) -> Result<(), String> {
    let conn_arc = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(connection_id)
            .ok_or("Connection not found")?
            .connection
            .clone()
    };

    *conn_arc.state.write().await = ConnectionState::Connecting;
    emit("Connecting");
    log_lifecycle(
        conn_arc.as_ref(),
        Direction::Request,
        "connection.connecting",
        json!({ "endpoint_url": conn_arc.config.endpoint_url.clone() }),
        "Connecting",
    );

    match conn_arc.connect().await {
        Ok(()) => {
            *conn_arc.state.write().await = ConnectionState::Connected;
            emit("Connected");
            log_lifecycle(
                conn_arc.as_ref(),
                Direction::Response,
                "connection.connected",
                json!({ "endpoint_url": conn_arc.config.endpoint_url.clone() }),
                "Connected",
            );
            Ok(())
        }
        Err(e) => {
            *conn_arc.state.write().await = ConnectionState::Disconnected;
            emit("Disconnected");
            log_lifecycle(
                conn_arc.as_ref(),
                Direction::Response,
                "connection.disconnected",
                json!({ "endpoint_url": conn_arc.config.endpoint_url.clone() }),
                "Disconnected",
            );
            Err(format!("Connection failed: {e}"))
        }
    }
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    connection_id: String,
) -> Result<(), String> {
    let app = app_handle.clone();
    let cid = connection_id.clone();
    let emit = move |s: &str| {
        let _ = app.emit(
            "connection-state",
            ConnectionStateEvent {
                id: cid.clone(),
                state: s.to_string(),
            },
        );
    };

    connect_impl(state.inner(), &connection_id, emit).await?;

    let conn_arc = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .connection
            .clone()
    };
    let sub_mgr = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .subscription_mgr
            .clone()
    };

    let on_state_change = {
        let app = app_handle.clone();
        let cid = connection_id.clone();
        let mgr = sub_mgr.clone();
        let conn = conn_arc.clone();
        move |s: ConnectionState| {
            let _ = app.emit(
                "connection-state",
                ConnectionStateEvent {
                    id: cid.clone(),
                    state: s.to_string(),
                },
            );
            match s {
                ConnectionState::Connected => {
                    log_lifecycle(
                        conn.as_ref(),
                        Direction::Response,
                        "connection.connected",
                        json!({ "endpoint_url": conn.config.endpoint_url.clone() }),
                        "Connected",
                    );
                    let app2 = app.clone();
                    let cid2 = cid.clone();
                    tokio::spawn(async move {
                        let st: State<'_, AppState> = app2.state();
                        restore_monitoring(&cid2, st.inner()).await;
                    });
                }
                ConnectionState::Reconnecting => {
                    log_lifecycle(
                        conn.as_ref(),
                        Direction::Request,
                        "connection.reconnecting",
                        json!({ "endpoint_url": conn.config.endpoint_url.clone() }),
                        "Reconnecting",
                    );
                    let mgr2 = mgr.clone();
                    tokio::spawn(async move {
                        mgr2.on_disconnect().await;
                    });
                }
                _ => {}
            }
        }
    };

    let conn_for_loop = conn_arc.clone();
    tokio::spawn(async move {
        conn_for_loop.start_reconnect_loop(on_state_change).await;
    });

    restore_monitoring(&connection_id, state.inner()).await;
    Ok(())
}

pub(crate) async fn disconnect_impl(state: &AppState, connection_id: &str) -> Result<(), String> {
    let conn_arc = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(connection_id)
            .ok_or("Connection not found")?
            .connection
            .clone()
    };

    let _ = conn_arc.disconnect().await;
    *conn_arc.state.write().await = ConnectionState::Disconnected;
    let sub_mgr = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(connection_id)
            .ok_or("Connection not found")?
            .subscription_mgr
            .clone()
    };
    sub_mgr.on_disconnect().await;

    log_lifecycle(
        conn_arc.as_ref(),
        Direction::Response,
        "connection.disconnected",
        json!({ "endpoint_url": conn_arc.config.endpoint_url.clone() }),
        "Disconnected",
    );
    Ok(())
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    connection_id: String,
) -> Result<(), String> {
    disconnect_impl(state.inner(), &connection_id).await?;
    let _ = app_handle.emit(
        "connection-state",
        ConnectionStateEvent {
            id: connection_id,
            state: "Disconnected".to_string(),
        },
    );
    Ok(())
}

pub(crate) async fn delete_connection_impl(
    state: &AppState,
    connection_id: &str,
) -> Result<(), String> {
    let (conn_arc, poll_mgr) = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(connection_id).ok_or("Connection not found")?;
        (entry.connection.clone(), entry.polling_mgr.clone())
    };

    // Best-effort teardown: disconnect() cancels the reconnect loop and clears
    // the session, so the detached reconnect/polling tasks cannot keep the
    // connection alive after the map entry is removed.
    let _ = conn_arc.disconnect().await;
    poll_mgr.stop_all().await;

    {
        let mut conns = state.connections.write().map_err(|e| e.to_string())?;
        conns.remove(connection_id).ok_or("Connection not found")?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    delete_connection_impl(state.inner(), &connection_id).await
}

pub(crate) async fn list_connections_impl(state: &AppState) -> Result<Vec<ConnectionInfo>, String> {
    let snapshot: Vec<(
        String,
        ConnectionConfig,
        Arc<tokio::sync::RwLock<ConnectionState>>,
    )> = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .iter()
            .map(|(id, entry)| {
                (
                    id.clone(),
                    entry.connection.config.clone(),
                    entry.connection.state.clone(),
                )
            })
            .collect()
    };

    let mut infos = Vec::with_capacity(snapshot.len());
    for (id, config, state_arc) in snapshot {
        let st = state_arc.read().await.clone();
        infos.push(ConnectionInfo {
            id,
            name: config.name,
            endpoint_url: config.endpoint_url,
            security_policy: config.security_policy,
            security_mode: config.security_mode,
            auth_type: auth_label(&config.auth).to_string(),
            state: st.to_string(),
        });
    }
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(infos)
}

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionInfo>, String> {
    list_connections_impl(state.inner()).await
}

// ---------------------------------------------------------------------------
// Browse / discovery / read / write
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn discover_endpoints(
    url: String,
    timeout_ms: u64,
) -> Result<Vec<DiscoveredEndpointDto>, String> {
    let list = discovery::discover_endpoints(&url, timeout_ms)
        .await
        .map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|e| DiscoveredEndpointDto {
            endpoint_url: e.endpoint_url,
            security_policy: e.security_policy,
            security_mode: e.security_mode,
            security_level: e.security_level,
            server_cert_thumbprint: e.server_cert_thumbprint,
            user_token_policy_ids: e
                .user_token_policies
                .into_iter()
                .map(|t| t.policy_id)
                .collect(),
        })
        .collect())
}

pub(crate) async fn browse_root_impl(
    state: &AppState,
    connection_id: &str,
) -> Result<Vec<BrowseItem>, String> {
    let session = get_session(state, connection_id).await?;
    let items = browse::browse_node(&session, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(items.into_iter().map(browse_item_to_dto).collect())
}

#[tauri::command]
pub async fn browse_root(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<BrowseItem>, String> {
    browse_root_impl(state.inner(), &connection_id).await
}

pub(crate) async fn browse_node_impl(
    state: &AppState,
    connection_id: &str,
    node_id: &str,
) -> Result<Vec<BrowseItem>, String> {
    let session = get_session(state, connection_id).await?;
    let items = browse::browse_node(&session, Some(node_id))
        .await
        .map_err(|e| e.to_string())?;
    Ok(items.into_iter().map(browse_item_to_dto).collect())
}

#[tauri::command]
pub async fn browse_node(
    state: State<'_, AppState>,
    connection_id: String,
    node_id: String,
) -> Result<Vec<BrowseItem>, String> {
    browse_node_impl(state.inner(), &connection_id, &node_id).await
}

#[tauri::command]
pub async fn collect_variables(
    state: State<'_, AppState>,
    connection_id: String,
    node_id: String,
    max_depth: u32,
) -> Result<Vec<BrowseItem>, String> {
    let session = get_session(state.inner(), &connection_id).await?;
    let variables = browse::collect_variables(&session, &node_id, max_depth)
        .await
        .map_err(|e| e.to_string())?;
    Ok(variables.into_iter().map(browse_item_to_dto).collect())
}

pub(crate) async fn read_attributes_impl(
    state: &AppState,
    connection_id: &str,
    node_id: &str,
) -> Result<NodeAttrsDto, String> {
    let session = get_session(state, connection_id).await?;
    let attrs = browse::read_node_attributes(&session, node_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(NodeAttrsDto {
        node_id: attrs.node_id,
        display_name: attrs.display_name,
        description: attrs.description,
        data_type: attrs.data_type,
        access_level: attrs.access_level,
        value: attrs.value,
        quality: attrs.quality,
        timestamp: attrs.timestamp,
    })
}

#[tauri::command]
pub async fn read_attributes(
    state: State<'_, AppState>,
    connection_id: String,
    node_id: String,
) -> Result<NodeAttrsDto, String> {
    read_attributes_impl(state.inner(), &connection_id, &node_id).await
}

pub(crate) async fn write_value_impl(
    state: &AppState,
    connection_id: &str,
    node_id: &str,
    value: &str,
    data_type: &str,
) -> Result<(), String> {
    let session = get_session(state, connection_id).await?;
    browse::write_node_value(&session, node_id, value, data_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn write_value(
    state: State<'_, AppState>,
    connection_id: String,
    node_id: String,
    value: String,
    data_type: String,
) -> Result<(), String> {
    write_value_impl(state.inner(), &connection_id, &node_id, &value, &data_type).await
}

// ---------------------------------------------------------------------------
// Monitoring
// ---------------------------------------------------------------------------

pub(crate) async fn add_monitored_nodes_impl(
    state: &AppState,
    connection_id: &str,
    nodes: Vec<MonitoredNodeReq>,
) -> Result<(), String> {
    add_monitored_core(state, connection_id, nodes_to_monitored(nodes)).await
}

#[tauri::command]
pub async fn add_monitored_nodes(
    state: State<'_, AppState>,
    connection_id: String,
    nodes: Vec<MonitoredNodeReq>,
) -> Result<(), String> {
    add_monitored_nodes_impl(state.inner(), &connection_id, nodes).await
}

#[tauri::command]
pub async fn add_variables_under_node(
    state: State<'_, AppState>,
    connection_id: String,
    request: AddVariablesUnderNodeRequest,
) -> Result<(), String> {
    let session = get_session(state.inner(), &connection_id).await?;
    let variables = browse::collect_variables(&session, &request.node_id, request.max_depth)
        .await
        .map_err(|e| e.to_string())?;
    if variables.is_empty() {
        return Ok(());
    }

    let mode = match request.access_mode.as_str() {
        "Polling" => AccessMode::Polling {
            interval_ms: request.interval_ms as u64,
        },
        _ => AccessMode::Subscription {
            interval_ms: request.interval_ms,
        },
    };
    let core_filter = request.filter.as_ref().map(filter_req_to_core);
    let nodes: Vec<MonitoredNode> = variables
        .into_iter()
        .map(|v| MonitoredNode {
            node_id: v.node_id,
            display_name: v.display_name,
            browse_path: String::new(),
            data_type: v.data_type.unwrap_or_else(|| "Unknown".to_string()),
            value: None,
            quality: None,
            timestamp: None,
            server_timestamp: None,
            access_mode: mode.clone(),
            group_id: None,
            update_seq: 0,
            user_access_level: 0,
            filter: core_filter,
        })
        .collect();

    add_monitored_core(state.inner(), &connection_id, nodes).await
}

#[tauri::command]
pub async fn remove_monitored_nodes(
    state: State<'_, AppState>,
    connection_id: String,
    node_ids: Vec<String>,
) -> Result<(), String> {
    let (sub_mgr, poll_mgr) = {
        let mut conns = state.connections.write().map_err(|e| e.to_string())?;
        let entry = conns
            .get_mut(&connection_id)
            .ok_or("Connection not found")?;
        entry
            .pending_subscriptions
            .retain(|n| !node_ids.contains(&n.node_id));
        entry
            .pending_polling
            .retain(|n| !node_ids.contains(&n.node_id));
        (entry.subscription_mgr.clone(), entry.polling_mgr.clone())
    };

    sub_mgr
        .remove_nodes(&node_ids)
        .await
        .map_err(|e| e.to_string())?;
    for node_id in &node_ids {
        poll_mgr.remove_polling_node(node_id).await;
    }
    Ok(())
}

pub(crate) async fn get_monitored_nodes_since_impl(
    state: &AppState,
    connection_id: &str,
    seq: u64,
) -> Result<MonitoredSnapshot, String> {
    let sub_mgr = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(connection_id)
            .ok_or("Connection not found")?
            .subscription_mgr
            .clone()
    };

    let current_seq = sub_mgr.get_update_seq().await;
    let (nodes, full) = if seq == 0 {
        (sub_mgr.get_monitored_nodes().await, true)
    } else {
        (sub_mgr.get_monitored_nodes_since(seq).await, false)
    };
    Ok(MonitoredSnapshot {
        seq: current_seq,
        full,
        nodes: nodes.into_iter().map(monitored_node_to_row).collect(),
    })
}

#[tauri::command]
pub async fn get_monitored_nodes_since(
    state: State<'_, AppState>,
    connection_id: String,
    seq: u64,
) -> Result<MonitoredSnapshot, String> {
    get_monitored_nodes_since_impl(state.inner(), &connection_id, seq).await
}

/// Full snapshot of polling-mode nodes. Polling nodes are stored in a separate
/// `PollingManager` with per-node (not global) update sequences, so they are
/// exposed as a full snapshot rather than through the incremental subscription
/// cursor.
#[tauri::command]
pub async fn get_polling_nodes(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<MonitoredRow>, String> {
    let poll_mgr = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .polling_mgr
            .clone()
    };
    Ok(poll_mgr
        .get_polling_nodes()
        .await
        .into_iter()
        .map(monitored_node_to_row)
        .collect())
}

// ---------------------------------------------------------------------------
// History / events
// ---------------------------------------------------------------------------

pub(crate) async fn read_history_impl(
    state: &AppState,
    connection_id: &str,
    request: ReadHistoryRequest,
) -> Result<Vec<HistoryPointDto>, String> {
    let session = get_session(state, connection_id).await?;
    let nid: opcua_types::NodeId = request
        .node_id
        .parse()
        .map_err(|_| format!("invalid node id: {}", request.node_id))?;
    let start = parse_iso_to_datetime(&request.start_iso)?;
    let end = parse_iso_to_datetime(&request.end_iso)?;

    let points: Vec<HistoryPointDto> = match request.mode {
        HistoryMode::Raw => {
            history::history_read_raw(&session, &nid, start, end, request.max_values, true)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(history_datapoint_to_dto)
                .collect()
        }
        HistoryMode::Processed => {
            let agg_nid = request
                .agg_type
                .as_deref()
                .and_then(agg_name_to_node_id)
                .ok_or_else(|| {
                    format!(
                        "不支持的聚合函数: {}",
                        request.agg_type.as_deref().unwrap_or("(空)")
                    )
                })?;
            let interval = request.processing_interval_ms.unwrap_or(2000);
            history::history_read_processed(
                &session,
                &nid,
                start,
                end,
                interval,
                agg_nid,
                request.max_values,
            )
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(history_datapoint_to_dto)
            .collect()
        }
        HistoryMode::Events => history::history_read_events(
            &session,
            &nid,
            start,
            end,
            request.max_values,
            opcua_types::EventFilter::default(),
        )
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(event_history_point_to_dto)
        .collect(),
    };

    Ok(points)
}

#[tauri::command]
pub async fn read_history(
    state: State<'_, AppState>,
    connection_id: String,
    request: ReadHistoryRequest,
) -> Result<Vec<HistoryPointDto>, String> {
    read_history_impl(state.inner(), &connection_id, request).await
}

#[tauri::command]
pub async fn subscribe_events(
    state: State<'_, AppState>,
    connection_id: String,
    source_node_id: String,
) -> Result<SubscribeResult, String> {
    let (sub_mgr, session_holder) = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(&connection_id).ok_or("Connection not found")?;
        (
            entry.subscription_mgr.clone(),
            entry.connection.get_session_holder(),
        )
    };

    let nid: opcua_types::NodeId = match source_node_id.parse() {
        Ok(nid) => nid,
        Err(_) => {
            return Ok(SubscribeResult {
                ok: false,
                detail: Some(format!("invalid source node id: {source_node_id}")),
            });
        }
    };

    let session = {
        let guard = session_holder.read().await;
        guard.clone()
    };
    let Some(session) = session else {
        return Ok(SubscribeResult {
            ok: false,
            detail: Some("Not connected — no active session".to_string()),
        });
    };

    match sub_mgr.subscribe_to_events(&session, &nid).await {
        Ok(()) => Ok(SubscribeResult {
            ok: true,
            detail: None,
        }),
        Err(e) => Ok(SubscribeResult {
            ok: false,
            detail: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn unsubscribe_events(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let (sub_mgr, session_holder) = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(&connection_id).ok_or("Connection not found")?;
        (
            entry.subscription_mgr.clone(),
            entry.connection.get_session_holder(),
        )
    };

    let session = {
        let guard = session_holder.read().await;
        guard.clone()
    };
    sub_mgr
        .unsubscribe_events(session.as_ref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_events(state: State<'_, AppState>, connection_id: String) -> Result<(), String> {
    let sub_mgr = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .subscription_mgr
            .clone()
    };
    sub_mgr.clear_events().await;
    Ok(())
}

#[tauri::command]
pub async fn get_events(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<EventItemDto>, String> {
    let sub_mgr = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .subscription_mgr
            .clone()
    };
    Ok(sub_mgr
        .get_events()
        .await
        .into_iter()
        .map(event_item_to_dto)
        .collect())
}

// ---------------------------------------------------------------------------
// Methods
// ---------------------------------------------------------------------------

pub(crate) async fn read_method_arguments_impl(
    state: &AppState,
    connection_id: &str,
    method_id: &str,
) -> Result<MethodArgsDto, String> {
    let session = get_session(state, connection_id).await?;
    let nid: opcua_types::NodeId = method_id
        .parse()
        .map_err(|_| format!("invalid node id: {method_id}"))?;
    let info = method::read_method_arguments(&session, &nid)
        .await
        .map_err(|e| e.to_string())?;
    Ok(MethodArgsDto {
        inputs: info.inputs.into_iter().map(arg_info_to_dto).collect(),
        outputs: info.outputs.into_iter().map(arg_info_to_dto).collect(),
    })
}

#[tauri::command]
pub async fn read_method_arguments(
    state: State<'_, AppState>,
    connection_id: String,
    method_id: String,
) -> Result<MethodArgsDto, String> {
    read_method_arguments_impl(state.inner(), &connection_id, &method_id).await
}

pub(crate) async fn call_method_impl(
    state: &AppState,
    connection_id: &str,
    request: CallMethodRequest,
) -> Result<MethodCallResultDto, String> {
    let session = get_session(state, connection_id).await?;
    let oid: opcua_types::NodeId = request
        .object_id
        .parse()
        .map_err(|_| format!("invalid object id: {}", request.object_id))?;
    let mid: opcua_types::NodeId = request
        .method_id
        .parse()
        .map_err(|_| format!("invalid method id: {}", request.method_id))?;

    let variants: Vec<opcua_types::Variant> = request
        .inputs
        .iter()
        .map(|a| string_to_variant(&a.data_type, &a.value))
        .collect::<Result<_, String>>()?;

    let outcome = method::call_method(&session, &oid, &mid, variants)
        .await
        .map_err(|e| e.to_string())?;

    let outputs: Vec<MethodArgValue> = outcome
        .outputs
        .into_iter()
        .map(|v| MethodArgValue {
            data_type: variant_type_label(&v),
            value: format!("{v}"),
        })
        .collect();

    Ok(MethodCallResultDto {
        status: format!("{}", outcome.status),
        outputs,
    })
}

#[tauri::command]
pub async fn call_method(
    state: State<'_, AppState>,
    connection_id: String,
    request: CallMethodRequest,
) -> Result<MethodCallResultDto, String> {
    call_method_impl(state.inner(), &connection_id, request).await
}

// ---------------------------------------------------------------------------
// Certificates
// ---------------------------------------------------------------------------

const PKI_DIR: &str = "./pki";

#[tauri::command]
pub fn list_certificates(role: CertRoleDto) -> Result<Vec<CertSummaryDto>, String> {
    let pki = std::path::Path::new(PKI_DIR);
    let list =
        cert_manager::list_certificates(pki, role_to_core(role)).map_err(|e| e.to_string())?;
    Ok(list.into_iter().map(cert_summary_to_dto).collect())
}

#[tauri::command]
pub fn move_certificate(path: String, to_role: CertRoleDto) -> Result<(), String> {
    let pki = std::path::Path::new(PKI_DIR);
    cert_manager::move_certificate(pki, std::path::Path::new(&path), role_to_core(to_role))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_certificate(path: String) -> Result<(), String> {
    cert_manager::delete_certificate(std::path::Path::new(&path)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_group(state: State<'_, AppState>, name: String) -> Result<Vec<NodeGroupDto>, String> {
    let id = Uuid::new_v4().to_string();
    {
        let mut groups = state.groups.write().map_err(|e| e.to_string())?;
        groups.push(NodeGroup {
            id,
            name,
            node_ids: Vec::new(),
        });
    }
    list_groups_impl(state.inner())
}

#[tauri::command]
pub fn delete_group(state: State<'_, AppState>, id: String) -> Result<Vec<NodeGroupDto>, String> {
    {
        let mut groups = state.groups.write().map_err(|e| e.to_string())?;
        groups.retain(|g| g.id != id);
    }
    list_groups_impl(state.inner())
}

#[tauri::command]
pub fn add_to_group(
    state: State<'_, AppState>,
    group_id: String,
    node_ids: Vec<String>,
) -> Result<Vec<NodeGroupDto>, String> {
    {
        let mut groups = state.groups.write().map_err(|e| e.to_string())?;
        let group = groups
            .iter_mut()
            .find(|g| g.id == group_id)
            .ok_or("Group not found")?;
        for node_id in node_ids {
            if !group.node_ids.contains(&node_id) {
                group.node_ids.push(node_id);
            }
        }
    }
    list_groups_impl(state.inner())
}

#[tauri::command]
pub fn list_groups(state: State<'_, AppState>) -> Result<Vec<NodeGroupDto>, String> {
    list_groups_impl(state.inner())
}

// ---------------------------------------------------------------------------
// Project
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let (conn_entries, groups_snapshot) = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let groups = state.groups.read().map_err(|e| e.to_string())?;
        let conn_data: Vec<ConnectionProjectEntry> = conns
            .values()
            .map(|entry| {
                let c = &entry.connection.config;
                ConnectionProjectEntry {
                    name: c.name.clone(),
                    endpoint_url: c.endpoint_url.clone(),
                    security_policy: c.security_policy.clone(),
                    security_mode: c.security_mode.clone(),
                    auth: c.auth.clone(),
                    timeout_ms: c.timeout_ms,
                    monitored_nodes: Vec::new(),
                }
            })
            .collect();
        (conn_data, groups.clone())
    };

    let mut project = ProjectFile::new_master();
    project.connections = conn_entries;
    project.groups = groups_snapshot;
    let json = project.to_json().map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let project = ProjectFile::from_json(&json).map_err(|e| e.to_string())?;

    {
        let mut conns = state.connections.write().map_err(|e| e.to_string())?;
        conns.clear();
        for ce in &project.connections {
            let id = Uuid::new_v4().to_string();
            let config = ConnectionConfig {
                id: id.clone(),
                name: ce.name.clone(),
                endpoint_url: ce.endpoint_url.clone(),
                security_policy: ce.security_policy.clone(),
                security_mode: ce.security_mode.clone(),
                auth: ce.auth.clone(),
                timeout_ms: ce.timeout_ms,
            };
            let connection = Arc::new(OpcUaConnection::new(config));
            let session_holder = connection.get_session_holder();
            conns.insert(
                id,
                ConnectionEntry {
                    connection,
                    subscription_mgr: SubscriptionManager::new(),
                    polling_mgr: Arc::new(PollingManager::new(session_holder)),
                    pending_subscriptions: Vec::new(),
                    pending_polling: Vec::new(),
                },
            );
        }
    }
    {
        let mut groups = state.groups.write().map_err(|e| e.to_string())?;
        *groups = project.groups;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Communication log
// ---------------------------------------------------------------------------

pub(crate) async fn get_communication_logs_impl(
    state: &AppState,
    connection_id: &str,
) -> Result<Vec<LogRow>, String> {
    let collector = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(connection_id)
            .ok_or("Connection not found")?
            .connection
            .log_collector
            .clone()
    };
    Ok(collector
        .get_all()
        .into_iter()
        .map(log_entry_to_row)
        .collect())
}

#[tauri::command]
pub async fn get_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<LogRow>, String> {
    get_communication_logs_impl(state.inner(), &connection_id).await
}

#[tauri::command]
pub async fn clear_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let collector = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .connection
            .log_collector
            .clone()
    };
    collector.clear();
    Ok(())
}

#[tauri::command]
pub async fn export_communication_logs(
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
) -> Result<(), String> {
    let csv = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        conns
            .get(&connection_id)
            .ok_or("Connection not found")?
            .connection
            .log_collector
            .export_csv()
    };
    std::fs::write(&path, csv).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use opcuasim_core::server::models::{
        DataType, ServerConfig, ServerFolder, ServerNode, SimulationMode,
    };
    use opcuasim_core::server::server::OpcUaServer;
    use std::sync::Arc;
    use std::time::Duration;

    const TEST_PORT: u16 = 49501;

    #[test]
    fn parse_iso_to_datetime_accepts_rfc3339_and_rejects_garbage() {
        assert!(parse_iso_to_datetime("2024-06-01T12:00:00Z").is_ok());
        assert!(parse_iso_to_datetime("2024-06-01T12:00:00+08:00").is_ok());
        assert!(parse_iso_to_datetime("not a date").is_err());
    }

    #[test]
    fn string_to_variant_parses_int32_and_rejects_garbage() {
        match string_to_variant("Int32", "42") {
            Ok(opcua_types::Variant::Int32(v)) => assert_eq!(v, 42),
            other => panic!("expected Int32(42), got: {other:?}"),
        }
        assert!(string_to_variant("Int32", "abc").is_err());
        assert!(string_to_variant("Unsupported", "1").is_err());
    }

    fn ensure(cond: bool, msg: impl Into<String>) -> Result<(), String> {
        if cond {
            Ok(())
        } else {
            Err(msg.into())
        }
    }

    fn server_config(port: u16) -> ServerConfig {
        ServerConfig {
            name: "MasterE2E".into(),
            endpoint_url: format!("opc.tcp://127.0.0.1:{port}"),
            port,
            security_policies: vec!["None".into()],
            security_modes: vec!["None".into()],
            users: Vec::new(),
            anonymous_enabled: true,
            max_sessions: 10,
            max_subscriptions_per_session: 10,
            history_buffer_size: 10_000,
            event_history_size: 1_000,
            ..Default::default()
        }
    }

    fn e2e_folders() -> Vec<ServerFolder> {
        vec![ServerFolder {
            node_id: "MyFolder".into(),
            display_name: "MyFolder".into(),
            browse_name: None,
            parent_id: "i=85".into(),
        }]
    }

    fn e2e_nodes() -> Vec<ServerNode> {
        vec![
            ServerNode {
                node_id: "Demo.Static".into(),
                display_name: "Static".into(),
                browse_name: None,
                parent_id: "MyFolder".into(),
                data_type: DataType::Double,
                writable: false,
                simulation: SimulationMode::Static {
                    value: "3.14".into(),
                },
                eu_range_low: 0.0,
                eu_range_high: 100.0,
            },
            ServerNode {
                node_id: "Demo.Sine".into(),
                display_name: "Sine".into(),
                browse_name: None,
                parent_id: "MyFolder".into(),
                data_type: DataType::Double,
                writable: false,
                simulation: SimulationMode::Sine {
                    amplitude: 10.0,
                    offset: 0.0,
                    period_ms: 2000,
                    interval_ms: 100,
                },
                eu_range_low: -10.0,
                eu_range_high: 10.0,
            },
            ServerNode {
                node_id: "Demo.Setpoint".into(),
                display_name: "Setpoint".into(),
                browse_name: None,
                parent_id: "MyFolder".into(),
                data_type: DataType::Double,
                writable: true,
                simulation: SimulationMode::Static { value: "0".into() },
                eu_range_low: 0.0,
                eu_range_high: 100.0,
            },
        ]
    }

    async fn run_master_scenario() -> Result<(), String> {
        let state = AppState::new();

        let conn = create_connection_impl(
            &state,
            CreateConnectionRequest {
                name: "e2e".into(),
                endpoint_url: format!("opc.tcp://127.0.0.1:{TEST_PORT}"),
                security_policy: "None".into(),
                security_mode: "None".into(),
                auth: AuthRequest::Anonymous,
                timeout_ms: 5_000,
            },
        )?;
        ensure(
            conn.state == "Disconnected",
            "new connection should start Disconnected",
        )?;

        connect_impl(&state, &conn.id, |_| {}).await?;

        let mut connected = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let infos = list_connections_impl(&state).await?;
            if infos
                .iter()
                .any(|c| c.id == conn.id && c.state == "Connected")
            {
                connected = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure(connected, "connection never reached Connected")?;

        let root = browse_root_impl(&state, &conn.id).await?;
        ensure(!root.is_empty(), "browse_root returned no items")?;
        ensure(
            root.iter().any(|i| i.display_name == "MyFolder"),
            "browse_root should include MyFolder",
        )?;

        let children = browse_node_impl(&state, &conn.id, "ns=2;s=MyFolder").await?;
        ensure(
            !children.is_empty(),
            "browse_node(MyFolder) returned no items",
        )?;
        ensure(
            children.iter().any(|i| i.display_name == "Sine"),
            "browse_node should include Sine",
        )?;

        add_monitored_nodes_impl(
            &state,
            &conn.id,
            vec![
                MonitoredNodeReq {
                    node_id: "ns=2;s=Demo.Sine".into(),
                    display_name: "Sine".into(),
                    data_type: Some("Double".into()),
                    access_mode: "Subscription".into(),
                    interval_ms: 100.0,
                    filter: None,
                },
                MonitoredNodeReq {
                    node_id: "ns=2;s=Demo.Static".into(),
                    display_name: "Static".into(),
                    data_type: Some("Double".into()),
                    access_mode: "Polling".into(),
                    interval_ms: 100.0,
                    filter: None,
                },
            ],
        )
        .await?;

        let mut monitored_value_seen = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let snap = get_monitored_nodes_since_impl(&state, &conn.id, 0).await?;
            if snap
                .nodes
                .iter()
                .any(|n| n.node_id == "ns=2;s=Demo.Sine" && n.value.is_some())
            {
                monitored_value_seen = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure(
            monitored_value_seen,
            "monitored Sine node never reported a value",
        )?;

        let attrs = read_attributes_impl(&state, &conn.id, "ns=2;s=Demo.Static").await?;
        ensure(attrs.value.is_some(), "Static node should have a value")?;
        ensure(
            !attrs.data_type.is_empty(),
            "Static node should have a data type",
        )?;

        write_value_impl(&state, &conn.id, "ns=2;s=Demo.Setpoint", "42.5", "Double").await?;
        let written = read_attributes_impl(&state, &conn.id, "ns=2;s=Demo.Setpoint").await?;
        ensure(
            written.value.as_deref().is_some_and(|v| v.contains("42.5")),
            "Setpoint should read back 42.5",
        )?;

        // Give the simulation engine a moment to record history samples.
        tokio::time::sleep(Duration::from_millis(900)).await;
        let start_iso = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let end_iso = chrono::Utc::now().to_rfc3339();
        let points = read_history_impl(
            &state,
            &conn.id,
            ReadHistoryRequest {
                node_id: "ns=2;s=Demo.Sine".into(),
                start_iso,
                end_iso,
                max_values: 100,
                mode: HistoryMode::Raw,
                agg_type: None,
                processing_interval_ms: None,
            },
        )
        .await?;
        ensure(!points.is_empty(), "expected at least one history point")?;

        let args = read_method_arguments_impl(&state, &conn.id, "ns=2;s=Demo.Echo").await?;
        ensure(!args.inputs.is_empty(), "Demo.Echo should declare inputs")?;

        let call = call_method_impl(
            &state,
            &conn.id,
            CallMethodRequest {
                object_id: "i=85".into(),
                method_id: "ns=2;s=Demo.Echo".into(),
                inputs: vec![MethodArgValue {
                    data_type: "String".into(),
                    value: "hello".into(),
                }],
            },
        )
        .await?;
        ensure(
            call.status.contains("Good"),
            format!("Demo.Echo should return Good status, got: {}", call.status),
        )?;

        let logs = get_communication_logs_impl(&state, &conn.id).await?;
        ensure(!logs.is_empty(), "communication logs should not be empty")?;
        ensure(
            logs.iter().any(|l| {
                l.detail_event
                    .as_ref()
                    .is_some_and(|e| e.kind.starts_with("connection."))
            }),
            "logs should include a connection.* lifecycle entry",
        )?;

        disconnect_impl(&state, &conn.id).await?;
        let infos = list_connections_impl(&state).await?;
        ensure(
            infos
                .iter()
                .any(|c| c.id == conn.id && c.state == "Disconnected"),
            "connection should be Disconnected after disconnect",
        )?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_smoke_against_real_server() {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info,opcua=warn"),
        )
        .is_test(true)
        .try_init();

        let server = Arc::new(OpcUaServer::new());
        server
            .start(&server_config(TEST_PORT), &e2e_folders(), &e2e_nodes())
            .await
            .expect("server start");

        let outcome = run_master_scenario().await;
        let stop_result = server.stop().await;

        if let Err(e) = outcome {
            panic!("master e2e scenario failed: {e}");
        }
        stop_result.expect("server stop");
    }
}
