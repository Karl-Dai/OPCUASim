<script setup lang="ts">
import { computed, reactive } from 'vue'
import { useI18n } from '@shared/i18n'
import { showConfirm, showPrompt } from '@shared/composables/useDialog'
import EmptyState from '@shared/components/EmptyState.vue'
import { useServerContext } from '../inject'
import { subfolderNodeId } from '../domain'
import type { AddressChild } from '../domain'
import type { AddressSpace } from '../types'

const { t } = useI18n()
const { addressSpace, selectedNodeId, selectNode, addFolder, removeNode } = useServerContext()

const ROOT_ID = 'Objects'
const expanded = reactive(new Set<string>([ROOT_ID]))

function buildIndex(space: AddressSpace): Map<string, AddressChild[]> {
  const index = new Map<string, AddressChild[]>()
  for (const folder of space.folders) {
    const list = index.get(folder.parent_id) ?? []
    list.push({ kind: 'folder', node_id: folder.node_id, display_name: folder.display_name })
    index.set(folder.parent_id, list)
  }
  for (const node of space.nodes) {
    const list = index.get(node.parent_id) ?? []
    list.push({ kind: 'node', node_id: node.node_id, display_name: node.display_name })
    index.set(node.parent_id, list)
  }
  return index
}

const index = computed(() => buildIndex(addressSpace.value))

const isEmpty = computed(
  () => addressSpace.value.folders.length === 0 && addressSpace.value.nodes.length === 0,
)

interface FlatRow {
  child: AddressChild
  depth: number
}

const rows = computed<FlatRow[]>(() => {
  const out: FlatRow[] = []
  const walk = (parentId: string, depth: number) => {
    for (const child of index.value.get(parentId) ?? []) {
      out.push({ child, depth })
      if (child.kind === 'folder' && expanded.has(child.node_id)) {
        walk(child.node_id, depth + 1)
      }
    }
  }
  if (expanded.has(ROOT_ID)) walk(ROOT_ID, 0)
  return out
})

function toggleFolder(id: string) {
  if (expanded.has(id)) expanded.delete(id)
  else expanded.add(id)
}

async function addSubfolder(parentId: string) {
  const name = await showPrompt(t('addressTree.newSubfolder'), '')
  if (!name || !name.trim()) return
  await addFolder({
    node_id: subfolderNodeId(parentId, name.trim()),
    display_name: name.trim(),
    parent_id: parentId,
  })
}

async function removeFolder(id: string) {
  if (await showConfirm(t('addressTree.deleteFolder'))) {
    await removeNode(id)
  }
}

async function removeSelectedNode(id: string) {
  if (await showConfirm(t('addressTree.deleteNode'))) {
    await removeNode(id)
  }
}

function onLabelClick(child: AddressChild) {
  if (child.kind === 'node') selectNode(child.node_id)
}

function onRemove(child: AddressChild) {
  if (child.kind === 'folder') {
    void removeFolder(child.node_id)
  } else {
    void removeSelectedNode(child.node_id)
  }
}
</script>

<template>
  <aside class="address-tree">
    <div class="tree-title">{{ t('addressTree.title') }}</div>
    <div class="tree-sep" />

    <EmptyState v-if="isEmpty" compact :title="t('addressTree.emptyTitle')" :hint="t('addressTree.emptyHint')">
      <span>🗂</span>
    </EmptyState>

    <div v-else class="tree-scroll">
      <div class="tree-row root">
        <button
          class="toggle"
          :class="{ open: expanded.has(ROOT_ID) }"
          @click="toggleFolder(ROOT_ID)"
        >▸</button>
        <span class="label folder">📁 Objects</span>
        <button class="action" title="+" @click="addSubfolder(ROOT_ID)">＋</button>
      </div>

      <template v-if="expanded.has(ROOT_ID)">
        <div
          v-for="row in rows"
          :key="`${row.child.kind}-${row.child.node_id}`"
          class="tree-row"
          :style="{ paddingLeft: `${8 + row.depth * 14}px` }"
          :class="{ selected: selectedNodeId === row.child.node_id }"
        >
          <button
            v-if="row.child.kind === 'folder'"
            class="toggle"
            :class="{ open: expanded.has(row.child.node_id) }"
            @click="toggleFolder(row.child.node_id)"
          >▸</button>
          <span v-else class="toggle-spacer" />

          <span
            class="label"
            :class="row.child.kind"
            :title="row.child.node_id"
            @click="onLabelClick(row.child)"
          >{{ row.child.kind === 'folder' ? '📁' : '📊' }} {{ row.child.display_name }}</span>

          <button
            v-if="row.child.kind === 'folder'"
            class="action"
            :title="t('addressTree.newSubfolder')"
            @click="addSubfolder(row.child.node_id)"
          >＋</button>
          <button
            class="action danger"
            :title="row.child.kind === 'folder' ? t('addressTree.deleteFolder') : t('addressTree.deleteNode')"
            @click="onRemove(row.child)"
          >🗑</button>
        </div>
      </template>
    </div>
  </aside>
</template>

<style scoped>
.address-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--c-mantle);
  overflow: hidden;
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

.tree-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 8px 4px;
}

.tree-row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding-right: 6px;
  border-radius: 4px;
  user-select: none;
}

.tree-row:hover {
  background: var(--c-surface0);
}

.tree-row.selected {
  background: var(--c-surface1);
}

.tree-row.root {
  font-weight: 600;
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

.toggle.open {
  transform: rotate(90deg);
}

.toggle-spacer {
  flex: none;
  width: 16px;
}

.label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--c-subtext1);
}

.label.node {
  cursor: pointer;
}

.label.node:hover {
  color: var(--c-text);
}

.action {
  flex: none;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 12px;
  line-height: 1;
  border-radius: 3px;
  opacity: 0;
}

.tree-row:hover .action {
  opacity: 1;
}

.action:hover {
  background: var(--c-surface1);
  color: var(--c-text);
}

.action.danger:hover {
  color: var(--c-red);
}
</style>
