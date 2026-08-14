import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'

export function useClipboardFlash(timeoutMs = 1500) {
  const { t } = useI18n()
  const flash = ref('')
  let timer: number | null = null

  async function copy(text: string, label?: string) {
    try {
      await navigator.clipboard.writeText(text)
      flash.value = `${label ?? text} ${t('about.copiedSuffix')}`
    } catch {
      flash.value = text
    }
    if (timer !== null) clearTimeout(timer)
    timer = window.setTimeout(() => { flash.value = ''; timer = null }, timeoutMs)
  }

  // Open an external URL in the system browser. Inside a Tauri webview a plain
  // <a>/window.open can't reach the OS browser, so we go through the opener
  // plugin. We call its IPC command directly via `invoke` instead of importing
  // `@tauri-apps/plugin-opener`: shared-frontend is compiled as its own
  // `vue-tsc -b` project and only bridges `@tauri-apps/api/*`
  // (see shared-frontend/vite/aliases.ts), so the plugin package can't resolve.
  // `openUrl(url)` is exactly `invoke('plugin:opener|open_url', { url })`.
  // Outside Tauri (pure-browser static render) invoke throws — fall back to
  // copying the URL so the action is never a dead click.
  async function openOrCopy(url: string, label?: string) {
    try {
      await invoke('plugin:opener|open_url', { url })
    } catch {
      await copy(url, label)
    }
  }

  onUnmounted(() => {
    if (timer !== null) clearTimeout(timer)
  })

  return { flash, copy, openOrCopy }
}
