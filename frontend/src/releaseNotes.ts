export const APP_NAME = 'OPCUA Server'
export const REPO_URL = 'https://github.com/Karl-Dai/OPCUASim'

// Keep in sync with CHANGELOG.md — see `release` skill.
export const RELEASE_NOTES: string[] = [
  'v0.8.0 主站连接重命名：工具栏或双击名称修改，保留正在运行的采集连接',
  'v0.8.0 主站自动保存与恢复：连接、节点、间隔、过滤条件和分组自动保存，启动恢复后点击连接即可采集',
  'v0.8.0 主站自动更新：启动检查、下载进度和签名校验，支持立即安装、下次启动安装或跳过',
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
    'v0.8.0 Master connection renaming: toolbar and double-click editing preserve active acquisition sessions.',
    'v0.8.0 Master autosave and restore: persist connections, nodes, intervals, filters and groups; reconnect after startup to resume acquisition.',
    'v0.8.0 Master automatic updates: startup checks, download progress and signature verification, with immediate, next-launch or skip choices.',
  ],
} as const
