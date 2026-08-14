# OPCUASim

跨平台 OPC UA 仿真套件 —— 基于 **Rust** · **Tauri 2** · **Vue 3 + TypeScript + Vite** 与 [`async-opcua`](https://crates.io/crates/async-opcua) 的桌面应用。

| 可执行文件 | 角色 |
|-----------|------|
| **OPCUAMaster** | 采集主站 / 客户端 — 连接、浏览、监控、历史、方法调用 |
| **OPCUAServer** | 地址空间仿真器 — 文件夹、带仿真模式的变量、可选写入 |

[English](README.md)

## 功能

### OPCUAMaster — 客户端 / 主站

- **OPC UA DA** — 连接任意 OPC UA 服务器，浏览地址空间，读写变量值
- **安全模式** — None / Sign / SignAndEncrypt；匿名、用户名密码、证书三种认证方式
- **端点发现** — 输入服务器 URL 即可枚举所有可用端点及其安全配置
- **地址空间懒加载浏览** — 无限深度树形，按需展开
- **智能变量收集** — 选中 Object 节点一键添加其下所有 Variable 子节点
- **订阅 + 轮询** — 服务器推送或客户端按可配间隔拉取，支持按节点配置 `DataChangeFilter`
- **实时表格** — 支持搜索、`Ctrl/Cmd+Click` 多选、质量颜色编码
- **值与写入面板** — 节点属性、手动读取、向可写节点写入
- **历史读取(HA)** — 读取历史原始值到 Plot + Table Tab，提供 1m … 24h 快捷范围
- **方法调用** — 自动发现入参/出参信息并从浏览器调用
- **通信日志** — 底部面板，方向过滤、搜索、CSV 导出
- **项目文件** — 把所有连接 + 分组保存/加载为 `.opcuaproj`
- **证书管理** — 列出、信任/拒绝、删除本地 PKI 证书

### OPCUAServer — 地址空间仿真器

- **内嵌 OPC UA 服务端** — 默认监听 `opc.tcp://127.0.0.1:4840`(监听地址可配置,局域网访问改为 `0.0.0.0`)
- **默认安全** — 默认安全策略 `Basic256Sha256` / `SignAndEncrypt`,证书与私钥路径可配置
- **文件夹 + 变量树** — 在 `Objects` 下添加文件夹和变量
- **仿真模式** — `Static`、`Random`、`Sine`、`Linear`（Repeat / Bounce）、`Script`（`evalexpr`）
- **实时数值** — 变量按各自的间隔更新并推送到 UI
- **可写节点** — 勾选 `RW` 即可让客户端写入
- **项目文件** — 把整个地址空间保存/加载为 `.opcuaproj`(注意:用户密码以明文存储,请勿外传含凭据的项目文件)

## 开发

### 环境要求

- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 18+
- [Tauri CLI](https://tauri.app/) —— `cargo install tauri-cli`

### 构建与运行

```bash
# 安装前端依赖（在仓库根目录执行）
cd frontend && npm install
cd master-frontend && npm install

# 启动服务端仿真器
cd crates/opcuaserver-app && cargo tauri dev

# 启动主站
cd crates/opcuamaster-app && cargo tauri dev
```

### 项目结构

```
OPCUASim/
├── crates/
│   ├── opcuasim-core/          # 核心库：client、server、browse、subscription、polling、history、methods
│   ├── opcuaserver-app/        # OPCUAServer Tauri 应用
│   └── opcuamaster-app/        # OPCUAMaster Tauri 应用
├── frontend/                   # 服务端 Vue 3 前端
├── master-frontend/            # 主站 Vue 3 前端
└── shared-frontend/            # 共享 Vue 组件、i18n、样式
```

## 参与贡献

1. Fork 仓库并从 `master` 创建特性分支
2. 提交前执行 `cargo fmt` 和 `cargo clippy --workspace -- -D warnings`
3. 使用 [Conventional Commits](https://www.conventionalcommits.org/) 前缀：`feat:`、`fix:`、`refactor:`、`docs:`、`chore:`
4. 向 `master` 发起 PR

## 更新日志

详见 [CHANGELOG.md](CHANGELOG.md) 与 [Releases](https://github.com/kelsoprotein-lab/OPCUASim/releases) 页面。

## macOS 首次启动

应用未做 Apple 公证（Notarization）。首次双击 `.app` 时，macOS 会弹窗 *"未打开 OPCUAServer / OPCUAMaster —— Apple 无法验证…"*，只提供 *完成* 与 *移到废纸篓* 两个按钮。这是 macOS 15 (Sequoia) 起对 ad-hoc 签名应用的标准拦截，**不是软件损坏**。

<details>
<summary><b>放行步骤（任选其一）</b></summary>

**1. 图形界面**

- 双击 `.app`，出现拦截弹窗，点 *完成*。
- 打开 *系统设置 → 隐私与安全性*，滚到底部。
- 看到 *"已阻止 OPCUAServer 的使用…"*，点 *仍要打开* 并输入密码。
- 弹窗变为 *打开*，点击即可，以后双击直接启动。

**2. 终端一行命令**

```bash
xattr -dr com.apple.quarantine "/Applications/OPCUAServer.app"
xattr -dr com.apple.quarantine "/Applications/OPCUAMaster.app"
```

清掉隔离标记，macOS 不再拦截。

</details>

## 许可证

MIT
