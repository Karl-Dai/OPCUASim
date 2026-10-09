// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { computed, defineComponent, h, ref } from 'vue'
import Toolbar from '../src/components/Toolbar.vue'
import ConnectionTree from '../src/components/ConnectionTree.vue'
import AppDialog from '@shared/components/AppDialog.vue'
import { locale } from '@shared/i18n'
import { dialogCancel } from '@shared/composables/useDialog'
import { masterContextKey, type MasterContext } from '../src/inject'
import type { ConnectionInfo } from '../src/types'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }))

describe('connection rename controls', () => {
  let wrapper: VueWrapper
  let rename: ReturnType<typeof vi.fn>
  let connections: ReturnType<typeof ref<ConnectionInfo[]>>
  let selectedId: ReturnType<typeof ref<string | null>>

  beforeEach(() => {
    locale.value = 'zh-CN'
    connections = ref([{ id: 'conn-1', name: 'New Connection', state: 'Connected',
      endpoint_url: 'opc.tcp://localhost:4840', security_policy: 'None',
      security_mode: 'None', auth_type: 'Anonymous' }])
    selectedId = ref('conn-1')
    rename = vi.fn(async (id: string, name: string) => {
      connections.value.find(c => c.id === id)!.name = name
    })
    const context = {
      persistenceStatus: ref({ enabled: true, error: null }),
      connections, selectedConnectionId: selectedId,
      selectedConnection: computed(() => connections.value.find(c => c.id === selectedId.value) ?? null),
      groups: ref([]), monitoredRows: computed(() => new Map()), selectedNodeId: ref(null),
      selectConnection: vi.fn(), renameConnection: rename,
    } as unknown as MasterContext
    wrapper = mount(defineComponent({ render: () => h('div', [h(Toolbar), h(ConnectionTree), h(AppDialog)]) }), {
      attachTo: document.body,
      global: {
        provide: { [masterContextKey as symbol]: context },
        stubs: { NewConnectionDialog: true, CertManagerDialog: true, MethodCallDialog: true,
          LangSwitch: true, VersionBadge: true, UpdateControl: true },
      },
    })
  })

  afterEach(() => {
    dialogCancel()
    wrapper.unmount()
    document.body.innerHTML = ''
  })

  function renameButton() {
    return wrapper.findAll('button').find(b => b.text() === '重命名')!
  }

  it('renames from toolbar using Enter and updates both labels without changing connection state', async () => {
    await renameButton().trigger('click')
    await flushPromises()
    const input = document.querySelector<HTMLInputElement>('.dialog-input')!
    expect(input.value).toBe('New Connection')
    expect(document.activeElement).toBe(input)
    input.value = '  风机采集  '
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushPromises()
    expect(rename).toHaveBeenCalledExactlyOnceWith('conn-1', '风机采集')
    expect(wrapper.get('.conn-label').text()).toBe('风机采集')
    expect(wrapper.get('.chip').text()).toContain('风机采集')
    expect(connections.value[0]!.state).toBe('Connected')
    expect(selectedId.value).toBe('conn-1')
  })

  it('opens from a double click and cancels with Escape', async () => {
    await wrapper.get('.conn-label').trigger('dblclick')
    await flushPromises()
    document.querySelector('.dialog-input')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(rename).not.toHaveBeenCalled()
    expect(connections.value[0]!.name).toBe('New Connection')
    expect(document.querySelector('.dialog')).toBeNull()
  })

  it('rejects blank names without invoking backend', async () => {
    await renameButton().trigger('click')
    await flushPromises()
    const input = document.querySelector<HTMLInputElement>('.dialog-input')!
    input.value = '   '
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushPromises()
    expect(rename).not.toHaveBeenCalled()
    expect(document.querySelector('.dialog-message')!.textContent).toContain('连接名不能为空')
    expect(connections.value[0]!.name).toBe('New Connection')
  })

  it('keeps the old name when backend fails and reports the error', async () => {
    rename.mockRejectedValueOnce(new Error('Connection not found'))
    await renameButton().trigger('click')
    await flushPromises()
    const input = document.querySelector<HTMLInputElement>('.dialog-input')!
    input.value = 'Updated'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushPromises()
    expect(document.querySelector('.dialog-message')!.textContent).toContain('重命名失败')
    expect(connections.value[0]!.name).toBe('New Connection')
  })

  it('disables toolbar rename when no connection is selected', async () => {
    selectedId.value = null
    await flushPromises()
    expect(renameButton().attributes('disabled')).toBeDefined()
  })
})
