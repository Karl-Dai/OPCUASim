// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { locale } from '@shared/i18n'
import UpdateControl from '../src/components/UpdateControl.vue'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))
const release = { version: '0.8.0', notes: '修复连接和节点恢复', pub_date: null }
const off = vi.fn()
let wrapper: VueWrapper
let onProgress: (event: { payload: { stage: string; downloaded: number; total: number | null } }) => void

function action(label: string) {
  return [...document.querySelectorAll<HTMLButtonElement>('.update-actions button')].find(b => b.textContent === label)!
}
async function start(result: typeof release | null = null, installable = true) {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'can_install_update') return installable
    if (command === 'check_for_update') return result
    return undefined
  })
  wrapper = mount(UpdateControl, { attachTo: document.body })
  await flushPromises()
}

describe('update controls', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    locale.value = 'zh-CN'
    vi.mocked(listen).mockImplementation(async (_name, handler) => {
      onProgress = handler as typeof onProgress
      return off
    })
  })
  afterEach(() => {
    wrapper?.unmount()
    document.body.innerHTML = ''
    vi.useRealTimers()
    vi.resetAllMocks()
  })

  it('checks after subscribing to progress, repeats hourly and manually bypasses throttle', async () => {
    await start()
    expect(listen).toHaveBeenCalledWith('update-progress', expect.any(Function))
    expect(invoke).toHaveBeenCalledWith('check_for_update', { force: false })
    expect(wrapper.text()).toBe('检查更新')
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000)
    expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'check_for_update')).toHaveLength(2)
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenLastCalledWith('check_for_update', { force: true })
    expect(wrapper.text()).toBe('已是最新版本')
    wrapper.unmount()
    expect(off).toHaveBeenCalledOnce()
  })

  it('shows progress immediately, prevents overlapping requests and waits for verified metadata', async () => {
    let resolve!: (value: typeof release) => void
    await start()
    vi.mocked(invoke).mockReturnValueOnce(new Promise(r => { resolve = r }))
    await wrapper.get('button').trigger('click')
    expect(wrapper.text()).toBe('检查中…')
    expect(wrapper.get('button').attributes('disabled')).toBeDefined()
    onProgress({ payload: { stage: 'downloading', downloaded: 250, total: 1000 } })
    await flushPromises()
    expect(wrapper.text()).toBe('下载更新 25%')
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    onProgress({ payload: { stage: 'verifying', downloaded: 0, total: null } })
    await flushPromises()
    expect(wrapper.text()).toBe('校验更新中…')
    resolve(release)
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('签名校验')
    expect(document.activeElement).toBe(document.querySelector('[role="dialog"]'))
  })

  it('defers a ready release without restarting and can reopen it from toolbar', async () => {
    await start(release)
    action('稍后提醒').click()
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(wrapper.get('button').element)
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000)
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(invoke).not.toHaveBeenCalledWith('install_update', expect.anything())
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
  })

  it.each([
    ['立即安装并重启', 'install_update'],
    ['下次启动安装', 'schedule_update_on_next_launch'],
    ['跳过此版本', 'skip_update'],
  ])('runs the chosen action: %s', async (label, command) => {
    await start(release)
    expect(vi.mocked(invoke).mock.calls.map(([c]) => c)).toEqual(['can_install_update', 'check_for_update'])
    action(label).click()
    await flushPromises()
    expect(invoke).toHaveBeenLastCalledWith(command, { version: release.version })
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    if (command === 'schedule_update_on_next_launch') expect(wrapper.text()).toBe('下次启动安装')
  })

  it('surfaces startup network failures and allows retry instead of claiming latest', async () => {
    await start()
    vi.mocked(invoke).mockRejectedValueOnce('Update check timed out')
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('失败')
    expect(wrapper.get('button').attributes('title')).toContain('timed out')
    expect(wrapper.text()).not.toContain('最新版本')
    vi.mocked(invoke).mockResolvedValueOnce(null)
    await wrapper.get('button').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toBe('已是最新版本')
  })

  it('keeps the dialog and package available when installation fails', async () => {
    await start(release)
    vi.mocked(invoke).mockRejectedValueOnce('Permission denied')
    action('立即安装并重启').click()
    await flushPromises()
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('Permission denied')
    expect(action('立即安装并重启').disabled).toBe(false)
  })

  it('disables installation in development builds and explains why', async () => {
    await start(release, false)
    expect(action('立即安装并重启').disabled).toBe(true)
    expect(action('下次启动安装').disabled).toBe(true)
    expect(document.querySelector('.update-note')?.textContent).toContain('开发版')
  })

  it('traps keyboard focus and Escape defers rather than installing', async () => {
    await start(release)
    const dialog = document.querySelector<HTMLDivElement>('[role="dialog"]')!
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }))
    expect(document.activeElement).toBe(action('稍后提醒'))
    action('稍后提醒').dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true }))
    expect(document.activeElement).toBe(action('立即安装并重启'))
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(invoke).toHaveBeenCalledTimes(2)
  })
})
