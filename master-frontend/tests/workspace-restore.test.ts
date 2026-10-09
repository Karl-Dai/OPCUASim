// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { locale } from '@shared/i18n'
import App from '../src/App.vue'
import { useMasterContext } from '../src/inject'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }))

const restoredConnection = { id: 'restored-1', name: '风机采集', state: 'Disconnected',
  endpoint_url: 'opc.tcp://localhost:4840', security_policy: 'None', security_mode: 'None', auth_type: 'Anonymous' }
const node = { node_id: 'ns=2;s=Temperature', display_name: '温度', data_type: 'Double',
  access_mode: 'Subscription', interval_ms: 500, value: null, quality: null, source_timestamp: null,
  server_timestamp: null, update_seq: 0, user_access_level: 0 }
const DataProbe = defineComponent({ setup() {
  const context = useMasterContext()
  return () => h('div', { class: 'restored-data' }, [
    h('span', context.selectedConnection.value?.name),
    ...[...context.monitoredRows.value.values()].map(n => h('span', n.display_name)),
  ])
} })

describe('startup workspace restore', () => {
  let wrapper: VueWrapper
  afterEach(() => { wrapper?.unmount(); vi.useRealTimers(); vi.resetAllMocks() })

  async function start(error: string | null = null) {
    vi.useFakeTimers()
    locale.value = 'zh-CN'
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      switch (command) {
        case 'list_connections': return [restoredConnection]
        case 'list_groups': return []
        case 'get_persistence_status': return { enabled: true, error }
        case 'get_monitored_nodes_since': return { seq: 0, full: true, nodes: [node] }
        case 'get_polling_nodes': return []
        default: throw new Error(`Unexpected command: ${command}`)
      }
    })
    wrapper = mount(App, { global: { stubs: {
      DataTable: DataProbe, ValuePanel: true, HistoryPanel: true, EventsPanel: true, LogPanel: true,
      LangSwitch: true, VersionBadge: true, NewConnectionDialog: true, CertManagerDialog: true, MethodCallDialog: true,
      UpdateControl: true,
    } } })
    await flushPromises()
  }

  it('selects a restored connection and shows its configured nodes before connecting', async () => {
    await start()
    expect(wrapper.get('.restored-data').text()).toContain('风机采集')
    expect(wrapper.get('.restored-data').text()).toContain('温度')
    expect(wrapper.get('.persistence-status').text()).toBe('自动保存')
    expect(vi.mocked(invoke).mock.calls.every(([command]) => command !== 'connect')).toBe(true)
  })

  it('shows persistence failures instead of a successful autosave indicator', async () => {
    await start('Automatic save failed: disk full')
    expect(wrapper.get('.persistence-status').text()).toBe('自动保存异常')
    expect(wrapper.get('.persistence-status').attributes('title')).toContain('disk full')
    expect(wrapper.get('.persistence-status').classes()).toContain('persistence-error')
  })
})
