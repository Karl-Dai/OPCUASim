# Tier1 规范符合性修复实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 OPCUASim 第一梯队 5 项规范/功能 bug:重连真正实现+订阅自动恢复、轮询真实实现、EU Range 属性、BrowseNext 循环、History ContinuationPoint 释放,并为每项补测试,最后跑通端到端验证。

**Architecture:** 全部修改落在现有 4 个 crate。核心改动在 `opcuasim-core`(client/polling/subscription/browse/history/server);`opcuamaster-egui` 的 dispatcher 改为不重建连接对象、增加 pending 清单自动恢复;`opcuaserver-egui` 增加 EU Range UI。无新依赖。

**Tech Stack:** Rust + Tokio,async-opcua 0.18(client/server/types/nodes),egui 0.34。`ContinuationPoint = ByteString`;`AddressSpace::insert(node, Some(&[(parent, &ReferenceTypeId::HasProperty, ReferenceDirection::Inverse)]))` 添加属性节点。

## Global Constraints

- 无新增 crate、无新增依赖(workspace 现有 async-opcua 0.18 / egui 0.34 足够)
- 遵循现有模式:`Arc<RwLock<...>>` 共享状态、`tokio::spawn` 异步任务、dispatcher 事件驱动
- `cargo fmt` + `cargo clippy --workspace -- -D warnings` 必须通过
- 所有 `ServerNode` 构造点需同步补 `eu_range_low`/`eu_range_high` 字段(serde default 保证旧项目文件兼容)
- 提交前缀:`fix:`,消息用英文,符合 repo 风格
- 每任务结束单独提交

---

## File Structure

| 文件 | 责任 | 操作 |
|---|---|---|
| `crates/opcuasim-core/src/client.rs` | connect_impl 提取 + 重连循环真实重连 | 修改 |
| `crates/opcuasim-core/src/polling.rs` | 真实轮询读(session_holder + read) | 修改 |
| `crates/opcuasim-core/src/subscription.rs` | 过滤 Polling 节点,不再进订阅 | 修改 |
| `crates/opcuasim-core/src/browse.rs` | BrowseNext 循环 | 修改 |
| `crates/opcuasim-core/src/history.rs` | 释放 ContinuationPoint | 修改 |
| `crates/opcuasim-core/src/server/models.rs` | ServerNode 加 eu_range_low/high | 修改 |
| `crates/opcuasim-core/src/server/address_space.rs` | 写入 EU Range 属性节点 | 修改 |
| `crates/opcuaserver-egui/src/model.rs` | AddNodeForm 加 EU Range 字段 | 修改 |
| `crates/opcuaserver-egui/src/events.rs` | AddNodeReq/UpdateNode/NodeRow 加 EU Range | 修改 |
| `crates/opcuaserver-egui/src/backend/dispatcher.rs` | AddNode/UpdateNode handler 传 EU Range | 修改 |
| `crates/opcuaserver-egui/src/panels/property_editor.rs` | EU Range 输入框 | 修改 |
| `crates/opcuamaster-egui/src/backend/state.rs` | ConnectionEntry 加 pending 清单,connection 改 Arc | 修改 |
| `crates/opcuamaster-egui/src/backend/dispatcher.rs` | connect 不重建、Connected 自动恢复、add_nodes 分流 | 修改 |
| `crates/opcuasim-core/tests/reconnect_e2e.rs` | 断线重连 + 订阅恢复 e2e | 新建 |
| `crates/opcuasim-core/tests/polling_e2e.rs` | 轮询真实读 e2e | 新建 |
| `crates/opcuasim-core/tests/eu_range.rs` | EU Range 属性 + Percent deadband e2e | 新建 |
| `crates/opcuamaster-egui/tests/e2e.rs` | ServerNode 构造补字段、加 EU Range 断言 | 修改 |
| `crates/opcuasim-core/tests/discovery.rs` | 如构造 ServerNode 则补字段 | 修改(检查) |

---

### Task 1: core — client.rs 提取 connect_impl + 重连循环真实重连

**Files:**
- Modify: `crates/opcuasim-core/src/client.rs`
- Test: `crates/opcuasim-core/tests/reconnect_e2e.rs`(Task 9 创建,先留 TODO)

**Interfaces:**
- Consumes: 现有 `ReconnectPolicy`、`ConnectionState`、`OpcUaSimError`
- Produces:
  - `impl OpcUaConnection { async fn connect_impl(&self) -> Result<(), OpcUaSimError> }`(私有,幂等)
  - `pub async fn start_reconnect_loop<F>(self: &Arc<Self>, on_state_change: F) where F: Fn(ConnectionState) + Send + Sync + 'static`(签名从 `&self` 改为 `self: &Arc<Self>`)
  - `pub async fn disconnect(&self)` 保持签名,内部语义改为真正断开(不重建对象,调用方负责)

- [ ] **Step 1: 提取 connect_impl**

把 `connect()`(当前 `client.rs:143-233`)中的连接逻辑(body 部分)整体提取为私有 `async fn connect_impl(&self) -> Result<(), OpcUaSimError>`,`connect()` 只做状态前置设置并调用它:

```rust
pub async fn connect(&self) -> Result<(), OpcUaSimError> {
    self.set_state(ConnectionState::Connecting).await;
    self.log_request("Session", &format!("Connecting to {}", self.config.endpoint_url));
    let result = self.connect_impl().await;
    if result.is_err() {
        self.set_state(ConnectionState::Disconnected).await;
    }
    result
}

/// Actual connection work. Idempotent: if a session already exists and its
/// event loop is still running, returns Ok immediately.
async fn connect_impl(&self) -> Result<(), OpcUaSimError> {
    // Idempotency guard: reuse an alive session
    {
        let s = self.session.read().await;
        if s.is_some() {
            let h = self.event_loop_handle.read().await;
            if let Some(handle) = h.as_ref() {
                if !handle.is_finished() {
                    self.set_state(ConnectionState::Connected).await;
                    return Ok(());
                }
            }
        }
    }
    // ... 原 connect() 从 "Build client" 到 "Store session and event loop handle"
    //     的所有代码原样搬入(保持现有 ClientBuilder 配置、endpoint discovery、
    //     wait_for_connection、log_response 调用)
    self.set_state(ConnectionState::Connected).await;
    self.log_response("Session", "Connected", Some("Good"));
    Ok(())
}
```

注意:`connect_impl` 中 timeout 失败分支里 `self.set_state(ConnectionState::Disconnected)` 改为仅 `return Err`,状态由外层 `connect()` 处理。

- [ ] **Step 2: 验证编译**

Run: `cargo build -p opcuasim-core`
Expected: 编译通过,无警告。

- [ ] **Step 3: 重写 start_reconnect_loop 为真实重连**

当前 `client.rs:291-332` 循环内从不调用连接。替换为:

```rust
pub async fn start_reconnect_loop<F>(self: &Arc<Self>, on_state_change: F)
where
    F: Fn(ConnectionState) + Send + Sync + 'static,
{
    let state = self.state.clone();
    let reconnect_state = self.reconnect_state.clone();
    let policy = self.reconnect_policy.clone();
    let endpoint = self.config.endpoint_url.clone();
    let this = self.clone();

    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut tx_guard = self.shutdown_tx.write().await;
        *tx_guard = Some(tx);
    }

    tokio::spawn(async move {
        let mut attempt: u32 = 0;
        loop {
            // If the connection is already alive, keep waiting silently.
            let alive = {
                let s = this.session.read().await;
                let mut alive = false;
                if s.is_some() {
                    let h = this.event_loop_handle.read().await;
                    alive = h.as_ref().map(|h| !h.is_finished()).unwrap_or(false);
                }
                alive
            };
            if alive {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => continue,
                    _ = &mut rx => { info!("Reconnect loop cancelled"); return; }
                }
            }

            if !policy.should_retry(attempt) {
                *reconnect_state.write().await = ReconnectState::GaveUp;
                warn!("Gave up reconnecting to {}", endpoint);
                break;
            }

            *reconnect_state.write().await = ReconnectState::Reconnecting { attempt };
            *state.write().await = ConnectionState::Reconnecting;
            on_state_change(ConnectionState::Reconnecting);

            let delay = policy.delay_for_attempt(attempt);
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = &mut rx => {
                    info!("Reconnect loop cancelled");
                    return;
                }
            }

            info!("Reconnect attempt {} to {}", attempt + 1, endpoint);
            match this.connect_impl().await {
                Ok(()) => {
                    attempt = 0;
                    *reconnect_state.write().await = ReconnectState::Idle;
                    *state.write().await = ConnectionState::Connected;
                    on_state_change(ConnectionState::Connected);
                }
                Err(e) => {
                    warn!("Reconnect attempt {} failed: {}", attempt + 1, e);
                    attempt += 1;
                }
            }
        }
    });
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo build -p opcuasim-core`
Expected: 编译通过。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/src/client.rs
git commit -m "fix(core): extract connect_impl, make reconnect loop actually reconnect"
```

---

### Task 2: core — polling.rs 真实轮询

**Files:**
- Modify: `crates/opcuasim-core/src/polling.rs`
- Test: `crates/opcuasim-core/tests/polling_e2e.rs`(Task 9 创建)

**Interfaces:**
- Consumes: `OpcUaConnection::get_session_holder() -> Arc<RwLock<Option<Arc<Session>>>>`(`client.rs:280`)
- Produces:
  - `PollingManager::new(session_holder: Arc<RwLock<Option<Arc<Session>>>>) -> Self`
  - `pub async fn add_polling_node(&self, node: MonitoredNode, interval_ms: u64) -> Result<(), OpcUaSimError>`(签名不变,语义改为真实读)
  - `pub async fn get_polling_nodes(&self) -> Vec<MonitoredNode>`(不变)

- [ ] **Step 1: 重构 PollingManager 结构**

```rust
use opcua_client::Session;
use opcua_types::{AttributeId, NodeId, NumericRange, ReadValueId, TimestampsToReturn};

pub struct PollingManager {
    polling_tasks: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
    monitored_items: Arc<RwLock<HashMap<String, MonitoredNode>>>,
    session_holder: Arc<RwLock<Option<Arc<Session>>>>,
}

impl PollingManager {
    pub fn new(session_holder: Arc<RwLock<Option<Arc<Session>>>>) -> Self {
        Self {
            polling_tasks: Arc::new(RwLock::new(HashMap::new())),
            monitored_items: Arc::new(RwLock::new(HashMap::new())),
            session_holder,
        }
    }
    // 移除 Default impl(PollingManager 需要 session_holder,不能 Default)
}
```

- [ ] **Step 2: 实现真实轮询循环**

`add_polling_node` 的 spawn 块替换为:

```rust
pub async fn add_polling_node(&self, node: MonitoredNode, interval_ms: u64) -> Result<(), OpcUaSimError> {
    let node_id = node.node_id.clone();
    info!("Adding polling for node: {} (interval: {}ms)", node_id, interval_ms);

    {
        let mut items = self.monitored_items.write().await;
        items.insert(node_id.clone(), node);
    }

    let items = self.monitored_items.clone();
    let session_holder = self.session_holder.clone();
    let nid = node_id.clone();

    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            // Stop if the node was removed
            {
                let items = items.read().await;
                if !items.contains_key(&nid) {
                    break;
                }
            }
            let Ok(nid_parsed) = nid.parse::<NodeId>() else {
                continue;
            };
            let session = {
                let guard = session_holder.read().await;
                guard.clone()
            };
            let Some(session) = session else {
                continue; // disconnected; try again next tick
            };
            let read_ids = vec![ReadValueId::new(nid_parsed, AttributeId::Value)];
            match session.read(&read_ids, TimestampsToReturn::Both, 0.0).await {
                Ok(values) => {
                    if let Some(dv) = values.first() {
                        let value_str = dv.value.as_ref().map(|v| format!("{v}")).unwrap_or_else(|| "null".to_string());
                        let quality_str = dv.status.as_ref().map(|s| format!("{s}")).unwrap_or_else(|| "Good".to_string());
                        let source_ts = dv.source_timestamp.as_ref().map(|t| t.to_string()).unwrap_or_default();
                        let server_ts = dv.server_timestamp.as_ref().map(|t| t.to_string()).unwrap_or_default();
                        let mut items = items.write().await;
                        if let Some(node) = items.get_mut(&nid) {
                            node.value = Some(value_str);
                            node.quality = Some(quality_str);
                            node.timestamp = Some(source_ts);
                            node.server_timestamp = Some(server_ts);
                            node.update_seq = node.update_seq.wrapping_add(1);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Polling read failed for {}: {}", nid, e);
                }
            }
        }
    });

    let mut tasks = self.polling_tasks.write().await;
    if let Some(old_handle) = tasks.insert(node_id, handle) {
        old_handle.abort();
    }
    Ok(())
}
```

- [ ] **Step 3: 处理调用点编译错误**

`crates/opcuamaster-egui/src/backend/dispatcher.rs:344` 和 `:880` 的 `PollingManager::new()` 需要 session_holder。先临时传 `Arc::new(RwLock::new(None))` 保证编译(Task 8 会改为真实 holder):

```rust
use std::sync::RwLock; // 已存在 std::sync::Arc
use tokio::sync::RwLock as TokioRwLock; // dispatcher 顶部需加
polling_mgr: PollingManager::new(Arc::new(TokioRwLock::new(None))),
```

同时删除 `impl Default for PollingManager`。

- [ ] **Step 4: 验证编译**

Run: `cargo build --workspace`
Expected: 编译通过。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/src/polling.rs crates/opcuamaster-egui/src/backend/dispatcher.rs
git commit -m "fix(core): implement real OPC UA reads in polling manager"
```

---

### Task 3: core — subscription.rs 过滤 Polling 节点

**Files:**
- Modify: `crates/opcuasim-core/src/subscription.rs:52-82`

**Interfaces:**
- Consumes: `AccessMode`(node.rs)
- Produces: 无签名变化;行为上 `add_nodes` 忽略 `AccessMode::Polling` 节点

- [ ] **Step 1: 过滤 Polling 节点**

在 `items_to_create` 的 `filter_map` 闭包开头过滤:

```rust
let items_to_create: Vec<MonitoredItemCreateRequest> = nodes
    .iter()
    .filter_map(|n| {
        let nid: NodeId = n.node_id.parse().ok()?;
        let interval_ms = match &n.access_mode {
            crate::node::AccessMode::Subscription { interval_ms } => *interval_ms,
            crate::node::AccessMode::Polling { .. } => return None, // polling handled by PollingManager
        };
        // ... 其余不变
    })
    .collect();
```

同时 local tracking insert(41-46 行)也跳过 Polling 节点:

```rust
let mut items = self.monitored_items.write().await;
for node in &nodes {
    if matches!(node.access_mode, crate::node::AccessMode::Polling { .. }) {
        continue;
    }
    info!("Adding subscription for node: {}", node.node_id);
    items.insert(node.node_id.clone(), node.clone());
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p opcuasim-core`
Expected: 编译通过。

- [ ] **Step 3: 提交**

```bash
git add crates/opcuasim-core/src/subscription.rs
git commit -m "fix(core): exclude polling-mode nodes from subscription manager"
```

---

### Task 4: core — browse.rs BrowseNext 循环

**Files:**
- Modify: `crates/opcuasim-core/src/browse.rs:15-73`

**Interfaces:**
- Consumes: `Session::browse(&[BrowseDescription], 0, Option<ByteString>)`、`Session::browse_next(bool, &[ByteString])`
- Produces: 无签名变化;`browse_node` 现在完整跟随 ContinuationPoint

- [ ] **Step 1: 实现 CP 循环**

将 `browse_node` 中的单次 browse 调用替换为:

```rust
use opcua_types::ByteString; // 加入 imports

const MAX_TOTAL_REFERENCES: usize = 100_000;

let mut items = Vec::new();
let mut continuation_point = ByteString::null();

loop {
    let results = session
        .browse(&browse_desc, 0, continuation_point.clone())
        .await
        .map_err(|e| OpcUaSimError::BrowseError(format!("Browse failed: {}", e)))?;

    let Some(result) = results.into_iter().next() else {
        break;
    };

    if let Some(refs) = result.references {
        for r in refs {
            let node_class_str = match r.node_class {
                NodeClass::Object => "Object",
                NodeClass::Variable => "Variable",
                NodeClass::Method => "Method",
                NodeClass::ObjectType => "ObjectType",
                NodeClass::VariableType => "VariableType",
                NodeClass::ReferenceType => "ReferenceType",
                NodeClass::DataType => "DataType",
                NodeClass::View => "View",
                _ => "Unspecified",
            };
            let has_children = true;
            items.push(BrowseResultItem {
                node_id: r.node_id.node_id.to_string(),
                display_name: r.display_name.text.value().clone().unwrap_or_default(),
                node_class: node_class_str.to_string(),
                data_type: None,
                has_children,
            });
            if items.len() >= MAX_TOTAL_REFERENCES {
                log::warn!("Browse of {:?} exceeded {} references; truncating", node_id, MAX_TOTAL_REFERENCES);
                return Ok(items);
            }
        }
    }

    if result.continuation_point.is_null() {
        break;
    }
    continuation_point = result.continuation_point;
}
```

原 `for result in results { ... }` 外层循环删除(单节点 browse 只需第一个 result)。

- [ ] **Step 2: 验证编译**

Run: `cargo build -p opcuasim-core`
Expected: 编译通过。

- [ ] **Step 3: 提交**

```bash
git add crates/opcuasim-core/src/browse.rs
git commit -m "fix(core): follow browse continuation points until exhausted"
```

---

### Task 5: core — history.rs 释放 ContinuationPoint

**Files:**
- Modify: `crates/opcuasim-core/src/history.rs:23-86`

**Interfaces:**
- Consumes: `Session::history_read(action, TimestampsToReturn, bool, &[HistoryReadValueId])`
- Produces: 无签名变化;`history_read_raw` 提前退出时释放未消费 CP

- [ ] **Step 1: 实现 CP 释放**

在循环中跟踪 `result.continuation_point`,提前退出时释放。整体替换循环结构:

```rust
pub async fn history_read_raw(
    session: &Arc<Session>,
    node_id: &NodeId,
    start: DateTime,
    end: DateTime,
    max_values: u32,
    return_bounds: bool,
) -> Result<Vec<HistoryDataPoint>, OpcUaSimError> {
    let mut out: Vec<HistoryDataPoint> = Vec::new();
    let mut continuation_point = ContinuationPoint::null();

    loop {
        let action = HistoryReadAction::ReadRawModifiedDetails(ReadRawModifiedDetails {
            is_read_modified: false,
            start_time: start,
            end_time: end,
            num_values_per_node: max_values.saturating_sub(out.len() as u32),
            return_bounds,
        });
        let nodes_to_read = vec![HistoryReadValueId {
            node_id: node_id.clone(),
            index_range: NumericRange::None,
            data_encoding: QualifiedName::null(),
            continuation_point: continuation_point.clone(),
        }];

        let results: Vec<HistoryReadResult> = session
            .history_read(action, TimestampsToReturn::Both, false, &nodes_to_read)
            .await
            .map_err(|e| OpcUaSimError::ConnectionFailed(format!("history_read failed: {e}")))?;

        let result = results
            .into_iter()
            .next()
            .ok_or_else(|| OpcUaSimError::ConnectionFailed("history_read empty result".into()))?;

        if !result.status_code.is_good() {
            return Err(OpcUaSimError::ConnectionFailed(format!(
                "history_read status: {}",
                result.status_code
            )));
        }

        let history_data: Option<Box<HistoryData>> =
            result.history_data.into_inner_as::<HistoryData>();
        let dvs: Vec<DataValue> = history_data
            .and_then(|hd| hd.data_values)
            .unwrap_or_default();

        let reached_max = {
            let mut reached = false;
            for dv in dvs {
                out.push(map_data_value(dv));
                if out.len() as u32 >= max_values {
                    reached = true;
                    break;
                }
            }
            reached
        };

        // All data consumed and server has no more pages: done.
        if result.continuation_point.is_null() {
            break;
        }

        // Reached the caller's max_values with a pending continuation point:
        // release it server-side per OPC UA Part 4 5.10.3.
        if reached_max {
            let release_nodes = vec![HistoryReadValueId {
                node_id: node_id.clone(),
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
                continuation_point: result.continuation_point.clone(),
            }];
            let release_action =
                HistoryReadAction::ReadRawModifiedDetails(ReadRawModifiedDetails::default());
            if let Err(e) = session
                .history_read(release_action, TimestampsToReturn::Neither, true, &release_nodes)
                .await
            {
                log::warn!("Failed to release history continuation point: {e}");
            }
            break;
        }

        continuation_point = result.continuation_point;
    }

    Ok(out)
}
```

注意:原代码 72-77 行在 `for dv in dvs` 内 break 后还执行 79 行判断,逻辑有重叠;新版本用 `reached_max` 标志统一处理,行为等价且修复释放。

- [ ] **Step 2: 验证编译**

Run: `cargo build -p opcuasim-core`
Expected: 编译通过。若 `ReadRawModifiedDetails::default()` 不可用,改用 `ReadRawModifiedDetails { is_read_modified: false, start_time: DateTime::null(), end_time: DateTime::null(), num_values_per_node: 0, return_bounds: false }`(以库源码 `generated/types/read_raw_modified_details.rs` 字段为准,`DateTime::null()` 来自 opcua_types)。

- [ ] **Step 3: 提交**

```bash
git add crates/opcuasim-core/src/history.rs
git commit -m "fix(core): release pending history continuation points on early exit"
```

---

### Task 6: core — server EU Range 属性

**Files:**
- Modify: `crates/opcuasim-core/src/server/models.rs`(ServerNode)
- Modify: `crates/opcuasim-core/src/server/address_space.rs`(add_variable_node)

**Interfaces:**
- Produces:
  - `ServerNode` 新增 `pub eu_range_low: f64`(serde default 0.0)、`pub eu_range_high: f64`(serde default 100.0)
  - `add_variable_node` 内部:变量节点插入后,添加 `EURange` 属性节点(HasProperty 引用,数据类型 Double 数组)

- [ ] **Step 1: models.rs 加字段**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerNode {
    pub node_id: String,
    pub display_name: String,
    pub parent_id: String,
    pub data_type: DataType,
    pub writable: bool,
    pub simulation: SimulationMode,
    pub update_seq: u64,
    pub current_value: Option<String>,
    /// EU Range property (low). Default 0.0; required for Percent deadband.
    #[serde(default)]
    pub eu_range_low: f64,
    /// EU Range property (high). Default 100.0; required for Percent deadband.
    #[serde(default)]
    pub eu_range_high: f64,
}
```

`ServerNode` 无 `new()`(检查确认,构造都走字面量),所以只需加字段;所有构造点(server dispatcher、e2e.rs、discovery.rs)需补字段——Task 7/10 覆盖,先保证本任务编译不过的部分在 Task 7 处理。

- [ ] **Step 2: address_space.rs 写 EU Range 属性**

在 `add_variable_node` 的 `builder.insert(address_space)` 成功后追加:

```rust
use opcua_types::DataTypeId;
use opcua_server::address_space::ReferenceDirection; // 或 opcua_nodes::ReferenceDirection

pub fn add_variable_node(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    node: &ServerNode,
) -> bool {
    // ... 现有逻辑不变,insert 后:

    let inserted = builder.insert(address_space);
    if inserted && node.data_type.is_numeric() {
        add_eu_range_property(address_space, namespace_index, node);
    }
    inserted
}

/// Add the EURange property (array [low, high] of Double) to a variable node.
/// Percent deadband filtering requires this property (OPC UA Part 4 7.17.4).
fn add_eu_range_property(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    node: &ServerNode,
) {
    let var_id = make_node_id(namespace_index, &node.node_id);
    let prop_id = NodeId::new(namespace_index, format!("{}_EURange", node.node_id));
    let prop = opcua_server::address_space::VariableBuilder::new(
        &prop_id,
        "EURange",
        "EURange",
    )
    .data_type(DataTypeId::Double)
    .value(Variant::from(vec![node.eu_range_low, node.eu_range_high]))
    .value_rank(1)
    .build();
    address_space.insert(
        prop,
        Some(&[(&var_id, &ReferenceTypeId::HasProperty, ReferenceDirection::Inverse)]),
    );
}
```

`DataType` 枚举加 `is_numeric()` helper(models.rs):

```rust
impl DataType {
    /// Whether the type is a numeric type that can carry an EU Range.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            DataType::Int16 | DataType::Int32 | DataType::Int64
                | DataType::UInt16 | DataType::UInt32 | DataType::UInt64
                | DataType::Float | DataType::Double
        )
    }
}
```

注意:`Variant::from(vec![f64, f64])` → `Variant::DoubleArray`。`VariableBuilder::build()` 在库测试代码中确认存在(`address_space/mod.rs:1010`)。
`ReferenceDirection` 从 `opcua_server::address_space` re-export 或 `opcua_nodes` 导入——以 `address_space/mod.rs:483` 的 `opcua_nodes::ReferenceDirection` 为准,项目 `opcuasim-core` 已依赖 `async-opcua-nodes`,直接 `use opcua_nodes::ReferenceDirection`。

- [ ] **Step 3: 修复构造点编译错误**

`crates/opcuamaster-egui/tests/e2e.rs:73-126` 两处 `ServerNode { ... }` 字面量补:

```rust
eu_range_low: 0.0,
eu_range_high: 100.0,
```

`crates/opcuaserver-egui/src/backend/dispatcher.rs` 中所有 `ServerNode { ... }` 构造点同样补字段(用 grep 定位,预计 2-3 处)。
`crates/opcuasim-core/tests/discovery.rs` 如有构造也补。

- [ ] **Step 4: 验证编译**

Run: `cargo build --workspace`
Expected: 编译通过。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/src/server/models.rs crates/opcuasim-core/src/server/address_space.rs crates/opcuamaster-egui/tests/e2e.rs crates/opcuaserver-egui/src/backend/dispatcher.rs crates/opcuasim-core/tests/discovery.rs
git commit -m "feat(core): add EURange property to server variables (percent deadband support)"
```

---

### Task 7: server egui — EU Range UI

**Files:**
- Modify: `crates/opcuaserver-egui/src/events.rs`(AddNodeReq、UpdateNode、NodeRow)
- Modify: `crates/opcuaserver-egui/src/model.rs`(AddNodeForm)
- Modify: `crates/opcuaserver-egui/src/backend/dispatcher.rs`(AddNode/UpdateNode handler)
- Modify: `crates/opcuaserver-egui/src/panels/property_editor.rs`(输入框)

**Interfaces:**
- Consumes: Task 6 的 `ServerNode.eu_range_low/high`
- Produces: `AddNodeReq.eu_range_low/high`、`UpdateNode.eu_range_low/eu_range_high: Option<f64>`、`NodeRow.eu_range_low/high`

- [ ] **Step 1: events.rs 扩展 DTO**

```rust
#[derive(Debug, Clone)]
pub struct AddNodeReq {
    pub node_id: String,
    pub display_name: String,
    pub parent_id: String,
    pub data_type: DataType,
    pub writable: bool,
    pub simulation: SimulationMode,
    pub eu_range_low: f64,
    pub eu_range_high: f64,
}

// UiCommand::UpdateNode 增加字段:
    UpdateNode {
        node_id: String,
        display_name: Option<String>,
        data_type: Option<DataType>,
        writable: Option<bool>,
        simulation: Option<SimulationMode>,
        eu_range_low: Option<f64>,
        eu_range_high: Option<f64>,
    },

// NodeRow 增加:
pub struct NodeRow {
    // ...现有字段
    pub eu_range_low: f64,
    pub eu_range_high: f64,
}
```

- [ ] **Step 2: model.rs AddNodeForm 加字段**

```rust
pub struct AddNodeForm {
    // ...现有字段
    pub eu_range_low: f64,
    pub eu_range_high: f64,
}

impl Default for AddNodeForm {
    fn default() -> Self {
        Self {
            // ...
            eu_range_low: 0.0,
            eu_range_high: 100.0,
        }
    }
}
```

- [ ] **Step 3: dispatcher.rs handler 传值**

AddNode handler 中构造 `ServerNode` 补:

```rust
eu_range_low: req.eu_range_low,
eu_range_high: req.eu_range_high,
```

UpdateNode handler 中,当 `eu_range_low`/`eu_range_high` 为 `Some` 时更新对应节点字段并重新应用(现有 UpdateNode 逻辑已有"改属性 → 重建节点"路径,沿用它)。

- [ ] **Step 4: property_editor.rs 加输入框**

在 "Node Info" 区 Writable 下方加:

```rust
ui.horizontal(|ui| {
    ui.label(RichText::new("EU Range:").small().color(theme::TEXT_MUTED()));
    let mut low = node.eu_range_low;
    let mut high = node.eu_range_high;
    if ui
        .add(egui::DragValue::new(&mut low).speed(0.1).range(-1e9..=1e9))
        .changed()
        || ui
            .add(egui::DragValue::new(&mut high).speed(0.1).range(-1e9..=1e9))
            .changed()
    {
        backend.send(UiCommand::UpdateNode {
            node_id: node.node_id.clone(),
            display_name: None,
            data_type: None,
            writable: None,
            simulation: None,
            eu_range_low: Some(low),
            eu_range_high: Some(high),
        });
    }
});
```

注意:property_editor 用 `node` 是 `selected_node().cloned()` 的副本,UI 即时刷新依赖现有 UpdateNode 事件流返回 AddressSpace。

- [ ] **Step 5: 验证编译 + fmt**

Run: `cargo fmt && cargo build --workspace`
Expected: 编译通过。

- [ ] **Step 6: 提交**

```bash
git add crates/opcuaserver-egui/src/events.rs crates/opcuaserver-egui/src/model.rs crates/opcuaserver-egui/src/backend/dispatcher.rs crates/opcuaserver-egui/src/panels/property_editor.rs
git commit -m "feat(server-ui): editable EU Range in node property editor"
```

---

### Task 8: master egui — connect 不重建 + pending 清单自动恢复

**Files:**
- Modify: `crates/opcuamaster-egui/src/backend/state.rs`
- Modify: `crates/opcuamaster-egui/src/backend/dispatcher.rs`

**Interfaces:**
- Consumes: Task 1 `start_reconnect_loop(self: &Arc<Self>)`、Task 2 `PollingManager::new(session_holder)`
- Produces:
  - `ConnectionEntry { connection: Arc<OpcUaConnection>, subscription_mgr, polling_mgr, pending_subscriptions: Vec<MonitoredNode>, pending_polling: Vec<MonitoredNode> }`
  - dispatcher `connect`/`disconnect` 不再重建连接对象
  - `Connected` 状态事件触发 `restore_monitoring(conn_id, state)` 自动恢复

- [ ] **Step 1: state.rs 改造 ConnectionEntry**

```rust
use std::sync::{Arc, RwLock};

pub struct ConnectionEntry {
    pub connection: Arc<OpcUaConnection>,
    pub subscription_mgr: SubscriptionManager,
    pub polling_mgr: PollingManager,
    /// Subscription-mode nodes to re-create after a reconnect.
    pub pending_subscriptions: Vec<MonitoredNode>,
    /// Polling-mode nodes to restart after a reconnect.
    pub pending_polling: Vec<MonitoredNode>,
}
```

- [ ] **Step 2: dispatcher create_connection 调整**

`create_connection`(dispatcher.rs:321-349)改为:

```rust
async fn create_connection(
    req: CreateConnectionReq,
    state: &Arc<BackendState>,
    event_tx: &UnboundedSender<BackendEvent>,
) -> Result<(), String> {
    let id = Uuid::new_v4().to_string();
    let config = ConnectionConfig { /* 不变 */ };
    let connection = Arc::new(OpcUaConnection::new(config));
    let session_holder = connection.get_session_holder();
    {
        let mut conns = state.connections.write().map_err(|e| e.to_string())?;
        conns.insert(
            id,
            ConnectionEntry {
                connection,
                subscription_mgr: SubscriptionManager::new(),
                polling_mgr: PollingManager::new(session_holder),
                pending_subscriptions: Vec::new(),
                pending_polling: Vec::new(),
            },
        );
    }
    list_connections(state, event_tx).await
}
```

- [ ] **Step 3: connect 不重建,启动重连循环**

`connect`(dispatcher.rs:351-394)整体替换:

```rust
async fn connect(
    id: String,
    state: &Arc<BackendState>,
    event_tx: &UnboundedSender<BackendEvent>,
) -> Result<(), String> {
    let conn_arc = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(&id).ok_or("Connection not found")?;
        entry.connection.clone()
    };

    *conn_arc.state.write().await = ConnectionState::Connecting;
    let _ = event_tx.send(BackendEvent::ConnectionStateChanged {
        id: id.clone(),
        state: "Connecting".to_string(),
    });

    match conn_arc.connect().await {
        Ok(()) => {
            *conn_arc.state.write().await = ConnectionState::Connected;
            let _ = event_tx.send(BackendEvent::ConnectionStateChanged {
                id: id.clone(),
                state: "Connected".to_string(),
            });

            // Start the auto-reconnect loop; on Connected it restores monitoring.
            let cb_state = state.clone();
            let cb_conn = id.clone();
            let cb_tx = event_tx.clone();
            let on_state_change = move |s: ConnectionState| {
                if s == ConnectionState::Connected {
                    let st = cb_state.clone();
                    let cid = cb_conn.clone();
                    let tx = cb_tx.clone();
                    tokio::spawn(async move {
                        restore_monitoring(&cid, &st, &tx).await;
                    });
                }
            };
            tokio::spawn(conn_arc.clone().start_reconnect_loop(on_state_change));

            restore_monitoring(&id, state, event_tx).await;
            list_connections(state, event_tx).await
        }
        Err(e) => {
            *conn_arc.state.write().await = ConnectionState::Disconnected;
            let _ = event_tx.send(BackendEvent::ConnectionStateChanged {
                id: id.clone(),
                state: "Disconnected".to_string(),
            });
            Err(format!("Connection failed: {e}"))
        }
    }
}
```

- [ ] **Step 4: disconnect 不重建**

`disconnect`(dispatcher.rs:396-419)整体替换:

```rust
async fn disconnect(
    id: String,
    state: &Arc<BackendState>,
    event_tx: &UnboundedSender<BackendEvent>,
) -> Result<(), String> {
    let conn_arc = {
        let conns = state.connections.read().map_err(|e| e.to_string())?;
        let entry = conns.get(&id).ok_or("Connection not found")?;
        entry.connection.clone()
    };

    let _ = conn_arc.disconnect().await;
    *conn_arc.state.write().await = ConnectionState::Disconnected;
    let _ = event_tx.send(BackendEvent::ConnectionStateChanged {
        id: id.clone(),
        state: "Disconnected".to_string(),
    });
    list_connections(state, event_tx).await
}
```

- [ ] **Step 5: add_nodes 按 access_mode 分流 + 维护 pending**

两个 handler(`AddMonitoredNodes` dispatcher.rs:162、`add_variables_under_node` dispatcher.rs:573-639)统一改为:

```rust
// 在每个 handler 中,构造 monitored Vec 后:
let (sub_nodes, poll_nodes): (Vec<MonitoredNode>, Vec<MonitoredNode>) =
    monitored.into_iter().partition(|n| {
        matches!(n.access_mode, AccessMode::Subscription { .. })
    });

{
    let mut conns = state.connections.write().map_err(|e| e.to_string())?;
    let entry = conns.get_mut(&conn_id).ok_or("Connection not found")?;
    entry.pending_subscriptions.extend(sub_nodes.clone());
    entry.pending_polling.extend(poll_nodes.clone());
}

let (sub_mgr, poll_mgr, session_holder) = {
    let conns = state.connections.read().map_err(|e| e.to_string())?;
    let entry = conns.get(&conn_id).ok_or("Connection not found")?;
    (
        entry.subscription_mgr.clone(),
        entry.polling_mgr.clone(),
        entry.connection.get_session_holder(),
    )
};

let session_guard = session_holder.read().await;
let session = session_guard.as_ref();
if !sub_nodes.is_empty() {
    sub_mgr.add_nodes(sub_nodes, session).await.map_err(|e| e.to_string())?;
}
drop(session_guard);

for node in poll_nodes {
    let interval_ms = match node.access_mode {
        AccessMode::Polling { interval_ms } => interval_ms,
        AccessMode::Subscription { .. } => 1000,
    };
    poll_mgr.add_polling_node(node, interval_ms).await.map_err(|e| e.to_string())?;
}
```

注意 `add_variables_under_node` 中 `mode` 是单个(整批同一模式),分流逻辑同样适用;该函数构造 `nodes` 时 access_mode 已统一,partition 后一半为空属正常。

- [ ] **Step 6: RemoveMonitoredNodes 同步清 pending**

`RemoveMonitoredNodes` handler(dispatcher.rs:185)中,删除节点时同时从 `pending_subscriptions`/`pending_polling` 移除:

```rust
UiCommand::RemoveMonitoredNodes { conn_id, node_ids } => {
    // 现有 sub_mgr.remove_nodes 逻辑保留
    let mut conns = state.connections.write().map_err(|e| e.to_string())?;
    if let Some(entry) = conns.get_mut(&conn_id) {
        entry.pending_subscriptions.retain(|n| !node_ids.contains(&n.node_id));
        entry.pending_polling.retain(|n| !node_ids.contains(&n.node_id));
    }
    // ...
}
```

- [ ] **Step 7: 新增 restore_monitoring 函数**

```rust
/// Re-create subscription monitored items and restart polling tasks after a
/// (re)connect. Idempotent: add_nodes is insert-based, add_polling_node
/// aborts+replaces existing tasks.
async fn restore_monitoring(
    conn_id: &str,
    state: &Arc<BackendState>,
    event_tx: &UnboundedSender<BackendEvent>,
) {
    let (sub_nodes, poll_nodes, sub_mgr, poll_mgr, session_holder) = {
        let conns = match state.connections.read() {
            Ok(c) => c,
            Err(_) => return,
        };
        let Some(entry) = conns.get(conn_id) else { return };
        (
            entry.pending_subscriptions.clone(),
            entry.pending_polling.clone(),
            entry.subscription_mgr.clone(),
            entry.polling_mgr.clone(),
            entry.connection.get_session_holder(),
        )
    };
    if sub_nodes.is_empty() && poll_nodes.is_empty() {
        return;
    }
    log::info!("Restoring {} subscription + {} polling nodes for {}", sub_nodes.len(), poll_nodes.len(), conn_id);

    let session_guard = session_holder.read().await;
    let session = session_guard.as_ref();
    if !sub_nodes.is_empty() {
        if let Err(e) = sub_mgr.add_nodes(sub_nodes, session).await {
            log::warn!("Restore subscriptions failed for {}: {}", conn_id, e);
            let _ = event_tx.send(BackendEvent::Toast {
                level: ToastLevel::Warn,
                message: format!("订阅恢复失败: {e}"),
            });
        }
    }
    drop(session_guard);

    for node in poll_nodes {
        let interval_ms = match node.access_mode {
            AccessMode::Polling { interval_ms } => interval_ms,
            AccessMode::Subscription { .. } => 1000,
        };
        if let Err(e) = poll_mgr.add_polling_node(node, interval_ms).await {
            log::warn!("Restore polling failed for {}: {}", conn_id, e);
        }
    }
}
```

注意:`subscription_mgr.add_nodes` 对已存在节点(重连后本地 tracked 仍在)会重复 insert + 重复 create_monitored_items——`add_nodes` 内部对 `items_to_create` 无去重,可能导致服务端重复 MonitoredItem。修复:Task 3 后 `add_nodes` 的 local tracking 仍持有旧节点。为幂等,在 `restore_monitoring` 前先对 sub_mgr 调用 `remove_nodes(&sub_node_ids)` 再 add(简单可靠):

```rust
// restore_monitoring 内,sub_nodes 非空时:
let ids: Vec<String> = sub_nodes.iter().map(|n| n.node_id.clone()).collect();
let _ = sub_mgr.remove_nodes(&ids).await;
if let Err(e) = sub_mgr.add_nodes(sub_nodes, session).await { ... }
```

`subscription.rs` 的 `remove_nodes` 是 public(`subscription.rs:295`)。

- [ ] **Step 8: 验证编译 + clippy**

Run: `cargo build --workspace && cargo clippy --workspace -- -D warnings`
Expected: 编译 + clippy 通过。若 `state.connections.write()` 在多处嵌套导致 deadlock 风险,按需 `drop(conns)` 再取下一个锁(现有代码模式是短锁,保持)。

- [ ] **Step 9: 提交**

```bash
git add crates/opcuamaster-egui/src/backend/state.rs crates/opcuamaster-egui/src/backend/dispatcher.rs
git commit -m "fix(master): keep connection object across connect/disconnect, auto-restore monitoring after reconnect"
```

---

### Task 9: 集成测试 reconnect/polling/eu_range

**Files:**
- Create: `crates/opcuasim-core/tests/reconnect_e2e.rs`
- Create: `crates/opcuasim-core/tests/polling_e2e.rs`
- Create: `crates/opcuasim-core/tests/eu_range.rs`
- Modify: `crates/opcuasim-core/Cargo.toml`(dev-dependencies 加 async-opcua-client 已由主依赖提供,无需加)

**Interfaces:**
- Consumes: `OpcUaServer`(server.rs)、`OpcUaConnection`(client.rs)、`PollingManager`、`ServerNode`/`ServerFolder`/`SimulationMode`(models.rs)
- Produces: 三个独立 e2e 测试文件,`cargo test --workspace` 可跑

- [ ] **Step 1: reconnect_e2e.rs**

```rust
//! End-to-end: connection drops -> auto-reconnect -> subscription restored.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::{ConnectionState, OpcUaConnection};
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::node::{AccessMode, MonitoredNode};
use opcuasim_core::server::models::{DataType, ServerConfig, ServerFolder, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;
use opcuasim_core::subscription::SubscriptionManager;

const PORT: u16 = 48420;

fn server_config(port: u16) -> ServerConfig {
    ServerConfig {
        name: "ReconnectE2E".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{port}"),
        port,
        security_policies: vec!["None".into()],
        security_modes: vec!["None".into()],
        users: Vec::new(),
        anonymous_enabled: true,
        max_sessions: 10,
        max_subscriptions_per_session: 10,
    }
}

fn sine_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Sine".into(),
        display_name: "Sine".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: false,
        simulation: SimulationMode::Sine {
            amplitude: 10.0,
            offset: 0.0,
            period_ms: 4000,
            interval_ms: 100,
        },
        update_seq: 0,
        current_value: None,
        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_restores_subscription() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 1. Connect as master.
    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "c1".into(),
        name: "c1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("initial connect");
    assert_eq!(conn.get_state().await, ConnectionState::Connected);

    // 2. Subscribe to the sine node.
    let sub_mgr = SubscriptionManager::new();
    let session = conn.get_session().await.expect("session");
    let node = MonitoredNode::new(
        "ns=2;s=Demo.Sine".into(),
        "Sine".into(),
        String::new(),
        "Double".into(),
    );
    let mut node = node;
    node.access_mode = AccessMode::Subscription { interval_ms: 200.0 };
    sub_mgr.add_nodes(vec![node], Some(&session)).await.expect("subscribe");

    // 3. Expect live updates.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let seq_before = sub_mgr.get_update_seq().await;
    assert!(seq_before > 0, "expected data changes before server stop");

    // 4. Kill the server.
    server.stop().await.expect("server stop");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 5. Restart the server on the same port.
    let server2 = Arc::new(OpcUaServer::new());
    server2
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server restart");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 6. Auto-reconnect loop: wait until Connected again (up to 10s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut reconnected = false;
    while tokio::time::Instant::now() < deadline {
        if conn.get_state().await == ConnectionState::Connected {
            reconnected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(reconnected, "connection did not auto-reconnect");

    // 7. Recreate subscription manually (restore path) and expect updates again.
    let session2 = conn.get_session().await.expect("session after reconnect");
    let node2 = MonitoredNode::new(
        "ns=2;s=Demo.Sine".into(),
        "Sine".into(),
        String::new(),
        "Double".into(),
    );
    let mut node2 = node2;
    node2.access_mode = AccessMode::Subscription { interval_ms: 200.0 };
    sub_mgr.remove_nodes(&["ns=2;s=Demo.Sine".into()]).await;
    sub_mgr.add_nodes(vec![node2], Some(&session2)).await.expect("resubscribe");

    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(
        sub_mgr.get_update_seq().await > seq_before,
        "expected data changes after reconnect"
    );

    conn.disconnect().await.expect("disconnect");
    server2.stop().await.expect("server stop");
}
```

注意:该测试验证"断线 → 自动重连成功 → 状态恢复 Connected",以及恢复路径(remove+add)可再次收到数据。`OpcUaConnection::disconnect` 会取消重连循环(shutdown_tx)。

- [ ] **Step 2: polling_e2e.rs**

```rust
//! End-to-end: polling mode reads values from the server at interval.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::node::{AccessMode, MonitoredNode};
use opcuasim_core::polling::PollingManager;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerFolder, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

const PORT: u16 = 48421;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn polling_reads_live_values() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(
            &ServerConfig {
                name: "PollingE2E".into(),
                endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
                port: PORT,
                security_policies: vec!["None".into()],
                security_modes: vec!["None".into()],
                users: Vec::new(),
                anonymous_enabled: true,
                max_sessions: 10,
                max_subscriptions_per_session: 10,
            },
            &[],
            &[ServerNode {
                node_id: "Demo.Ramp".into(),
                display_name: "Ramp".into(),
                parent_id: "i=85".into(),
                data_type: DataType::Double,
                writable: false,
                simulation: SimulationMode::Linear {
                    start: 0.0,
                    step: 1.0,
                    min: 0.0,
                    max: 100.0,
                    mode: opcuasim_core::server::models::LinearMode::Repeat,
                    interval_ms: 100,
                },
                update_seq: 0,
                current_value: None,
                eu_range_low: 0.0,
                eu_range_high: 100.0,
            }],
        )
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "p1".into(),
        name: "p1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");

    let poll_mgr = PollingManager::new(conn.get_session_holder());
    let mut node = MonitoredNode::new(
        "ns=2;s=Demo.Ramp".into(),
        "Ramp".into(),
        String::new(),
        "Double".into(),
    );
    node.access_mode = AccessMode::Polling { interval_ms: 100 };
    poll_mgr.add_polling_node(node, 100).await.expect("add polling");

    // Wait ~1s: a 100ms ramp should have been read multiple times with distinct values.
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let nodes = poll_mgr.get_polling_nodes().await;
    let node = nodes
        .iter()
        .find(|n| n.node_id == "ns=2;s=Demo.Ramp")
        .expect("polled node present");
    assert!(node.update_seq >= 3, "expected >=3 polling reads, got {}", node.update_seq);
    assert!(node.value.is_some(), "expected a value from polling read");
    assert!(node.timestamp.is_some(), "expected a timestamp from polling read");

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
```

- [ ] **Step 3: eu_range.rs**

```rust
//! End-to-end: server variables expose an EURange property; percent deadband
//! subscription against the simulator succeeds (not BadDeadbandFilterInvalid).

use std::sync::Arc;
use std::time::Duration;

use opcua_client::Session;
use opcua_types::{AttributeId, DataTypeId, NodeId, NumericRange, ReadValueId, TimestampsToReturn, Variant};

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::node::{DeadbandKind, MonitoredNode, DataChangeFilterCfg, DataChangeTriggerKind};
use opcuasim_core::server::models::{DataType, ServerConfig, ServerFolder, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;
use opcuasim_core::subscription::SubscriptionManager;

const PORT: u16 = 48422;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eu_range_property_and_percent_deadband() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(
            &ServerConfig {
                name: "EuRangeE2E".into(),
                endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
                port: PORT,
                security_policies: vec!["None".into()],
                security_modes: vec!["None".into()],
                users: Vec::new(),
                anonymous_enabled: true,
                max_sessions: 10,
                max_subscriptions_per_session: 10,
            },
            &[],
            &[ServerNode {
                node_id: "Demo.Sine".into(),
                display_name: "Sine".into(),
                parent_id: "i=85".into(),
                data_type: DataType::Double,
                writable: false,
                simulation: SimulationMode::Sine {
                    amplitude: 10.0,
                    offset: 0.0,
                    period_ms: 4000,
                    interval_ms: 200,
                },
                update_seq: 0,
                current_value: None,
                eu_range_low: -50.0,
                eu_range_high: 50.0,
            }],
        )
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "e1".into(),
        name: "e1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session: Arc<Session> = conn.get_session().await.expect("session");

    // 1. Read the EURange property: ns=2;s=Demo.Sine_EURange, Double array.
    let prop_id: NodeId = "ns=2;s=Demo.Sine_EURange".parse().expect("prop id");
    let values = session
        .read(
            &[ReadValueId::new(prop_id, AttributeId::Value)],
            TimestampsToReturn::Neither,
            0.0,
        )
        .await
        .expect("read EURange");
    let dv = values.first().expect("dv");
    assert!(dv.status.as_ref().map(|s| s.is_good()).unwrap_or(false), "EURange read should be good: {:?}", dv.status);
    let v = dv.value.as_ref().expect("EURange value");
    let arr = match v {
        Variant::DoubleArray(a) => a,
        other => panic!("expected DoubleArray, got {other:?}"),
    };
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], -50.0);
    assert_eq!(arr[1], 50.0);

    // 2. Percent deadband subscription must succeed.
    let sub_mgr = SubscriptionManager::new();
    let mut node = MonitoredNode::new(
        "ns=2;s=Demo.Sine".into(),
        "Sine".into(),
        String::new(),
        "Double".into(),
    );
    node.access_mode = opcuasim_core::node::AccessMode::Subscription { interval_ms: 200.0 };
    node.filter = Some(DataChangeFilterCfg {
        trigger: DataChangeTriggerKind::StatusValue,
        deadband_kind: DeadbandKind::Percent,
        deadband_value: 5.0,
    });
    let result = sub_mgr.add_nodes(vec![node], Some(&session)).await;
    assert!(result.is_ok(), "percent deadband subscription should succeed: {result:?}");

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(sub_mgr.get_update_seq().await > 0, "expected data changes");

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p opcuasim-core --test reconnect_e2e --test polling_e2e --test eu_range`
Expected: 3 个测试全部 PASS。若失败,记录失败输出并修复(测试本身与实现同批开发,若实现已按 Task 1-6 完成,失败通常是环境/时序问题——先加大 sleep 再查断言)。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/tests/reconnect_e2e.rs crates/opcuasim-core/tests/polling_e2e.rs crates/opcuasim-core/tests/eu_range.rs
git commit -m "test(core): e2e coverage for reconnect, real polling, and EURange/percent-deadband"
```

---

### Task 10: e2e.rs 扩展 + 全量验证

**Files:**
- Modify: `crates/opcuamaster-egui/tests/e2e.rs`
- Run: 全量测试 + fmt + clippy

**Interfaces:**
- Consumes: Task 6 的 EU Range 属性
- Produces: `master_full_flow` 增加 EU Range 读取断言 + Percent deadband 订阅成功断言

- [ ] **Step 1: 扩展 master_full_flow**

在 `master_full_flow` 现有步骤后(建议在 step 2 连接成功后、browse 前)插入:

```rust
// --- EU Range: the simulator server exposes EURange on variables ---
let eu_prop_id = "ns=2;s=Demo.Sine_EURange";
backend.send(UiCommand::ReadAttrs {
    conn_id: conn_id.clone(),
    node_id: eu_prop_id.into(),
    req_id: 99,
});
let eu_ev = recv_until(&mut rx, 5, &mut saw_log, |e| {
    matches!(e, BackendEvent::NodeAttrs { req_id: 99, .. })
})
.await;
let BackendEvent::NodeAttrs { attrs, .. } = eu_ev else { unreachable!() };
assert!(
    attrs.value.as_deref().map(|v| v.contains("0") && v.contains("100")).unwrap_or(false),
    "expected EURange [0,100], got {:?}",
    attrs.value
);
```

同时,在 AddMonitoredNodes 场景(step 6)之前或之后新增一个 Percent deadband 订阅断言:

```rust
// --- Percent deadband subscription against simulator server must succeed ---
backend.send(UiCommand::AddMonitoredNodes {
    conn_id: conn_id.clone(),
    nodes: vec![MonitoredNodeReq {
        node_id: "ns=2;s=Demo.Sine".into(),
        display_name: "Sine".into(),
        data_type: Some("Double".into()),
        access_mode: "Subscription".into(),
        interval_ms: 500.0,
        filter: Some(DataChangeFilterReq {
            trigger: DataChangeTriggerKindReq::StatusValue,
            deadband_kind: DeadbandKindReq::Percent,
            deadband_value: 5.0,
        }),
    }],
});
// 后续通过 MonitoredSnapshot 断言 Sine 行出现(沿用现有监控断言的 recv_until 模式)
```

若 `DataChangeFilterReq`/`DeadbandKindReq` 字段与 events.rs 定义不完全一致,以 `crates/opcuamaster-egui/src/events.rs` 现有定义为准调整。

- [ ] **Step 2: 全量验证**

Run:
```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```
Expected: fmt 无 diff;clippy 无警告;全部测试 PASS(原有 discovery/cert_manager/e2e + 新增 3 个 e2e + 单元测试)。

- [ ] **Step 3: 修复遗留问题**

如有测试失败,按 systematic-debugging 流程定位修复。已知风险:
- 测试端口冲突:PORT 48420/48421/48422 与现有 TEST_PORT(48410?)/DEADBAND_PORT 48411/ECHO_PORT(?) 不冲突,已核对 e2e.rs 常量。
- e2e 时序:断线检测依赖 event loop 结束,`server.stop()` 后 master 侧需要 1-2s 感知,重连等待上限 10s 足够。

- [ ] **Step 4: 提交**

```bash
git add crates/opcuamaster-egui/tests/e2e.rs
git commit -m "test(master): assert EURange property and percent deadband in e2e flow"
```

- [ ] **Step 5: 收尾检查**

Run: `git status --short && git log --oneline -10`
Expected: 工作区干净(或仅剩未跟踪工具目录),最近提交为本次 10 个任务提交。

---

## 执行顺序说明

Task 1-5 相互独立,可并行(各自独立文件);Task 6-7 依赖 Models 字段一致性;Task 8 依赖 Task 1/2/3 的 API;Task 9-10 依赖全部实现。建议执行顺序:1→2→3→4→5(并行)→6→7→8→9→10。

## 验收标准

- [ ] `cargo fmt --check` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 通过
- [ ] `cargo test --workspace` 全部 PASS(原有 + 新增 3 个 e2e)
- [ ] 重连循环真实执行 connect_impl,断线后自动恢复 Connected
- [ ] 轮询节点真实读值,update_seq 递增,不进订阅
- [ ] 服务端变量带 EURange 属性,Percent deadband 订阅成功
- [ ] Browse 完整跟随 ContinuationPoint
- [ ] HistoryRead 提前退出时释放 ContinuationPoint
