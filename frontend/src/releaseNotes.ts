export const APP_NAME = 'OPCUA Server'
export const REPO_URL = 'https://github.com/Karl-Dai/OPCUASim'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  'v0.7.0 全新 Tauri 2 + Vue 3 架构: 两个应用整体迁移到 Tauri 2 + Vue 3 + Vite,移除 egui 前端',
  'v0.7.0 默认安全加固: 默认 Basic256Sha256/SignAndEncrypt、监听 127.0.0.1,证书路径与客户端证书信任可配置',
  'v0.7.0 Static 写轮询 + Script 真实求值: 客户端写入 Static 节点约 500ms 内回显;Script 模式改为真实 evalexpr 表达式',
  'v0.6.0 签名静默后台更新: 内置签名校验的 Tauri 更新器,支持静默自动更新',
  'v0.5.0 聚合历史读取与内容过滤: 服务端支持 processing_interval 分桶聚合读取 (Average / Max / Min / Count / TimeAverage) 与 ContentFilter where_clause 求值 (比较 / Like / InList)',
  'v0.5.0 事件与告警系统: 服务端事件 (越限、方法触发、心跳、连接状态) 及事件历史读取,并完成 DoS 安全加固',
]

// Keep the complete release history above for release automation, while the
// About dialog shows a concise, localized summary of the current release.
export const ABOUT_RELEASE_NOTES = {
  'zh-CN': RELEASE_NOTES.slice(0, 3),
  'en-US': [
    'v0.7.0 Tauri 2 + Vue 3 architecture: both apps rebuilt on Tauri 2 + Vue 3 + Vite; egui frontends removed.',
    'v0.7.0 Secure by default: Basic256Sha256/SignAndEncrypt and 127.0.0.1 bind host; certificate paths and client-cert trust are configurable.',
    'v0.7.0 Static write polling + real Script mode: client writes to Static nodes surface within ~500 ms; Script mode evaluates real evalexpr expressions.',
  ],
} as const
