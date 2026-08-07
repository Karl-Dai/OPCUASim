# Tier1 规范符合性修复设计文档

日期:2026-08-07
范围:第一梯队 5 项规范/功能 bug 修复 + 配套测试 + e2e 验证

## 概述

OPCUASim 经过对照 IEC 62541(OPC UA 规范)与 IEC60870-5-104-Simulator 项目的双重视角审查,发现一批规范符合性与功能缺口。本文档覆盖**第一梯队**修复——直接影响规范符合性和核心功能的 5 项 bug:

| # | 问题 | 规范依据 |
|---|------|---------|
| 1 | 重连循环只改状态、不真正重连;手动 connect 替换连接对象导致订阅丢失 | Part 4 Session 生命周期 |
| 2 | 轮询模式是空壳(TODO 占位),Polling 节点被静默降级为 1000ms 订阅 | 功能缺口 |
| 3 | 仿真节点无 EU Range 属性,Percent deadband 对自研服务端必然失败 | Part 4 7.17.4 |
| 4 | Browse 客户端不跟随 ContinuationPoint,引用数 > 1000 被静默截断 | Part 4 5.8.3 |
| 5 | HistoryRead 提前退出不释放 ContinuationPoint,服务端悬挂资源 | Part 4 5.10.3 |

## 技术栈

| 层级 | 技术 |
|------|------|
| 后端 | Rust + Tokio 异步运行时 |
| OPC UA 库 | async-opcua(async-opcua-client / async-opcua-server / async-opcua-types 0.18) |
| GUI | egui 0.34(eframe) |

**无新增依赖,无新 crate。** 全部修改落在现有 4 个 crate 内。

## 功能范围

- 重连循环真实执行 `connect_impl()`,断线后自动重连成功
- 重连成功后自动恢复订阅节点与轮询节点
- 轮询模式真实读取节点值(按 interval_ms 客户端拉取)
- 服务端节点生成时写入 EU Range 属性,UI 可编辑
- Browse 完整跟随 ContinuationPoint(循环 BrowseNext 直至取尽)
- HistoryRead 退出时释放未消费的 ContinuationPoint

## 项目结构(修改面)

```
crates/opcuasim-core/
├── src/
│   ├── client.rs           # 修改:提取 connect_impl,重连循环真实连接
│   ├── polling.rs          # 修改:实现真实轮询读
│   ├── subscription.rs     # 修改:轮询节点不再进订阅(仅订阅模式)
│   ├── browse.rs           # 修改:BrowseNext 循环
│   ├── history.rs          # 修改:释放 ContinuationPoint
│   ├── server/
│   │   ├── models.rs       # 修改:ServerNode 加 eu_range_low/high
│   │   └── address_space.rs# 修改:add_variable_node 写入 EU Range 属性
│   └── lib.rs              # 修改:如暴露新符号
crates/opcuamaster-egui/
├── src/
│   ├── backend/
│   │   ├── state.rs        # 修改:ConnectionEntry 加 pending 订阅/轮询清单
│   │   └── dispatcher.rs   # 修改:connect 不重建对象;Connected 事件自动恢复
│   ├── model.rs            # 修改:轮询/订阅模式路由
│   └── panels/
│       └── value_panel.rs  # 修改(如涉及模式展示)
crates/opcuaserver-egui/
└── src/
    ├── model.rs            # 修改:ServerNode 构造带 EU Range 默认值
    └── panels/
        └── property_editor.rs  # 修改:EU Range 输入框
```

## 详细设计

### 1. 重连真正实现 + 订阅/轮询自动恢复(核心)

**现状问题**:
- `client.rs:291-332` `start_reconnect_loop` 只写 `Reconnecting` 状态 + sleep + `attempt += 1`,从不调用 `connect()`
- `dispatcher.rs:368-377` `connect` 用 `temp_conn` 整体替换 `entry.connection`,并把 `subscription_mgr` 重置为 `SubscriptionManager::new()` → 订阅必然丢失
- `dispatcher.rs:410` `disconnect` 同样重建连接对象

**设计(方案 A:提取连接流程 + 事件驱动恢复)**:

1. **`client.rs` 提取 `connect_impl`**:
   - 把 `connect()` 中现有逻辑(ClientBuilder 构建 → endpoint discovery → 直连 → spawn event loop → wait_for_connection)提取为 `async fn connect_impl(&self) -> Result<(), OpcUaSimError>`
   - `connect()` 公开入口不变:调用 `connect_impl()`,失败时清理状态
   - `connect_impl` 幂等:调用前若已有 session 且 event loop 存活,直接返回 Ok(避免重入)

2. **重连循环真实重连**:
   - `start_reconnect_loop` 签名保持 `&self`,但内部改为 `Arc<Self>` 持有方式(dispatcher 传入 Arc 引用)
   - 循环内:`sleep(delay)` 后调用 `self.connect_impl()`,成功则 `on_state_change(ConnectionState::Connected)`,重置 attempt=0 并继续监听;失败则 attempt+=1 继续退避
   - 保持 `shutdown_tx` 取消语义不变

3. **`dispatcher.rs` 不再重建连接对象**:
   - `connect`:直接对 `entry.connection` 调 `connect()`(不再 `OpcUaConnection::new`)
   - `disconnect`:调 `connection.disconnect()`,不重建
   - `delete_connection`:不变(整体移除)

4. **订阅/轮询自动恢复(pending 清单)**:
   - `state.rs` 的 `ConnectionEntry` 新增:
     ```rust
     pub struct ConnectionEntry {
         pub connection: OpcUaConnection,
         pub subscription_mgr: SubscriptionManager,
         pub polling_mgr: PollingManager,
         /// 节点创建时记录,用于断线重连后自动恢复
         pub pending_subscriptions: Vec<MonitoredNode>,
         pub pending_polling: Vec<MonitoredNode>,
     }
     ```
   - dispatcher 中 `add_nodes`/`add_polling_node` 的 handler:在调用 mgr 的同时写入对应 pending 清单(节点被移除时同步删除)
   - dispatcher 监听 `ConnectionStateChanged`:`Connected` 事件到达时,若 `pending_subscriptions`/`pending_polling` 非空,自动重新 `subscription_mgr.add_nodes(session)` / `polling_mgr.add_polling_node(...)`
   - 幂等保护:恢复时清空 pending?不——pending 保留为"期望状态"清单,恢复操作本身幂等(add_nodes 内部 HashMap insert 是幂等的,轮询 add 也是幂等 insert+abort 旧任务)

5. **错误处理**:
   - 重连失败不产生 UI 错误弹窗(日志 + 状态即可),避免断网期间刷屏
   - 重连成功但订阅恢复失败:记录 warning 日志,保留 pending 清单等待下次 Connected 重试

### 2. 轮询真实实现(方案 A:每节点一 task,保持现状结构)

**现状问题**: `polling.rs:35-45` task 内 `interval.tick()` 后只检查 map 成员存在就空转,`// TODO: Task 8 will implement actual OPC UA read here`。

**设计**:

1. **`PollingManager` 增加 session 依赖**:
   - `PollingManager` 构造时持有与 `OpcUaConnection.session` 相同的 Arc:
     ```rust
     pub struct PollingManager {
         polling_tasks: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
         monitored_items: Arc<RwLock<HashMap<String, MonitoredNode>>>,
         session_holder: Arc<RwLock<Option<Arc<Session>>>>,
     }
     pub fn new(session_holder: Arc<RwLock<Option<Arc<Session>>>>) -> Self;
     ```
   - `add_polling_node(&self, node: MonitoredNode, interval_ms: u64)` 不再传 session 参数(从 `session_holder` 读取)
   - task 内循环:`interval.tick()` → 从 `session_holder.read()` 取当前 session(有则读,无则跳过等待下一拍)→ 若已从 monitored_items 移除则 break

2. **单节点读**:
   - 复用 `subscription.rs` 的批量读模式,但单节点只需 `ReadValueId::new(nid, AttributeId::Value)` 一个属性
   - 读结果更新 `MonitoredNode.value/quality/timestamp/server_timestamp/update_seq`(与订阅回调写入的字段一致,保证 UI 渲染路径统一)
   - 读失败(连接断开):记 warning 一次(节流),不 panic;session 恢复后下一拍自动恢复

3. **与订阅互斥**:
   - `subscription.rs add_nodes` 中 `AccessMode::Polling` 分支删除——轮询节点不再进入订阅(删除 `interval_ms = match ... Polling => 1000.0` 分支,过滤掉 Polling 节点)
   - dispatcher 的 `add_nodes` handler:按 access_mode 分流——`Subscription` → `subscription_mgr.add_nodes`,`Polling` → `polling_mgr.add_polling_node` + pending_polling

4. **时序语义**:
   - `interval.tick()` 首次立即触发?改为 `tokio::time::interval` 默认行为即可(首 tick 立即)。考虑加 `MissedTickBehavior::Delay` 防止慢读时积压

### 3. EU Range 属性(模型字段 + 默认值 + UI 可编辑)

**现状问题**: `address_space.rs:96-105` `add_variable_node` 不设 EU Range 属性;库要求 Percent deadband 需要节点 EU Range 存在(`async-opcua-types-0.18.0/src/data_change.rs:121-140`),否则 `BadDeadbandFilterInvalid`。

**设计**:

1. **`server/models.rs` `ServerNode` 加字段**:
   ```rust
   #[serde(default = "default_eu_range_low")]
   pub eu_range_low: f64,
   #[serde(default = "default_eu_range_high")]
   pub eu_range_high: f64,
   ```
   默认 `0.0` / `100.0`。`#[serde(default)]` 保证旧 `.opcuaproj` 文件反序列化不破坏(缺失字段用默认值)。

2. **`address_space.rs add_variable_node`**:
   - 构造 VariableBuilder 时调用 `.eu_range(eu_range_low, eu_range_high)`(async-opcua 支持该 builder 方法,内部写入 EU Range 属性 i=111 的 HasProperty)

3. **`opcuaserver-egui`**:
   - `model.rs` 节点创建默认值走 `ServerNode::new` 的默认(0-100)
   - `property_editor.rs` 属性区新增 EU Range Low / EU Range High 两个 `DragValue` 输入框
   - 修改后通过现有 update_node 事件流更新地址空间(重新 add_variable_node 或 set 属性)

4. **兼容性**:
   - 旧项目文件无字段 → serde 默认 0-100,加载后重新生成地址空间时 EU Range 生效
   - 不引入迁移逻辑

### 4. Browse 跟随 ContinuationPoint

**现状问题**: `browse.rs:37` `session.browse(&browse_desc, 0, None)` 传 0(服务端默认 1000)且从不处理返回的 `continuation_point`。

**设计**:
- `browse_node` 内部循环:
  ```rust
  let mut out = Vec::new();
  let mut cp = ContinuationPoint::null();
  loop {
      let result = session.browse(&browse_desc, 0, cp.clone()).await?;
      out.extend(result.references.unwrap_or_default());
      if result.continuation_point.is_null() { break; }
      cp = result.continuation_point;
      if out.len() > MAX_TOTAL_REFERENCES { break; } // 防御性上限,如 100_000
  }
  ```
- 所有返回引用的 handler(目录树、收集变量)共用该逻辑
- `requested_max_references_per_node` 保持 0(由服务端决定),客户端必须跟随 CP —— 这正是规范要求

### 5. HistoryRead 释放 ContinuationPoint

**现状问题**: `history.rs:32-83` 循环取历史,max_values 达到提前 break 时不释放 CP;`release_continuation_points=false` 恒传。

**设计**:
- 循环中每轮记录当前 `continuation_point`
- 循环正常耗尽(null CP):无需释放
- 提前退出(max_values 达到或读错误)且当前 CP 非 null:调用
  ```rust
  session.history_read(
      HistoryReadAction::ReadRawModifiedDetails(ReadRawModifiedDetails::default()),
      TimestampsToReturn::Both,
      true, // release_continuation_points
      &[HistoryReadValueId { node_id, index_range: NumericRange::None, data_encoding: QualifiedName::null(), continuation_point: Some(cp) }],
  ).await?;
  ```
  (async-opcua `HistoryReadValueId` 带 continuation_point 字段,release=true 时服务端释放该 CP)
- 释放失败只记 warning,不掩盖主错误

## 错误处理策略

| 场景 | 行为 |
|------|------|
| 重连失败 | warning 日志 + `Reconnecting{attempt}` 状态,继续退避,不弹 UI 错误 |
| 重连成功但订阅恢复失败 | warning 日志,pending 保留,下次 Connected 再试 |
| 轮询读失败(断连) | 节流 warning,下拍重试,不 panic |
| Browse 超限 | 防御上限截断 + warning 日志(正常场景不会触发) |
| History CP 释放失败 | warning 日志,不掩盖主结果 |

## 测试策略

### 单元测试(核心逻辑,不依赖网络)
- `client.rs`:`connect_impl` 幂等性(有 session 时不重复建)——需要 mock,若不可行降级为集成覆盖
- `reconnect.rs`:已有 4 个策略测试,补 `should_retry` 边界
- `browse.rs`:CP 循环合并逻辑(若可分离纯函数则单测,否则集成)
- `history.rs`:CP 释放判定逻辑(提前退出分支)

### 集成测试(`crates/opcuasim-core/tests/`)
- **`reconnect_e2e.rs`(新建)**:起真实 `OpcUaServer`(测试端点)→ 主站连接 → 订阅节点 → **停服务端** → 重启服务端(新端口或同端口)→ 等待自动重连 → 断言 `ConnectionState::Connected` 且订阅值恢复更新
- **`polling_e2e.rs`(新建)**:起服务端(sine 仿真节点)→ 轮询模式加节点(interval 200ms)→ 等待 1s → 断言值变化多次且 `update_seq` 递增
- **`eu_range.rs`(新建)**:起服务端 → 建节点(默认 EU Range)→ Browse/Read EU Range 属性 → 断言 0.0/100.0;Percent deadband 订阅成功(不返回 BadDeadbandFilterInvalid)

### e2e 扩展(`crates/opcuamaster-egui/tests/e2e.rs`)
- `master_full_flow` 增加:EU Range 读取断言 + Percent deadband 订阅成功断言
- 新增场景(如时间允许):断连重连后订阅恢复(复用 `master_full_flow` 的 server 生命周期)

### 验证命令
```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## 不在范围内(YAGNI)

- 服务端历史存储(HistoryNode)——第三梯队
- Method 注册——第三梯队
- max_sessions 接入 builder limits——第三梯队
- DateTime/ByteString Variant 类型修复——第三梯队
- master/server dispatcher 层去重重构——第三梯队
- CI workflow 改造——第二梯队
- 聚合历史(ReadProcessedDetails)——规范增强,非本次 bug

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 重连循环与手动 connect 并发竞态 | `connect_impl` 幂等 + 连接中状态检查;dispatcher connect 与重连共用同一入口 |
| 订阅恢复重复创建 MonitoredItem | add_nodes 幂等(HashMap insert);如服务端报 BadSubscriptionIdInvalid 走现有 recreate 分支 |
| async-opcua `eu_range` builder 方法名/签名不确定 | 实现前查库源码确认;若为 set 属性方式则用 `AddressSpace` 属性写入 |
| HistoryRead 释放 CP 的 API 形态不确定 | 实现前查 `HistoryReadValueId.continuation_point` 字段与 release 参数语义,以库源码为准 |
