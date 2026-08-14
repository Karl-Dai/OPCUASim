<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import { localizeLegacyBackendText } from '@shared/i18n/backendText'
import { formatTimestampMs } from '../domain'
import { useMasterContext } from '../inject'
import type { LogRow } from '../types'

const props = defineProps<{ expanded: boolean }>()
const emit = defineEmits<{ (e: 'toggle'): void }>()

const { t, locale } = useI18n()
const { selectedConnectionId } = useMasterContext()

const logs = ref<LogRow[]>([])
const directionFilter = ref<'all' | 'Request' | 'Response'>('all')
const searchQuery = ref('')
const autoScroll = ref(true)
const isExporting = ref(false)

const MAX_ROWS = 2000

function formatDetail(log: LogRow): string {
  const ev = log.detail_event
  if (ev && ev.kind) {
    const payload = ev.payload && typeof ev.payload === 'object' ? ev.payload : undefined
    return t(`log.${ev.kind}`, payload)
  }
  return localizeLegacyBackendText(log.detail, locale.value, t, 'log.backendDetailFallback')
}

const filteredLogs = computed(() => {
  const query = searchQuery.value.trim().toLowerCase()
  return logs.value.filter((log) => {
    const dirOk =
      directionFilter.value === 'all' || log.direction === directionFilter.value
    if (!dirOk) return false
    if (!query) return true
    const detail = formatDetail(log).toLowerCase()
    return log.service.toLowerCase().includes(query) || detail.includes(query) || log.detail.toLowerCase().includes(query)
  })
})

const displayLogs = computed(() => {
  const arr = filteredLogs.value
  return arr.length > MAX_ROWS ? arr.slice(arr.length - MAX_ROWS) : arr
})

const scrollContainer = ref<HTMLDivElement | null>(null)

function scrollToBottom() {
  void nextTick(() => {
    if (!scrollContainer.value) return
    scrollContainer.value.scrollTop = scrollContainer.value.scrollHeight
  })
}

async function loadLogs() {
  if (!selectedConnectionId.value) {
    logs.value = []
    return
  }
  try {
    logs.value = await invoke<LogRow[]>('get_communication_logs', {
      connectionId: selectedConnectionId.value,
    })
    if (autoScroll.value) scrollToBottom()
  } catch { /* ignore */ }
}

let timer: number | null = null

function startPolling() {
  if (timer !== null) return
  timer = window.setInterval(() => {
    if (props.expanded) void loadLogs()
  }, 2000)
}

function stopPolling() {
  if (timer !== null) {
    clearInterval(timer)
    timer = null
  }
}

watch(selectedConnectionId, () => {
  logs.value = []
  void loadLogs()
})

watch(
  () => props.expanded,
  (expanded) => {
    if (expanded) {
      void loadLogs()
      startPolling()
    } else {
      stopPolling()
    }
  },
)

onMounted(() => {
  void loadLogs()
  if (props.expanded) startPolling()
})

onUnmounted(stopPolling)

async function onClear() {
  if (!selectedConnectionId.value) return
  try {
    await invoke('clear_communication_logs', { connectionId: selectedConnectionId.value })
    logs.value = []
  } catch { /* ignore */ }
}

async function onExport() {
  if (!selectedConnectionId.value || isExporting.value) return
  const connectionId = selectedConnectionId.value
  const path = await save({
    defaultPath: `opcua_logs_${Date.now()}.csv`,
    filters: [{ name: 'CSV', extensions: ['csv'] }],
  })
  if (!path) return
  isExporting.value = true
  try {
    await invoke('export_communication_logs', { connectionId, path })
  } catch (error) {
    await showAlert(t('log.exportFailed', { error: String(error) }))
  } finally {
    isExporting.value = false
  }
}
</script>

<template>
  <div :class="['log-panel', { expanded }]">
    <div class="log-header" @click="emit('toggle')">
      <span class="log-toggle">{{ expanded ? '▼' : '▲' }}</span>
      <span class="log-dot" :class="{ active: logs.length > 0 }" />
      <span class="log-title">{{ t('log.title') }}</span>
      <span v-if="!expanded && logs.length > 0" class="log-count">{{ logs.length }}</span>
      <div class="log-controls" @click.stop>
        <button class="log-btn" @click="loadLogs">{{ t('log.refresh') }}</button>
        <button class="log-btn" @click="onClear">{{ t('log.clear') }}</button>
        <button class="log-btn" :disabled="isExporting" @click="onExport">
          {{ isExporting ? t('log.exporting') : t('log.export') }}
        </button>
      </div>
    </div>

    <div v-if="expanded" class="log-content">
      <div class="log-filters">
        <button :class="['auto-btn', { active: autoScroll }]" @click="autoScroll = !autoScroll">
          {{ t('log.autoScroll') }}
        </button>
        <label class="filter-field">
          <span>{{ t('log.direction') }}</span>
          <select v-model="directionFilter" class="filter-select">
            <option value="all">{{ t('log.directionAll') }}</option>
            <option value="Request">{{ t('log.directionRequest') }}</option>
            <option value="Response">{{ t('log.directionResponse') }}</option>
          </select>
        </label>
        <input v-model="searchQuery" class="log-search" type="search" :placeholder="t('log.searchPlaceholder')" />
        <span class="filter-count">{{ t('log.filteredCount', { visible: filteredLogs.length, total: logs.length }) }}</span>
      </div>

      <div ref="scrollContainer" class="log-body">
        <div v-if="!selectedConnectionId" class="log-empty">{{ t('log.noConnection') }}</div>
        <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
        <div v-else-if="filteredLogs.length === 0" class="log-empty">{{ t('log.noMatches') }}</div>
        <table v-else class="log-table">
          <thead>
            <tr>
              <th class="col-time">{{ t('log.colTime') }}</th>
              <th class="col-dir">{{ t('log.colDirection') }}</th>
              <th class="col-service">{{ t('log.colService') }}</th>
              <th class="col-detail">{{ t('log.colDetail') }}</th>
              <th class="col-status">{{ t('log.colStatus') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="log in displayLogs" :key="log.seq">
              <td class="col-time mono">{{ formatTimestampMs(log.timestamp_ms) }}</td>
              <td :class="['col-dir', log.direction.toLowerCase()]">
                {{ log.direction === 'Request' ? '→' : '←' }} {{ log.direction }}
              </td>
              <td class="col-service">{{ log.service }}</td>
              <td class="col-detail" :title="formatDetail(log)">{{ formatDetail(log) }}</td>
              <td class="col-status">{{ log.status ?? '' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.log-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  border-top: 1px solid var(--c-surface0);
}
.log-panel:not(.expanded) { height: 32px; }

.log-header {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 8px;
  cursor: pointer;
  flex-shrink: 0;
  background: var(--c-crust);
}

.log-toggle { font-size: 10px; color: var(--c-overlay0); width: 16px; text-align: center; }
.log-title { font-size: 12px; color: var(--c-overlay0); white-space: nowrap; }
.log-count {
  font-size: 10px;
  background: var(--c-blue);
  color: var(--c-base);
  padding: 0 6px;
  border-radius: 8px;
  font-weight: 600;
}
.log-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--c-overlay0); }
.log-dot.active { background: var(--c-green); }

.log-controls {
  display: flex;
  gap: 4px;
  margin-left: auto;
}

.log-btn {
  padding: 2px 8px;
  background: transparent;
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  color: var(--c-text);
  cursor: pointer;
  font-size: 11px;
}
.log-btn:hover { background: var(--c-surface0); }
.log-btn:disabled { opacity: 0.45; cursor: not-allowed; }

.log-content {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.log-filters {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 6px 8px;
  border-bottom: 1px solid var(--c-base);
  background: var(--c-mantle);
  flex-shrink: 0;
}

.auto-btn {
  padding: 2px 9px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
  font-size: 11px;
}
.auto-btn.active {
  background: var(--c-green);
  border-color: var(--c-green);
  color: var(--c-base);
}

.filter-field {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--c-overlay0);
  font-size: 11px;
}

.filter-select {
  padding: 2px 6px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 11px;
}

.log-search {
  flex: 1 1 160px;
  min-width: 120px;
  padding: 3px 8px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 11px;
}

.filter-count {
  margin-left: auto;
  color: var(--c-overlay0);
  font: 11px var(--font-mono);
  white-space: nowrap;
}

.log-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--c-crust);
}

.log-empty {
  padding: 24px;
  text-align: center;
  color: var(--c-overlay0);
  font-size: 12px;
}

.log-table {
  border-collapse: collapse;
  font-size: 12px;
  font-family: var(--font-mono);
  width: 100%;
  table-layout: fixed;
}

.log-table th,
.log-table td {
  padding: 4px 10px;
  text-align: left;
  border-bottom: 1px solid var(--c-base);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.log-table th {
  background: var(--c-base);
  color: var(--c-overlay0);
  font-weight: 500;
  position: sticky;
  top: 0;
  z-index: 1;
}

.col-time { width: 110px; }
.col-dir { width: 90px; }
.col-service { width: 130px; }
.col-detail { width: auto; }
.col-status { width: 90px; }

.col-dir.request { color: var(--c-green); font-weight: 600; }
.col-dir.response { color: var(--c-blue); font-weight: 600; }
.col-time { color: var(--c-overlay0); }
</style>
