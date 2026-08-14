<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from '@shared/i18n'
import { showConfirm } from '@shared/composables/useDialog'
import EmptyState from '@shared/components/EmptyState.vue'
import { formatHms, isComplexValue, qualityColor, truncateSafe } from '../domain'
import { useMasterContext } from '../inject'
import type { MonitoredRow } from '../types'

const { t } = useI18n()
const {
  selectedConnectionId,
  monitoredRows,
  selectedNodeId,
  selectNode,
  groups,
  removeMonitoredNodes,
  addToGroup,
  openHistory,
} = useMasterContext()

const search = ref('')
const multiSelected = ref<Set<string>>(new Set())
const selectedGroupId = ref('')

const rows = computed<MonitoredRow[]>(() => {
  const query = search.value.trim().toLowerCase()
  const all = [...monitoredRows.value.values()]
  if (!query) return all
  return all.filter((r) => {
    const hay = `${r.node_id} ${r.display_name} ${r.value ?? ''}`.toLowerCase()
    return hay.includes(query)
  })
})

const selectedCount = computed(() => multiSelected.value.size)

function valuePreview(row: MonitoredRow): string {
  const v = row.value ?? '—'
  if (v !== '—' && isComplexValue(v)) return truncateSafe(v, 18)
  return v
}

function onRowClick(row: MonitoredRow, event: MouseEvent) {
  const additive = event.metaKey || event.ctrlKey
  if (additive) {
    if (multiSelected.value.has(row.node_id)) multiSelected.value.delete(row.node_id)
    else multiSelected.value.add(row.node_id)
  } else {
    multiSelected.value = new Set([row.node_id])
    selectNode(row.node_id)
  }
}

function toggleMulti(nodeId: string) {
  if (multiSelected.value.has(nodeId)) multiSelected.value.delete(nodeId)
  else multiSelected.value.add(nodeId)
}

async function removeOne(nodeId: string) {
  if (!selectedConnectionId.value) return
  if (!(await showConfirm(t('dataTable.remove')))) return
  await removeMonitoredNodes(selectedConnectionId.value, [nodeId])
  multiSelected.value.delete(nodeId)
  if (selectedNodeId.value === nodeId) selectNode(null)
}

async function removeSelected() {
  if (!selectedConnectionId.value || multiSelected.value.size === 0) return
  if (!(await showConfirm(t('dataTable.removeSelected')))) return
  const ids = [...multiSelected.value]
  await removeMonitoredNodes(selectedConnectionId.value, ids)
  for (const id of ids) {
    if (selectedNodeId.value === id) selectNode(null)
  }
  multiSelected.value = new Set()
}

async function onAddToGroup() {
  if (!selectedGroupId.value || multiSelected.value.size === 0) return
  await addToGroup(selectedGroupId.value, [...multiSelected.value])
  multiSelected.value = new Set()
}

function onHistory(row: MonitoredRow) {
  if (selectedConnectionId.value) {
    openHistory(selectedConnectionId.value, row.node_id, row.display_name)
  }
}
</script>

<template>
  <main class="data-table">
    <div class="table-head">
      <span class="head-title">{{ t('dataTable.title') }}</span>
      <span class="head-count">{{ t('dataTable.count', { count: rows.length }) }}</span>

      <input v-model="search" class="search" type="search" :placeholder="t('dataTable.searchPlaceholder')" />

      <template v-if="selectedCount > 0">
        <span class="head-multi">{{ t('dataTable.selectedCount', { count: selectedCount }) }}</span>
        <button class="head-action" @click="removeSelected">{{ t('dataTable.removeSelected') }}</button>

        <template v-if="groups.length > 0">
          <select v-model="selectedGroupId" class="group-select">
            <option value="" disabled>{{ t('dataTable.addToGroup') }}</option>
            <option v-for="g in groups" :key="g.id" :value="g.id">{{ g.name }} ({{ g.node_ids.length }})</option>
          </select>
          <button class="head-action" :disabled="!selectedGroupId" @click="onAddToGroup">
            {{ t('common.add') }}
          </button>
        </template>
        <span v-else class="head-multi muted">{{ t('dataTable.noGroups') }}</span>
      </template>
    </div>
    <div class="table-sep" />

    <EmptyState
      v-if="!selectedConnectionId"
      :title="t('valuePanel.emptyTitle')"
      :hint="t('dataTable.emptyHint')"
    >
      <span>📡</span>
    </EmptyState>
    <EmptyState v-else-if="rows.length === 0" :title="t('dataTable.emptyTitle')" :hint="t('dataTable.emptyHint')">
      <span>🌲</span>
    </EmptyState>

    <div v-else class="table-scroll">
      <table>
        <thead>
          <tr>
            <th class="col-check"></th>
            <th>{{ t('dataTable.colNodeId') }}</th>
            <th>{{ t('dataTable.colName') }}</th>
            <th>{{ t('dataTable.colType') }}</th>
            <th>{{ t('dataTable.colValue') }}</th>
            <th>{{ t('dataTable.colQuality') }}</th>
            <th>{{ t('dataTable.colSrcTs') }}</th>
            <th>{{ t('dataTable.colSrvTs') }}</th>
            <th>{{ t('dataTable.colMode') }}</th>
            <th class="col-actions"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="row in rows"
            :key="row.node_id"
            :class="{ selected: multiSelected.has(row.node_id) || selectedNodeId === row.node_id }"
            @click="onRowClick(row, $event)"
          >
            <td class="col-check" @click.stop>
              <input
                type="checkbox"
                :checked="multiSelected.has(row.node_id)"
                @change="toggleMulti(row.node_id)"
              />
            </td>
            <td class="mono" :title="row.node_id">{{ row.node_id }}</td>
            <td class="cell-name">{{ row.display_name }}</td>
            <td>{{ row.data_type }}</td>
            <td class="mono" :title="row.value ?? ''">{{ valuePreview(row) }}</td>
            <td :style="{ color: qualityColor(row.quality) }">{{ row.quality ?? '' }}</td>
            <td class="mono">{{ formatHms(row.source_timestamp) }}</td>
            <td class="mono">{{ formatHms(row.server_timestamp) }}</td>
            <td>{{ row.access_mode }} · {{ Math.round(row.interval_ms) }}ms</td>
            <td class="col-actions" @click.stop>
              <button class="row-action" :title="t('tree.viewHistory')" @click="onHistory(row)">
                📈
              </button>
              <button class="row-action" :title="t('dataTable.remove')" @click="removeOne(row.node_id)">🗑</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </main>
</template>

<style scoped>
.data-table {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--c-crust);
  overflow: hidden;
}

.table-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px 6px;
  flex-wrap: wrap;
}

.head-title {
  font-size: 12px;
  font-weight: 700;
  color: var(--c-text);
}

.head-count {
  font-size: 11px;
  color: var(--c-overlay0);
}

.search {
  margin-left: auto;
  width: 220px;
  padding: 4px 8px;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
}
.search:focus { outline: none; border-color: var(--c-blue); }

.head-multi {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--c-blue);
}
.head-multi.muted { color: var(--c-overlay0); }

.head-action {
  padding: 2px 8px;
  border: 1px solid var(--c-surface1);
  background: var(--c-surface0);
  color: var(--c-red);
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
}
.head-action:disabled { opacity: 0.4; cursor: default; }

.group-select {
  padding: 2px 6px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 11px;
}

.table-sep {
  height: 1px;
  margin: 0 10px;
  background: var(--c-surface0);
}

.table-scroll {
  flex: 1;
  overflow: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

thead th {
  position: sticky;
  top: 0;
  background: var(--c-base);
  color: var(--c-overlay0);
  font-weight: 600;
  text-align: left;
  padding: 6px 10px;
  border-bottom: 1px solid var(--c-surface0);
  white-space: nowrap;
  z-index: 1;
}

tbody td {
  padding: 5px 10px;
  border-bottom: 1px solid var(--c-surface0);
  color: var(--c-subtext1);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 0;
}

tbody tr { cursor: pointer; }
tbody tr:hover { background: var(--c-surface0); }
tbody tr.selected { background: var(--c-surface1); }

.cell-name {
  color: var(--c-text);
  font-weight: 500;
}

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-subtext0);
}

.col-check { width: 28px; }
.col-actions { width: 60px; }

.row-action {
  border: none;
  background: transparent;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 12px;
  opacity: 0;
  margin-right: 2px;
}
tbody tr:hover .row-action { opacity: 1; }
.row-action:hover { color: var(--c-text); }
</style>
