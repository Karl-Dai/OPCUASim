<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { showAlert } from '@shared/composables/useDialog'
import type { CallMethodRequest, MethodArgInfo, MethodArgValue } from '../types'

const props = defineProps<{
  visible: boolean
  connectionId: string
  objectId: string
  methodId: string
  displayName: string
}>()
const emit = defineEmits<{ (e: 'close'): void }>()

const { t } = useI18n()

const inputsMeta = ref<MethodArgInfo[]>([])
const outputsMeta = ref<MethodArgInfo[]>([])
const inputValues = ref<string[]>([])
const loadingArgs = ref(false)
const calling = ref(false)
const resultStatus = ref<string | null>(null)
const resultOutputs = ref<MethodArgValue[]>([])
const error = ref('')

function defaultForType(dataType: string): string {
  switch (dataType) {
    case 'Boolean':
      return 'false'
    case 'String':
      return ''
    case 'Float':
    case 'Double':
      return '0.0'
    default:
      return '0'
  }
}

async function loadArgs() {
  loadingArgs.value = true
  error.value = ''
  try {
    const args = await invoke<{ inputs: MethodArgInfo[]; outputs: MethodArgInfo[] }>(
      'read_method_arguments',
      { connectionId: props.connectionId, methodId: props.methodId },
    )
    inputsMeta.value = args.inputs
    outputsMeta.value = args.outputs
    inputValues.value = args.inputs.map((a) => defaultForType(a.data_type))
  } catch (err) {
    error.value = String(err)
    await showAlert(t('methodCall.argsFailed', { error: String(err) }))
  } finally {
    loadingArgs.value = false
  }
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      inputsMeta.value = []
      outputsMeta.value = []
      inputValues.value = []
      resultStatus.value = null
      resultOutputs.value = []
      error.value = ''
      void loadArgs()
    }
  },
)

async function onCall() {
  if (calling.value) return
  calling.value = true
  error.value = ''
  try {
    const inputs: MethodArgValue[] = inputsMeta.value.map((meta, i) => ({
      data_type: meta.data_type,
      value: inputValues.value[i] ?? '',
    }))
    const request: CallMethodRequest = {
      object_id: props.objectId,
      method_id: props.methodId,
      inputs,
    }
    const result = await invoke<{ status: string; outputs: MethodArgValue[] }>('call_method', {
      connectionId: props.connectionId,
      request,
    })
    resultStatus.value = result.status
    resultOutputs.value = result.outputs
  } catch (err) {
    error.value = String(err)
    await showAlert(t('methodCall.callFailed', { error: String(err) }))
  } finally {
    calling.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="dialog-backdrop dialog-blur" @mousedown.self="emit('close')">
        <div class="method-dialog" role="dialog" aria-modal="true">
          <div class="dialog-header">
            <span class="dialog-title">{{ t('methodCall.title', { name: displayName }) }}</span>
          </div>
          <div class="dialog-body">
            <div class="ids">
              <div class="kv"><span>{{ t('methodCall.method') }}</span><span class="mono">{{ methodId }}</span></div>
              <div class="kv"><span>{{ t('methodCall.object') }}</span><span class="mono">{{ objectId }}</span></div>
            </div>

            <div class="section">
              <div class="section-label">{{ t('methodCall.inputs') }}</div>
              <div v-if="loadingArgs" class="hint">{{ t('methodCall.loadingArgs') }}</div>
              <div v-else-if="inputsMeta.length === 0" class="hint">{{ t('methodCall.noInputs') }}</div>
              <div v-else class="inputs">
                <div v-for="(arg, i) in inputsMeta" :key="i" class="input-row">
                  <span class="arg-label mono">{{ arg.name }} ({{ arg.data_type }})</span>
                  <input v-model="inputValues[i]" class="input" type="text" />
                </div>
              </div>
            </div>

            <p v-if="error" class="error">{{ error }}</p>

            <div class="section">
              <div class="section-label">{{ t('methodCall.outputs') }}</div>
              <div v-if="resultStatus === null" class="hint">{{ t('methodCall.notExecuted') }}</div>
              <div v-else class="outputs">
                <div class="status">{{ t('methodCall.status', { status: resultStatus }) }}</div>
                <div v-if="resultOutputs.length === 0" class="hint">{{ t('methodCall.noInputs') }}</div>
                <div v-for="(out, i) in resultOutputs" :key="i" class="kv">
                  <span>{{ outputsMeta[i]?.name ?? `[${i}]` }} ({{ out.data_type }})</span>
                  <span class="mono">{{ out.value }}</span>
                </div>
              </div>
            </div>
          </div>
          <div class="dialog-footer">
            <button class="btn btn-secondary" @click="emit('close')">{{ t('methodCall.close') }}</button>
            <button class="btn btn-primary" :disabled="calling" @click="onCall">
              {{ calling ? t('methodCall.executing') : t('methodCall.execute') }}
            </button>
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

.method-dialog {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 720px;
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
.btn-primary { background: var(--c-blue); color: var(--c-base); }
.btn-primary:hover { background: var(--c-sapphire); }
.btn-primary:disabled { opacity: 0.5; cursor: default; }
.btn-secondary { background: var(--c-surface1); color: var(--c-text); }
.btn-secondary:hover { background: var(--c-surface2); }

.ids { margin-bottom: 10px; }

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

.section { margin-top: 12px; }

.section-label {
  font-size: 12px;
  font-weight: 700;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}

.hint {
  font-size: 12px;
  color: var(--c-overlay0);
}

.inputs { display: flex; flex-direction: column; gap: 8px; }

.input-row { display: flex; flex-direction: column; gap: 3px; }

.arg-label { font-size: 11px; color: var(--c-overlay0); }

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
.input:focus { outline: none; border-color: var(--c-blue); }

.error {
  margin-top: 10px;
  font-size: 12px;
  color: var(--c-red);
  overflow-wrap: anywhere;
}

.status {
  font-size: 12px;
  color: var(--c-green);
  margin-bottom: 4px;
}
</style>
