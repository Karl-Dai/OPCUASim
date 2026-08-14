// OPC UA domain types shared across the frontends.
//
// These mirror the serde representation of `opcuasim-core`'s `DataType` /
// `SimulationMode` enums exactly, so both apps can describe server-side
// variables with one canonical type surface.

/** Scalar `DataType` variants (externally-tagged serde unit variants). */
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

/** Field of a user-defined structure (serde `StructField`). */
export interface StructFieldDto {
  name: string
  type: DataType
}

/**
 * `DataType` serializes as an externally-tagged enum: unit variants are plain
 * strings, struct variants are `{ Variant: { ... } }` objects.
 */
export type DataType =
  | ScalarDataType
  | { Array: { elementType: DataType } }
  | { Array2D: { elementType: DataType; dims: [number, number] } }
  | { Enum: { name: string; fields: Array<[number, string]> } }
  | { Structure: { name: string; fields: StructFieldDto[] } }

/** Linear simulation wrap-around mode (serde `LinearMode`). */
export type LinearMode = 'Repeat' | 'Bounce'

/** `SimulationMode` is internally tagged with `type`. */
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

/** User role for server authentication (serde `UserRole`). */
export type UserRole = 'ReadOnly' | 'ReadWrite' | 'Admin'

/** Human-readable label for a (possibly complex) data type. */
export function dataTypeLabel(dt: DataType): string {
  if (typeof dt === 'string') return dt
  if ('Array' in dt) return `Array(${dataTypeLabel(dt.Array.elementType)})`
  if ('Array2D' in dt) {
    const { elementType, dims } = dt.Array2D
    return `Array2D(${dataTypeLabel(elementType)}${dims[0]}x${dims[1]})`
  }
  if ('Enum' in dt) return `Enum(${dt.Enum.name})`
  if ('Structure' in dt) return `Structure(${dt.Structure.name})`
  return String(dt)
}

/** Short label for a simulation mode. */
export function simulationLabel(mode: SimulationMode): string {
  return mode.type
}
