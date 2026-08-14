<script setup lang="ts">
import { computed } from 'vue'
import type { ChartPoint } from '../domain'

const props = defineProps<{ points: ChartPoint[] }>()

const W = 1000
const H = 220
const PAD = 12

const view = computed(() => {
  const pts = props.points
  if (pts.length === 0) return { polyline: '', dot: null as { cx: number; cy: number } | null }

  const xs = pts.map((p) => p.x)
  const ys = pts.map((p) => p.y)
  const minX = Math.min(...xs)
  const maxX = Math.max(...xs)
  const minY = Math.min(...ys)
  const maxY = Math.max(...ys)
  const spanX = maxX - minX || 1
  const spanY = maxY - minY || 1

  const mapX = (x: number) => PAD + ((x - minX) / spanX) * (W - PAD * 2)
  const mapY = (y: number) => H - PAD - ((y - minY) / spanY) * (H - PAD * 2)

  const coords = pts.map((p) => ({ cx: mapX(p.x), cy: mapY(p.y) }))
  const polyline = coords.map((c) => `${c.cx.toFixed(1)},${c.cy.toFixed(1)}`).join(' ')
  return { polyline, dot: pts.length === 1 ? coords[0] : null }
})
</script>

<template>
  <svg
    class="history-chart"
    :viewBox="`0 0 ${W} ${H}`"
    preserveAspectRatio="none"
    role="img"
    aria-label="history line chart"
  >
    <polyline
      v-if="view.polyline"
      :points="view.polyline"
      fill="none"
      stroke="var(--c-blue)"
      stroke-width="1.6"
      vector-effect="non-scaling-stroke"
    />
    <circle
      v-if="view.dot"
      :cx="view.dot.cx"
      :cy="view.dot.cy"
      r="3"
      fill="var(--c-blue)"
    />
  </svg>
</template>

<style scoped>
.history-chart {
  display: block;
  width: 100%;
  height: 220px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface0);
  border-radius: 6px;
}
</style>
