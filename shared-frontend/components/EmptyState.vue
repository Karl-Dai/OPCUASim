<script setup lang="ts">
// Guiding empty-state block: icon (via default slot) + title + optional hint.
// `compact` suits narrow side panels; the default `full` variant stretches to
// fill its container and lays a faint blue glow behind the icon for depth.
defineProps<{
  title: string
  hint?: string
  compact?: boolean
}>()
</script>

<template>
  <div :class="['empty-state-box', compact ? 'compact' : 'full']">
    <div class="empty-state-icon"><slot /></div>
    <div class="empty-state-title">{{ title }}</div>
    <div v-if="hint" class="empty-state-hint">{{ hint }}</div>
  </div>
</template>

<style scoped>
.empty-state-box {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: 10px;
}

.empty-state-box.compact {
  padding: 36px 18px;
  gap: 8px;
}

.empty-state-box.full {
  flex: 1;
  width: 100%;
  padding: 24px;
  background: radial-gradient(ellipse 55% 45% at 50% 44%, rgba(137, 180, 250, 0.045), transparent 72%);
}

.empty-state-icon {
  display: flex;
  color: var(--c-surface2);
}

.empty-state-box.compact .empty-state-icon :deep(svg) { width: 32px; height: 32px; }
.empty-state-box.full .empty-state-icon :deep(svg) { width: 48px; height: 48px; }

.empty-state-title {
  color: var(--c-subtext0);
  font-size: 13px;
  font-weight: 600;
}
.empty-state-box.compact .empty-state-title { font-size: 12px; }

.empty-state-hint {
  color: var(--c-overlay0);
  font-size: 12px;
  line-height: 1.5;
  max-width: 280px;
}
.empty-state-box.compact .empty-state-hint { font-size: 11px; }
</style>
