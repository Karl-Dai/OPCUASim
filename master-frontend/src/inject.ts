import { inject as vueInject, type ComputedRef, type InjectionKey, type Ref } from 'vue'
import type {
  AddVariablesUnderNodeRequest,
  CentralTab,
  ConnectionInfo,
  CreateConnectionRequest,
  HistoryTarget,
  MonitoredNodeReq,
  MonitoredRow,
  NodeGroupDto,
} from './types'

export interface MasterContext {
  connections: Ref<ConnectionInfo[]>
  selectedConnectionId: Ref<string | null>
  selectedConnection: ComputedRef<ConnectionInfo | null>
  selectedNodeId: Ref<string | null>
  groups: Ref<NodeGroupDto[]>
  /** Merged subscription + polling rows for the selected connection. */
  monitoredRows: ComputedRef<Map<string, MonitoredRow>>
  activeTab: Ref<CentralTab>
  historyTarget: Ref<HistoryTarget | null>

  refreshConnections: () => Promise<void>
  refreshGroups: () => Promise<void>
  selectConnection: (id: string | null) => void
  selectNode: (id: string | null) => void
  applyConnectionState: (id: string, state: string) => void

  connect: (connId: string) => Promise<void>
  disconnect: (connId: string) => Promise<void>
  deleteConnection: (connId: string) => Promise<void>
  createConnection: (request: CreateConnectionRequest) => Promise<ConnectionInfo>
  loadProject: (path: string) => Promise<void>
  saveProject: (path: string) => Promise<void>

  addMonitoredNodes: (connId: string, nodes: MonitoredNodeReq[]) => Promise<void>
  addVariablesUnderNode: (connId: string, request: AddVariablesUnderNodeRequest) => Promise<void>
  removeMonitoredNodes: (connId: string, nodeIds: string[]) => Promise<void>

  createGroup: (name: string) => Promise<void>
  deleteGroup: (id: string) => Promise<void>
  addToGroup: (groupId: string, nodeIds: string[]) => Promise<void>

  openHistory: (connId: string, nodeId: string, displayName: string) => void
  setActiveTab: (tab: CentralTab) => void
}

export const masterContextKey: InjectionKey<MasterContext> = Symbol('masterContext')

export function useMasterContext(): MasterContext {
  const ctx = vueInject(masterContextKey)
  if (!ctx) throw new Error('master context was not provided')
  return ctx
}
