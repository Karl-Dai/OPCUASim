<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from '@shared/i18n'
import { showConfirm } from '@shared/composables/useDialog'
import EmptyState from '@shared/components/EmptyState.vue'
import { useServerContext } from '../inject'
import { dataTypeLabel, simulationLabel } from '../domain'
import type { NodeRow } from '../types'

const { t } = useI18n()
const { addressSpace, selectedNodeId, selectNode, currentValues, removeNode } = useServerContext()

const multiSelected = ref(new Set<string>())

function valueFor(node: NodeRow): string {
  return currentValues.value.get(node.node_id) ?? node.current_value ?? '—'
}

function isSelected(node: NodeRow): boolean {
  return selectedNodeId.value === node.node_id
}

function toggleMulti(nodeId: string) {
  if (multiSelected.value.has(nodeId)) multiSelected.value.delete(nodeId)
  else multiSelected.value.add(nodeId)
}

async function removeOne(nodeId: string) {
  if (await showConfirm(t('nodeTable.remove'))) {
    await removeNode(nodeId)
    multiSelected.value.delete(nodeId)
  }
}

async function removeSelected() {
  if (multiSelected.value.size === 0) return
  if (!(await showConfirm(t('nodeTable.removeSelected')))) return
  const ids = [...multiSelected.value]
  multiSelected.value.clear()
  for (const id of ids) {
    await removeNode(id)
  }
}
</script>

<template>
  <main class="node-table">
    <div class="table-head">
      <span class="head-title">{{ t('nodeTable.title') }}</span>
      <span class="head-count">{{ t('nodeTable.count', { count: addressSpace.nodes.length }) }}</span>
      <span v-if="multiSelected.size > 1" class="head-multi">
        {{ t('nodeTable.selectedCount', { count: multiSelected.size }) }}
        <button class="head-action" @click="removeSelected">{{ t('nodeTable.removeSelected') }}</button>
      </span>
    </div>
    <div class="table-sep" />

    <EmptyState
      v-if="addressSpace.nodes.length === 0"
      :title="t('nodeTable.emptyTitle')"
      :hint="t('nodeTable.emptyHint')"
    >
      <span>📊</span>
    </EmptyState>

    <div v-else class="table-scroll">
      <table>
        <thead>
          <tr>
            <th class="col-check"></th>
            <th>{{ t('nodeTable.colName') }}</th>
            <th>{{ t('nodeTable.colNodeId') }}</th>
            <th>{{ t('nodeTable.colDataType') }}</th>
            <th>{{ t('nodeTable.colSimMode') }}</th>
            <th>{{ t('nodeTable.colValue') }}</th>
            <th>{{ t('nodeTable.colRw') }}</th>
            <th class="col-actions"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="node in addressSpace.nodes"
            :key="node.node_id"
            :class="{ selected: isSelected(node) }"
            @click="selectNode(node.node_id)"
          >
            <td class="col-check" @click.stop>
              <input
                type="checkbox"
                :checked="multiSelected.has(node.node_id)"
                @change="toggleMulti(node.node_id)"
              />
            </td>
            <td class="cell-name">{{ node.display_name }}</td>
            <td class="mono">{{ node.node_id }}</td>
            <td>{{ dataTypeLabel(node.data_type) }}</td>
            <td>{{ simulationLabel(node.simulation) }}</td>
            <td class="mono">{{ valueFor(node) }}</td>
            <td>
              <span :class="node.writable ? 'rw' : 'ro'">{{ node.writable ? 'RW' : 'R' }}</span>
            </td>
            <td class="col-actions" @click.stop>
              <button class="row-action" :title="t('nodeTable.remove')" @click="removeOne(node.node_id)">
                🗑
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </main>
</template>

<style scoped>
.node-table {
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

.head-multi {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--c-blue);
}

.head-action {
  padding: 2px 8px;
  border: 1px solid var(--c-surface1);
  background: var(--c-surface0);
  color: var(--c-red);
  border-radius: 4px;
  cursor: pointer;
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

tbody tr {
  cursor: pointer;
}

tbody tr:hover {
  background: var(--c-surface0);
}

tbody tr.selected {
  background: var(--c-surface1);
}

.cell-name {
  color: var(--c-text);
  font-weight: 500;
}

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-subtext0);
}

.col-check {
  width: 28px;
}

.col-actions {
  width: 34px;
}

.rw {
  color: var(--c-blue);
  font-weight: 600;
}

.ro {
  color: var(--c-overlay0);
}

.row-action {
  border: none;
  background: transparent;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 12px;
  opacity: 0;
}

tbody tr:hover .row-action {
  opacity: 1;
}

.row-action:hover {
  color: var(--c-red);
}
</style>
