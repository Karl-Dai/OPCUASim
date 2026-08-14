<script setup lang="ts">
import { computed, onMounted, onUnmounted, provide, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useI18n } from '@shared/i18n'
import Toolbar from './components/Toolbar.vue'
import ConnectionTree from './components/ConnectionTree.vue'
import DataTable from './components/DataTable.vue'
import ValuePanel from './components/ValuePanel.vue'
import HistoryPanel from './components/HistoryPanel.vue'
import EventsPanel from './components/EventsPanel.vue'
import LogPanel from './components/LogPanel.vue'
import AppDialog from '@shared/components/AppDialog.vue'
import { masterContextKey, type MasterContext } from './inject'
import type {
  AddVariablesUnderNodeRequest,
  CentralTab,
  ConnectionInfo,
  CreateConnectionRequest,
  HistoryTarget,
  MonitoredNodeReq,
  MonitoredRow,
  MonitoredSnapshot,
  NodeGroupDto,
} from './types'

const { t } = useI18n()

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

const connections = ref<ConnectionInfo[]>([])
const selectedConnectionId = ref<string | null>(null)
const selectedNodeId = ref<string | null>(null)
const groups = ref<NodeGroupDto[]>([])

const selectedConnection = computed<ConnectionInfo | null>(
  () => connections.value.find((c) => c.id === selectedConnectionId.value) ?? null,
)

// Monitoring data for the selected connection. Subscription rows come from the
// incremental `get_monitored_nodes_since` cursor; polling rows come from the
// separate full-snapshot `get_polling_nodes` command.
const subRows = ref<Map<string, MonitoredRow>>(new Map())
const pollRows = ref<Map<string, MonitoredRow>>(new Map())
const monitorSeq = new Map<string, number>()

const monitoredRows = computed<Map<string, MonitoredRow>>(() => {
  const merged = new Map<string, MonitoredRow>(subRows.value)
  for (const [nodeId, row] of pollRows.value) merged.set(nodeId, row)
  return merged
})

const activeTab = ref<CentralTab>('data')
const historyTarget = ref<HistoryTarget | null>(null)
const logExpanded = ref(false)

// ---------------------------------------------------------------------------
// Backend actions
// ---------------------------------------------------------------------------

async function refreshConnections(): Promise<void> {
  try {
    const list = await invoke<ConnectionInfo[]>('list_connections')
    connections.value = list
    if (selectedConnectionId.value && !list.some((c) => c.id === selectedConnectionId.value)) {
      selectedConnectionId.value = null
      selectedNodeId.value = null
      subRows.value = new Map()
      pollRows.value = new Map()
    }
    if (historyTarget.value && !list.some((c) => c.id === historyTarget.value!.connection_id)) {
      historyTarget.value = null
      if (activeTab.value === 'history') activeTab.value = 'data'
    }
  } catch (error) {
    console.warn('list_connections failed', error)
  }
}

async function refreshGroups(): Promise<void> {
  try {
    groups.value = await invoke<NodeGroupDto[]>('list_groups')
  } catch (error) {
    console.warn('list_groups failed', error)
  }
}

function selectConnection(id: string | null): void {
  selectedConnectionId.value = id
  selectedNodeId.value = null
  subRows.value = new Map()
  pollRows.value = new Map()
  void pollMonitor()
}

function selectNode(id: string | null): void {
  selectedNodeId.value = id
}

function applyConnectionState(id: string, state: string): void {
  const conn = connections.value.find((c) => c.id === id)
  if (conn) conn.state = state
}

async function connect(connId: string): Promise<void> {
  await invoke('connect', { connectionId: connId })
  void pollMonitor()
}

async function disconnect(connId: string): Promise<void> {
  await invoke('disconnect', { connectionId: connId })
}

async function deleteConnection(connId: string): Promise<void> {
  await invoke('delete_connection', { connectionId: connId })
  monitorSeq.delete(connId)
  if (selectedConnectionId.value === connId) {
    selectedConnectionId.value = null
    selectedNodeId.value = null
    subRows.value = new Map()
    pollRows.value = new Map()
  }
  if (historyTarget.value?.connection_id === connId) {
    historyTarget.value = null
    if (activeTab.value === 'history') activeTab.value = 'data'
  }
  await Promise.all([refreshConnections(), refreshGroups()])
}

async function createConnection(request: CreateConnectionRequest): Promise<ConnectionInfo> {
  const conn = await invoke<ConnectionInfo>('create_connection', { request })
  await refreshConnections()
  return conn
}

async function loadProject(path: string): Promise<void> {
  await invoke('load_project', { path })
  monitorSeq.clear()
  selectedConnectionId.value = null
  selectedNodeId.value = null
  subRows.value = new Map()
  pollRows.value = new Map()
  activeTab.value = 'data'
  historyTarget.value = null
  await Promise.all([refreshConnections(), refreshGroups()])
}

async function saveProject(path: string): Promise<void> {
  await invoke('save_project', { path })
}

async function addMonitoredNodes(connId: string, nodes: MonitoredNodeReq[]): Promise<void> {
  await invoke('add_monitored_nodes', { connectionId: connId, nodes })
  monitorSeq.set(connId, 0)
  void pollMonitor()
}

async function addVariablesUnderNode(connId: string, request: AddVariablesUnderNodeRequest): Promise<void> {
  await invoke('add_variables_under_node', { connectionId: connId, request })
  monitorSeq.set(connId, 0)
  void pollMonitor()
}

async function removeMonitoredNodes(connId: string, nodeIds: string[]): Promise<void> {
  await invoke('remove_monitored_nodes', { connectionId: connId, nodeIds })
  for (const id of nodeIds) {
    subRows.value.delete(id)
    pollRows.value.delete(id)
  }
  monitorSeq.set(connId, 0)
  void pollMonitor()
}

async function createGroup(name: string): Promise<void> {
  groups.value = await invoke<NodeGroupDto[]>('create_group', { name })
}

async function deleteGroup(id: string): Promise<void> {
  groups.value = await invoke<NodeGroupDto[]>('delete_group', { id })
}

async function addToGroup(groupId: string, nodeIds: string[]): Promise<void> {
  groups.value = await invoke<NodeGroupDto[]>('add_to_group', { groupId, nodeIds })
}

function openHistory(connId: string, nodeId: string, displayName: string): void {
  historyTarget.value = { connection_id: connId, node_id: nodeId, display_name: displayName }
  activeTab.value = 'history'
}

function setActiveTab(tab: CentralTab): void {
  activeTab.value = tab
}

// ---------------------------------------------------------------------------
// Monitoring poll
// ---------------------------------------------------------------------------

async function pollMonitor(): Promise<void> {
  const connId = selectedConnectionId.value
  if (!connId) {
    if (subRows.value.size > 0 || pollRows.value.size > 0) {
      subRows.value = new Map()
      pollRows.value = new Map()
    }
    return
  }
  const captured = connId

  try {
    const snap = await invoke<MonitoredSnapshot>('get_monitored_nodes_since', {
      connectionId: connId,
      seq: monitorSeq.get(connId) ?? 0,
    })
    if (captured !== selectedConnectionId.value) return
    if (snap.full) {
      subRows.value = new Map(snap.nodes.map((n): [string, MonitoredRow] => [n.node_id, n]))
    } else {
      for (const n of snap.nodes) subRows.value.set(n.node_id, n)
    }
    monitorSeq.set(connId, snap.seq)
  } catch {
    // Not connected yet, or the connection was removed.
  }

  try {
    const polls = await invoke<MonitoredRow[]>('get_polling_nodes', { connectionId: connId })
    if (captured !== selectedConnectionId.value) return
    pollRows.value = new Map(polls.map((n): [string, MonitoredRow] => [n.node_id, n]))
  } catch {
    // Same as above.
  }
}

let monitorTimer: number | null = null
let unlistenConnState: (() => void) | null = null

onMounted(async () => {
  await Promise.all([refreshConnections(), refreshGroups()])

  unlistenConnState = await listen<{ id: string; state: string }>('connection-state', (event) => {
    applyConnectionState(event.payload.id, event.payload.state)
  })

  monitorTimer = window.setInterval(pollMonitor, 500)
})

onUnmounted(() => {
  unlistenConnState?.()
  if (monitorTimer !== null) {
    clearInterval(monitorTimer)
    monitorTimer = null
  }
})

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

const context: MasterContext = {
  connections,
  selectedConnectionId,
  selectedConnection,
  selectedNodeId,
  groups,
  monitoredRows,
  activeTab,
  historyTarget,
  refreshConnections,
  refreshGroups,
  selectConnection,
  selectNode,
  applyConnectionState,
  connect,
  disconnect,
  deleteConnection,
  createConnection,
  loadProject,
  saveProject,
  addMonitoredNodes,
  addVariablesUnderNode,
  removeMonitoredNodes,
  createGroup,
  deleteGroup,
  addToGroup,
  openHistory,
  setActiveTab,
}

provide(masterContextKey, context)

// Log panel expands to a fixed height in the grid.
const gridRows = computed(() => (logExpanded.value ? '42px 1fr 280px' : '42px 1fr 32px'))
</script>

<template>
  <div class="app-layout" :style="{ gridTemplateRows: gridRows }">
    <header class="toolbar-area">
      <Toolbar />
    </header>

    <aside class="tree-area">
      <ConnectionTree />
    </aside>

    <main class="content-area">
      <div class="tab-bar">
        <button :class="['tab-btn', { active: activeTab === 'data' }]" @click="setActiveTab('data')">
          {{ t('dataTable.title') }}
        </button>
        <button :class="['tab-btn', { active: activeTab === 'history' }]" @click="setActiveTab('history')">
          {{ t('history.title') }}
        </button>
        <button :class="['tab-btn', { active: activeTab === 'events' }]" @click="setActiveTab('events')">
          {{ t('events.title') }}
        </button>
      </div>
      <div class="tab-body">
        <DataTable v-show="activeTab === 'data'" />
        <HistoryPanel v-show="activeTab === 'history'" />
        <EventsPanel v-show="activeTab === 'events'" />
      </div>
    </main>

    <aside class="panel-area">
      <ValuePanel />
    </aside>

    <footer class="log-area">
      <LogPanel :expanded="logExpanded" @toggle="logExpanded = !logExpanded" />
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
  grid-template-columns: 300px 1fr 320px;
  grid-template-areas:
    'toolbar toolbar toolbar'
    'tree content panel'
    'log log log';
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
  display: flex;
  flex-direction: column;
}

.panel-area {
  grid-area: panel;
  background: var(--c-mantle);
  overflow: hidden;
  border-left: 1px solid var(--c-surface0);
}

.log-area {
  grid-area: log;
  background: var(--c-base);
  border-top: 1px solid var(--c-surface0);
  overflow: hidden;
}

.tab-bar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 8px;
  height: 34px;
  background: var(--c-base);
  border-bottom: 1px solid var(--c-surface0);
  flex: none;
}

.tab-btn {
  padding: 4px 12px;
  border: none;
  background: transparent;
  color: var(--c-subtext0);
  cursor: pointer;
  font-size: 12px;
  border-radius: 4px;
}
.tab-btn:hover {
  background: var(--c-surface0);
}
.tab-btn.active {
  background: var(--c-blue);
  color: var(--c-base);
}

.tab-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.tab-body > * {
  flex: 1;
  min-height: 0;
}
</style>
