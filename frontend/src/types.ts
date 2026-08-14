// Command/response DTOs matching the Rust `opcuaserver-app` DTOs exactly.
// The OPC UA domain types (DataType / SimulationMode / UserRole) are shared
// with the other frontends via `@shared/types` and mirror the serde
// representation of `opcuasim-core`.

import type {
  ScalarDataType,
  StructFieldDto,
  DataType,
  LinearMode,
  SimulationMode,
  UserRole,
} from '@shared/types'

export type {
  ScalarDataType,
  StructFieldDto,
  DataType,
  LinearMode,
  SimulationMode,
  UserRole,
}

export interface ServerStatus {
  state: string
  node_count: number
  folder_count: number
  endpoint_url: string
}

export interface FolderRow {
  node_id: string
  display_name: string
  parent_id: string
}

export interface NodeRow {
  node_id: string
  display_name: string
  parent_id: string
  data_type: DataType
  writable: boolean
  simulation: SimulationMode
  eu_range_low: number
  eu_range_high: number
}

export interface AddressSpace {
  folders: FolderRow[]
  nodes: NodeRow[]
}

export interface SimValue {
  node_id: string
  value: string
}

export interface SimValuesResponse {
  seq: number
  values: SimValue[]
}

export interface UserAccount {
  username: string
  password: string
  role: UserRole
}

export interface ServerConfig {
  name: string
  application_uri: string
  host: string
  endpoint_url: string
  port: number
  security_policies: string[]
  security_modes: string[]
  users: UserAccount[]
  anonymous_enabled: boolean
  max_sessions: number
  max_subscriptions_per_session: number
  certificate_path: string | null
  private_key_path: string | null
  trust_client_certs: boolean
  history_buffer_size: number
  event_history_size: number
}

// Command request payloads (mirror `opcuaserver-app` command args).

export interface AddFolderRequest {
  node_id: string
  display_name: string
  parent_id: string
}

export interface AddNodeRequest {
  node_id: string
  display_name: string
  parent_id: string
  data_type: DataType
  writable: boolean
  simulation: SimulationMode
  eu_range_low: number
  eu_range_high: number
}

export interface UpdateNodeRequest {
  node_id: string
  display_name?: string | null
  data_type?: DataType | null
  writable?: boolean | null
  simulation?: SimulationMode | null
  eu_range_low?: number | null
  eu_range_high?: number | null
}
