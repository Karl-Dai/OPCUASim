import { ref } from 'vue'
import { useI18n } from '@shared/i18n'
import { showAlert, showPrompt } from '@shared/composables/useDialog'
import { useMasterContext } from './inject'
import type { ConnectionInfo } from './types'

export function useConnectionRename() {
  const { t } = useI18n()
  const { renameConnection } = useMasterContext()
  const renaming = ref(false)

  async function requestRename(conn: ConnectionInfo) {
    if (renaming.value) return
    renaming.value = true
    try {
      const input = await showPrompt(t('toolbar.renamePrompt'), conn.name)
      if (input === null) return
      const name = input.trim()
      if (!name) {
        await showAlert(t('newConn.nameRequired'))
        return
      }
      if (name !== conn.name) await renameConnection(conn.id, name)
    } catch (error) {
      await showAlert(t('toolbar.renameFailed', { error: String(error) }))
    } finally {
      renaming.value = false
    }
  }

  return { requestRename, renaming }
}
