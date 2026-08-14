<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert, showConfirm } from '@shared/composables/useDialog'
import type { CertRole, CertSummaryDto } from '../types'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()

const { t } = useI18n()

const trusted = ref<CertSummaryDto[]>([])
const rejected = ref<CertSummaryDto[]>([])
const selectedPath = ref<string | null>(null)
const error = ref('')

async function load() {
  error.value = ''
  try {
    const [tList, rList] = await Promise.all([
      invoke<CertSummaryDto[]>('list_certificates', { role: 'trusted' as CertRole }),
      invoke<CertSummaryDto[]>('list_certificates', { role: 'rejected' as CertRole }),
    ])
    trusted.value = tList
    rejected.value = rList
    if (selectedPath.value && ![...tList, ...rList].some((c) => c.path === selectedPath.value)) {
      selectedPath.value = null
    }
  } catch (err) {
    error.value = String(err)
    await showAlert(t('cert.loadFailed', { error: String(err) }))
  }
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      selectedPath.value = null
      void load()
    }
  },
)

async function move(path: string, toRole: CertRole) {
  try {
    await invoke('move_certificate', { path, toRole })
    await load()
  } catch (err) {
    error.value = String(err)
    await showAlert(t('cert.moveFailed', { error: String(err) }))
  }
}

async function remove(path: string) {
  if (!(await showConfirm(t('common.delete')))) return
  try {
    await invoke('delete_certificate', { path })
    if (selectedPath.value === path) selectedPath.value = null
    await load()
  } catch (err) {
    error.value = String(err)
    await showAlert(t('cert.deleteFailed', { error: String(err) }))
  }
}

function detailFor(path: string): CertSummaryDto | null {
  return trusted.value.find((c) => c.path === path)
    ?? rejected.value.find((c) => c.path === path)
    ?? null
}

const selectedDetail = (): CertSummaryDto | null =>
  selectedPath.value ? detailFor(selectedPath.value) : null
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="dialog-backdrop dialog-blur" @mousedown.self="emit('close')">
        <div class="cert-dialog" role="dialog" aria-modal="true">
          <div class="dialog-header">
            <span class="dialog-title">{{ t('cert.title') }}</span>
            <button class="refresh-btn" @click="load">{{ t('cert.refresh') }}</button>
          </div>
          <div class="dialog-body">
            <p class="pki-hint">{{ t('cert.pkiDir') }}: <code>./pki</code></p>
            <p v-if="error" class="error">{{ error }}</p>

            <div class="panes">
              <div class="pane">
                <div class="pane-title">{{ t('cert.trusted') }} ({{ trusted.length }})</div>
                <div class="pane-list">
                  <div
                    v-for="c in trusted"
                    :key="c.path"
                    :class="['cert-item', { selected: selectedPath === c.path }]"
                    @click="selectedPath = c.path"
                  >
                    <span class="cert-name">📄 {{ c.subject_cn }}</span>
                  </div>
                  <div v-if="trusted.length === 0" class="pane-empty">{{ t('common.loading') }}</div>
                </div>
              </div>
              <div class="pane">
                <div class="pane-title">{{ t('cert.rejected') }} ({{ rejected.length }})</div>
                <div class="pane-list">
                  <div
                    v-for="c in rejected"
                    :key="c.path"
                    :class="['cert-item', { selected: selectedPath === c.path }]"
                    @click="selectedPath = c.path"
                  >
                    <span class="cert-name">📄 {{ c.subject_cn }}</span>
                  </div>
                  <div v-if="rejected.length === 0" class="pane-empty">{{ t('common.loading') }}</div>
                </div>
              </div>
            </div>

            <div v-if="selectedDetail()" class="detail">
              <div class="kv"><span>{{ t('cert.file') }}</span><span class="mono">{{ selectedDetail()!.file_name }}</span></div>
              <div class="kv"><span>{{ t('cert.issuer') }}</span><span>{{ selectedDetail()!.issuer_cn }}</span></div>
              <div class="kv"><span>{{ t('cert.thumbprint') }}</span><span class="mono">{{ selectedDetail()!.thumbprint }}</span></div>
              <div class="kv"><span>{{ t('cert.validity') }}</span><span>{{ selectedDetail()!.valid_from }} → {{ selectedDetail()!.valid_to }}</span></div>
              <div class="detail-actions">
                <button
                  v-if="selectedDetail()!.role === 'trusted'"
                  class="btn-action"
                  @click="move(selectedDetail()!.path, 'rejected')"
                >{{ t('cert.moveToRejected') }}</button>
                <button
                  v-else
                  class="btn-action"
                  @click="move(selectedDetail()!.path, 'trusted')"
                >{{ t('cert.moveToTrusted') }}</button>
                <button class="btn-action danger" @click="remove(selectedDetail()!.path)">
                  {{ t('cert.delete') }}
                </button>
              </div>
            </div>
          </div>
          <div class="dialog-footer">
            <button class="btn btn-secondary" @click="emit('close')">{{ t('cert.close') }}</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
}

.cert-dialog {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 900px;
  max-width: 94vw;
  max-height: 92vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.dialog-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 20px 0;
}

.dialog-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
  flex: 1 1 auto;
}

.refresh-btn {
  flex: none;
  padding: 4px 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
  font-size: 12px;
}
.refresh-btn:hover { background: var(--c-surface1); }

.dialog-body {
  padding: 12px 20px 8px;
  overflow-y: auto;
  min-height: 0;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 20px 16px;
}

.btn {
  padding: 7px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}
.btn-secondary { background: var(--c-surface1); color: var(--c-text); }
.btn-secondary:hover { background: var(--c-surface2); }

.pki-hint {
  font-size: 12px;
  color: var(--c-subtext0);
  margin: 0 0 8px;
}
.pki-hint code {
  font-family: var(--font-mono);
  color: var(--c-subtext1);
}

.error {
  font-size: 12px;
  color: var(--c-red);
  margin: 0 0 8px;
  overflow-wrap: anywhere;
}

.panes {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.pane {
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  overflow: hidden;
}

.pane-title {
  padding: 6px 10px;
  font-size: 12px;
  font-weight: 700;
  color: var(--c-subtext0);
  background: var(--c-mantle);
  border-bottom: 1px solid var(--c-surface0);
}

.pane-list {
  max-height: 300px;
  overflow-y: auto;
}

.pane-empty {
  padding: 12px;
  font-size: 11px;
  color: var(--c-overlay0);
}

.cert-item {
  padding: 6px 10px;
  font-size: 12px;
  color: var(--c-subtext1);
  cursor: pointer;
  border-bottom: 1px solid var(--c-surface0);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.cert-item:last-child { border-bottom: none; }
.cert-item:hover { background: var(--c-surface0); }
.cert-item.selected { background: var(--c-surface1); }

.cert-name { overflow: hidden; text-overflow: ellipsis; }

.detail {
  margin-top: 12px;
  border-top: 1px solid var(--c-surface0);
  padding-top: 10px;
}

.kv {
  display: flex;
  gap: 10px;
  font-size: 12px;
  color: var(--c-subtext1);
  padding: 2px 0;
}
.kv > span:first-child { color: var(--c-overlay0); flex: none; }

.mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-subtext0);
  overflow-wrap: anywhere;
}

.detail-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.btn-action {
  padding: 5px 14px;
  border: 1px solid var(--c-surface1);
  border-radius: 5px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
  font-size: 12px;
}
.btn-action:hover { background: var(--c-surface1); }
.btn-action.danger { color: var(--c-red); }
</style>
