import type { DataType, SimulationMode } from './types'

// Scalar types offered by the legacy "add node" form. Complex types (Array /
// Enum / Structure) can still arrive in the model via a loaded project.
export const SCALAR_DATA_TYPES: DataType[] = [
  'Boolean',
  'Int16',
  'Int32',
  'Int64',
  'UInt16',
  'UInt32',
  'UInt64',
  'Float',
  'Double',
  'String',
  'DateTime',
  'ByteString',
]

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

export function simulationLabel(mode: SimulationMode): string {
  switch (mode.type) {
    case 'Static':
      return 'Static'
    case 'Random':
      return 'Random'
    case 'Sine':
      return 'Sine'
    case 'Linear':
      return 'Linear'
    case 'Script':
      return 'Script'
  }
}

export type SimKind = 'Static' | 'Random' | 'Sine' | 'Linear' | 'Script'

export const SIM_KINDS: SimKind[] = ['Static', 'Random', 'Sine', 'Linear', 'Script']

export function simKindLabel(kind: SimKind): string {
  return kind
}

// Default parameters matching the legacy egui `AddNodeForm::build_simulation`.
export function defaultSimulation(kind: SimKind): SimulationMode {
  switch (kind) {
    case 'Static':
      return { type: 'Static', value: '0' }
    case 'Random':
      return { type: 'Random', min: 0, max: 100, interval_ms: 1000 }
    case 'Sine':
      return { type: 'Sine', amplitude: 1, offset: 0, period_ms: 10000, interval_ms: 1000 }
    case 'Linear':
      return {
        type: 'Linear',
        start: 0,
        step: 1,
        min: 0,
        max: 100,
        mode: 'Repeat',
        interval_ms: 1000,
      }
    case 'Script':
      return { type: 'Script', expression: 't * 0.1', interval_ms: 1000 }
  }
}

// Node-id construction helpers matching the legacy egui panels.
export function nodeIdFromName(name: string): string {
  return `ns=2;s=${name.replace(/ /g, '_')}`
}

export function subfolderNodeId(parentId: string, name: string): string {
  return `ns=2;s=${parentId.replace(/:/g, '_')}_${name.replace(/ /g, '_')}`
}

// Derived tree shape used by AddressTree: a flat list of folders and nodes is
// re-indexed by parent for hierarchical rendering.
export type AddressChild =
  | { kind: 'folder'; node_id: string; display_name: string }
  | { kind: 'node'; node_id: string; display_name: string }
