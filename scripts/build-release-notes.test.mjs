import { describe, it, expect } from 'vitest'
import { buildBody } from './build-release-notes.mjs'

const md = `# Changelog

## [1.2.3] - 2026-04-28

### Highlights / 亮点

- 新功能 X / Feature X.

## [1.2.2] - 2026-04-27

- old
`

describe('buildBody', () => {
  it('renders the version header', () => {
    expect(buildBody('v1.2.3', md)).toMatch(/^# OPCUASim v1\.2\.3\b/)
  })
  it('embeds the matching CHANGELOG section', () => {
    expect(buildBody('v1.2.3', md)).toContain('### Highlights / 亮点')
    expect(buildBody('v1.2.3', md)).toContain('新功能 X')
    // must NOT pull in the older section
    expect(buildBody('v1.2.3', md)).not.toContain('1.2.2')
    expect(buildBody('v1.2.3', md)).not.toContain('old')
  })
  it('includes the per-OS download table for both apps', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toContain('OPCUAServer_1.2.3_aarch64.dmg')
    expect(body).toContain('OPCUAMaster_1.2.3_aarch64.dmg')
    expect(body).toContain('OPCUAServer_1.2.3_x64-setup.exe')
    expect(body).toContain('OPCUAServer_1.2.3_x64-portable.exe')
    expect(body).toContain('OPCUAServer_1.2.3_arm64-setup.exe')
    expect(body).toContain('OPCUAServer_1.2.3_arm64_en-US.msi')
    expect(body).toContain('OPCUAMaster_1.2.3_arm64-portable.exe')
    expect(body).toContain('OPCUAMaster_1.2.3_amd64.AppImage')
    expect(body).toContain('OPCUAServer-1.2.3-1.x86_64.rpm')
  })
  it('uses OPCUAServer / OPCUAMaster column headers', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toContain('OPCUAServer (服务器)')
    expect(body).toContain('OPCUAMaster (主站)')
  })
  it('warns when the version section is missing', () => {
    expect(buildBody('v9.9.9', md)).toContain('CHANGELOG.md 缺少 `9.9.9`')
  })
  it('keeps the footer with full-changelog and releases links', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toContain('blob/main/CHANGELOG.md')
    expect(body).toContain('/releases>')
  })
  it('includes the macOS first-launch guidance block', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toContain('macOS 首次启动 / First launch on macOS')
    expect(body).toContain('xattr -dr com.apple.quarantine')
    expect(body).toContain('System Settings → Privacy & Security')
    expect(body).toContain('系统设置 → 隐私与安全性')
    expect(body).toContain('#first-launch-on-macos')
  })
  it('places the CN mirror banner above the download table', () => {
    const body = buildBody('v1.2.3', md)
    const bannerIdx = body.indexOf('ghfast.top/https://github.com/Karl-Dai/OPCUASim/releases/latest')
    const tableIdx = body.indexOf('## 下载 / Downloads')
    expect(bannerIdx).toBeGreaterThanOrEqual(0)
    expect(tableIdx).toBeGreaterThanOrEqual(0)
    expect(bannerIdx).toBeLessThan(tableIdx)
  })
  it('mentions the one-time manual download caveat', () => {
    const body = buildBody('v1.2.3', md)
    expect(body).toMatch(/首次升级|first.*upgrade/i)
  })
})
