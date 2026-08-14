<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import { useMasterContext } from '../inject'
import type { AuthRequest, ConnectionInfo, CreateConnectionRequest, DiscoveredEndpointDto } from '../types'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{
  (e: 'close'): void
  (e: 'created', conn: ConnectionInfo): void
}>()

const { t } = useI18n()
const { createConnection } = useMasterContext()

const SECURITY_POLICIES = [
  'None',
  'Basic128Rsa15',
  'Basic256',
  'Basic256Sha256',
  'Aes128_Sha256_RsaOaep',
  'Aes256_Sha256_RsaPss',
]

const SECURITY_MODES = ['None', 'Sign', 'SignAndEncrypt']

type AuthKind = 'anonymous' | 'user_password' | 'certificate'

const name = ref('New Connection')
const endpointUrl = ref('opc.tcp://localhost:4840')
const securityPolicy = ref('None')
const securityMode = ref('None')
const authKind = ref<AuthKind>('anonymous')
const username = ref('')
const password = ref('')
const certPath = ref('')
const keyPath = ref('')
const timeoutMs = ref(5000)

const discovering = ref(false)
const discovered = ref<DiscoveredEndpointDto[]>([])
const error = ref('')

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      error.value = ''
      discovered.value = []
      discovering.value = false
    }
  },
)

async function onDiscover() {
  const url = endpointUrl.value.trim()
  if (!url) {
    error.value = t('newConn.urlRequired')
    return
  }
  discovering.value = true
  error.value = ''
  try {
    discovered.value = await invoke<DiscoveredEndpointDto[]>('discover_endpoints', {
      url,
      timeoutMs: timeoutMs.value,
    })
  } catch (err) {
    error.value = String(err)
  } finally {
    discovering.value = false
  }
}

function selectEndpoint(ep: DiscoveredEndpointDto) {
  securityPolicy.value = ep.security_policy
  securityMode.value = ep.security_mode
  endpointUrl.value = ep.endpoint_url
}

function validate(): string | null {
  if (!name.value.trim()) return t('newConn.nameRequired')
  const url = endpointUrl.value.trim()
  if (!url) return t('newConn.urlRequired')
  if (!url.startsWith('opc.tcp://')) return t('newConn.urlInvalid')
  if (authKind.value === 'user_password' && !username.value.trim()) {
    return t('newConn.usernameRequired')
  }
  if (authKind.value === 'certificate' && (!certPath.value.trim() || !keyPath.value.trim())) {
    return t('newConn.certPathsRequired')
  }
  return null
}

async function onSubmit() {
  const validationError = validate()
  if (validationError) {
    error.value = validationError
    return
  }

  let auth: AuthRequest
  if (authKind.value === 'user_password') {
    auth = { type: 'user_password', username: username.value.trim(), password: password.value }
  } else if (authKind.value === 'certificate') {
    auth = { type: 'certificate', cert_path: certPath.value.trim(), key_path: keyPath.value.trim() }
  } else {
    auth = { type: 'anonymous' }
  }

  const request: CreateConnectionRequest = {
    name: name.value.trim(),
    endpoint_url: endpointUrl.value.trim(),
    security_policy: securityPolicy.value,
    security_mode: securityMode.value,
    auth,
    timeout_ms: timeoutMs.value,
  }

  try {
    const conn = await createConnection(request)
    emit('created', conn)
    emit('close')
  } catch (err) {
    error.value = String(err)
    await showAlert(t('newConn.createFailed', { error: String(err) }))
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="dialog-backdrop dialog-blur" @mousedown.self="emit('close')">
        <div class="conn-dialog" role="dialog" aria-modal="true">
          <div class="dialog-header">
            <span class="dialog-title">{{ t('newConn.title') }}</span>
          </div>
          <div class="dialog-body">
            <div class="grid">
              <label class="field field-wide">
                <span>{{ t('newConn.name') }}</span>
                <input v-model="name" class="input" type="text" :placeholder="t('newConn.nameHint')" />
              </label>

              <label class="field field-wide">
                <span>{{ t('newConn.endpointUrl') }}</span>
                <div class="url-row">
                  <input v-model="endpointUrl" class="input url-input" type="text" />
                  <button class="discover-btn" :disabled="discovering" @click="onDiscover">
                    {{ discovering ? t('newConn.discovering') : t('newConn.discover') }}
                  </button>
                </div>
              </label>

              <label class="field">
                <span>{{ t('newConn.securityPolicy') }}</span>
                <select v-model="securityPolicy" class="input">
                  <option v-for="p in SECURITY_POLICIES" :key="p" :value="p">{{ p }}</option>
                </select>
              </label>

              <label class="field">
                <span>{{ t('newConn.securityMode') }}</span>
                <select v-model="securityMode" class="input">
                  <option v-for="m in SECURITY_MODES" :key="m" :value="m">{{ m }}</option>
                </select>
              </label>

              <label class="field">
                <span>{{ t('newConn.auth') }}</span>
                <select v-model="authKind" class="input">
                  <option value="anonymous">{{ t('newConn.authAnonymous') }}</option>
                  <option value="user_password">{{ t('newConn.authUserPassword') }}</option>
                  <option value="certificate">{{ t('newConn.authCertificate') }}</option>
                </select>
              </label>

              <label class="field">
                <span>{{ t('newConn.timeoutMs') }}</span>
                <input v-model.number="timeoutMs" class="input" type="number" min="500" max="60000" />
              </label>

              <template v-if="authKind === 'user_password'">
                <label class="field">
                  <span>{{ t('newConn.username') }}</span>
                  <input v-model="username" class="input" type="text" />
                </label>
                <label class="field">
                  <span>{{ t('newConn.password') }}</span>
                  <input v-model="password" class="input" type="password" />
                </label>
              </template>

              <template v-else-if="authKind === 'certificate'">
                <label class="field field-wide">
                  <span>{{ t('newConn.certPath') }}</span>
                  <input v-model="certPath" class="input" type="text" />
                </label>
                <label class="field field-wide">
                  <span>{{ t('newConn.keyPath') }}</span>
                  <input v-model="keyPath" class="input" type="text" />
                </label>
              </template>
            </div>

            <div v-if="discovered.length > 0" class="discovered">
              <span class="section-label">{{ t('newConn.discovered', { count: discovered.length }) }}</span>
              <div class="endpoint-table">
                <div class="ep-row ep-head">
                  <span>{{ t('newConn.securityPolicy') }}</span>
                  <span>{{ t('newConn.securityMode') }}</span>
                  <span>{{ t('newConn.endpointUrl') }}</span>
                </div>
                <div
                  v-for="ep in discovered"
                  :key="ep.endpoint_url"
                  class="ep-row"
                  @click="selectEndpoint(ep)"
                >
                  <span class="mono">{{ ep.security_policy }}</span>
                  <span class="mono">{{ ep.security_mode }}</span>
                  <span class="mono url">{{ ep.endpoint_url }}</span>
                </div>
              </div>
            </div>

            <p v-if="error" class="error">{{ error }}</p>
          </div>
          <div class="dialog-footer">
            <button class="btn btn-secondary" @click="emit('close')">{{ t('common.cancel') }}</button>
            <button class="btn btn-primary" @click="onSubmit">{{ t('newConn.create') }}</button>
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

.conn-dialog {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 640px;
  max-width: 94vw;
  max-height: 92vh;
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
.btn-primary:hover { background: var(--c-sapphire); }

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}
.btn-secondary:hover { background: var(--c-surface2); }

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
  width: 100%;
}

.input:focus {
  outline: none;
  border-color: var(--c-blue);
}

.url-row {
  display: flex;
  gap: 6px;
}

.url-input {
  flex: 1 1 auto;
  min-width: 0;
}

.discover-btn {
  flex: none;
  padding: 0 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  background: var(--c-surface0);
  color: var(--c-text);
  cursor: pointer;
  font-size: 12px;
}
.discover-btn:disabled { opacity: 0.5; cursor: default; }

.discovered {
  margin-top: 14px;
}

.section-label {
  display: block;
  font-size: 12px;
  font-weight: 600;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}

.endpoint-table {
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  overflow: hidden;
}

.ep-row {
  display: grid;
  grid-template-columns: 160px 120px 1fr;
  gap: 8px;
  padding: 6px 10px;
  font-size: 11px;
  color: var(--c-subtext1);
  border-bottom: 1px solid var(--c-surface0);
  cursor: pointer;
}
.ep-row:last-child { border-bottom: none; }
.ep-row:not(.ep-head):hover { background: var(--c-surface0); }
.ep-head {
  background: var(--c-mantle);
  color: var(--c-overlay0);
  font-weight: 600;
  cursor: default;
}

.mono {
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mono.url { color: var(--c-subtext0); }

.error {
  margin-top: 10px;
  font-size: 12px;
  color: var(--c-red);
  overflow-wrap: anywhere;
}
</style>
