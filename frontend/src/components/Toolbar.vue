<script setup lang="ts">
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import LangSwitch from '@shared/components/LangSwitch.vue'
import VersionBadge from '@shared/components/VersionBadge.vue'
import ConfigDialog from './ConfigDialog.vue'
import { useServerContext } from '../inject'
import {
  SCALAR_DATA_TYPES,
  SIM_KINDS,
  defaultSimulation,
  nodeIdFromName,
  simKindLabel,
} from '../domain'
import type { SimKind } from '../domain'
import type { AddNodeRequest, DataType } from '../types'

const { t } = useI18n()
const { status, addFolder, addNode, refreshAll } = useServerContext()

const stateView = computed(() => {
  switch (status.value.state) {
    case 'Running':
      return { icon: '●', color: 'var(--c-green)', label: t('serverState.running') }
    case 'Starting':
      return { icon: '◐', color: 'var(--c-yellow)', label: t('serverState.starting') }
    case 'Stopping':
      return { icon: '◑', color: 'var(--c-yellow)', label: t('serverState.stopping') }
    case 'Stopped':
      return { icon: '○', color: 'var(--c-red)', label: t('serverState.stopped') }
    default:
      return { icon: '·', color: 'var(--c-overlay0)', label: status.value.state }
  }
})

const running = computed(() => status.value.state === 'Running')
const starting = computed(() => status.value.state === 'Starting')

async function onStart() {
  try {
    await invoke('start_server')
  } catch (error) {
    await showAlert(String(error))
  }
}

async function onStop() {
  try {
    await invoke('stop_server')
  } catch (error) {
    await showAlert(String(error))
  }
}

const configVisible = ref(false)

const newFolderName = ref('')

async function onAddFolder() {
  const name = newFolderName.value.trim()
  if (!name) return
  await addFolder({ node_id: nodeIdFromName(name), display_name: name, parent_id: 'Objects' })
  newFolderName.value = ''
}

const nodeName = ref('')
const nodeDataType = ref<DataType>('Double')
const nodeSimKind = ref<SimKind>('Random')
const nodeWritable = ref(false)

async function onAddNode() {
  const name = nodeName.value.trim()
  if (!name) return
  const request: AddNodeRequest = {
    node_id: nodeIdFromName(name),
    display_name: name,
    parent_id: 'Objects',
    data_type: nodeDataType.value,
    writable: nodeWritable.value,
    simulation: defaultSimulation(nodeSimKind.value),
    eu_range_low: 0,
    eu_range_high: 100,
  }
  await addNode(request)
  nodeName.value = ''
}

async function onSave() {
  const path = await save({
    defaultPath: 'server.opcuaproj',
    filters: [{ name: 'OPC UA Project', extensions: ['opcuaproj'] }],
  })
  if (!path) return
  try {
    await invoke('save_project', { path })
    await showAlert(t('project.saved', { path }))
  } catch (error) {
    await showAlert(t('project.saveFailed', { error: String(error) }))
  }
}

async function onLoad() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: 'OPC UA Project', extensions: ['opcuaproj'] }],
  })
  const path = Array.isArray(selected) ? selected[0] : selected
  if (!path) return
  try {
    await invoke('load_project', { path })
    await refreshAll()
    await showAlert(t('project.loaded', { path }))
  } catch (error) {
    await showAlert(t('project.loadFailed', { error: String(error) }))
  }
}
</script>

<template>
  <header class="server-toolbar">
    <div class="tb-row">
      <div class="tb-main">
        <span class="tb-title">OPCUAServer</span>
        <span class="chip" :style="{ color: stateView.color }">
          <span class="chip-icon">{{ stateView.icon }}</span>
          <span>{{ stateView.label }}</span>
        </span>
        <button
          class="toolbar-btn btn-start"
          :disabled="running || starting"
          @click="onStart"
        >▶ {{ t('toolbar.start') }}</button>
        <button class="toolbar-btn btn-stop" :disabled="!running" @click="onStop">
          ■ {{ t('toolbar.stop') }}
        </button>
        <span class="tb-divider" />
        <span class="tb-label">{{ t('toolbar.endpoint') }}</span>
        <span class="tb-endpoint">{{ status.endpoint_url }}</span>
      </div>
      <div class="tb-aside">
        <button class="toolbar-btn" @click="configVisible = true">{{ t('toolbar.config') }}</button>
        <button class="toolbar-btn" @click="onLoad">{{ t('toolbar.openProject') }}</button>
        <button class="toolbar-btn" @click="onSave">{{ t('toolbar.saveProject') }}</button>
        <LangSwitch />
        <VersionBadge />
      </div>
    </div>

    <div class="tb-row">
      <div class="tb-main">
        <span class="tb-label">{{ t('toolbar.newFolder') }}</span>
        <input
          v-model="newFolderName"
          class="tb-input"
          :placeholder="t('toolbar.folderNameHint')"
          @keydown.enter="onAddFolder"
        />
        <button
          class="toolbar-btn"
          :disabled="!newFolderName.trim()"
          @click="onAddFolder"
        >{{ t('toolbar.addFolder') }}</button>

        <span class="tb-divider" />

        <span class="tb-label">{{ t('toolbar.newNode') }}</span>
        <input
          v-model="nodeName"
          class="tb-input"
          :placeholder="t('toolbar.nodeNameHint')"
          @keydown.enter="onAddNode"
        />
        <select v-model="nodeDataType" class="tb-select">
          <option v-for="dt in SCALAR_DATA_TYPES" :key="String(dt)" :value="dt">{{ dt }}</option>
        </select>
        <select v-model="nodeSimKind" class="tb-select">
          <option v-for="kind in SIM_KINDS" :key="kind" :value="kind">{{ simKindLabel(kind) }}</option>
        </select>
        <label class="tb-check" :title="t('toolbar.writable')">
          <input v-model="nodeWritable" type="checkbox" />
          {{ t('toolbar.writable') }}
        </label>
        <button class="toolbar-btn" :disabled="!nodeName.trim()" @click="onAddNode">
          {{ t('toolbar.addNode') }}
        </button>
      </div>
    </div>

    <ConfigDialog :visible="configVisible" @close="configVisible = false" />
  </header>
</template>

<style scoped>
.server-toolbar {
  display: flex;
  flex-direction: column;
  background: var(--c-base);
  border-bottom: 1px solid var(--c-surface0);
  user-select: none;
}

.tb-row {
  display: flex;
  align-items: center;
  min-height: 34px;
  padding: 0 8px;
  gap: 6px;
}

.tb-row + .tb-row {
  border-top: 1px solid var(--c-surface0);
}

.tb-main {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  overflow-x: auto;
  scrollbar-width: thin;
}

.tb-aside {
  flex: none;
  display: flex;
  align-items: center;
  gap: 2px;
  padding-left: 4px;
}

.tb-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--c-overlay0);
  padding-right: 4px;
  white-space: nowrap;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-weight: 600;
  font-size: 12px;
  white-space: nowrap;
  padding: 0 2px;
}

.chip-icon {
  font-size: 11px;
}

.tb-divider {
  width: 1px;
  height: 20px;
  background: var(--c-surface0);
  margin: 0 4px;
  flex: none;
}

.tb-label {
  font-size: 12px;
  color: var(--c-subtext0);
  white-space: nowrap;
}

.tb-endpoint {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-subtext0);
  white-space: nowrap;
}

.tb-input {
  width: 120px;
  padding: 3px 8px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
  box-sizing: border-box;
}

.tb-input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.tb-select {
  padding: 3px 6px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
}

.tb-check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--c-subtext0);
  cursor: pointer;
  white-space: nowrap;
}
</style>
