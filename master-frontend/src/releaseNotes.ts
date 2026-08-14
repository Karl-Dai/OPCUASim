export const APP_NAME = 'OPCUA Master'
export const REPO_URL = 'https://github.com/Karl-Dai/OPCUASim'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  'v0.5.0 复杂类型字段树: 主站将 Structure / DynamicStructure / 数组 / 枚举渲染为可展开字段树,不再显示不透明的 Variant 块',
  'v0.5.0 事件订阅面板: 主站新增事件订阅面板与历史读取三模式 (原始 / 聚合 / 事件)',
]

// Keep the complete release history above for release automation, while the
// About dialog shows a concise, localized summary of the current release.
export const ABOUT_RELEASE_NOTES = {
  'zh-CN': RELEASE_NOTES.slice(0, 2),
  'en-US': [
    'v0.5.0 complex-type field trees: the master renders Structure / DynamicStructure / arrays / enums as expandable field trees instead of opaque Variant blobs.',
    'v0.5.0 event subscription panel: the master gains an event subscription panel and three history-read modes (raw / aggregate / events).',
  ],
} as const
