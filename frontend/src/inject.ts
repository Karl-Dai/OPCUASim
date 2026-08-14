import { inject as vueInject, type InjectionKey, type Ref } from 'vue'
import type {
  AddFolderRequest,
  AddNodeRequest,
  AddressSpace,
  ServerConfig,
  ServerStatus,
  UpdateNodeRequest,
} from './types'

export interface ServerContext {
  status: Ref<ServerStatus>
  addressSpace: Ref<AddressSpace>
  config: Ref<ServerConfig | null>
  selectedNodeId: Ref<string | null>
  currentValues: Ref<Map<string, string>>
  lastSimSeq: Ref<number>
  refreshAll: () => Promise<void>
  refreshStatus: () => Promise<void>
  refreshAddressSpace: () => Promise<void>
  refreshConfig: () => Promise<void>
  selectNode: (id: string | null) => void
  addFolder: (request: AddFolderRequest) => Promise<void>
  addNode: (request: AddNodeRequest) => Promise<void>
  updateNode: (request: UpdateNodeRequest) => Promise<void>
  removeNode: (nodeId: string) => Promise<void>
}

export const serverContextKey: InjectionKey<ServerContext> = Symbol('serverContext')

export function useServerContext(): ServerContext {
  const ctx = vueInject(serverContextKey)
  if (!ctx) throw new Error('server context was not provided')
  return ctx
}
