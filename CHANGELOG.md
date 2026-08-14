# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0] - 2026-08-14

### Highlights / 亮点

- 🖥️ **全新 Tauri 2 + Vue 3 架构** / Both apps rebuilt on Tauri 2 + Vue 3 + TypeScript + Vite, replacing the egui frontends with a shared bilingual UI foundation.
- 🔐 **默认安全加固** / Secure by default: `Basic256Sha256` / `SignAndEncrypt` and bind host `127.0.0.1`; certificate/private-key paths and client-cert trust are configurable.
- ✍️ **Static 节点写入回显** / Client writes to Static nodes now surface in the server UI within ~500 ms via a dedicated address-space polling task.
- 🧮 **Script 仿真模式真实求值** / Script mode now evaluates real `evalexpr` expressions (`t` / `iteration` variables) instead of returning a constant.
- 🧩 **共享前端类型面** / OPC UA domain types (DataType / SimulationMode) consolidated into `shared-frontend`, placeholder removed.
- 🧪 **132 测试全绿** / 132 tests pass on CI (Linux + Windows), clippy clean, both frontends build.

### Added 新增

- Tauri 2 desktop apps `opcuaserver-app` / `opcuamaster-app` with Vue 3 + Vite frontends (`frontend/`, `master-frontend/`, `shared-frontend/`) / 全新的 Tauri 2 桌面应用与 Vue 3 + Vite 前端.
- `ServerConfig` gains `application_uri`, `host`, `certificate_path`, `private_key_path`, `trust_client_certs` / 服务端配置新增应用 URI、监听地址、证书与私钥路径、客户端证书信任开关.
- `ServerNode` / `ServerFolder` gain optional `browse_name` (defaults to display name without spaces) / 节点与文件夹支持可选 browse_name.
- Structured `DetailEvent` logging in the core, rendering richer master log entries / 核心新增结构化 DetailEvent 日志,主站日志可读性提升.

### Changed 改进

- Default security policy/mode changed from `None` to `Basic256Sha256` + `SignAndEncrypt`; default bind host from `0.0.0.0` to `127.0.0.1` / 默认安全模式由 None 改为 Basic256Sha256/SignAndEncrypt,默认监听改为 127.0.0.1.
- Server PKI now lives under the platform config dir (`pki-server-<port>`), keeping per-port isolation / 服务端 PKI 迁移至平台配置目录并保持按端口隔离.
- Value tracking is engine-side only: `update_seq` / `current_value` removed from `ServerNode` / 数值跟踪收敛到仿真引擎,ServerNode 移除冗余字段.
- History write hook records only `Value`-attribute writes; metadata writes no longer pollute value history / 历史记录钩子只记录 Value 属性写入,元数据写入不再污染值历史.
- `DateTime` / `ByteString` variants now carry real typed values; new variables get `value_rank`, `minimum_sampling_interval` and `BaseDataVariableType` / DateTime/ByteString 变体改为真实类型值,新变量补齐规范属性.
- CI reworked into test / nightly release-prep / release-on-merge / download-notify workflows with a Node release chain / CI 重构为测试、夜间发版准备、合入即发、下载通知四套工作流.

### Fixed 修复

- Script simulation mode no longer returns the constant `0.0` — real `evalexpr` evaluation with error logging / Script 仿真不再恒返回 0,改为真实表达式求值并记录错误.
- Linear `Bounce` mode uses f64 modulo, avoiding i64 overflow at extreme iteration counts / Bounce 模式改用 f64 取模,消除大迭代次数下的 i64 溢出.
- Vite native config-loader warnings fixed (`.ts` import extension + shared-frontend package type) / 修复 Vite 原生配置加载警告.
- Server builder now applies configured `Limits`, advertises discovery URL from the bind host, and warns on unknown policy/mode instead of silently skipping / 服务端构建应用配置的限制参数、按绑定地址发布发现 URL,并对未知策略/模式给出告警.

### Removed 移除

- egui crates (`opcuaegui-shared`, `opcuamaster-egui`, `opcuaserver-egui`) replaced by the Tauri 2 + Vue 3 stack / 移除 egui 三件套,由 Tauri 2 + Vue 3 取代.

### Tests 测试

- New `static_write_poll` e2e: client writes to a writable Static node surface in engine `current_values` / 新增 Static 写轮询 e2e 测试.
- New generator unit tests: script evaluation, invalid expression fallback, Bounce overflow / 新增 Script 求值与 Bounce 溢出单测.
- Full suite: 132 tests / 0 failures (CI on Linux + Windows); 24 release-script vitest cases / 全量 132 测试零失败(CI Linux + Windows),发布脚本 24 条 vitest 用例.

## [0.6.0] - 2026-08-13

### Highlights / 亮点

- Automated release of 1 commit(s) since v0.5.1. / 自 v0.5.1 以来的 1 个提交自动发布.

- Add signed silent background updates.

### Added 新增

- Add signed silent background updates.

## [0.5.1] - 2026-08-12

### Highlights / 亮点

- Automated release of 3 commit(s) since v0.5.0. / 自 v0.5.0 以来的 3 个提交自动发布.

- Isolate PKI dir per port to fix parallel e2e race.

### Fixed 修复

- Isolate PKI dir per port to fix parallel e2e race.

### Internal 内部

- Auto-release daily at Beijing 00:00 when new commits exist.
- Cap test parallelism for 2-vCPU runners, trim e2e worker threads.

## [0.5.0] - 2026-08-12

### Highlights / 亮点

- 🕘 **聚合历史读取** / OPC UA aggregated history reads (Average / Max / Min / Count / TimeAverage …) via `processing_interval`, served from the in-memory history store.
- 🔍 **内容过滤求值** / Server now evaluates `ContentFilter` `where_clause`s on history reads — comparison, `Like`, `InList` — with depth and length hardening.
- 🌳 **主站复杂类型字段树** / Master inspector renders `Structure` / `DynamicStructure` / arrays / enums as an expandable field tree, no more opaque "Variant" blobs.
- 🚨 **事件与告警系统** / Server raises events (threshold alarms, method-triggered, heartbeat, connection-state), readable via event history; master gets an event subscription panel.
- 🔐 **安全加固** / DoS hardening: LIKE pattern/string length caps, filter recursion depth limit, aggregate bucket caps, 0 ms interval rejection, filter errors propagated instead of silently dropped.
- 🧪 **CI 管线 + 测试翻倍** / New GitHub Actions CI (fmt + clippy -D warnings + tests); test suite grew from 28 to 121, zero failures.

### Added 新增

- Server: aggregated history reads with `processing_interval` (bucketed Average/Max/Min/Count/TimeAverage …), bounded by `MAX_BUCKETS` / 服务端聚合历史读取，支持 `processing_interval` 分桶聚合，受桶数上限保护.
- Server: `ContentFilter`/`where_clause` evaluation on event-history reads (comparison, `Like`, `InList`, nesting up to depth 64) / 服务端在事件历史读取时求值 `ContentFilter` where_clause（比较、Like、InList，嵌套深度限制 64）.
- Core: `variant_to_tree` — structured `Variant` → `TreeNode` conversion for complex types / 结构化 Variant → TreeNode 转换,支撑主站字段树.
- Master: history tab with three read modes — raw / aggregate / events / 主站历史页三模式:原始 / 聚合 / 事件.
- Master: complex-type values render as recursive field trees in the value panel and data table (with 📂 popup) / 主站详情面板与数据表将复杂类型值渲染为递归字段树(含 📂 弹窗).
- Server: events/alarms (threshold, method-triggered, heartbeat, connection-state) with event history read / 服务端事件/告警(越限、方法触发、心跳、连接状态)与事件历史读取.
- Server: complex data types (arrays, 2D arrays, enums, nested structures) with simulation / 服务端复杂数据类型(数组、二维数组、枚举、嵌套结构)及仿真支持.
- Master: event subscription panel / 主站事件订阅面板.
- Server: in-memory history buffer with HistoryRead support and paged continuation points / 服务端内存历史缓冲,支持 HistoryRead 与分页续读.
- Server: preset demo methods (Echo / Add / RandomValue / SetNodeValue) callable from any client / 服务端预置演示方法(Echo / Add / RandomValue / SetNodeValue),任意客户端可调用.
- Server UI: editable EU Range (low/high) + per-node history buffer capacity (0 = disabled) / 服务端 UI:可编辑 EU Range + 每节点历史缓冲容量配置.

### Changed 改进

- Server variables expose an `EURange` property, enabling percent deadband filters / 服务端变量暴露 `EURange` 属性,支持百分比死区过滤.
- Filter evaluation errors now propagate to the client as `StatusCode` instead of silently returning an empty result / 过滤求值错误现在以 `StatusCode` 传播给客户端,不再静默返回空结果.
- Client `HistoryRead` wrapper: `processing_interval` + aggregate details + event-field selection, cast/aggregate errors surface properly / 客户端 HistoryRead 封装支持聚合参数与事件字段选择,错误正确上抛.
- Duplicated `variant_to_f64` consolidated into `values.rs` / 重复的 `variant_to_f64` 合并至 `values.rs`.

### Fixed 修复

- Master auto-reconnect now actually re-establishes the session and restores subscriptions/polling after a drop / 主站断线后真正重连会话并恢复订阅/轮询.
- Polling mode now performs real OPC UA reads at the configured interval / 轮询模式真正按间隔执行 OPC UA 读取.
- Browse fully follows continuation points (no more truncated reference lists) / 浏览完整跟随续读点,不再截断引用列表.
- History reads release pending continuation points on early exit / 提前退出时释放挂起的续读点.
- e2e suites given unique ports to eliminate parallel `AddrInUse` race (content_filter vs complex_types, aggregates vs methods) / e2e 测试端口去重,消除并行 AddrInUse 竞态.

### Security 安全

- LIKE pattern/string length caps (`MAX_PAT_LEN` 256 B / `MAX_STR_LEN` 4 KB) bound the O(m×n) DP table / Like 模式/字符串长度上限约束 DP 表内存.
- Filter element recursion capped at depth 64 to prevent stack overflow / 过滤元素递归深度上限 64,防止栈溢出.
- `processing_interval` truncating to 0 ms is rejected (would spin forever); total buckets capped at 1_000_000 before allocation / 0ms 截断的聚合间隔被拒绝;桶数分配前上限 100 万.
- All mitigations target the default-bind `0.0.0.0:4840`, unauthenticated server surface / 防护覆盖默认 0.0.0.0:4840 未认证暴露面.

### Internal

- Added GitHub Actions `ci.yml`: fmt --check + clippy --workspace --all-targets -D warnings + cargo test on push/PR / 新增 CI 管线:fmt + clippy -D warnings + 全量测试.
- Test suite: 121 tests, 0 failures (lib unit 91 + server complex types/aggregates/content-filter/variant-tree/methods/events e2e) / 测试套件 121 个全绿(核心单测 91 + 各 e2e).

## [0.4.0] - 2026-05-02

### Highlights / 亮点

- 🎨 **统一视觉系统** / Unified industrial-dark theme + shared widget kit (`status_chip`, `info_row`, `empty_state`, `toast_card`) across master and server.
- 🧩 **Master UI 全面打磨** / Master UI overhaul: grouped toolbar with shortcut tooltips, status chips on connection list, inline-closable history tabs, friendly empty states.
- 📈 **历史数据可读性提升** / History tab now plots against real timestamps (HH:MM:SS axis), highlights the active quick-range, and shows hover values.
- 🔧 **Server UI 多选 + 防抖编辑** / Server gains Ctrl/Cmd multi-select with bulk delete, right-click "新建子文件夹", and lost-focus property commits (no more per-pixel `UpdateNode` spam).
- 🔐 **新增 Endpoint Discovery / 证书管理 / 方法调用 / DataChangeFilter / 历史读取** / New since v0.3.0: endpoint discovery, PKI trust manager, method call dialog, `DataChangeFilter`/deadband, history read raw — all from prior commits, polished and shipped together.
- 🧹 **架构清理** / Cleanup: removed legacy `master-frontend/`, `server-frontend/`, `crates/opcuamaster-app/` Vue/Tauri leftovers; rewrote `release.yml` to plain `cargo build` + `softprops/action-gh-release`.

### Added 新增

- `opcuaegui-shared::theme` — industrial-dark palette (teal accent, status colours) + `apply(ctx)` / `opcuaegui-shared::theme` — 工业暗色主题（teal 强调色 + 状态色）+ 一键 `apply(ctx)`.
- `opcuaegui-shared::widgets` — reusable `section_label`, `info_row`, `status_chip`, `empty_state`, `toast_card` / 通用展示组件 5 件套，统一两个 app 的视觉.
- Master: closable per-tab history view with rounded "browser-tab" visuals / Master 中央 tab 内嵌 ✕ 关闭 + 圆角浏览器风格.
- Master: history plot uses real timestamps + active quick-range highlight + hover label / 历史 Plot 时间轴 + 激活范围高亮 + hover tooltip.
- Server: Ctrl/Cmd multi-select in node table with bulk-delete chip / 节点表 Ctrl/Cmd 多选 + 顶部批量删除.
- Server: right-click "新建子文件夹" on any folder in the address tree / 地址空间任意文件夹右键新建子文件夹.
- CI: `release.yml` extracts the active CHANGELOG section into `RELEASE_BODY.md` and feeds it to `softprops/action-gh-release` so the GitHub release page mirrors the changelog / CI 自动从 CHANGELOG 抽取当版本 section 写入 release body.

### Changed 改进

- All toasts now render through `widgets::toast_card` (rounded card, accent border, theme-aware text) instead of inline `Frame::popup` / 所有 toast 统一走 `toast_card`，圆角带强调色描边.
- Master toolbar regrouped into Connection / Data / Project / System with shortcut tooltips and a header-side status chip / Master toolbar 按"连接 / 数据 / 项目 / 系统"分组并显示当前选中连接的 status chip.
- Master `value_panel` switches to shared `section_label` + `info_row`, drops redundant write-success text in favour of the toast / Master 详情面板改用共享 widget，删除冗余写入提示文本.
- Server property editor commits on `lost_focus + changed`, eliminating per-pixel network spam during DragValue interaction / Server 属性编辑改为 `lost_focus + changed` 才提交.
- Server `node_table` shows `RW` in accent colour + selection chip; address tree, status bar and toolbar all migrate to theme constants / Server 节点表 `RW` 强调色，状态栏/工具栏/地址树统一用主题常量.
- README/README_CN rewritten to reflect the actual egui-based architecture (no more Tauri/Vue references) / README 重写为反映实际 egui 架构.

### Fixed 修复

- Connection-tree state badge is now an actual chip (background + border + label) instead of a single hard-to-read coloured dot / 连接树状态徽章改为完整的 chip（背景+描边+文字），不再只有难辨识的彩色圆点.
- Browse panel and data table no longer show bare `(无数据)` strings; they render proper `empty_state` cards with guidance / 浏览面板/监控表移除裸的 `(无数据)` 文案，改为带操作引导的空状态卡片.

### Removed 移除

- Deleted orphan `master-frontend/` and `server-frontend/` directories (Vue dist remains, no source) / 删除孤立的 `master-frontend/` 与 `server-frontend/` 目录.
- Deleted empty `crates/opcuamaster-app/` (Tauri shell from the pre-rewrite era) / 删除空壳 `crates/opcuamaster-app/`.
- Removed `model::ValuePanelState::last_result` rendering — toast already covers it / 移除 `value_panel.last_result` 的冗余渲染.

### Internal

- Added `subfolder_inputs: HashMap<String, String>` and `selected_node_ids: HashSet<String>` to server `AppModel` to back the new sub-folder + multi-select flows / 新增字段支撑子文件夹与多选.
- All clippy `-D warnings` clean across the workspace; full test suite passes (28 tests, 0 failed) / 全 workspace clippy `-D warnings` 通过；测试套件 28 个全绿.

## [0.3.0] - prior

Endpoint discovery, PKI trust-list manager, method call dialog with auto-discovered I/O,
`DataChangeFilter` + deadband on subscriptions, history read raw with continuation-point
loop and Plot/Table viewer. See `git log v0.2.0..v0.3.0` for details.

## [0.2.0] - prior

Initial Rust+egui rewrite of master and server, replacing the Tauri/Vue prototype.
See `git log v0.1.0..v0.2.0` for details.

## [0.1.0] - prior

Initial public release (Tauri 2 + Vue 3 prototype). Superseded by the egui rewrite in v0.2.0.
