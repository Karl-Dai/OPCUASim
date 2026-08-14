<script setup lang="ts">
import { onMounted, onUnmounted, provide, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import Toolbar from './components/Toolbar.vue'
import AddressTree from './components/AddressTree.vue'
import NodeTable from './components/NodeTable.vue'
import PropertyEditor from './components/PropertyEditor.vue'
import StatusBar from './components/StatusBar.vue'
import AppDialog from '@shared/components/AppDialog.vue'
import { serverContextKey } from './inject'
import type {
  AddFolderRequest,
  AddNodeRequest,
  AddressSpace,
  ServerConfig,
  ServerStatus,
  SimValuesResponse,
  UpdateNodeRequest,
} from './types'

const status = ref<ServerStatus>({
  state: 'Stopped',
  node_count: 0,
  folder_count: 0,
  endpoint_url: 'opc.tcp://0.0.0.0:4840',
})
const addressSpace = ref<AddressSpace>({ folders: [], nodes: [] })
const config = ref<ServerConfig | null>(null)
const selectedNodeId = ref<string | null>(null)
const currentValues = ref<Map<string, string>>(new Map())
const lastSimSeq = ref(0)

async function refreshStatus() {
  status.value = await invoke<ServerStatus>('refresh_status')
}

async function refreshAddressSpace() {
  addressSpace.value = await invoke<AddressSpace>('refresh_address_space')
}

async function refreshConfig() {
  config.value = await invoke<ServerConfig>('get_config')
}

async function refreshAll() {
  await Promise.all([refreshStatus(), refreshAddressSpace(), refreshConfig()])
}

function selectNode(id: string | null) {
  selectedNodeId.value = id
}

async function addFolder(request: AddFolderRequest) {
  addressSpace.value = await invoke<AddressSpace>('add_folder', { request })
}

async function addNode(request: AddNodeRequest) {
  addressSpace.value = await invoke<AddressSpace>('add_node', { request })
}

async function updateNode(request: UpdateNodeRequest) {
  addressSpace.value = await invoke<AddressSpace>('update_node', { request })
}

async function removeNode(nodeId: string) {
  addressSpace.value = await invoke<AddressSpace>('remove_node', { nodeId })
}

provide(serverContextKey, {
  status,
  addressSpace,
  config,
  selectedNodeId,
  currentValues,
  lastSimSeq,
  refreshAll,
  refreshStatus,
  refreshAddressSpace,
  refreshConfig,
  selectNode,
  addFolder,
  addNode,
  updateNode,
  removeNode,
})

// Incremental simulation-value polling. The backend does NOT push values as
// events; the frontend polls `get_simulation_values_since` like the 104 app
// polls `list_data_points_since`.
let simTimer: number | null = null

async function pollSimValues() {
  try {
    const resp = await invoke<SimValuesResponse>('get_simulation_values_since', {
      seq: lastSimSeq.value,
    })
    if (resp.values.length > 0) {
      for (const value of resp.values) {
        currentValues.value.set(value.node_id, value.value)
      }
    }
    lastSimSeq.value = resp.seq
  } catch (error) {
    console.warn('simulation poll failed', error)
  }
}

let unlistenServerState: (() => void) | null = null

onMounted(async () => {
  unlistenServerState = await listen<ServerStatus>('server-state', (event) => {
    status.value = event.payload
  })
  await refreshAll()
  simTimer = window.setInterval(pollSimValues, 500)
})

onUnmounted(() => {
  unlistenServerState?.()
  if (simTimer !== null) {
    window.clearInterval(simTimer)
    simTimer = null
  }
})
</script>

<template>
  <div class="app-layout">
    <header class="toolbar-area">
      <Toolbar />
    </header>
    <aside class="tree-area">
      <AddressTree />
    </aside>
    <main class="content-area">
      <NodeTable />
    </main>
    <aside class="property-area">
      <PropertyEditor />
    </aside>
    <footer class="status-area">
      <StatusBar />
    </footer>
    <AppDialog />
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html,
body,
#app {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
  background: var(--c-crust);
  color: var(--c-text);
}

/* Dark scrollbars across the app. */
*::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
*::-webkit-scrollbar-track {
  background: var(--c-mantle);
}
*::-webkit-scrollbar-thumb {
  background: var(--c-surface0);
  border-radius: 5px;
  border: 2px solid var(--c-mantle);
}
*::-webkit-scrollbar-thumb:hover {
  background: var(--c-surface1);
}
*::-webkit-scrollbar-corner {
  background: var(--c-mantle);
}
* {
  scrollbar-color: var(--c-surface0) var(--c-mantle);
  scrollbar-width: thin;
}

:focus {
  outline: none;
}
:focus-visible {
  outline: 2px solid var(--c-blue);
  outline-offset: 1px;
  border-radius: 2px;
}

.app-layout {
  display: grid;
  grid-template-columns: 260px 1fr 320px;
  grid-template-rows: auto 1fr 28px;
  grid-template-areas:
    'toolbar toolbar toolbar'
    'tree    content property'
    'status  status  status';
  height: 100vh;
  width: 100vw;
}

.toolbar-area {
  grid-area: toolbar;
  background: var(--c-base);
  border-bottom: 1px solid var(--c-surface0);
}

.tree-area {
  grid-area: tree;
  background: var(--c-mantle);
  overflow: hidden;
  border-right: 1px solid var(--c-surface0);
}

.content-area {
  grid-area: content;
  background: var(--c-crust);
  overflow: hidden;
}

.property-area {
  grid-area: property;
  background: var(--c-mantle);
  overflow: hidden;
  border-left: 1px solid var(--c-surface0);
}

.status-area {
  grid-area: status;
  background: var(--c-base);
  border-top: 1px solid var(--c-surface0);
  overflow: hidden;
}
</style>
