<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import EmptyState from '@shared/components/EmptyState.vue'
import { useMasterContext } from '../inject'
import type { EventItemDto, SubscribeResult } from '../types'

const { t } = useI18n()
const { connections, selectedConnectionId } = useMasterContext()

const eventsConnId = ref(selectedConnectionId.value ?? '')
const sourceNodeId = ref('')
const items = ref<EventItemDto[]>([])
const subscribed = ref(false)
const subscribeInFlight = ref(false)

watch(selectedConnectionId, (id) => {
  if (id) eventsConnId.value = id
})

watch(eventsConnId, () => {
  items.value = []
  subscribed.value = false
})

function severityColor(severity: number): string {
  if (severity <= 100) return 'var(--c-green)'
  if (severity <= 500) return 'var(--c-yellow)'
  return 'var(--c-red)'
}

function formatTimeShort(time: string): string {
  return time.length >= 19 ? time.slice(11, 19) : time
}

async function onSubscribe() {
  if (!eventsConnId.value || !sourceNodeId.value.trim() || subscribeInFlight.value) return
  subscribeInFlight.value = true
  try {
    const result = await invoke<SubscribeResult>('subscribe_events', {
      connectionId: eventsConnId.value,
      sourceNodeId: sourceNodeId.value.trim(),
    })
    subscribed.value = result.ok
    if (!result.ok) {
      await showAlert(t('events.subscribeFailed', { error: result.detail ?? '' }))
    }
  } catch (err) {
    subscribed.value = false
    await showAlert(t('events.subscribeFailed', { error: String(err) }))
  } finally {
    subscribeInFlight.value = false
  }
}

async function onUnsubscribe() {
  if (!eventsConnId.value) return
  try {
    await invoke('unsubscribe_events', { connectionId: eventsConnId.value })
  } catch { /* ignore */ }
  subscribed.value = false
}

async function onClear() {
  if (!eventsConnId.value) return
  try {
    await invoke('clear_events', { connectionId: eventsConnId.value })
  } catch { /* ignore */ }
  items.value = []
}

async function pollEvents() {
  if (!eventsConnId.value) return
  try {
    items.value = await invoke<EventItemDto[]>('get_events', {
      connectionId: eventsConnId.value,
    })
  } catch { /* connection may be gone */ }
}

let timer: number | null = null

onMounted(() => {
  timer = window.setInterval(pollEvents, 1000)
})

onUnmounted(() => {
  if (timer !== null) clearInterval(timer)
})
</script>

<template>
  <div class="events-panel">
    <div class="head">
      <span class="head-title">🔔 {{ t('events.title') }}</span>
      <span class="head-count">{{ t('events.count', { count: items.length }) }}</span>
    </div>

    <div class="controls">
      <label class="field">
        <span>{{ t('events.connection') }}</span>
        <select v-model="eventsConnId" class="input">
          <option v-for="conn in connections" :key="conn.id" :value="conn.id">{{ conn.name }}</option>
        </select>
      </label>
      <label class="field">
        <span>{{ t('events.sourceNode') }}</span>
        <input v-model="sourceNodeId" class="input node" type="text" :placeholder="t('events.sourceNodeHint')" />
      </label>
      <button
        class="action-btn"
        :disabled="!eventsConnId || !sourceNodeId.trim() || subscribed || subscribeInFlight"
        @click="onSubscribe"
      >{{ subscribed ? `✓ ${t('events.subscribed')}` : `📡 ${t('events.subscribe')}` }}</button>
      <button class="action-btn" :disabled="!subscribed" @click="onUnsubscribe">{{ t('events.unsubscribe') }}</button>
      <button class="action-btn" :disabled="items.length === 0" @click="onClear">{{ t('events.clear') }}</button>
    </div>

    <EmptyState v-if="items.length === 0" :title="t('events.emptyTitle')" :hint="t('events.emptyHint')">
      <span>🔔</span>
    </EmptyState>

    <div v-else class="table-scroll">
      <table>
        <thead>
          <tr>
            <th>{{ t('events.colTime') }}</th>
            <th>{{ t('events.colSeverity') }}</th>
            <th>{{ t('events.colSource') }}</th>
            <th>{{ t('events.colMessage') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(item, i) in [...items].reverse()" :key="i">
            <td class="mono">{{ formatTimeShort(item.time) }}</td>
            <td class="mono" :style="{ color: severityColor(item.severity) }">{{ item.severity }}</td>
            <td>{{ item.source }}</td>
            <td>{{ item.message }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.events-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--c-crust);
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

.head-count {
  font-size: 11px;
  color: var(--c-overlay0);
}

.controls {
  display: flex;
  align-items: flex-end;
  gap: 10px;
  padding: 0 12px 10px;
  border-bottom: 1px solid var(--c-surface0);
  flex-wrap: wrap;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 11px;
  color: var(--c-overlay0);
}

.input {
  padding: 4px 8px;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
}
.input.node { width: 180px; font-family: var(--font-mono); }
.input:focus { outline: none; border-color: var(--c-blue); }

.action-btn {
  padding: 5px 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
  font-size: 12px;
}
.action-btn:hover:not(:disabled) { background: var(--c-surface1); }
.action-btn:disabled { opacity: 0.45; cursor: default; }

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

.mono { font-family: var(--font-mono); font-size: 11px; }
</style>
