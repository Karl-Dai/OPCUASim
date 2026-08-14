<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import EmptyState from '@shared/components/EmptyState.vue'
import { accessString, formatHms, isComplexValue, isWritable, qualityColor, truncateSafe } from '../domain'
import { useMasterContext } from '../inject'
import type { MonitoredRow, NodeAttrsDto } from '../types'

const { t } = useI18n()
const { selectedConnectionId, selectedNodeId, monitoredRows } = useMasterContext()

const attrs = ref<NodeAttrsDto | null>(null)
const writeValue = ref('')
const reading = ref(false)
const writing = ref(false)

const row = computed<MonitoredRow | null>(() => {
  const id = selectedNodeId.value
  if (!id) return null
  return monitoredRows.value.get(id) ?? null
})

const writable = computed(() => {
  if (row.value) return isWritable(row.value.user_access_level)
  if (attrs.value) return /write/i.test(attrs.value.access_level)
  return false
})

watch(selectedNodeId, () => {
  attrs.value = null
  writeValue.value = ''
})

async function onRead() {
  if (!selectedConnectionId.value || !selectedNodeId.value) return
  reading.value = true
  try {
    attrs.value = await invoke<NodeAttrsDto>('read_attributes', {
      connectionId: selectedConnectionId.value,
      nodeId: selectedNodeId.value,
    })
  } catch (error) {
    await showAlert(t('valuePanel.readFailed', { error: String(error) }))
  } finally {
    reading.value = false
  }
}

/** Client-side type check; returns an i18n key or null when the input is acceptable. */
function parseError(dataType: string, raw: string): string | null {
  const s = raw.trim()
  if (!s) return null
  if (s.includes(',') || s.includes(';')) return null
  switch (dataType) {
    case 'Boolean': {
      const lower = s.toLowerCase()
      return ['true', 'false', '0', '1'].includes(lower) ? null : 'valuePanel.errBoolean'
    }
    case 'Float':
    case 'Double':
      return Number.isNaN(Number(s)) ? 'valuePanel.errFloat' : null
    case 'SByte':
    case 'Int16':
    case 'Int32':
    case 'Int64':
      return Number.isInteger(Number(s)) && !Number.isNaN(Number(s)) ? null : 'valuePanel.errInt'
    case 'Byte':
    case 'UInt16':
    case 'UInt32':
    case 'UInt64': {
      const n = Number(s)
      return Number.isInteger(n) && n >= 0 ? null : 'valuePanel.errUint'
    }
    default:
      return null
  }
}

const writeError = computed(() => {
  const dataType = row.value?.data_type ?? attrs.value?.data_type ?? 'Unknown'
  const key = parseError(dataType, writeValue.value)
  return key ? t(key) : ''
})

async function onWrite() {
  if (!selectedConnectionId.value || !selectedNodeId.value) return
  if (writeError.value) return
  const dataType = row.value?.data_type ?? attrs.value?.data_type ?? 'Unknown'
  writing.value = true
  try {
    await invoke('write_value', {
      connectionId: selectedConnectionId.value,
      nodeId: selectedNodeId.value,
      value: writeValue.value.trim(),
      dataType,
    })
    await showAlert(t('valuePanel.writeSuccess', { nodeId: selectedNodeId.value }))
    writeValue.value = ''
    attrs.value = null
  } catch (error) {
    await showAlert(t('valuePanel.writeFailed', { error: String(error) }))
  } finally {
    writing.value = false
  }
}
</script>

<template>
  <aside class="value-panel">
    <div class="panel-title">{{ t('valuePanel.title') }}</div>
    <div class="panel-sep" />

    <EmptyState
      v-if="!selectedNodeId"
      compact
      :title="t('valuePanel.emptyTitle')"
      :hint="t('valuePanel.emptyHint')"
    >
      <span>👈</span>
    </EmptyState>

    <div v-else class="panel-scroll">
      <div class="section">
        <div class="section-label">{{ t('valuePanel.nodeInfo') }}</div>
        <div class="kv"><span>NodeId</span><span class="mono">{{ selectedNodeId }}</span></div>
        <div class="kv"><span>{{ t('dataTable.colName') }}</span><span>{{ row?.display_name ?? attrs?.display_name ?? '—' }}</span></div>
        <div class="kv"><span>{{ t('valuePanel.dataType') }}</span><span>{{ row?.data_type ?? attrs?.data_type ?? '—' }}</span></div>
        <div v-if="row" class="kv"><span>{{ t('valuePanel.access') }}</span><span>{{ accessString(row.user_access_level) }}</span></div>
        <div v-if="row" class="kv"><span>{{ t('valuePanel.mode') }}</span><span>{{ row.access_mode }} · {{ Math.round(row.interval_ms) }}ms</span></div>
      </div>

      <div class="section">
        <div class="section-label">{{ t('valuePanel.currentValue') }}</div>
        <div v-if="row?.value && isComplexValue(row.value)" class="mono value-complex" :title="row.value">
          {{ truncateSafe(row.value, 60) }}
        </div>
        <div v-else class="current-value mono">{{ row?.value ?? '—' }}</div>
        <div v-if="row?.quality" :style="{ color: qualityColor(row.quality) }" class="quality">{{ row.quality }}</div>
        <div v-if="row?.source_timestamp" class="kv"><span>{{ t('valuePanel.sourceTimestamp') }}</span><span class="mono">{{ formatHms(row.source_timestamp) }}</span></div>
        <div v-if="row?.server_timestamp" class="kv"><span>{{ t('valuePanel.serverTimestamp') }}</span><span class="mono">{{ formatHms(row.server_timestamp) }}</span></div>
      </div>

      <div class="section">
        <div class="section-label">{{ t('valuePanel.actions') }}</div>
        <button class="action-btn" :disabled="reading" @click="onRead">
          {{ reading ? t('common.loading') : `⟳ ${t('valuePanel.read')}` }}
        </button>
      </div>

      <div v-if="attrs && attrs.node_id === selectedNodeId" class="section">
        <div class="section-label">{{ t('valuePanel.readResult') }}</div>
        <div class="kv"><span>{{ t('valuePanel.dataType') }}</span><span>{{ attrs.data_type }}</span></div>
        <div class="kv"><span>{{ t('valuePanel.accessLevel') }}</span><span>{{ attrs.access_level }}</span></div>
        <div v-if="attrs.value" class="kv"><span>{{ t('valuePanel.value') }}</span><span class="mono">{{ attrs.value }}</span></div>
        <div v-if="attrs.quality" class="kv"><span>{{ t('valuePanel.quality') }}</span><span>{{ attrs.quality }}</span></div>
        <div v-if="attrs.description" class="kv"><span>{{ t('valuePanel.desc') }}</span><span>{{ attrs.description }}</span></div>
      </div>

      <div v-if="writable" class="section">
        <div class="section-label">{{ t('valuePanel.write') }}</div>
        <div class="write-row">
          <span class="mono type-label">{{ row?.data_type ?? attrs?.data_type ?? 'Unknown' }}</span>
          <input v-model="writeValue" class="write-input" type="text" :class="{ invalid: writeError }" />
          <button class="action-btn" :disabled="!writeValue.trim() || !!writeError || writing" @click="onWrite">
            {{ writing ? t('common.loading') : t('valuePanel.writeValue') }}
          </button>
        </div>
        <div v-if="writeError" class="write-error">{{ writeError }}</div>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.value-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--c-mantle);
  overflow: hidden;
}

.panel-title {
  padding: 10px 12px 6px;
  font-size: 11px;
  font-weight: 700;
  color: var(--c-overlay0);
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.panel-sep {
  height: 1px;
  margin: 0 10px;
  background: var(--c-surface0);
}

.panel-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 10px 12px;
}

.section {
  margin-bottom: 16px;
}

.section-label {
  font-size: 11px;
  font-weight: 700;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}

.kv {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--c-subtext1);
  padding: 2px 0;
}
.kv > span:first-child { color: var(--c-overlay0); flex: none; }

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-subtext0);
  overflow-wrap: anywhere;
}

.current-value {
  font-size: 22px;
  color: var(--c-text);
  padding: 2px 0;
}

.value-complex {
  font-size: 12px;
  color: var(--c-text);
  padding: 4px 0;
  white-space: pre-wrap;
}

.quality { font-size: 12px; padding: 2px 0; }

.action-btn {
  padding: 6px 16px;
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
  font-size: 12px;
}
.action-btn:hover:not(:disabled) { background: var(--c-surface1); }
.action-btn:disabled { opacity: 0.5; cursor: default; }

.write-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.type-label { flex: none; color: var(--c-overlay0); }

.write-input {
  flex: 1 1 auto;
  min-width: 0;
  padding: 5px 8px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  color: var(--c-text);
  font-size: 12px;
  box-sizing: border-box;
}
.write-input:focus { outline: none; border-color: var(--c-blue); }
.write-input.invalid { border-color: var(--c-red); }

.write-error {
  margin-top: 4px;
  font-size: 11px;
  color: var(--c-red);
}
</style>
