<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import EmptyState from '@shared/components/EmptyState.vue'
import HistoryChart from './HistoryChart.vue'
import { qualityColor, toChartPoints } from '../domain'
import { useMasterContext } from '../inject'
import type { HistoryMode, HistoryPointDto, ReadHistoryRequest } from '../types'

const { t } = useI18n()
const { historyTarget } = useMasterContext()

const QUICK_RANGES: Array<[string, number]> = [
  ['1m', 60],
  ['5m', 300],
  ['30m', 1800],
  ['1h', 3600],
  ['6h', 21600],
  ['24h', 86400],
]

const AGG_OPTIONS = ['平均', '最小', '最大', '计数', 'TimeAvg', '总计', 'Delta', 'PercentGood']

const mode = ref<HistoryMode>('raw')
const aggType = ref('平均')
const processingIntervalMs = ref(2000)
const startIso = ref('')
const endIso = ref('')
const maxValues = ref(5000)
const points = ref<HistoryPointDto[]>([])
const loading = ref(false)
const error = ref('')

function resetRange() {
  const now = new Date()
  const start = new Date(now.getTime() - 5 * 60 * 1000)
  startIso.value = start.toISOString()
  endIso.value = now.toISOString()
}

resetRange()

const chartPoints = computed(() => toChartPoints(points.value))

function isRangeValid(): boolean {
  const start = Date.parse(startIso.value)
  const end = Date.parse(endIso.value)
  return !Number.isNaN(start) && !Number.isNaN(end) && start < end
}

async function load() {
  const target = historyTarget.value
  if (!target) return
  if (!isRangeValid()) {
    error.value = t('history.invalidRange')
    return
  }
  loading.value = true
  error.value = ''
  try {
    const request: ReadHistoryRequest = {
      node_id: target.node_id,
      start_iso: startIso.value,
      end_iso: endIso.value,
      max_values: maxValues.value,
      mode: mode.value,
      agg_type: mode.value === 'processed' ? aggType.value : null,
      processing_interval_ms: mode.value === 'processed' ? processingIntervalMs.value : null,
    }
    points.value = await invoke<HistoryPointDto[]>('read_history', {
      connectionId: target.connection_id,
      request,
    })
  } catch (err) {
    error.value = String(err)
    await showAlert(String(err))
  } finally {
    loading.value = false
  }
}

function applyQuickRange(secs: number) {
  const now = new Date()
  endIso.value = now.toISOString()
  startIso.value = new Date(now.getTime() - secs * 1000).toISOString()
  void load()
}

function onModeChange() {
  points.value = []
  error.value = ''
  void load()
}

watch(historyTarget, () => {
  points.value = []
  error.value = ''
  resetRange()
  void load()
})

watch([mode], () => {
  points.value = []
  error.value = ''
})
</script>

<template>
  <div class="history-panel">
    <EmptyState
      v-if="!historyTarget"
      :title="t('history.emptyTitle')"
      :hint="t('history.emptyHint')"
    >
      <span>📈</span>
    </EmptyState>

    <div v-else class="history-body">
      <div class="head">
        <span class="head-title">📈 {{ historyTarget.display_name }}</span>
        <span class="head-node mono">{{ historyTarget.node_id }}</span>
      </div>

      <div class="controls">
        <div class="ctl-group">
          <span class="ctl-label">{{ t('history.mode') }}</span>
          <button :class="['chip', { active: mode === 'raw' }]" @click="mode = 'raw'; onModeChange()">{{ t('history.modeRaw') }}</button>
          <button :class="['chip', { active: mode === 'processed' }]" @click="mode = 'processed'; onModeChange()">{{ t('history.modeProcessed') }}</button>
          <button :class="['chip', { active: mode === 'events' }]" @click="mode = 'events'; onModeChange()">{{ t('history.modeEvents') }}</button>
        </div>

        <div v-if="mode === 'processed'" class="ctl-group">
          <span class="ctl-label">{{ t('history.aggType') }}</span>
          <select v-model="aggType" class="ctl-input">
            <option v-for="a in AGG_OPTIONS" :key="a" :value="a">{{ a }}</option>
          </select>
          <span class="ctl-label">{{ t('history.intervalMs') }}</span>
          <input v-model.number="processingIntervalMs" class="ctl-input num" type="number" min="100" />
        </div>

        <div class="ctl-group">
          <span class="ctl-label">{{ t('history.quick') }}</span>
          <button
            v-for="[label, secs] in QUICK_RANGES"
            :key="label"
            class="chip"
            @click="applyQuickRange(secs)"
          >{{ label }}</button>
        </div>

        <div class="ctl-group">
          <span class="ctl-label">起</span>
          <input v-model="startIso" class="ctl-input time" type="text" />
          <span class="ctl-label">止</span>
          <input v-model="endIso" class="ctl-input time" type="text" />
          <span class="ctl-label">{{ t('history.maxValues') }}</span>
          <input v-model.number="maxValues" class="ctl-input num" type="number" min="10" max="50000" />
          <button class="refresh-btn" :disabled="loading || !isRangeValid()" @click="load">
            {{ loading ? t('history.loading') : `🔄 ${t('history.refresh')}` }}
          </button>
        </div>
      </div>

      <p v-if="error" class="error">{{ error }}</p>

      <div v-if="points.length > 0" class="count">
        {{ mode === 'events' ? t('history.eventCount', { count: points.length }) : t('history.pointCount', { count: points.length }) }}
      </div>

      <HistoryChart v-if="mode !== 'events' && chartPoints.length > 0" :points="chartPoints" />

      <div v-if="points.length === 0 && !loading" class="empty-hint">{{ t('history.noData') }}</div>

      <div v-else class="table-scroll">
        <table>
          <template v-if="mode === 'events'">
            <thead>
              <tr>
                <th>{{ t('history.colTime') }}</th>
                <th>{{ t('history.colSeverity') }}</th>
                <th>{{ t('history.colMessage') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(p, i) in points" :key="i">
                <td class="mono">{{ p.source_timestamp }}</td>
                <td class="mono">{{ p.status }}</td>
                <td class="mono">{{ p.value }}</td>
              </tr>
            </tbody>
          </template>
          <template v-else>
            <thead>
              <tr>
                <th>{{ t('history.colTime') }}</th>
                <th>{{ t('history.colValue') }}</th>
                <th>{{ t('history.colStatus') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(p, i) in points" :key="i">
                <td class="mono">{{ p.source_timestamp }}</td>
                <td class="mono">{{ p.value }}</td>
                <td :style="{ color: qualityColor(p.status) }">{{ p.status }}</td>
              </tr>
            </tbody>
          </template>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.history-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--c-crust);
  overflow: hidden;
}

.history-body {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 10px 12px 6px;
}

.head-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--c-text);
}

.head-node {
  font-size: 11px;
  color: var(--c-overlay0);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.controls {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 0 12px 8px;
  border-bottom: 1px solid var(--c-surface0);
}

.ctl-group {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.ctl-label {
  font-size: 11px;
  color: var(--c-overlay0);
  white-space: nowrap;
}

.chip {
  padding: 3px 10px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-base);
  color: var(--c-subtext1);
  cursor: pointer;
  font-size: 11px;
}
.chip:hover { background: var(--c-surface0); }
.chip.active { background: var(--c-blue); color: var(--c-base); border-color: var(--c-blue); }

.ctl-input {
  padding: 3px 6px;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 11px;
}
.ctl-input.time { width: 200px; font-family: var(--font-mono); }
.ctl-input.num { width: 80px; }

.refresh-btn {
  padding: 4px 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-blue);
  color: var(--c-base);
  cursor: pointer;
  font-size: 11px;
}
.refresh-btn:hover:not(:disabled) { background: var(--c-sapphire); }
.refresh-btn:disabled { opacity: 0.5; cursor: default; }

.error {
  padding: 4px 12px 0;
  font-size: 12px;
  color: var(--c-red);
  overflow-wrap: anywhere;
}

.count {
  padding: 8px 12px 4px;
  font-size: 11px;
  color: var(--c-overlay0);
}

.empty-hint {
  padding: 24px;
  text-align: center;
  color: var(--c-overlay0);
  font-size: 12px;
}

.table-scroll {
  flex: 1;
  overflow: auto;
  padding: 8px 12px 12px;
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

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
}
</style>
