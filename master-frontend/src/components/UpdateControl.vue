<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from '@shared/i18n'

interface UpdateMeta { version: string; notes: string; pub_date: string | null }
interface Progress { stage: string; downloaded: number; total: number | null }
const { t } = useI18n()
const busy = ref(false)
const stage = ref('idle')
const downloaded = ref(0)
const total = ref<number | null>(null)
const error = ref('')
const update = ref<UpdateMeta | null>(null)
const visible = ref(false)
const installable = ref(false)
const scheduled = ref(false)
const control = ref<HTMLButtonElement | null>(null)
const dialog = ref<HTMLDivElement | null>(null)
let notifiedVersion = ''
let stopped = false
let unlisten: UnlistenFn | undefined
let timer: ReturnType<typeof setInterval> | undefined

const percentage = computed(() => total.value && total.value > 0
  ? Math.min(100, Math.floor(downloaded.value / total.value * 100)) : null)
const label = computed(() => {
  if (error.value) return t('update.failed')
  if (stage.value === 'downloading') return percentage.value === null
    ? t('update.downloading') : t('update.downloadPercent', { percent: percentage.value })
  if (busy.value) return t(`update.${stage.value}`)
  if (scheduled.value) return t('update.scheduled')
  if (update.value) return t('update.ready', { version: update.value.version })
  return stage.value === 'latest' ? t('update.latest') : t('update.check')
})

async function showDialog() {
  visible.value = true
  await nextTick()
  dialog.value?.focus()
}

async function check(force: boolean) {
  if (busy.value || (!force && (update.value || scheduled.value))) return
  busy.value = true
  stage.value = 'checking'
  error.value = ''
  downloaded.value = 0
  total.value = null
  try {
    const result = await invoke<UpdateMeta | null>('check_for_update', { force })
    if (stopped) return
    update.value = result
    stage.value = result ? 'ready' : force ? 'latest' : 'idle'
    if (result && (force || result.version !== notifiedVersion)) {
      notifiedVersion = result.version
      await showDialog()
    }
  } catch (cause) {
    if (!stopped) { error.value = String(cause); stage.value = 'idle' }
  } finally {
    busy.value = false
  }
}

function close() {
  if (busy.value) return
  visible.value = false
  void nextTick(() => control.value?.focus())
}

async function choose(command: 'install_update' | 'schedule_update_on_next_launch' | 'skip_update') {
  if (!update.value || busy.value) return
  busy.value = true
  stage.value = command === 'install_update' ? 'installing' : 'saving'
  error.value = ''
  try {
    await invoke(command, { version: update.value.version })
    if (command === 'schedule_update_on_next_launch') scheduled.value = true
    if (command === 'skip_update') { update.value = null; notifiedVersion = '' }
    busy.value = false
    stage.value = 'idle'
    close()
  } catch (cause) {
    error.value = String(cause)
  } finally { busy.value = false }
}

function onControl() {
  if (update.value && !error.value) void showDialog()
  else void check(true)
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); close() }
  if (event.key !== 'Tab') return
  const buttons = dialog.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')
  if (!buttons?.length) { event.preventDefault(); return }
  const first = buttons[0]!
  const last = buttons[buttons.length - 1]!
  if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.value)) {
    event.preventDefault(); last.focus()
  } else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialog.value)) {
    event.preventDefault(); first.focus()
  }
}

onMounted(async () => {
  try {
    const off = await listen<Progress>('update-progress', ({ payload }) => {
      if (!busy.value) return
      stage.value = payload.stage
      downloaded.value = payload.downloaded
      total.value = payload.total
    })
    if (stopped) { off(); return }
    unlisten = off
    installable.value = await invoke<boolean>('can_install_update')
    if (stopped) return
    void check(false)
    // The backend throttles successful checks to six hours. Retry a failed
    // request on the next hourly tick without repeatedly showing ready updates.
    timer = setInterval(() => void check(false), 60 * 60 * 1000)
  } catch (cause) { if (!stopped) error.value = String(cause) }
})
onUnmounted(() => { stopped = true; unlisten?.(); if (timer) clearInterval(timer) })
</script>

<template>
  <div class="update-control" :aria-busy="busy">
    <button ref="control" type="button" class="toolbar-btn" :class="{ 'update-error': error }"
      :disabled="busy" :title="error || t('update.hint')" @click="onControl">
      <span role="status" aria-live="polite">{{ label }}</span>
    </button>
    <Teleport to="body">
      <div v-if="visible && update" class="update-backdrop" @mousedown.self="close" @keydown="onKey">
        <div ref="dialog" class="update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title"
          tabindex="-1" :aria-busy="busy">
          <h2 id="update-title">{{ t('update.title', { version: update.version }) }}</h2>
          <p>{{ t('update.verified') }}</p>
          <p v-if="!installable" class="update-note">{{ t('update.devBuild') }}</p>
          <pre v-if="update.notes" class="update-notes">{{ update.notes }}</pre>
          <p v-if="error" class="update-error" role="alert">{{ t('update.errorDetail', { error }) }}</p>
          <p v-if="busy" role="status" aria-live="polite">{{ label }}</p>
          <div class="update-actions">
            <button type="button" :disabled="busy" @click="close">{{ t('update.later') }}</button>
            <button type="button" :disabled="busy" @click="choose('skip_update')">{{ t('update.skip') }}</button>
            <button type="button" :disabled="busy || !installable" @click="choose('schedule_update_on_next_launch')">{{ t('update.nextLaunch') }}</button>
            <button type="button" class="update-primary" :disabled="busy || !installable" @click="choose('install_update')">{{ t('update.install') }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.update-error { color: var(--c-red); overflow-wrap: anywhere; }
.update-backdrop { position: fixed; inset: 0; z-index: 1800; background: rgba(0, 0, 0, .6); display: flex; align-items: center; justify-content: center; }
.update-dialog { width: 560px; max-width: 90vw; max-height: 85vh; overflow-y: auto; background: var(--c-base); color: var(--c-text); border: 1px solid var(--c-surface1); border-radius: 8px; padding: 20px; }
.update-dialog h2 { font-size: 16px; margin-bottom: 12px; }
.update-dialog p { font-size: 13px; line-height: 1.6; margin-bottom: 12px; }
.update-note { color: var(--c-yellow); }
.update-notes { font: inherit; font-size: 13px; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 40vh; overflow-y: auto; margin: 16px 0; }
.update-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.update-actions button { font: inherit; font-size: 12px; padding: 8px 12px; border: 1px solid var(--c-surface1); border-radius: 4px; background: var(--c-surface0); color: var(--c-text); cursor: pointer; }
.update-actions button:hover:not(:disabled) { background: var(--c-surface1); }
.update-actions .update-primary { background: var(--c-blue); color: var(--c-base); }
.update-actions button:disabled { opacity: .4; cursor: default; }
</style>
