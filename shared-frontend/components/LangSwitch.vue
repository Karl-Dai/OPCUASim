<script setup lang="ts">
import { useI18n } from '../i18n'

const { locale, setLocale } = useI18n()

function toggle() {
  setLocale(locale.value === 'zh-CN' ? 'en-US' : 'zh-CN')
}
</script>

<template>
  <button class="lang-switch" :title="locale === 'zh-CN' ? 'Switch to English' : '切换到中文'" @click="toggle">
    <span :class="['seg', { active: locale === 'zh-CN' }]">中</span>
    <span class="sep">/</span>
    <span :class="['seg', { active: locale === 'en-US' }]">EN</span>
  </button>
</template>

<style scoped>
.lang-switch {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  margin-right: 8px;
  background: transparent;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-overlay0);
  cursor: pointer;
  font-size: 12px;
  font-family: inherit;
}
.lang-switch:hover { color: var(--c-text); background: var(--c-surface0); }
.seg { padding: 0 2px; }
.seg.active { color: var(--c-text); font-weight: 600; }
.sep { color: var(--c-surface1); }
</style>
