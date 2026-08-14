<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showConfirm } from '@shared/composables/useDialog'
import { nodeIcon } from '../domain'
import MethodCallDialog from './MethodCallDialog.vue'
import { useMasterContext } from '../inject'
import type {
  BrowseItem,
  ConnectionInfo,
  DataChangeFilterReq,
  DataChangeTriggerKind,
  DeadbandKind,
  MonitoredNodeReq,
} from '../types'

const { t } = useI18n()
const {
  connections,
  selectedConnectionId,
  groups,
  monitoredRows,
  selectConnection,
  selectNode,
  selectedNodeId,
  addMonitoredNodes,
  addVariablesUnderNode,
  createGroup,
  deleteGroup,
  openHistory,
} = useMasterContext()

// ---------------------------------------------------------------------------
// Browse state (per connection, lazy)
// ---------------------------------------------------------------------------

interface BrowseNodeState {
  item: BrowseItem
  expanded: boolean
  children: string[] | null
  loading: boolean
}

interface BrowseState {
  rootLoaded: boolean
  loadingRoot: boolean
  nodes: Map<string, BrowseNodeState>
  roots: string[]
  selected: Set<string>
  parentOf: Map<string, string>
}

function newBrowseState(): BrowseState {
  return {
    rootLoaded: false,
    loadingRoot: false,
    nodes: new Map(),
    roots: [],
    selected: new Set(),
    parentOf: new Map(),
  }
}

interface ConnUiState {
  expanded: boolean
  browseOpen: boolean
  browse: BrowseState
}

const connStates = reactive<Record<string, ConnUiState>>({})

function stateFor(connId: string): ConnUiState {
  return connStates[connId]!
}

watch(
  () => connections.value.map((c) => c.id),
  (ids) => {
    for (const id of ids) {
      if (!connStates[id]) {
        connStates[id] = { expanded: true, browseOpen: false, browse: newBrowseState() }
      }
    }
  },
  { immediate: true },
)

// Browse controls (shared, mirror the legacy browse dialog)
const accessMode = ref('Subscription')
const intervalMs = ref(1000)
const maxDepth = ref(1)
const filterEnabled = ref(false)
const filterTrigger = ref<DataChangeTriggerKind>('status_value')
const filterDeadband = ref<DeadbandKind>('none')
const filterDeadbandValue = ref(0)

function currentFilter(): DataChangeFilterReq | null {
  if (!filterEnabled.value) return null
  return {
    trigger: filterTrigger.value,
    deadband_kind: filterDeadband.value,
    deadband_value: filterDeadbandValue.value,
  }
}

async function loadRoot(connId: string) {
  const state = stateFor(connId)
  if (state.browse.rootLoaded || state.browse.loadingRoot) return
  state.browse.loadingRoot = true
  try {
    const items = await invoke<BrowseItem[]>('browse_root', { connectionId: connId })
    for (const item of items) {
      state.browse.nodes.set(item.node_id, {
        item,
        expanded: false,
        children: null,
        loading: false,
      })
    }
    state.browse.roots = items.map((it) => it.node_id)
    state.browse.rootLoaded = true
  } catch {
    // connection may have dropped; leave rootLoaded false so a later expand retries
  } finally {
    state.browse.loadingRoot = false
  }
}

async function toggleBrowseNode(connId: string, nodeId: string) {
  const state = stateFor(connId)
  const node = state.browse.nodes.get(nodeId)
  if (!node) return
  if (node.expanded) {
    node.expanded = false
    return
  }
  node.expanded = true
  if (node.children !== null || node.loading) return
  node.loading = true
  try {
    const items = await invoke<BrowseItem[]>('browse_node', { connectionId: connId, nodeId })
    const childIds: string[] = []
    for (const item of items) {
      state.browse.nodes.set(item.node_id, {
        item,
        expanded: false,
        children: null,
        loading: false,
      })
      childIds.push(item.node_id)
      state.browse.parentOf.set(item.node_id, nodeId)
    }
    node.children = childIds
  } catch {
    node.expanded = false
  } finally {
    node.loading = false
  }
}

function toggleBrowseOpen(connId: string) {
  const state = stateFor(connId)
  state.browseOpen = !state.browseOpen
  if (state.browseOpen) void loadRoot(connId)
}

interface BrowseRow {
  nodeId: string
  item: BrowseItem
  depth: number
  expanded: boolean
  hasChildren: boolean
  loading: boolean
}

function flattenBrowse(state: BrowseState): BrowseRow[] {
  const out: BrowseRow[] = []
  const walk = (ids: string[], depth: number) => {
    for (const id of ids) {
      const node = state.nodes.get(id)
      if (!node) continue
      out.push({
        nodeId: id,
        item: node.item,
        depth,
        expanded: node.expanded,
        hasChildren: node.item.has_children,
        loading: node.loading,
      })
      if (node.expanded && node.children) walk(node.children, depth + 1)
    }
  }
  walk(state.roots, 0)
  return out
}

function isSelected(connId: string, nodeId: string): boolean {
  return stateFor(connId).browse.selected.has(nodeId)
}

function toggleSelect(connId: string, nodeId: string) {
  const sel = stateFor(connId).browse.selected
  if (sel.has(nodeId)) sel.delete(nodeId)
  else sel.add(nodeId)
}

function onBrowseLabel(connId: string, row: BrowseRow) {
  if (row.hasChildren) {
    void toggleBrowseNode(connId, row.nodeId)
    return
  }
  if (row.item.node_class === 'Method') {
    openMethodCall(connId, row.nodeId)
    return
  }
  selectNode(row.nodeId)
}

function openMethodCall(connId: string, methodId: string) {
  const state = stateFor(connId)
  const objectId = state.browse.parentOf.get(methodId) ?? methodId
  const displayName = state.browse.nodes.get(methodId)?.item.display_name ?? methodId
  methodCallState.value = { connectionId: connId, objectId, methodId, displayName }
}

async function addSelectedVariables(connId: string) {
  const state = stateFor(connId)
  const nodes: MonitoredNodeReq[] = []
  for (const nodeId of state.browse.selected) {
    const node = state.browse.nodes.get(nodeId)
    if (!node) continue
    nodes.push({
      node_id: nodeId,
      display_name: node.item.display_name,
      data_type: node.item.data_type,
      access_mode: accessMode.value,
      interval_ms: intervalMs.value,
      filter: currentFilter(),
    })
  }
  if (nodes.length === 0) return
  await addMonitoredNodes(connId, nodes)
  state.browse.selected.clear()
}

async function addAllUnderNode(connId: string, nodeId: string) {
  await addVariablesUnderNode(connId, {
    node_id: nodeId,
    access_mode: accessMode.value,
    interval_ms: intervalMs.value,
    max_depth: maxDepth.value,
    filter: currentFilter(),
  })
}

function stateChip(state: string): { color: string; label: string } {
  switch (state) {
    case 'Connected':
      return { color: 'var(--c-green)', label: t('state.connected') }
    case 'Connecting':
      return { color: 'var(--c-yellow)', label: t('state.connecting') }
    case 'Reconnecting':
      return { color: 'var(--c-peach)', label: t('state.reconnecting') }
    default:
      return { color: 'var(--c-overlay0)', label: state || t('state.disconnected') }
  }
}

// Monitored / polling split (only meaningful for the selected connection).
const subNodes = computed(() =>
  [...monitoredRows.value.values()].filter((r) => r.access_mode !== 'Polling'),
)
const pollNodes = computed(() =>
  [...monitoredRows.value.values()].filter((r) => r.access_mode === 'Polling'),
)

// Groups
const groupInput = ref('')
const expandedGroups = reactive(new Set<string>())

async function onAddGroup() {
  const name = groupInput.value.trim()
  if (!name) return
  await createGroup(name)
  groupInput.value = ''
}

async function onDeleteGroup(id: string) {
  if (await showConfirm(t('toolbar.confirmDeleteGroup'))) {
    await deleteGroup(id)
  }
}

function toggleGroup(id: string) {
  if (expandedGroups.has(id)) expandedGroups.delete(id)
  else expandedGroups.add(id)
}

// Method call dialog
const methodCallState = ref<{
  connectionId: string
  objectId: string
  methodId: string
  displayName: string
} | null>(null)

function selectConnectionNode(conn: ConnectionInfo) {
  selectConnection(conn.id)
  stateFor(conn.id).expanded = true
}
</script>

<template>
  <div class="conn-tree">
    <div class="tree-title">{{ t('tree.title') }}</div>
    <div class="tree-sep" />

    <div v-if="connections.length === 0" class="tree-empty">{{ t('tree.noConnections') }}</div>

    <div v-else class="tree-scroll">
      <template v-for="conn in connections" :key="conn.id">
        <div
          :class="['conn-row', { selected: selectedConnectionId === conn.id }]"
          @click="selectConnectionNode(conn)"
        >
          <button
            class="toggle"
            :class="{ open: stateFor(conn.id).expanded }"
            @click.stop="stateFor(conn.id).expanded = !stateFor(conn.id).expanded"
          >▸</button>
          <span class="state-dot" :style="{ background: stateChip(conn.state).color }" />
          <span class="conn-label">{{ conn.name }}</span>
          <span class="conn-state">{{ stateChip(conn.state).label }}</span>
        </div>

        <div v-if="stateFor(conn.id).expanded" class="conn-children">
          <div class="conn-meta">
            <div class="mono">{{ conn.endpoint_url }}</div>
            <div class="muted">{{ conn.auth_type }} · {{ conn.security_policy }} · {{ conn.security_mode }}</div>
          </div>

          <template v-if="conn.state === 'Connected'">
            <div class="section-row" @click="toggleBrowseOpen(conn.id)">
              <span class="toggle" :class="{ open: stateFor(conn.id).browseOpen }">▸</span>
              <span class="section-label">🌲 {{ t('tree.browse') }}</span>
            </div>

            <div v-if="stateFor(conn.id).browseOpen" class="browse-controls">
              <label class="ctl">
                <span>{{ t('valuePanel.mode') }}</span>
                <select v-model="accessMode" class="ctl-input">
                  <option value="Subscription">Subscription</option>
                  <option value="Polling">Polling</option>
                </select>
              </label>
              <label class="ctl">
                <span>{{ t('history.intervalMs') }}</span>
                <input v-model.number="intervalMs" class="ctl-input num" type="number" min="100" max="60000" />
              </label>
              <label class="ctl">
                <span>深度</span>
                <input v-model.number="maxDepth" class="ctl-input num" type="number" min="1" max="10" />
              </label>
            </div>

            <div v-if="stateFor(conn.id).browseOpen" class="browse-tree">
              <div v-if="stateFor(conn.id).browse.loadingRoot" class="browse-hint">
                {{ t('tree.loadingRoot') }}
              </div>
              <div
                v-else-if="stateFor(conn.id).browse.rootLoaded && stateFor(conn.id).browse.roots.length === 0"
                class="browse-hint"
              >{{ t('tree.emptyRoot') }}</div>

              <template v-for="row in flattenBrowse(stateFor(conn.id).browse)" :key="row.nodeId">
                <div class="browse-row" :style="{ paddingLeft: `${4 + row.depth * 14}px` }">
                  <button
                    v-if="row.hasChildren"
                    class="toggle"
                    :class="{ open: row.expanded }"
                    @click="toggleBrowseNode(conn.id, row.nodeId)"
                  >▸</button>
                  <span v-else class="toggle-spacer" />

                  <input
                    v-if="row.item.node_class === 'Variable'"
                    type="checkbox"
                    :checked="isSelected(conn.id, row.nodeId)"
                    @click.stop
                    @change="toggleSelect(conn.id, row.nodeId)"
                  />

                  <span
                    class="browse-label"
                    :class="{ selected: selectedNodeId === row.nodeId }"
                    :title="row.nodeId"
                    @click="onBrowseLabel(conn.id, row)"
                  >{{ nodeIcon(row.item.node_class) }} {{ row.item.display_name }}
                    <span v-if="row.item.data_type" class="type-hint">: {{ row.item.data_type }}</span>
                  </span>

                  <span v-if="row.loading" class="row-busy">…</span>

                  <span class="row-actions">
                    <button
                      v-if="row.hasChildren"
                      class="row-action"
                      :title="t('tree.addAllVariables')"
                      @click="addAllUnderNode(conn.id, row.nodeId)"
                    >＋</button>
                    <button
                      v-if="row.item.node_class === 'Variable'"
                      class="row-action"
                      :title="t('tree.viewHistory')"
                      @click="openHistory(conn.id, row.nodeId, row.item.display_name)"
                    >📈</button>
                    <button
                      v-if="row.item.node_class === 'Method'"
                      class="row-action"
                      :title="t('tree.callMethod')"
                      @click="openMethodCall(conn.id, row.nodeId)"
                    >⚙</button>
                  </span>
                </div>
              </template>

              <div
                v-if="stateFor(conn.id).browse.selected.size > 0"
                class="browse-footer"
              >
                <span>{{ t('tree.selected', { count: stateFor(conn.id).browse.selected.size }) }}</span>
                <button class="add-btn" @click="addSelectedVariables(conn.id)">
                  {{ t('common.add') }} ({{ stateFor(conn.id).browse.selected.size }})
                </button>
              </div>
            </div>
          </template>

          <template v-if="selectedConnectionId === conn.id">
            <div class="section-row">
              <span class="section-spacer" />
              <span class="section-label">📡 {{ t('tree.monitored') }} ({{ subNodes.length }})</span>
            </div>
            <div v-for="row in subNodes" :key="row.node_id" class="leaf-row" @click="selectNode(row.node_id)">
              <span class="leaf-id mono">{{ row.node_id }}</span>
            </div>

            <div class="section-row">
              <span class="section-spacer" />
              <span class="section-label">⏱ {{ t('tree.polling') }} ({{ pollNodes.length }})</span>
            </div>
            <div v-for="row in pollNodes" :key="row.node_id" class="leaf-row" @click="selectNode(row.node_id)">
              <span class="leaf-id mono">{{ row.node_id }}</span>
            </div>
          </template>
        </div>
      </template>

      <div class="tree-sep group-sep" />
      <div class="tree-title">{{ t('tree.groups') }}</div>

      <div class="group-create">
        <input v-model="groupInput" class="group-input" :placeholder="t('tree.groupNameHint')" @keydown.enter="onAddGroup" />
        <button class="group-add" :disabled="!groupInput.trim()" @click="onAddGroup">＋</button>
      </div>

      <div v-if="groups.length === 0" class="tree-empty small">{{ t('tree.noGroups') }}</div>

      <div v-for="group in groups" :key="group.id" class="group-item">
        <div class="group-row">
          <button class="toggle" :class="{ open: expandedGroups.has(group.id) }" @click="toggleGroup(group.id)">▸</button>
          <span class="group-label" @click="toggleGroup(group.id)">· {{ group.name }} ({{ group.node_ids.length }})</span>
          <button class="group-del" :title="t('common.delete')" @click="onDeleteGroup(group.id)">🗑</button>
        </div>
        <div v-if="expandedGroups.has(group.id)" class="group-children">
          <div v-for="nodeId in group.node_ids" :key="nodeId" class="leaf-row" @click="selectNode(nodeId)">
            <span class="leaf-id mono">{{ nodeId }}</span>
          </div>
        </div>
      </div>
    </div>

    <MethodCallDialog
      :visible="methodCallState !== null"
      :connection-id="methodCallState?.connectionId ?? ''"
      :object-id="methodCallState?.objectId ?? ''"
      :method-id="methodCallState?.methodId ?? ''"
      :display-name="methodCallState?.displayName ?? ''"
      @close="methodCallState = null"
    />
  </div>
</template>

<style scoped>
.conn-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--c-mantle);
  overflow: hidden;
  user-select: none;
}

.tree-title {
  padding: 10px 12px 6px;
  font-size: 11px;
  font-weight: 700;
  color: var(--c-overlay0);
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.tree-sep {
  height: 1px;
  margin: 0 10px;
  background: var(--c-surface0);
}

.group-sep {
  margin-top: 12px;
}

.tree-empty {
  padding: 24px 12px;
  color: var(--c-overlay0);
  font-size: 12px;
  text-align: center;
}
.tree-empty.small { padding: 8px 12px; }

.tree-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 4px 4px 12px;
}

.conn-row {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 6px;
  border-radius: 4px;
  cursor: pointer;
}
.conn-row:hover { background: var(--c-surface0); }
.conn-row.selected { background: var(--c-surface1); }

.state-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
}

.conn-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 600;
  color: var(--c-text);
}

.conn-state {
  font-size: 10px;
  color: var(--c-overlay0);
  white-space: nowrap;
}

.conn-children {
  margin: 2px 0 6px 14px;
  border-left: 1px solid var(--c-surface0);
  padding-left: 6px;
}

.conn-meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 2px 0 6px;
  font-size: 10px;
}
.conn-meta .mono { color: var(--c-subtext0); overflow-wrap: anywhere; }
.conn-meta .muted { color: var(--c-overlay0); }

.section-row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  cursor: pointer;
}
.section-row:hover { background: var(--c-surface0); }
.section-spacer { width: 16px; flex: none; }

.section-label {
  font-size: 12px;
  color: var(--c-subtext1);
  font-weight: 500;
}

.toggle {
  flex: none;
  width: 16px;
  height: 16px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 11px;
  line-height: 1;
  transition: transform 120ms ease;
}
.toggle.open { transform: rotate(90deg); }
.toggle-spacer { flex: none; width: 16px; }

.browse-controls {
  display: flex;
  gap: 8px;
  padding: 4px 0 6px 18px;
  flex-wrap: wrap;
}

.ctl {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--c-overlay0);
}

.ctl-input {
  padding: 2px 4px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 11px;
  width: 88px;
  box-sizing: border-box;
}
.ctl-input.num { width: 60px; }

.browse-tree {
  padding: 2px 0 6px;
}

.browse-hint {
  padding: 6px 4px;
  font-size: 11px;
  color: var(--c-overlay0);
}

.browse-row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding-right: 4px;
  border-radius: 4px;
}
.browse-row:hover { background: var(--c-surface0); }

.browse-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--c-subtext1);
  cursor: pointer;
}
.browse-label:hover { color: var(--c-text); }
.browse-label.selected { color: var(--c-blue); }

.type-hint { color: var(--c-overlay0); font-size: 10px; }

.row-busy { font-size: 10px; color: var(--c-yellow); }

.row-actions {
  display: inline-flex;
  gap: 2px;
  opacity: 0;
}
.browse-row:hover .row-actions { opacity: 1; }

.row-action {
  flex: none;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 11px;
  border-radius: 3px;
}
.row-action:hover { background: var(--c-surface1); color: var(--c-text); }

.browse-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 0 0 18px;
  font-size: 11px;
  color: var(--c-subtext0);
}

.add-btn {
  padding: 3px 10px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-blue);
  color: var(--c-base);
  cursor: pointer;
  font-size: 11px;
}
.add-btn:hover { background: var(--c-sapphire); }

.leaf-row {
  padding: 4px 6px 4px 28px;
  font-size: 11px;
  cursor: pointer;
}
.leaf-row:hover { background: var(--c-surface0); }

.leaf-id { color: var(--c-subtext0); }

.mono { font-family: var(--font-mono); }

.group-create {
  display: flex;
  gap: 4px;
  padding: 6px 10px;
}

.group-input {
  flex: 1 1 auto;
  min-width: 0;
  padding: 4px 8px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
}
.group-input:focus { outline: none; border-color: var(--c-blue); }

.group-add {
  flex: none;
  padding: 0 10px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
}
.group-add:disabled { opacity: 0.4; cursor: default; }

.group-item { margin-top: 2px; }

.group-row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 6px;
  border-radius: 4px;
}
.group-row:hover { background: var(--c-surface0); }

.group-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--c-text);
  cursor: pointer;
}

.group-del {
  flex: none;
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 11px;
  border-radius: 3px;
  opacity: 0;
}
.group-row:hover .group-del { opacity: 1; }
.group-del:hover { color: var(--c-red); }

.group-children {
  margin-left: 16px;
  border-left: 1px solid var(--c-surface0);
}
</style>
