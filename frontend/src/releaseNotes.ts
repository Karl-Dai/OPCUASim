export const APP_NAME = 'OPCUA Server'
export const REPO_URL = 'https://github.com/Karl-Dai/OPCUASim'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  'v0.5.0 聚合历史读取与内容过滤: 服务端支持 processing_interval 分桶聚合读取 (Average / Max / Min / Count / TimeAverage) 与 ContentFilter where_clause 求值 (比较 / Like / InList)',
  'v0.5.0 事件与告警系统: 服务端事件 (越限、方法触发、心跳、连接状态) 及事件历史读取,并完成 DoS 安全加固',
]

// Keep the complete release history above for release automation, while the
// About dialog shows a concise, localized summary of the current release.
export const ABOUT_RELEASE_NOTES = {
  'zh-CN': RELEASE_NOTES.slice(0, 2),
  'en-US': [
    'v0.5.0 aggregated history reads and content filters: the server serves processing_interval-bucketed aggregates (Average / Max / Min / Count / TimeAverage) and evaluates ContentFilter where_clause (comparison / Like / InList).',
    'v0.5.0 events and alarms: server-side events (threshold, method-triggered, heartbeat, connection-state) with event-history reads, plus DoS hardening.',
  ],
} as const
