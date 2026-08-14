<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from '@shared/i18n'
import { useServerContext } from '../inject'

const { t } = useI18n()
const { status, lastSimSeq } = useServerContext()

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
</script>

<template>
  <footer class="status-bar">
    <span class="chip" :style="{ color: stateView.color }">
      <span class="chip-icon">{{ stateView.icon }}</span>
      <span>{{ stateView.label }}</span>
    </span>
    <span class="divider" />
    <span class="meta">{{ t('statusBar.folders', { count: status.folder_count }) }}</span>
    <span class="meta">{{ t('statusBar.nodes', { count: status.node_count }) }}</span>
    <span class="divider" />
    <span class="meta faint">{{ t('statusBar.endpoint') }}</span>
    <span class="endpoint">{{ status.endpoint_url }}</span>
    <span class="spacer" />
    <span class="seq">{{ t('statusBar.seq', { seq: lastSimSeq }) }}</span>
  </footer>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 100%;
  padding: 0 12px;
  font-size: 12px;
  color: var(--c-subtext0);
  user-select: none;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-weight: 600;
  white-space: nowrap;
}

.chip-icon {
  font-size: 11px;
}

.divider {
  width: 1px;
  height: 14px;
  background: var(--c-surface0);
}

.meta {
  color: var(--c-subtext0);
  white-space: nowrap;
}

.faint {
  color: var(--c-overlay0);
}

.endpoint {
  color: var(--c-subtext0);
  font-family: var(--font-mono);
  font-size: 11px;
  white-space: nowrap;
}

.spacer {
  flex: 1;
}

.seq {
  color: var(--c-overlay0);
  font-family: var(--font-mono);
  font-size: 11px;
  white-space: nowrap;
}
</style>
