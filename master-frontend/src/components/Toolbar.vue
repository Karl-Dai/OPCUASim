<script setup lang="ts">
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useI18n } from '@shared/i18n'
import { showAlert, showConfirm, showPrompt } from '@shared/composables/useDialog'
import LangSwitch from '@shared/components/LangSwitch.vue'
import VersionBadge from '@shared/components/VersionBadge.vue'
import NewConnectionDialog from './NewConnectionDialog.vue'
import CertManagerDialog from './CertManagerDialog.vue'
import { useMasterContext } from '../inject'
import type { ConnectionInfo, DiscoveredEndpointDto } from '../types'

const { t } = useI18n()
const {
  selectedConnectionId,
  selectedConnection,
  connect,
  disconnect,
  deleteConnection,
  refreshConnections,
  refreshGroups,
  saveProject,
  loadProject,
  createGroup,
  selectConnection,
} = useMasterContext()

const stateView = computed(() => {
  const state = selectedConnection.value?.state ?? ''
  switch (state) {
    case 'Connected':
      return { icon: '●', color: 'var(--c-green)', label: t('state.connected') }
    case 'Connecting':
      return { icon: '◐', color: 'var(--c-yellow)', label: t('state.connecting') }
    case 'Reconnecting':
      return { icon: '◑', color: 'var(--c-peach)', label: t('state.reconnecting') }
    case 'Disconnected':
      return { icon: '○', color: 'var(--c-red)', label: t('state.disconnected') }
    default:
      return { icon: '·', color: 'var(--c-overlay0)', label: state }
  }
})

const isConnected = computed(() => selectedConnection.value?.state === 'Connected')
const isConnecting = computed(
  () => selectedConnection.value?.state === 'Connecting' || selectedConnection.value?.state === 'Reconnecting',
)
const hasSelection = computed(() => selectedConnectionId.value !== null)

const newConnVisible = ref(false)
const certVisible = ref(false)

async function onConnect() {
  if (!selectedConnectionId.value) return
  try {
    await connect(selectedConnectionId.value)
  } catch (error) {
    await showAlert(String(error))
  }
}

async function onDisconnect() {
  if (!selectedConnectionId.value) return
  try {
    await disconnect(selectedConnectionId.value)
  } catch (error) {
    await showAlert(String(error))
  }
}

async function onDelete() {
  if (!selectedConnectionId.value) return
  if (!(await showConfirm(t('toolbar.confirmDeleteConnection')))) return
  const id = selectedConnectionId.value
  try {
    await deleteConnection(id)
    await refreshConnections()
    await refreshGroups()
  } catch (error) {
    await showAlert(String(error))
  }
}

async function onRefresh() {
  await Promise.all([refreshConnections(), refreshGroups()])
}

async function onSave() {
  const path = await save({
    defaultPath: 'master.opcuaproj',
    filters: [{ name: 'OPC UA Project', extensions: ['opcuaproj'] }],
  })
  if (!path) return
  try {
    await saveProject(path)
    await showAlert(t('toolbar.projectSaved', { path }))
  } catch (error) {
    await showAlert(t('toolbar.projectSaveFailed', { error: String(error) }))
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
    await loadProject(path)
    await onRefresh()
    await showAlert(t('toolbar.projectLoaded', { path }))
  } catch (error) {
    await showAlert(t('toolbar.projectLoadFailed', { error: String(error) }))
  }
}

async function onDiscover() {
  const url = await showPrompt(t('toolbar.discoverPrompt'), 'opc.tcp://localhost:4840')
  if (!url || !url.trim()) return
  try {
    const endpoints = await invoke<DiscoveredEndpointDto[]>('discover_endpoints', {
      url: url.trim(),
      timeoutMs: 5000,
    })
    if (endpoints.length === 0) {
      await showAlert(t('toolbar.discoverEmpty'))
      return
    }
    const lines = endpoints.map(
      (ep) => `${ep.security_policy} / ${ep.security_mode} — ${ep.endpoint_url}`,
    )
    await showAlert(`${t('toolbar.discoverResults', { count: endpoints.length })}\n\n${lines.join('\n')}`)
  } catch (error) {
    await showAlert(t('toolbar.discoverFailed', { error: String(error) }))
  }
}

async function onNewGroup() {
  const name = await showPrompt(t('toolbar.groupPrompt'), '')
  if (!name || !name.trim()) return
  try {
    await createGroup(name.trim())
  } catch (error) {
    await showAlert(String(error))
  }
}

function onCreated(conn: ConnectionInfo) {
  selectConnection(conn.id)
  void refreshConnections()
  void refreshGroups()
}
</script>

<template>
  <header class="master-toolbar">
    <div class="toolbar">
      <div class="toolbar-main">
        <span class="toolbar-title">OPCUAMaster</span>

        <span v-if="selectedConnection" class="chip" :style="{ color: stateView.color }">
          <span class="chip-icon">{{ stateView.icon }}</span>
          <span>{{ selectedConnection.name }}</span>
          <span class="chip-state">· {{ stateView.label }}</span>
        </span>

        <span class="toolbar-divider" />

        <button class="toolbar-btn" @click="newConnVisible = true">{{ t('toolbar.newConnection') }}</button>
        <button
          class="toolbar-btn btn-start"
          :disabled="!hasSelection || isConnected || isConnecting"
          @click="onConnect"
        >▶ {{ t('toolbar.connect') }}</button>
        <button class="toolbar-btn btn-stop" :disabled="!isConnected" @click="onDisconnect">
          ■ {{ t('toolbar.disconnect') }}
        </button>
        <button class="toolbar-btn btn-close" :disabled="!hasSelection" @click="onDelete">
          🗑 {{ t('toolbar.deleteConnection') }}
        </button>

        <span class="toolbar-divider" />

        <button class="toolbar-btn" @click="onRefresh">⟳ {{ t('toolbar.refresh') }}</button>
        <button class="toolbar-btn" @click="onLoad">{{ t('toolbar.openProject') }}</button>
        <button class="toolbar-btn" @click="onSave">{{ t('toolbar.saveProject') }}</button>

        <span class="toolbar-divider" />

        <button class="toolbar-btn" @click="onDiscover">🔍 {{ t('toolbar.discover') }}</button>
        <button class="toolbar-btn" @click="onNewGroup">➕ {{ t('toolbar.newGroup') }}</button>
        <button class="toolbar-btn" @click="certVisible = true">🔐 {{ t('toolbar.certManager') }}</button>
      </div>
      <div class="toolbar-aside">
        <LangSwitch />
        <VersionBadge />
      </div>
    </div>

    <NewConnectionDialog :visible="newConnVisible" @close="newConnVisible = false" @created="onCreated" />
    <CertManagerDialog :visible="certVisible" @close="certVisible = false" />
  </header>
</template>

<style scoped>
.master-toolbar {
  background: var(--c-base);
  border-bottom: 1px solid var(--c-surface0);
  user-select: none;
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

.chip-state {
  color: var(--c-overlay0);
  font-weight: 500;
}
</style>
