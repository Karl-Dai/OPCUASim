// Command/response DTOs matching the Rust `opcuaserver-app` DTOs exactly.
// The OPC UA domain types below mirror the serde representation of
// `opcuasim-core`'s `DataType` / `SimulationMode` enums.

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

// `DataType` serializes as an externally-tagged enum: unit variants are plain
// strings, struct variants are `{ Variant: { ... } }` objects.
export type ScalarDataType =
  | 'Boolean'
  | 'Int16'
  | 'Int32'
  | 'Int64'
  | 'UInt16'
  | 'UInt32'
  | 'UInt64'
  | 'Float'
  | 'Double'
  | 'String'
  | 'DateTime'
  | 'ByteString'

export interface StructFieldDto {
  name: string
  type: DataType
}

export type DataType =
  | ScalarDataType
  | { Array: { elementType: DataType } }
  | { Array2D: { elementType: DataType; dims: [number, number] } }
  | { Enum: { name: string; fields: Array<[number, string]> } }
  | { Structure: { name: string; fields: StructFieldDto[] } }

export type LinearMode = 'Repeat' | 'Bounce'

// `SimulationMode` is internally tagged with `type`.
export type SimulationMode =
  | { type: 'Static'; value: string }
  | { type: 'Random'; min: number; max: number; interval_ms: number }
  | { type: 'Sine'; amplitude: number; offset: number; period_ms: number; interval_ms: number }
  | {
      type: 'Linear'
      start: number
      step: number
      min: number
      max: number
      mode: LinearMode
      interval_ms: number
    }
  | { type: 'Script'; expression: string; interval_ms: number }

export interface NodeRow {
  node_id: string
  display_name: string
  parent_id: string
  data_type: DataType
  writable: boolean
  simulation: SimulationMode
  current_value: string | null
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

export type UserRole = 'ReadOnly' | 'ReadWrite' | 'Admin'

export interface UserAccount {
  username: string
  password: string
  role: UserRole
}

export interface ServerConfig {
  name: string
  endpoint_url: string
  port: number
  security_policies: string[]
  security_modes: string[]
  users: UserAccount[]
  anonymous_enabled: boolean
  max_sessions: number
  max_subscriptions_per_session: number
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
