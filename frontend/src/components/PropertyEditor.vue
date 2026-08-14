<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from '@shared/i18n'
import EmptyState from '@shared/components/EmptyState.vue'
import { useServerContext } from '../inject'
import { dataTypeLabel } from '../domain'
import type { NodeRow, SimulationMode, UpdateNodeRequest } from '../types'

const { t } = useI18n()
const { addressSpace, selectedNodeId, currentValues, updateNode } = useServerContext()

const node = computed<NodeRow | null>(() => {
  const id = selectedNodeId.value
  if (!id) return null
  return addressSpace.value.nodes.find((n) => n.node_id === id) ?? null
})

const currentValue = computed(() => {
  if (!node.value) return '—'
  return currentValues.value.get(node.value.node_id) ?? '—'
})

function num(e: Event): number {
  return Number((e.target as HTMLInputElement).value)
}

function commit(partial: Omit<UpdateNodeRequest, 'node_id'>) {
  if (!node.value) return
  void updateNode({ node_id: node.value.node_id, ...partial })
}

function commitSimulation(simulation: SimulationMode) {
  commit({ simulation })
}

// Static
function onStaticValue(e: Event) {
  commitSimulation({ type: 'Static', value: (e.target as HTMLInputElement).value })
}

// Random
function onRandomMin(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Random') return
  commitSimulation({ type: 'Random', min: num(e), max: s.max, interval_ms: s.interval_ms })
}
function onRandomMax(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Random') return
  commitSimulation({ type: 'Random', min: s.min, max: num(e), interval_ms: s.interval_ms })
}
function onRandomInterval(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Random') return
  commitSimulation({ type: 'Random', min: s.min, max: s.max, interval_ms: num(e) })
}

// Sine
function onSineAmplitude(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Sine') return
  commitSimulation({
    type: 'Sine',
    amplitude: num(e),
    offset: s.offset,
    period_ms: s.period_ms,
    interval_ms: s.interval_ms,
  })
}
function onSineOffset(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Sine') return
  commitSimulation({
    type: 'Sine',
    amplitude: s.amplitude,
    offset: num(e),
    period_ms: s.period_ms,
    interval_ms: s.interval_ms,
  })
}
function onSinePeriod(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Sine') return
  commitSimulation({
    type: 'Sine',
    amplitude: s.amplitude,
    offset: s.offset,
    period_ms: num(e),
    interval_ms: s.interval_ms,
  })
}
function onSineInterval(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Sine') return
  commitSimulation({
    type: 'Sine',
    amplitude: s.amplitude,
    offset: s.offset,
    period_ms: s.period_ms,
    interval_ms: num(e),
  })
}

// Linear
function onLinearStart(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Linear') return
  commitSimulation({ type: 'Linear', start: num(e), step: s.step, min: s.min, max: s.max, mode: s.mode, interval_ms: s.interval_ms })
}
function onLinearStep(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Linear') return
  commitSimulation({ type: 'Linear', start: s.start, step: num(e), min: s.min, max: s.max, mode: s.mode, interval_ms: s.interval_ms })
}
function onLinearMin(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Linear') return
  commitSimulation({ type: 'Linear', start: s.start, step: s.step, min: num(e), max: s.max, mode: s.mode, interval_ms: s.interval_ms })
}
function onLinearMax(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Linear') return
  commitSimulation({ type: 'Linear', start: s.start, step: s.step, min: s.min, max: num(e), mode: s.mode, interval_ms: s.interval_ms })
}
function onLinearBounce(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Linear') return
  const bounce = (e.target as HTMLInputElement).checked
  commitSimulation({
    type: 'Linear',
    start: s.start,
    step: s.step,
    min: s.min,
    max: s.max,
    mode: bounce ? 'Bounce' : 'Repeat',
    interval_ms: s.interval_ms,
  })
}
function onLinearInterval(e: Event) {
  const s = node.value?.simulation
  if (s?.type !== 'Linear') return
  commitSimulation({ type: 'Linear', start: s.start, step: s.step, min: s.min, max: s.max, mode: s.mode, interval_ms: num(e) })
}

// Script
const scriptExpression = ref('t * 0.1')
watch(
  () => node.value?.node_id,
  () => {
    const s = node.value?.simulation
    scriptExpression.value = s?.type === 'Script' ? s.expression : 't * 0.1'
  },
  { immediate: true },
)

function scriptInterval(): number {
  const s = node.value?.simulation
  return s?.type === 'Script' ? s.interval_ms : 1000
}

function onScriptExpression() {
  commitSimulation({ type: 'Script', expression: scriptExpression.value, interval_ms: scriptInterval() })
}
function onScriptInterval(e: Event) {
  commitSimulation({ type: 'Script', expression: scriptExpression.value, interval_ms: num(e) })
}

// Writable + EU range
function onWritable(e: Event) {
  commit({ writable: (e.target as HTMLInputElement).checked })
}
function onEuLow(e: Event) {
  commit({ eu_range_low: num(e) })
}
function onEuHigh(e: Event) {
  commit({ eu_range_high: num(e) })
}
</script>

<template>
  <aside class="property-editor">
    <div class="panel-title">{{ t('propertyEditor.title') }}</div>
    <div class="panel-sep" />

    <EmptyState
      v-if="!node"
      compact
      :title="t('propertyEditor.emptyTitle')"
      :hint="t('propertyEditor.emptyHint')"
    >
      <span>👈</span>
    </EmptyState>

    <div v-else class="panel-scroll">
      <div class="section">
        <div class="section-label">{{ t('propertyEditor.nodeInfo') }}</div>
        <div class="kv"><span>{{ t('propertyEditor.nodeId') }}</span><span class="mono">{{ node.node_id }}</span></div>
        <div class="kv"><span>{{ t('propertyEditor.name') }}</span><span>{{ node.display_name }}</span></div>
        <div class="kv"><span>{{ t('propertyEditor.parent') }}</span><span class="mono">{{ node.parent_id }}</span></div>
        <div class="kv"><span>{{ t('propertyEditor.dataType') }}</span><span>{{ dataTypeLabel(node.data_type) }}</span></div>
        <label class="check">
          <input type="checkbox" :checked="node.writable" @change="onWritable" />
          <span>{{ t('propertyEditor.writable') }}</span>
        </label>
        <div class="kv">
          <span>{{ t('propertyEditor.euRange') }}</span>
          <span class="range">
            <input class="num" type="number" step="0.1" :value="node.eu_range_low" @change="onEuLow" />
            <input class="num" type="number" step="0.1" :value="node.eu_range_high" @change="onEuHigh" />
          </span>
        </div>
      </div>

      <div class="section">
        <div class="section-label">{{ t('propertyEditor.currentValue') }}</div>
        <div class="current-value mono">{{ currentValue }}</div>
      </div>

      <div class="section">
        <div class="section-label">{{ t('propertyEditor.simulation') }}</div>

        <template v-if="node.simulation.type === 'Static'">
          <div class="field">
            <span class="field-label">{{ t('simulation.value') }}</span>
            <input class="text-input" :value="node.simulation.value" @change="onStaticValue" />
          </div>
        </template>

        <template v-else-if="node.simulation.type === 'Random'">
          <div class="field">
            <span class="field-label">{{ t('simulation.min') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.min" @change="onRandomMin" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.max') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.max" @change="onRandomMax" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.intervalMs') }}</span>
            <input class="num" type="number" step="1" min="50" :value="node.simulation.interval_ms" @change="onRandomInterval" />
          </div>
        </template>

        <template v-else-if="node.simulation.type === 'Sine'">
          <div class="field">
            <span class="field-label">{{ t('simulation.amplitude') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.amplitude" @change="onSineAmplitude" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.offset') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.offset" @change="onSineOffset" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.periodMs') }}</span>
            <input class="num" type="number" step="1" min="50" :value="node.simulation.period_ms" @change="onSinePeriod" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.intervalMs') }}</span>
            <input class="num" type="number" step="1" min="50" :value="node.simulation.interval_ms" @change="onSineInterval" />
          </div>
        </template>

        <template v-else-if="node.simulation.type === 'Linear'">
          <div class="field">
            <span class="field-label">{{ t('simulation.start') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.start" @change="onLinearStart" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.step') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.step" @change="onLinearStep" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.min') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.min" @change="onLinearMin" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.max') }}</span>
            <input class="num" type="number" step="0.1" :value="node.simulation.max" @change="onLinearMax" />
          </div>
          <label class="check">
            <input type="checkbox" :checked="node.simulation.mode === 'Bounce'" @change="onLinearBounce" />
            <span>{{ t('simulation.bounce') }}</span>
          </label>
          <div class="field">
            <span class="field-label">{{ t('simulation.intervalMs') }}</span>
            <input class="num" type="number" step="1" min="50" :value="node.simulation.interval_ms" @change="onLinearInterval" />
          </div>
        </template>

        <template v-else>
          <div class="field">
            <span class="field-label">{{ t('simulation.expression') }}</span>
            <textarea class="text-area" v-model="scriptExpression" rows="3" />
          </div>
          <div class="field">
            <span class="field-label">{{ t('simulation.intervalMs') }}</span>
            <input class="num" type="number" step="1" min="50" :value="scriptInterval()" @change="onScriptInterval" />
          </div>
          <button class="apply-btn" @click="onScriptExpression">{{ t('propertyEditor.apply') }}</button>
        </template>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.property-editor {
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
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--c-subtext1);
  padding: 2px 0;
}

.kv > span:first-child {
  color: var(--c-overlay0);
}

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-subtext0);
}

.current-value {
  font-size: 20px;
  color: var(--c-text);
  padding: 2px 0;
}

.check {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--c-subtext1);
  padding: 2px 0;
  cursor: pointer;
  user-select: none;
}

.range {
  display: flex;
  gap: 6px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 3px;
  margin: 6px 0;
}

.field-label {
  font-size: 11px;
  color: var(--c-overlay0);
}

.num {
  width: 100%;
  padding: 5px 8px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  color: var(--c-text);
  font-size: 12px;
  box-sizing: border-box;
}

.num:focus {
  outline: none;
  border-color: var(--c-blue);
}

.text-input {
  width: 100%;
  padding: 5px 8px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  color: var(--c-text);
  font-size: 12px;
  box-sizing: border-box;
}

.text-input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.text-area {
  width: 100%;
  padding: 6px 8px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  color: var(--c-text);
  font-size: 12px;
  font-family: var(--font-mono);
  box-sizing: border-box;
  resize: vertical;
}

.text-area:focus {
  outline: none;
  border-color: var(--c-blue);
}

.apply-btn {
  margin-top: 6px;
  padding: 6px 16px;
  border: none;
  border-radius: 5px;
  background: var(--c-blue);
  color: var(--c-base);
  cursor: pointer;
  font-size: 12px;
}

.apply-btn:hover {
  background: var(--c-sapphire);
}
</style>
