<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import { useServerContext } from '../inject'
import type { ServerConfig } from '../types'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { config, refreshConfig, refreshStatus } = useServerContext()

const SECURITY_POLICIES = [
  'None',
  'Basic128Rsa15',
  'Basic256',
  'Basic256Sha256',
  'Aes128Sha256RsaOaep',
  'Aes256Sha256RsaPss',
]

const SECURITY_MODES = ['None', 'Sign', 'SignAndEncrypt']

function defaultDraft(): ServerConfig {
  return {
    name: 'OPCUAServer Simulator',
    endpoint_url: 'opc.tcp://0.0.0.0:4840',
    port: 4840,
    security_policies: ['None'],
    security_modes: ['None'],
    users: [],
    anonymous_enabled: true,
    max_sessions: 100,
    max_subscriptions_per_session: 50,
    history_buffer_size: 10000,
    event_history_size: 1000,
  }
}

const draft = ref<ServerConfig>(defaultDraft())

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      draft.value = config.value
        ? (JSON.parse(JSON.stringify(config.value)) as ServerConfig)
        : defaultDraft()
    }
  },
)

function toggleList(list: string[], value: string, checked: boolean) {
  const index = list.indexOf(value)
  if (checked && index < 0) list.push(value)
  else if (!checked && index >= 0) list.splice(index, 1)
}

function onPolicyChange(policy: string, e: Event) {
  toggleList(draft.value.security_policies, policy, (e.target as HTMLInputElement).checked)
}

function onModeChange(mode: string, e: Event) {
  toggleList(draft.value.security_modes, mode, (e.target as HTMLInputElement).checked)
}

async function saveConfig() {
  try {
    await invoke('update_config', { config: draft.value })
    await refreshConfig()
    await refreshStatus()
    emit('close')
  } catch (error) {
    await showAlert(String(error))
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="dialog-backdrop dialog-blur" @mousedown.self="emit('close')">
        <div class="config-dialog" role="dialog" aria-modal="true">
          <div class="dialog-header">
            <span class="dialog-title">{{ t('config.title') }}</span>
          </div>
          <div class="dialog-body">
            <div class="grid">
              <label class="field">
                <span>{{ t('config.name') }}</span>
                <input v-model="draft.name" class="input" type="text" />
              </label>
              <label class="field">
                <span>{{ t('config.port') }}</span>
                <input v-model.number="draft.port" class="input" type="number" min="1" max="65535" />
              </label>
              <label class="field field-wide">
                <span>{{ t('config.endpointUrl') }}</span>
                <input v-model="draft.endpoint_url" class="input" type="text" />
              </label>
              <label class="field">
                <span>{{ t('config.maxSessions') }}</span>
                <input v-model.number="draft.max_sessions" class="input" type="number" min="1" />
              </label>
              <label class="field">
                <span>{{ t('config.maxSubscriptions') }}</span>
                <input
                  v-model.number="draft.max_subscriptions_per_session"
                  class="input"
                  type="number"
                  min="1"
                />
              </label>
              <label class="field">
                <span>{{ t('config.historyBuffer') }}</span>
                <input
                  v-model.number="draft.history_buffer_size"
                  class="input"
                  type="number"
                  min="0"
                />
              </label>
              <label class="field">
                <span>{{ t('config.eventHistory') }}</span>
                <input
                  v-model.number="draft.event_history_size"
                  class="input"
                  type="number"
                  min="0"
                />
              </label>
              <label class="check">
                <input v-model="draft.anonymous_enabled" type="checkbox" />
                <span>{{ t('config.anonymousEnabled') }}</span>
              </label>
            </div>

            <div class="section">
              <span class="section-label">{{ t('config.securityPolicies') }}</span>
              <div class="checks">
                <label v-for="policy in SECURITY_POLICIES" :key="policy" class="check">
                  <input
                    type="checkbox"
                    :checked="draft.security_policies.includes(policy)"
                    @change="onPolicyChange(policy, $event)"
                  />
                  <span>{{ policy }}</span>
                </label>
              </div>
            </div>

            <div class="section">
              <span class="section-label">{{ t('config.securityModes') }}</span>
              <div class="checks">
                <label v-for="mode in SECURITY_MODES" :key="mode" class="check">
                  <input
                    type="checkbox"
                    :checked="draft.security_modes.includes(mode)"
                    @change="onModeChange(mode, $event)"
                  />
                  <span>{{ mode }}</span>
                </label>
              </div>
            </div>
          </div>
          <div class="dialog-footer">
            <button class="btn btn-secondary" @click="emit('close')">{{ t('common.cancel') }}</button>
            <button class="btn btn-primary" @click="saveConfig">{{ t('config.save') }}</button>
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

.config-dialog {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 560px;
  max-width: 92vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.dialog-header {
  padding: 16px 20px 0;
}

.dialog-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
}

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

.btn-primary {
  background: var(--c-blue);
  color: var(--c-base);
}

.btn-primary:hover {
  background: var(--c-sapphire);
}

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}

.btn-secondary:hover {
  background: var(--c-surface2);
}

.grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--c-subtext0);
}

.field-wide {
  grid-column: 1 / -1;
}

.input {
  padding: 6px 10px;
  background: var(--c-crust);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  color: var(--c-text);
  font-size: 13px;
  box-sizing: border-box;
}

.input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.section {
  margin-top: 14px;
}

.section-label {
  display: block;
  font-size: 12px;
  font-weight: 600;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}

.checks {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 16px;
}

.check {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--c-subtext1);
  cursor: pointer;
  user-select: none;
}
</style>
