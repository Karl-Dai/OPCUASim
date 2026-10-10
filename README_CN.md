<div align="center">

# 🔌 OPCUASim

**跨平台 OPC UA 协议仿真工具 —— 服务端与采集主站，一套桌面工具完成联调。**

[![Release](https://img.shields.io/github/v/release/Karl-Dai/OPCUASim?label=release&color=2ea043)](https://github.com/Karl-Dai/OPCUASim/releases) [![Downloads](https://img.shields.io/github/downloads/Karl-Dai/OPCUASim/total?color=1f6feb)](https://github.com/Karl-Dai/OPCUASim/releases) [![Stars](https://img.shields.io/github/stars/Karl-Dai/OPCUASim?color=e3b341)](https://github.com/Karl-Dai/OPCUASim/stargazers) [![License: MIT](https://img.shields.io/badge/License-MIT-lightgrey.svg)](#许可证) ![Platform](https://img.shields.io/badge/Platform-Windows%20·%20macOS%20·%20Linux-informational)

基于 **Rust** · **Tauri 2** · **Vue 3** · **async-opcua**

[English](README.md) · **中文**

<img src="docs/screenshots/master-monitoring.png" alt="OPCUAMaster monitoring demo" width="100%">
<br>
<sub>地址空间仿真 → 订阅 / 轮询采集 → 读写与历史 → 通信日志</sub>

</div>

---

## 项目简介

调试 OPC UA 采集、历史读取或设备接口时，不一定随时有真实设备可用。OPCUASim 把**通信两端放到桌面上**：

- 🏭 **服务端与主站同仓** —— 用 `OPCUAServer` 创建仿真变量，用 `OPCUAMaster` 连接它，也可以直接对接已有 OPC UA 设备。
- 📡 **订阅与轮询并存** —— 按节点选择服务器推送或客户端定时读取，观察数值、质量和时间戳。
- 🎛️ **多种变量仿真** —— 静态、随机、正弦、线性和脚本，方便构造正常变化、越限或写入场景。
- 🗂️ **采集配置自动恢复** —— 主站记住连接、节点、间隔、过滤条件和分组，重启后点击连接即可继续采集。
- 🖥️ **跨平台桌面应用** —— Windows x64 / ARM64、macOS Apple Silicon / Intel、Linux x64。
- 🌏 **中英双语界面** —— 两个应用都可通过 **中 / EN** 即时切换语言。

| 应用 | 用途 |
|---|---|
| **OPCUAServer** | 提供 OPC UA 服务端，创建地址空间、配置变量与仿真模式 |
| **OPCUAMaster** | 连接设备，浏览节点，采集、读写、读取历史及调用方法 |

## 目录

- [应用截图](#应用截图)
- [功能特性](#功能特性)
- [工作区保存与恢复](#工作区保存与恢复)
- [安全与证书](#安全与证书)
- [下载安装](#下载安装)
- [从源码构建](#从源码构建)
- [快速开始](#快速开始)
- [协议支持](#协议支持)
- [项目结构](#项目结构)
- [参与贡献](#参与贡献)
- [更新日志](#更新日志)
- [macOS 首次启动](#macos-首次启动)
- [许可证](#许可证)

## 应用截图

以下截图来自 v0.8.0 前端界面，使用本机示例节点与模拟后端数据展示操作布局，不包含真实设备或现场采集数据。

**服务端 · 地址空间与变量仿真**

左侧浏览文件夹与变量，中央查看类型、仿真模式、当前值和读写权限，右侧调整所选节点的仿真参数。

![OPCUAServer 地址空间与变量仿真](docs/screenshots/server-simulation.png)

**主站 · 订阅、轮询与通信日志**

连接树区分订阅和轮询节点，数据表展示数值、质量与时间戳；选中可写节点后可在右侧读取或写入，底部日志记录 OPC UA 服务请求与响应。

![OPCUAMaster 采集数据、值面板与通信日志](docs/screenshots/master-monitoring.png)

**主站 · 连接与端点发现**

新建连接时配置端点、安全策略、消息安全模式和用户认证；通过端点发现选择服务器实际提供的组合。

![OPCUAMaster 新建连接](docs/screenshots/master-new-connection.png)

## 功能特性

### 🏭 服务端 —— `OPCUAServer`

- **OPC UA 服务端** —— 默认监听 `opc.tcp://127.0.0.1:4840`，可配置监听地址、端口和应用 URI。
- **文件夹与变量树** —— 在 `Objects` 下组织地址空间，添加、编辑或删除变量。
- **常用标量类型** —— Boolean、整数、Float / Double、String、DateTime、ByteString。
- **五种仿真模式** —— `Static`、`Random`、`Sine`、`Linear`（Repeat / Bounce）和 `Script`（`evalexpr` 表达式）。
- **逐节点参数** —— 调整更新间隔、上下限、幅度、偏移、周期或表达式，表格同步显示当前值。
- **可写变量** —— 设置 `RW` 后允许客户端写入；Static 节点的外部写入会回显到服务端界面。
- **历史与事件能力** —— 核心提供内存历史缓冲、聚合读取、事件与事件历史，主站可用于验证相应服务。
- **项目文件** —— 手动保存和加载 `.opcuaproj`，保留服务端配置、文件夹、变量和仿真参数。

### 📡 采集主站 —— `OPCUAMaster`

- **多连接管理** —— 添加、连接、断开和删除连接；工具栏或双击名称可重命名，保留正在运行的会话。
- **端点发现与认证** —— 选择服务器提供的安全策略与消息安全模式，支持匿名、用户名密码和 X.509 用户证书认证。
- **地址空间浏览** —— 按需展开 Object / Variable / Method 节点，勾选变量采集，或批量收集对象下的变量。
- **订阅与轮询** —— 按节点设置采集模式和间隔；订阅支持数据变化触发条件及绝对 / 百分比死区过滤。
- **实时数据表** —— 搜索 NodeId、名称或数值，查看质量、源时间戳和服务器时间戳，多选后移除或加入分组。
- **读取与写入** —— 查看节点属性，对具有写权限的变量下发新值。
- **历史趋势** —— 原始历史、聚合历史与事件历史，提供时间范围、曲线和表格视图。
- **事件订阅** —— 指定事件源，查看时间、严重度、来源和消息。
- **方法调用** —— 读取方法入参和出参定义，填写参数并查看返回结果。
- **通信日志** —— 按请求 / 响应过滤，搜索服务与详情，导出 CSV。
- **自动保存与恢复** —— 连接名称、配置、监控节点、间隔、过滤条件和分组自动保存到本机。
- **应用内更新** —— 启动检查与工具栏手动检查，显示下载进度，签名校验后选择立即安装、下次启动安装或跳过版本。

### 仿真模式

| 模式 | 行为 | 示例用途 |
|---|---|---|
| Static | 保持配置值，允许可写节点接收客户端写入 | 设定值、状态量、写入测试 |
| Random | 在指定上下限内按间隔生成随机值 | 随机采样、范围测试 |
| Sine | 由幅度、偏移和周期生成正弦值 | 平滑变化、趋势测试 |
| Linear | 按步长递增，越界后循环或往返 | 计数器、斜坡与边界测试 |
| Script | 求值表达式，支持 `t` 和 `iteration` | `t * 0.1` 等自定义变化 |

## 工作区保存与恢复

**主站**在配置变更后自动保存本机工作区，并在启动时恢复。保存内容包括连接 ID / 名称、端点与认证配置、采集节点、订阅 / 轮询模式及间隔、过滤条件和分组。恢复后连接处于**断开**状态，点击**连接**即可恢复采集；工具栏显示自动保存状态及失败信息。

工具栏的**保存 / 打开**用于显式导出或导入 `.opcuaproj`。主站项目文件同样包含监控节点；**服务端**项目文件包含服务器设置、地址空间和仿真定义，当前版本需要手动保存和打开。

项目保存的是配置，不包含持续运行的会话、实时值、历史缓冲或事件记录。证书和私钥以路径引用，迁移到另一台计算机后需要保证路径有效。旧版项目若未包含监控节点，首次升级后需重新添加一次。

认证配置中的密码可能以明文保存在项目文件或本机工作区中。分享文件前请移除凭据和敏感设备地址；私钥文件不会随项目打包。

## 安全与证书

连接前需要让**安全策略、消息安全模式和用户认证方式**与服务器端点一致。

- **服务端默认值**：`Basic256Sha256` / `SignAndEncrypt`，匿名认证开启，监听地址为 `127.0.0.1`。可指定应用证书与匹配的私钥；未指定时使用自动生成的证书。
- **主站默认值**：新建连接默认 `None` / `None`。连接默认服务端时，请先发现端点并选择 `Basic256Sha256` / `SignAndEncrypt`。
- **证书信任**：当前主站自动信任服务器证书；服务端默认也自动信任客户端应用证书，可在配置中关闭后者。主站工具栏提供 PKI 证书的列出、信任 / 拒绝和删除管理。
- **用户权限**：服务端账户可记录 ReadOnly / ReadWrite / Admin 角色，但当前角色元数据尚未实现逐节点 RBAC；节点写入能力由可写配置控制。

本机联调可保持默认监听地址。需要其他机器接入时，在服务端配置监听网卡地址或 `0.0.0.0`，并在主站填写服务端的实际 IP；同时确认所选端口可访问。

## 下载安装

本机联调教程需要安装**服务端和主站两个应用**。下表对应 [v0.8.0 的实际发行资产](https://github.com/Karl-Dai/OPCUASim/releases/tag/v0.8.0)；后续版本请查看 [Releases](https://github.com/Karl-Dai/OPCUASim/releases)。

| 平台 / 格式 | 服务端 Server | 采集主站 Master |
|---|---|---|
| macOS Apple Silicon | [OPCUAServer_0.8.0_aarch64.dmg](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_aarch64.dmg) | [OPCUAMaster_0.8.0_aarch64.dmg](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_aarch64.dmg) |
| macOS Intel | [OPCUAServer_0.8.0_x64.dmg](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_x64.dmg) | [OPCUAMaster_0.8.0_x64.dmg](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_x64.dmg) |
| Windows x64 · NSIS | [OPCUAServer_0.8.0_x64-setup.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_x64-setup.exe) | [OPCUAMaster_0.8.0_x64-setup.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_x64-setup.exe) |
| Windows x64 · MSI | [OPCUAServer_0.8.0_x64_en-US.msi](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_x64_en-US.msi) | [OPCUAMaster_0.8.0_x64_en-US.msi](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_x64_en-US.msi) |
| Windows x64 · Portable | [OPCUAServer_0.8.0_x64-portable.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_x64-portable.exe) | [OPCUAMaster_0.8.0_x64-portable.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_x64-portable.exe) |
| Windows ARM64 · NSIS | [OPCUAServer_0.8.0_arm64-setup.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_arm64-setup.exe) | [OPCUAMaster_0.8.0_arm64-setup.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_arm64-setup.exe) |
| Windows ARM64 · MSI | [OPCUAServer_0.8.0_arm64_en-US.msi](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_arm64_en-US.msi) | [OPCUAMaster_0.8.0_arm64_en-US.msi](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_arm64_en-US.msi) |
| Windows ARM64 · Portable | [OPCUAServer_0.8.0_arm64-portable.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_arm64-portable.exe) | [OPCUAMaster_0.8.0_arm64-portable.exe](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_arm64-portable.exe) |
| Linux x64 · AppImage | [OPCUAServer_0.8.0_amd64.AppImage](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_amd64.AppImage) | [OPCUAMaster_0.8.0_amd64.AppImage](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_amd64.AppImage) |
| Linux x64 · deb | [OPCUAServer_0.8.0_amd64.deb](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer_0.8.0_amd64.deb) | [OPCUAMaster_0.8.0_amd64.deb](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster_0.8.0_amd64.deb) |
| Linux x64 · rpm | [OPCUAServer-0.8.0-1.x86_64.rpm](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAServer-0.8.0-1.x86_64.rpm) | [OPCUAMaster-0.8.0-1.x86_64.rpm](https://github.com/Karl-Dai/OPCUASim/releases/download/v0.8.0/OPCUAMaster-0.8.0-1.x86_64.rpm) |

Windows `-setup.exe` 和 `.msi` 是安装包，`-portable.exe` 可免安装运行，仍需要 WebView2。Linux 当前提供 x64 包；AppImage 首次运行可能需要：

```bash
chmod +x OPCUAMaster_0.8.0_amd64.AppImage
./OPCUAMaster_0.8.0_amd64.AppImage
```

`.sig`、macOS `.app.tar.gz` 和 `latest-master*.json` / `latest-server*.json` 用于更新器；手动安装请选择上表中的安装包。macOS 首次打开请参见[放行步骤](#macos-首次启动)。

**主站 v0.8.0 起提供可见的自动更新入口。** 成功的自动检查每 6 小时最多一次，工具栏手动检查可立即重试；开发版可检查更新，安装需要打包后的应用。旧版主站若没有更新入口，请先手动安装一次新版。

## 从源码构建

### 环境要求

- [Rust stable](https://rustup.rs/)，使用当前稳定工具链构建。
- [Node.js](https://nodejs.org/) **20.19+（20.x）或 22.12+**，与当前 Vite 8 的运行要求一致。
- Tauri CLI 2：`cargo install tauri-cli --version '^2' --locked`。
- 对应平台的原生依赖：Windows 的 C++ 构建工具 / WebView2、macOS 的 Xcode Command Line Tools、Linux 的 WebKitGTK 等，见 [Tauri 官方环境要求](https://v2.tauri.app/start/prerequisites/)。

### 安装依赖

从仓库根目录执行，两个前端分别安装：

```bash
git clone https://github.com/Karl-Dai/OPCUASim.git
cd OPCUASim
(cd frontend && npm ci)
(cd master-frontend && npm ci)
```

### 开发运行

分别使用两个终端，均从仓库根目录开始：

```bash
# 终端 1：服务端，前端端口 5178
cd crates/opcuaserver-app
cargo tauri dev
```

```bash
# 终端 2：采集主站，前端端口 5179
cd crates/opcuamaster-app
cargo tauri dev
```

仅启动 Vite 不能替代 Tauri / Rust 后端或建立 OPC UA 连接；完整联调请使用 `cargo tauri dev`。

### 打包

当前 Tauri 配置没有 `beforeBuildCommand`，因此应先构建前端资源，再构建桌面包：

```bash
# 从仓库根目录执行
(cd frontend && npm run build)
(cd master-frontend && npm run build)
(cd crates/opcuaserver-app && cargo tauri build)
(cd crates/opcuamaster-app && cargo tauri build)
```

签名更新产物需要配置 `TAURI_SIGNING_PRIVATE_KEY`，必要时再配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。完整多平台发行由 [Release 工作流](.github/workflows/release.yml) 构建；安装包通常位于 `target/release/bundle/`，指定目标时位于 `target/<target-triple>/release/bundle/`。

## 快速开始

用主站采集本机仿真服务端，先跑通**创建变量 → 连接 → 采集 → 写入 → 保存**。

### 第 1 步 · 服务端：配置并创建变量

打开 **OPCUAServer**，点击**配置**，确认监听地址 `127.0.0.1`、端口 `4840`，保留默认 `Basic256Sha256` / `SignAndEncrypt` 和匿名认证，保存配置。

在上方**新建节点**区域填名称、选择类型和仿真模式，然后点击**添加**。可先添加：

| 名称 | 类型 | 模式 | 可写 |
|---|---|---|---|
| Temperature | Double | Sine | 否 |
| Counter | Int32 | Linear | 否 |
| Setpoint | Double | Static | 是（勾选 RW） |

选中变量后，在右侧属性面板调整仿真参数；例如给 Temperature 设置幅度、偏移、周期和采样间隔。点击**启动**，服务端开始监听。

### 第 2 步 · 主站：发现端点并连接

打开 **OPCUAMaster**，点击**新建连接**：

1. 填写连接名称，例如“本机仿真采集”。
2. 端点 URL 填 `opc.tcp://127.0.0.1:4840`。
3. 点击对话框中的端点发现按钮，选择 `Basic256Sha256` / `SignAndEncrypt` 端点。
4. 选择匿名认证，点击**创建**，再点击工具栏**连接**。

若手动配置端点，安全策略和消息安全模式必须与服务端启用的组合一致。

### 第 3 步 · 浏览并添加采集节点

展开连接下的**地址空间**，浏览 `Objects` 下刚创建的变量。在浏览区域选择 `Subscription` 或 `Polling`，设置间隔（例如 `1000 ms`）。

勾选变量后点击**添加**；也可点击对象旁的 **＋**，按配置深度收集对象下的变量。中央**监控数据**表会显示数值、质量与时间戳，连接树分别列出订阅和轮询节点。NodeId 以实际浏览结果为准。

### 第 4 步 · 对可写变量下发数值

在主站数据表选择 Setpoint，在右侧值面板点击**读取**查看属性。输入新值（例如 `42.5`）并点击**写入**。

只对具备写权限的变量进行写入。服务端 Static 节点的当前值会随后回显；持续仿真的变量可能在下一个周期生成新值。

### 第 5 步 · 查看历史、事件和通信日志

- **历史趋势**：选择变量或点击行内趋势按钮，设置时间范围，查看原始或聚合数据；对外部设备使用其实际提供的历史服务。
- **事件订阅**：进入事件页，选择连接和事件源，再开始订阅。
- **方法调用**：浏览到 Method 节点后点击调用按钮，填写参数并查看输出。
- **通信日志**：展开底部面板，按请求 / 响应筛选，搜索服务与详情，必要时导出 CSV。

### 第 6 步 · 保存与恢复

主站添加节点、重命名或修改分组后会自动保存；也可点击**保存**导出项目。服务端请点击**保存**保留变量和仿真定义。

重新打开主站后，采集节点配置会恢复，点击**连接**继续采集。服务端用**打开**加载项目，再点击**启动**。

## 协议支持

下表列出当前实现范围；目标设备仍需支持对应服务与类型。

| 能力 | 范围 |
|---|---|
| 传输与会话 | `opc.tcp`，端点发现、会话建立、保持连接与重连 |
| 数据访问 | Browse、属性读取、Value 写入、Subscription / MonitoredItem、客户端轮询 |
| 采集过滤 | Status / Value / Timestamp 触发条件，None / Absolute / Percent 死区 |
| 标量类型 | Boolean、Int16/32/64、UInt16/32/64、Float、Double、String、DateTime、ByteString |
| 复杂类型 | 核心模型与项目定义支持 Array、Array2D、Enum、Structure；快捷添加表单提供标量类型 |
| 历史 | 原始历史读取、分桶聚合读取、事件历史读取；本机历史保存在内存缓冲中 |
| 内置聚合 | Average、Minimum、Maximum、Count、TimeAverage、Total、Delta、PercentGood |
| 事件与方法 | 事件订阅、事件历史、方法参数发现与调用 |
| 安全 | None / Sign / SignAndEncrypt，匿名 / 用户名密码 / X.509 用户证书认证；信任行为见[安全与证书](#安全与证书) |

项目用于桌面仿真与联调，协议支持以本表和对端实际提供的能力为准。CSV 点表导入仍在独立设计 / 开发中，不属于 v0.8.0 的已发布功能。

## 项目结构

```text
OPCUASim/
├── crates/
│   ├── opcuasim-core/        # OPC UA 客户端、服务端与共享协议逻辑
│   ├── opcuaserver-app/      # OPCUAServer Tauri 应用
│   └── opcuamaster-app/      # OPCUAMaster Tauri 应用
├── frontend/               # 服务端 Vue 3 前端
├── master-frontend/        # 主站 Vue 3 前端
├── shared-frontend/        # 共享组件、类型、i18n 与样式
├── scripts/                # 版本准备、更新清单与发布说明
└── docs/                   # 设计文档、手动验证说明与截图
```

| 层 | 技术栈 |
|---|---|
| 协议与后端 | Rust、Tokio、async-opcua |
| 前端 | Vue 3、TypeScript、Vite |
| 桌面端 | Tauri 2 |
| 发布 | GitHub Actions、多平台安装包、签名更新清单 |

## 参与贡献

欢迎提交 Issue 与 Pull Request。从 `master` 创建特性分支，使用 Conventional Commits 描述改动。安装原生依赖后，从仓库根目录执行：

```bash
(cd frontend && npm ci && npm test && npm run build)
(cd master-frontend && npm ci && npm test && npm run build)
(cd scripts && npm ci && npm test)
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
node scripts/prepare-release.mjs verify-current
```

两个前端 build 都会进行类型检查，并生成 Tauri 编译所需的 `dist`。仅测试 Rust 时可像 CI 一样先执行 `mkdir -p frontend/dist master-frontend/dist`；空目录不能用于打包。测试工作流在 Ubuntu 和 Windows 运行 Rust 测试，在 Ubuntu 检查两套前端。

## 更新日志

最新变更见 [CHANGELOG.md](CHANGELOG.md) 与 [Releases 页面](https://github.com/Karl-Dai/OPCUASim/releases)。v0.8.0 增加主站连接重命名、自动保存 / 恢复和可见的更新入口。

<a id="first-launch-on-macos"></a>
<a id="macos-first-launch"></a>

## macOS 首次启动

macOS 发行包使用 ad-hoc 签名，尚未做 Apple 公证（Notarization）。首次启动可能需要按 [Apple 的放行说明](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac)确认允许打开。

<details>
<summary><b>放行步骤</b></summary>

1. 将应用放入“应用程序”，尝试打开；若被系统拦截，关闭提示。
2. 打开**系统设置 → 隐私与安全性**，找到对应应用的阻止记录。
3. 点击**仍要打开**，按系统提示确认，再次启动应用。

也可针对已确认来源的下载包清除隔离属性：

```bash
xattr -dr com.apple.quarantine "/Applications/OPCUAServer.app"
xattr -dr com.apple.quarantine "/Applications/OPCUAMaster.app"
```

</details>

## 许可证

MIT。
