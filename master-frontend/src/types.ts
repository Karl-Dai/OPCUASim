// Command/response DTOs matching the Rust `opcuamaster-app` DTOs exactly.
// Field names follow serde's `rename_all = "snake_case"` so they line up with
// the JSON the Tauri backend produces/consumes. Enums are typed as string
// unions because serde serializes unit variants as snake_case strings.

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/** Certificate role discriminator (serde `CertRoleDto`). */
export type CertRole = 'trusted' | 'rejected'

/** History read mode (serde `HistoryMode`). */
export type HistoryMode = 'raw' | 'processed' | 'events'

/** DataChangeTriggerKind request enum (serde `DataChangeTriggerKindReq`). */
export type DataChangeTriggerKind = 'status' | 'status_value' | 'status_value_timestamp'

/** DeadbandKind request enum (serde `DeadbandKindReq`). */
export type DeadbandKind = 'none' | 'absolute' | 'percent'

/**
 * Authentication request. Internally tagged by serde with `tag = "type"` and
 * `rename_all = "snake_case"`, so the discriminator is `type` and the variant
 * names are snake_cased.
 */
export type AuthRequest =
  | { type: 'anonymous' }
  | { type: 'user_password'; username: string; password: string }
  | { type: 'certificate'; cert_path: string; key_path: string }

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

export interface ConnectionInfo {
  id: string
  name: string
  endpoint_url: string
  security_policy: string
  security_mode: string
  auth_type: string
  state: string
}

export interface BrowseItem {
  node_id: string
  display_name: string
  node_class: string
  data_type: string | null
  has_children: boolean
}

export interface NodeAttrsDto {
  node_id: string
  display_name: string
  description: string
  data_type: string
  access_level: string
  value: string | null
  quality: string | null
  timestamp: string | null
}

export interface MonitoredRow {
  node_id: string
  display_name: string
  data_type: string
  value: string | null
  quality: string | null
  source_timestamp: string | null
  server_timestamp: string | null
  access_mode: string
  interval_ms: number
  update_seq: number
  user_access_level: number
}

/** Incremental subscription snapshot (mirrors `get_monitored_nodes_since`). */
export interface MonitoredSnapshot {
  seq: number
  full: boolean
  nodes: MonitoredRow[]
}

export interface DiscoveredEndpointDto {
  endpoint_url: string
  security_policy: string
  security_mode: string
  security_level: number
  server_cert_thumbprint: string
  user_token_policy_ids: string[]
}

export interface CertSummaryDto {
  path: string
  file_name: string
  role: CertRole
  thumbprint: string
  subject_cn: string
  issuer_cn: string
  valid_from: string
  valid_to: string
}

export interface NodeGroupDto {
  id: string
  name: string
  node_ids: string[]
}

export interface DetailEvent {
  kind: string
  payload: Record<string, unknown>
}

export interface LogRow {
  seq: number
  /** UTC milliseconds since the Unix epoch. */
  timestamp_ms: number
  /** "Request" | "Response". */
  direction: string
  service: string
  detail: string
  status: string | null
  detail_event: DetailEvent | null
}

export interface EventItemDto {
  time: string
  severity: number
  source: string
  message: string
  event_type: string
}

export interface MethodArgInfo {
  name: string
  data_type: string
  description: string
}

export interface MethodArgValue {
  data_type: string
  value: string
}

export interface MethodArgsDto {
  inputs: MethodArgInfo[]
  outputs: MethodArgInfo[]
}

export interface MethodCallResultDto {
  status: string
  outputs: MethodArgValue[]
}

export interface HistoryPointDto {
  source_timestamp: string
  server_timestamp: string
  value: string
  numeric: number | null
  status: string
}

/** Non-fatal subscription result payload. */
export interface SubscribeResult {
  ok: boolean
  detail: string | null
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

export interface CreateConnectionRequest {
  name: string
  endpoint_url: string
  security_policy: string
  security_mode: string
  auth: AuthRequest
  timeout_ms: number
}

export interface DataChangeFilterReq {
  trigger: DataChangeTriggerKind
  deadband_kind: DeadbandKind
  deadband_value: number
}

export interface MonitoredNodeReq {
  node_id: string
  display_name: string
  data_type: string | null
  /** "Subscription" | "Polling". */
  access_mode: string
  interval_ms: number
  filter: DataChangeFilterReq | null
}

export interface AddVariablesUnderNodeRequest {
  node_id: string
  access_mode: string
  interval_ms: number
  max_depth: number
  filter: DataChangeFilterReq | null
}

export interface ReadHistoryRequest {
  node_id: string
  start_iso: string
  end_iso: string
  max_values: number
  mode: HistoryMode
  agg_type: string | null
  processing_interval_ms: number | null
}

export interface CallMethodRequest {
  object_id: string
  method_id: string
  inputs: MethodArgValue[]
}

// ---------------------------------------------------------------------------
// Frontend-only shared shapes
// ---------------------------------------------------------------------------

export type CentralTab = 'data' | 'history' | 'events'

export interface HistoryTarget {
  connection_id: string
  node_id: string
  display_name: string
}
