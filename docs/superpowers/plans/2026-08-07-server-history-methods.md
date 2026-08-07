# 子站历史存储 + 方法注册实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 OPCUASim 子站实现 A1 历史数据存储(内存环形缓冲,支持 HistoryRead raw + ContinuationPoint 分页,仿真与外部写入都记录)和 A2 方法注册(4 个预置演示方法,生产启动注册)。

**Architecture:** 自定义 `HistoryNodeManagerImpl`(实现 `InMemoryNodeManagerImpl` trait)替换内置 `SimpleNodeManagerImpl`——内部组合 `SimpleNodeManagerImpl` 委托全部常规服务,仅覆盖 `history_read_raw_modified`(从 `HistoryStore` 环形缓冲查询)与 `write`(委托后记录外部写入);`SimulationEngine` 值更新循环自行记录仿真历史;`OpcUaServer::start()` 注册预置方法。无新依赖。

**Tech Stack:** Rust + Tokio,async-opcua 0.18(async-opcua-server/types/nodes),egui 0.34。已确认库事实:`InMemoryNodeManagerImplBuilder` 对 `FnOnce(ServerContext, &mut AddressSpace) -> R` 有 blanket impl;`SimpleNodeManagerBuilder::build()` 是 pub trait 方法;`InMemoryNodeManager::set_values` 不触发 impl `write()`(无双写);`HistoryNode` 提供 `set_result(HistoryData)/set_next_continuation_point/set_status`。

## Global Constraints

- 无新增 crate、无新增依赖(workspace 现有 async-opcua 0.18 / egui 0.34 足够)
- `cargo fmt` + `cargo clippy --workspace -- -D warnings` 必须通过(注意 16 个 pre-existing `float_literal_f32_fallback` 警告在 theme.rs/widgets.rs/app.rs,非本分支引入,不改)
- 所有 `ServerConfig` 构造点需同步(新增 `history_buffer_size` 字段;serde default 保证旧 `.opcuaproj` 兼容)
- `OpcUaServer::node_manager()` 返回类型变更(`Arc<SimpleNodeManager>` → `Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>`)波及所有调用点,用 grep 定位
- 提交前缀:`feat:` / `test:` / `docs:`,消息英文,符合 repo 风格
- 每任务结束单独提交
- 服务端重启历史清空(内存缓冲,设计决策)

---

## File Structure

| 文件 | 责任 | 操作 |
|---|---|---|
| `crates/opcuasim-core/src/server/history_store.rs` | 环形缓冲 + 查询/分页 | 新建 |
| `crates/opcuasim-core/src/server/history_node_manager.rs` | 自定义 InMemoryNodeManagerImpl | 新建 |
| `crates/opcuasim-core/src/server/methods.rs` | 预置 4 方法注册 | 新建 |
| `crates/opcuasim-core/src/server/test_methods.rs` | 旧演示方法 | 删除 |
| `crates/opcuasim-core/src/server/mod.rs` | 模块声明 | 修改 |
| `crates/opcuasim-core/src/server/models.rs` | ServerConfig 加 history_buffer_size | 修改 |
| `crates/opcuasim-core/src/server/address_space.rs` | 变量节点 history_readable() | 修改 |
| `crates/opcuasim-core/src/server/simulation.rs` | 值更新循环记录历史 | 修改 |
| `crates/opcuasim-core/src/server/server.rs` | 换 node manager、注册方法 | 修改 |
| `crates/opcuaserver-egui/src/model.rs` | 设置表单字段 | 修改 |
| `crates/opcuaserver-egui/src/events.rs` | Config DTO 字段 | 修改 |
| `crates/opcuaserver-egui/src/backend/dispatcher.rs` | Config handler 传递 | 修改 |
| `crates/opcuaserver-egui/src/panels/*.rs` | 设置面板输入框 | 修改 |
| `crates/opcuasim-core/tests/server_history.rs` | 历史 e2e | 新建 |
| `crates/opcuasim-core/tests/server_methods.rs` | 方法 e2e | 新建 |

---

### Task 1: core — HistoryStore 环形缓冲

**Files:**
- Create: `crates/opcuasim-core/src/server/history_store.rs`
- Modify: `crates/opcuasim-core/src/server/mod.rs`

**Interfaces:**
- Consumes: `opcua_types::{ByteString, DataValue, DateTime, NodeId}`
- Produces:
  - `pub struct HistoryStore`
  - `impl HistoryStore { pub fn new(capacity: usize) -> Self; pub async fn record(&self, node_id: &NodeId, dv: DataValue); pub async fn query(&self, node_id: &NodeId, start: DateTime, end: DateTime, max_values: u32, skip: usize) -> (Vec<DataValue>, Option<usize>); pub async fn len(&self, node_id: &NodeId) -> usize; }`

- [ ] **Step 1: 写失败测试**

创建 `crates/opcuasim-core/src/server/history_store.rs` 底部的 `#[cfg(test)] mod tests`(测试与实现同文件):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use opcua_types::Variant;

    fn dv(ts_secs: i64, value: i64) -> DataValue {
        let mut d = DataValue::new_now(Variant::Int64(value));
        d.source_timestamp = Some(DateTime::from_timestamp(ts_secs));
        d
    }

    #[tokio::test]
    async fn ring_buffer_drops_oldest() {
        let store = HistoryStore::new(3);
        let id = NodeId::new(2, "A");
        for i in 0..5 {
            store.record(&id, dv(1000 + i, i)).await;
        }
        assert_eq!(store.len(&id).await, 3);
        let (vals, _) = store.query(&id, DateTime::min(), DateTime::max(), 100, 0).await;
        assert_eq!(vals.len(), 3);
        // Oldest two dropped; newest three kept, oldest-first
        assert_eq!(vals[0].value.as_ref().map(|v| format!("{v}")), Some("2".into()));
        assert_eq!(vals[2].value.as_ref().map(|v| format!("{v}")), Some("4".into()));
    }

    #[tokio::test]
    async fn query_filters_by_time_range() {
        let store = HistoryStore::new(100);
        let id = NodeId::new(2, "A");
        for i in 0..10 {
            store.record(&id, dv(1000 + i * 100, i)).await;
        }
        let start = DateTime::from_timestamp(1200);
        let end = DateTime::from_timestamp(1500);
        let (vals, _) = store.query(&id, start, end, 100, 0).await;
        assert_eq!(vals.len(), 4); // ts 1200,1300,1400,1500
    }

    #[tokio::test]
    async fn query_paginates_with_skip() {
        let store = HistoryStore::new(100);
        let id = NodeId::new(2, "A");
        for i in 0..5 {
            store.record(&id, dv(1000 + i, i)).await;
        }
        let (page1, next1) = store.query(&id, DateTime::min(), DateTime::max(), 2, 0).await;
        assert_eq!(page1.len(), 2);
        assert_eq!(next1, Some(2));
        let (page2, next2) = store.query(&id, DateTime::min(), DateTime::max(), 2, 2).await;
        assert_eq!(page2.len(), 2);
        assert_eq!(next2, Some(4));
        let (page3, next3) = store.query(&id, DateTime::min(), DateTime::max(), 2, 4).await;
        assert_eq!(page3.len(), 1);
        assert_eq!(next3, None);
    }

    #[tokio::test]
    async fn zero_capacity_disables() {
        let store = HistoryStore::new(0);
        let id = NodeId::new(2, "A");
        store.record(&id, dv(1000, 1)).await;
        assert_eq!(store.len(&id).await, 0);
        let (vals, _) = store.query(&id, DateTime::min(), DateTime::max(), 10, 0).await;
        assert!(vals.is_empty());
    }
}
```

`DateTime::from_timestamp(secs)` 签名以库为准——查 `async-opcua-types-0.18.0/src/date_time.rs`,若无该方法,用 `DateTime::from_ymd(2026, 8, 7, 0, 0, secs as u32)` 或测试内直接构造可比较的 DateTime 序列(记录 `ts: i64` 到结构体再构造 DataValue,过滤逻辑改测内部函数)。**若 API 不便,将 query 的时间过滤抽为纯函数 `fn in_range(dv: &DataValue, start: DateTime, end: DateTime) -> bool` 单测之。**

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p opcuasim-core history_store`
Expected: 编译失败(`HistoryStore` 未定义)。

- [ ] **Step 3: 实现 HistoryStore**

`crates/opcuasim-core/src/server/history_store.rs` 主体:

```rust
//! Per-node in-memory ring buffer of historical samples.

use std::collections::{HashMap, VecDeque};

use tokio::sync::RwLock;
use opcua_types::{DataValue, DateTime, NodeId};

/// Per-node ring buffer of historical samples, oldest-first.
pub struct HistoryStore {
    buffers: RwLock<HashMap<NodeId, VecDeque<DataValue>>>,
    capacity: usize,
}

impl HistoryStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffers: RwLock::new(HashMap::new()),
            capacity,
        }
    }

    /// Record a sample; drops oldest when at capacity. No-op when capacity is 0.
    pub async fn record(&self, node_id: &NodeId, dv: DataValue) {
        if self.capacity == 0 {
            return;
        }
        let mut buffers = self.buffers.write().await;
        let buf = buffers.entry(node_id.clone()).or_default();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(dv);
    }

    /// Query samples with timestamp in [start, end], oldest-first, skipping the
    /// first `skip` in-range samples. Returns (samples, next_skip) where
    /// next_skip is Some(skip + returned) when more in-range samples remain.
    pub async fn query(
        &self,
        node_id: &NodeId,
        start: DateTime,
        end: DateTime,
        max_values: u32,
        skip: usize,
    ) -> (Vec<DataValue>, Option<usize>) {
        let buffers = self.buffers.read().await;
        let Some(buf) = buffers.get(node_id) else {
            return (Vec::new(), None);
        };
        let mut in_range: Vec<&DataValue> = buf
            .iter()
            .filter(|dv| sample_time(dv).map(|t| t >= start && t <= end).unwrap_or(false))
            .collect();
        let total = in_range.len();
        if skip >= total {
            return (Vec::new(), None);
        }
        let take = (total - skip).min(max_values as usize);
        let samples: Vec<DataValue> = in_range
            .drain(skip..skip + take)
            .cloned()
            .collect();
        let next_skip = if skip + take < total {
            Some(skip + take)
        } else {
            None
        };
        (samples, next_skip)
    }

    /// Current sample count for a node.
    pub async fn len(&self, node_id: &NodeId) -> usize {
        self.buffers.read().await.get(node_id).map(|b| b.len()).unwrap_or(0)
    }
}

/// Extract the sample timestamp: source_timestamp, falling back to
/// server_timestamp, else None (sample excluded from range queries).
fn sample_time(dv: &DataValue) -> Option<DateTime> {
    dv.source_timestamp.or(dv.server_timestamp)
}
```

`crates/opcuasim-core/src/server/mod.rs` 加 `pub mod history_store;`(若 tests 需要 pub)。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p opcuasim-core history_store`
Expected: 4 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/src/server/history_store.rs crates/opcuasim-core/src/server/mod.rs
git commit -m "feat(core): in-memory ring-buffer history store with paged query"
```

---

### Task 2: core — HistoryNodeManagerImpl

**Files:**
- Create: `crates/opcuasim-core/src/server/history_node_manager.rs`
- Modify: `crates/opcuasim-core/src/server/mod.rs`

**Interfaces:**
- Consumes: Task 1 的 `HistoryStore`;库的 `SimpleNodeManagerImpl`(委托)、`InMemoryNodeManagerImpl` trait、`HistoryNode`/`HistoryData`/`ReadRawModifiedDetails`、`WriteNode`
- Produces:
  - `pub struct HistoryNodeManagerImpl { inner: SimpleNodeManagerImpl, history: Arc<HistoryStore> }`
  - `impl HistoryNodeManagerImpl { pub fn new(inner: SimpleNodeManagerImpl, history: Arc<HistoryStore>) -> Self; pub fn add_method_callback(...); pub fn add_write_callback(...); pub fn add_read_callback(...) }`
  - 实现 `InMemoryNodeManagerImpl`:全部方法委托 `inner`,除 `write`(委托+记录)、`history_read_raw_modified`(从 HistoryStore 查询)

- [ ] **Step 1: 读库源码确认委托面**

读 `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/async-opcua-server-0.18.0/src/node_manager/memory/memory_mgr_impl.rs` 的 trait 定义(46-380 行),列出全部方法签名。**重点确认 `write` 的完整签名**(context、address_space: &RwLock<AddressSpace>、nodes_to_write: &mut [&mut WriteNode])与 `SimpleNodeManagerImpl` 上 `add_method_callback`/`add_write_callback`/`add_read_callback` 的签名(simple.rs:402-445)。委托方法包括(按 trait 顺序):init、namespaces、name、register_nodes、read_values、create_value_monitored_items、create_event_monitored_items、set_monitoring_mode、modify_monitored_items、delete_monitored_items、unregister_nodes、history_*(除 raw_modified 外全部委托——它们默认返回 Unsupported,委托后行为一致)、write(自定义)、call(委托)、add_nodes、add_references、delete_nodes、delete_node_references、delete_references、history_update(委托)。

- [ ] **Step 2: 实现 HistoryNodeManagerImpl**

```rust
//! Custom in-memory node manager: delegates all standard services to an inner
//! SimpleNodeManagerImpl, adds real history read (ring buffer) and records
//! client writes into the history store.

use std::sync::Arc;

use async_trait::async_trait;
use opcua_server::node_manager::memory::{
    InMemoryNodeManagerImpl, SimpleNodeManagerImpl,
};
use opcua_server::node_manager::{
    MethodCall, ParsedReadValueId, RequestContext, WriteNode,
};
use opcua_server::address_space::AddressSpace;
use opcua_core::sync::RwLock;
use opcua_types::{
    ByteString, DataValue, DateTime, HistoryData, NodeId, NumericRange, ReadRawModifiedDetails,
    StatusCode, TimestampsToReturn, Variant,
};

use super::history_store::HistoryStore;

/// In-memory node manager with history support.
pub struct HistoryNodeManagerImpl {
    inner: SimpleNodeManagerImpl,
    history: Arc<HistoryStore>,
}

impl HistoryNodeManagerImpl {
    pub fn new(inner: SimpleNodeManagerImpl, history: Arc<HistoryStore>) -> Self {
        Self { inner, history }
    }

    /// Forward method callbacks to the inner manager.
    pub fn add_method_callback(
        &self,
        id: NodeId,
        cb: impl Fn(&[Variant]) -> Result<Vec<Variant>, StatusCode> + Send + Sync + 'static,
    ) {
        self.inner.add_method_callback(id, cb);
    }

    /// Forward write callbacks to the inner manager.
    pub fn add_write_callback(
        &self,
        id: NodeId,
        cb: impl Fn(DataValue, &NumericRange) -> StatusCode + Send + Sync + 'static,
    ) {
        self.inner.add_write_callback(id, cb);
    }

    /// Forward read callbacks to the inner manager.
    pub fn add_read_callback(
        &self,
        id: NodeId,
        cb: impl Fn(&NumericRange, TimestampsToReturn, f64) -> Result<DataValue, StatusCode>
            + Send
            + Sync
            + 'static,
    ) {
        self.inner.add_read_callback(id, cb);
    }
}

#[async_trait]
impl InMemoryNodeManagerImpl for HistoryNodeManagerImpl {
    async fn init(&self, address_space: &mut AddressSpace, context: opcua_server::node_manager::ServerContext) {
        self.inner.init(address_space, context).await;
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn namespaces(&self) -> Vec<opcua_server::diagnostics::NamespaceMetadata> {
        self.inner.namespaces()
    }

    async fn read_values(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes: &[&ParsedReadValueId],
        max_age: f64,
        timestamps_to_return: TimestampsToReturn,
    ) -> Vec<DataValue> {
        self.inner
            .read_values(context, address_space, nodes, max_age, timestamps_to_return)
            .await
    }

    async fn create_value_monitored_items(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        items: &mut [&mut &mut opcua_server::CreateMonitoredItem],
    ) {
        self.inner
            .create_value_monitored_items(context, address_space, items)
            .await;
    }

    async fn create_event_monitored_items(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        items: &mut [&mut &mut opcua_server::CreateMonitoredItem],
    ) {
        self.inner
            .create_event_monitored_items(context, address_space, items)
            .await;
    }

    async fn set_monitoring_mode(
        &self,
        context: &RequestContext,
        mode: opcua_types::MonitoringMode,
        items: &[&opcua_server::node_manager::MonitoredItemRef],
    ) {
        self.inner.set_monitoring_mode(context, mode, items).await;
    }

    async fn modify_monitored_items(
        &self,
        context: &RequestContext,
        items: &[&opcua_server::node_manager::MonitoredItemUpdateRef],
    ) {
        self.inner.modify_monitored_items(context, items).await;
    }

    async fn delete_monitored_items(
        &self,
        context: &RequestContext,
        items: &[&opcua_server::node_manager::MonitoredItemRef],
    ) {
        self.inner.delete_monitored_items(context, items).await;
    }

    async fn write(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        nodes_to_write: &mut [&mut WriteNode],
    ) -> Result<(), StatusCode> {
        let result = self
            .inner
            .write(context, address_space, nodes_to_write)
            .await;
        if result.is_ok() {
            for node in nodes_to_write.iter() {
                if node.status().is_good() {
                    let pv = node.value();
                    let mut dv = pv.value.clone();
                    let now = DateTime::now();
                    if dv.source_timestamp.is_none() {
                        dv.source_timestamp = Some(now);
                    }
                    if dv.server_timestamp.is_none() {
                        dv.server_timestamp = Some(now);
                    }
                    self.history.record(&pv.node_id, dv).await;
                }
            }
        }
        result
    }

    async fn call(
        &self,
        context: &RequestContext,
        address_space: &RwLock<AddressSpace>,
        methods_to_call: &mut [&mut &mut MethodCall],
    ) -> Result<(), StatusCode> {
        self.inner.call(context, address_space, methods_to_call).await
    }

    /// History read, raw only. Serves samples from the in-memory ring buffer
    /// with continuation-point paging (CP = decimal skip count as bytes).
    async fn history_read_raw_modified(
        &self,
        _context: &RequestContext,
        details: &ReadRawModifiedDetails,
        nodes: &mut [&mut &mut opcua_server::node_manager::HistoryNode],
        _timestamps_to_return: TimestampsToReturn,
    ) -> Result<(), StatusCode> {
        if details.is_read_modified {
            return Err(StatusCode::BadHistoryOperationUnsupported);
        }
        for node in nodes {
            let node_id = node.node_id().clone();
            let skip = match node.continuation_point() {
                Some(cp) => parse_skip(cp).ok_or(StatusCode::BadContinuationPointInvalid)?,
                None => 0,
            };
            let (values, next_skip) = self
                .history
                .query(&node_id, details.start_time, details.end_time, details.num_values_per_node, skip)
                .await;
            node.set_result(HistoryData {
                data_values: Some(values),
            });
            node.set_next_continuation_point(next_skip.map(encode_skip));
            node.set_status(StatusCode::Good);
        }
        Ok(())
    }

    // history_read_processed / at_time / events / annotations / history_update:
    // delegate to inner (default = BadHistoryOperationUnsupported / Unsupported),
    // keeping single-method override focused on raw.
}

/// Encode a skip count as the continuation point payload (decimal ASCII).
fn encode_skip(skip: usize) -> ByteString {
    ByteString::from(format!("{skip}").into_bytes())
}

/// Decode a continuation point payload back to a skip count.
fn parse_skip(cp: &ByteString) -> Option<usize> {
    std::str::from_utf8(cp.as_ref()).ok()?.parse().ok()
}
```

**注意:**
- `DateTime::now()` 返回带微秒精度,足够排序
- `ByteString::from(Vec<u8>)` / `cp.as_ref()` 以库 API 为准(ByteString = Vec<u8> 包装,`from`/`as_ref` 或 `.value()`;实现时查 `byte_string.rs` 调整)
- trait 中其余 history 方法(processed/at_time/events/annotations)与 `history_update`、`register_nodes`/`unregister_nodes`、`add_nodes`/`add_references`/`delete_nodes`/`delete_node_references`/`delete_references` 全部委托 `self.inner`(签名以 Step 1 确认的 trait 定义为准,逐一照抄委托)
- `opcua_core::sync::RwLock` 是库内部的锁类型(`address_space` 参数用)——确认 server crate 是否 re-export;若无,用 `opcua_server::node_manager::memory::...` 或 `async_std`/`parking_lot` 的对应类型(以 trait 签名中出现的类型为准,直接引用库源码中的路径)

- [ ] **Step 3: 验证编译**

Run: `cargo build -p opcuasim-core`
Expected: 编译通过(尚未接线,无调用方)。

- [ ] **Step 4: 提交**

```bash
git add crates/opcuasim-core/src/server/history_node_manager.rs crates/opcuasim-core/src/server/mod.rs
git commit -m "feat(core): history-capable node manager delegating to SimpleNodeManagerImpl"
```

---

### Task 3: core — server.rs 接线 + address_space history_readable

**Files:**
- Modify: `crates/opcuasim-core/src/server/server.rs`
- Modify: `crates/opcuasim-core/src/server/address_space.rs`

**Interfaces:**
- Consumes: Task 1 `HistoryStore`、Task 2 `HistoryNodeManagerImpl`、Task 6 `register_demo_methods`(后接,本任务先留 TODO 注册点或 Task 6 单独加)
- Produces:
  - `OpcUaServer.node_manager: Arc<RwLock<Option<Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>>>>`
  - `OpcUaServer::node_manager() -> Option<Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>>`(签名变更)
  - `build_server` 用闭包构造自定义 node manager
  - `add_variable_node` 对变量节点调 `.history_readable()`

- [ ] **Step 1: address_space.rs 加 history_readable**

`crates/opcuasim-core/src/server/address_space.rs` 的 `add_variable_node`(约 96-99 行):

```rust
let mut builder = VariableBuilder::new(&node_id, &node.display_name, &node.display_name)
    .data_type(dt_node_id)
    .value(initial_value)
    .organized_by(parent_id)
    .history_readable();
```

(`VariableBuilder::history_readable()` 已在 async-opcua-nodes variable.rs:85 确认存在;注意若 `history_buffer_size == 0` 时仍设置该属性无副作用——属性只是允许读取历史,数据为空时返回空 HistoryData)

- [ ] **Step 2: server.rs 换 node manager**

`build_server` 中替换 `simple_node_manager(...)` 调用(约 58-64 行):

```rust
use opcua_server::node_manager::memory::{
    InMemoryNodeManagerBuilder, SimpleNodeManagerBuilder,
};
use opcua_server::node_manager::ServerContext;
use opcua_server::address_space::AddressSpace;
use super::history_node_manager::HistoryNodeManagerImpl;
use super::history_store::HistoryStore;

// build_server 内,替换 with_node_manager 参数:
let history = Arc::new(HistoryStore::new(config.history_buffer_size));
let ns_meta = NamespaceMetadata {
    namespace_uri: NAMESPACE_URI.to_string(),
    ..Default::default()
};
let history_for_impl = history.clone();
let nm_builder = move |context: ServerContext, address_space: &mut AddressSpace| {
    let inner = SimpleNodeManagerBuilder::new(ns_meta.clone(), "SimNodeManager")
        .build(context, address_space);
    HistoryNodeManagerImpl::new(inner, history_for_impl.clone())
};
builder = builder.with_node_manager(InMemoryNodeManagerBuilder::new(nm_builder));
```

**注意:**
- `ServerBuilder::with_node_manager(impl NodeManagerBuilder)`——`InMemoryNodeManagerBuilder<T>` 实现 `NodeManagerBuilder`(mod.rs:85 已确认)
- 闭包签名 `FnOnce(ServerContext, &mut AddressSpace) -> R` 由 blanket impl 覆盖(已确认)
- `BuildResult` 增加 `history: Arc<HistoryStore>` 字段;`OpcUaServer` 增加 `history_store: Arc<RwLock<Option<Arc<HistoryStore>>>>` 字段(`new()` 初始化 None,`start()` 设 Some)——供 Task 4/UI 查询占用
- 原 `build_server` 中 `let ns_index = ...` 逻辑不变(`SimpleNodeManagerBuilder::build` 内部已完成 namespace 注册与 import——闭包内 build 后地址空间已填充;若发现 populate_address_space 时机问题,调整闭包:先 build 再 populate 或保持原顺序,以实际行为为准)

- [ ] **Step 3: 更新 node_manager() 返回类型与调用点**

```rust
pub async fn node_manager(&self) -> Option<Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>> {
    self.node_manager.read().await.clone()
}
```

grep 定位调用点:
- `crates/opcuasim-core/src/server/test_methods.rs`(Task 6 删除,本任务先改为编译兼容或直接删除后由 Task 6 补)
- `crates/opcuasim-core/tests/*.rs`(如有调用,更新类型或删除)
- `simulation_engine.start(sim_nm, subscriptions)`——`sim_nm` 类型变化后 `set_values`/`set_value` 仍可用(InMemoryNodeManager 泛型方法)

- [ ] **Step 4: 验证编译**

Run: `cargo build --workspace`
Expected: 编译通过(若有遗留调用点错误,逐一修复)。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/src/server/server.rs crates/opcuasim-core/src/server/address_space.rs
git commit -m "feat(core): wire history node manager into server build, mark nodes history-readable"
```

---

### Task 4: core — SimulationEngine 记录历史

**Files:**
- Modify: `crates/opcuasim-core/src/server/simulation.rs`
- Modify: `crates/opcuasim-core/src/server/server.rs`

**Interfaces:**
- Consumes: Task 1 `HistoryStore`
- Produces:
  - `SimulationEngine` 增加 `history_store: Option<Arc<HistoryStore>>` 字段
  - `pub fn set_history_store(&self, store: Arc<HistoryStore>)`(start 前调用)
  - 值更新循环中 `store.record(&node_state.opcua_node_id, dv.clone()).await`

- [ ] **Step 1: SimulationEngine 加字段与 setter**

```rust
pub struct SimulationEngine {
    cancel_token: CancellationToken,
    node_states: Arc<RwLock<HashMap<String, NodeSimState>>>,
    update_seq: Arc<RwLock<u64>>,
    current_values: Arc<RwLock<HashMap<String, (String, u64)>>>,
    history_store: Option<Arc<HistoryStore>>,
}

impl SimulationEngine {
    pub fn new() -> Self {
        Self {
            cancel_token: CancellationToken::new(),
            node_states: Arc::new(RwLock::new(HashMap::new())),
            update_seq: Arc::new(RwLock::new(0)),
            current_values: Arc::new(RwLock::new(HashMap::new())),
            history_store: None,
        }
    }

    /// Attach the history store; simulation updates will be recorded there.
    pub fn set_history_store(&self, store: Arc<HistoryStore>) {
        // SimulationEngine is behind Arc; take write lock to set
        // (or store in an Arc<RwLock<Option<...>>> field if set after start —
        // 本设计在 start() 前调用,用 Arc<Mutex<Option<...>>> 或直接字段+Arc 内部可变:
        // 因 self 是 &self,需内部可变——用 tokio::sync::RwLock<Option<Arc<HistoryStore>>>)
    }
}
```

**注意:** `set_history_store(&self, ...)` 需要内部可变。改为字段 `history_store: Arc<RwLock<Option<Arc<HistoryStore>>>>`;循环内 `let store = self.history_store.read().await.clone()` 后 `if let Some(s) = store { s.record(...).await; }`。`OpcUaServer::start()` 在 `sim_engine.start(...)` 前调用 `sim_engine.set_history_store(history.clone()).await`(若 setter 为 async)或同步版本。

- [ ] **Step 2: 值更新循环记录**

`simulation.rs` 生成值处(约 127-141 行),`updates.push(...)` 前/后:

```rust
let mut dv = DataValue::new_now(variant);
dv.source_timestamp = Some(now);
dv.server_timestamp = Some(now);

// Record into history ring buffer (best-effort; store may be absent)
if let Some(store) = history_store.read().await.as_ref().cloned() {
    store.record(&node_state.opcua_node_id, dv.clone()).await;
}

updates.push((&node_state.opcua_node_id, None, dv));
```

`start()` 中把 `history_store` clone 进各 group task(与 `vals` 相同方式捕获)。

- [ ] **Step 3: server.rs 调用 set_history_store**

`OpcUaServer::start()` 中(约 224-228 行,sim_engine 启动前):

```rust
let sim_engine = Arc::new(SimulationEngine::new());
sim_engine.set_history_store(history.clone());
sim_engine.register_nodes(nodes, ns_index).await;
sim_engine.start(sim_nm, subscriptions);
```

`history` 来自 `BuildResult`(Task 3 已加字段),并存入 `self.history_store`。

- [ ] **Step 4: 验证编译**

Run: `cargo build --workspace`
Expected: 编译通过。

- [ ] **Step 5: 提交**

```bash
git add crates/opcuasim-core/src/server/simulation.rs crates/opcuasim-core/src/server/server.rs
git commit -m "feat(core): record simulation value updates into history store"
```

---

### Task 5: core — ServerConfig.history_buffer_size

**Files:**
- Modify: `crates/opcuasim-core/src/server/models.rs`

**Interfaces:**
- Produces:
  - `ServerConfig.history_buffer_size: usize`(`#[serde(default = "default_history_buffer_size")]`)
  - `fn default_history_buffer_size() -> usize { 10_000 }`

- [ ] **Step 1: models.rs 加字段**

`ServerConfig`(约 130 行)加:

```rust
/// Per-node history ring buffer capacity. 0 disables history recording.
#[serde(default = "default_history_buffer_size")]
pub history_buffer_size: usize,
```

文件内加:

```rust
fn default_history_buffer_size() -> usize {
    10_000
}
```

- [ ] **Step 2: 修复构造点**

grep `ServerConfig {` 定位所有构造点(e2e.rs、server dispatcher、tests),补 `history_buffer_size: 10_000`(或测试按需覆盖)。

- [ ] **Step 3: 验证编译**

Run: `cargo build --workspace`
Expected: 编译通过。

- [ ] **Step 4: 提交**

```bash
git add crates/opcuasim-core/src/server/models.rs crates/opcuamaster-egui/tests/e2e.rs crates/opcuaserver-egui/src/backend/dispatcher.rs
git commit -m "feat(core): configurable history buffer size per node"
```

---

### Task 6: core — methods.rs 预置方法 + 注册

**Files:**
- Create: `crates/opcuasim-core/src/server/methods.rs`
- Modify: `crates/opcuasim-core/src/server/server.rs`
- Modify: `crates/opcuasim-core/src/server/mod.rs`
- Delete: `crates/opcuasim-core/src/server/test_methods.rs`

**Interfaces:**
- Consumes: Task 2 `HistoryNodeManagerImpl::add_method_callback`(经 `nm.inner()`)、Task 3 的 `OpcUaServer`
- Produces:
  - `pub async fn register_demo_methods(server: &OpcUaServer) -> Result<Vec<NodeId>, OpcUaSimError>`

- [ ] **Step 1: 实现 methods.rs**

```rust
//! Preset demo methods registered at server startup (A2).

use opcua_nodes::MethodBuilder;
use opcua_types::{
    Argument, DataTypeId, LocalizedText, NodeId, StatusCode, UAString, Variant,
};

use crate::error::OpcUaSimError;
use crate::server::server::OpcUaServer;

/// Register the preset demo methods. Returns their NodeIds.
pub async fn register_demo_methods(server: &OpcUaServer) -> Result<Vec<NodeId>, OpcUaSimError> {
    let nm = server
        .node_manager()
        .await
        .ok_or_else(|| OpcUaSimError::ServerError("Server not started".into()))?;
    let ns = server.namespace_index().await;

    let mut ids = Vec::new();

    // Echo: String -> String
    ids.push(register_method(
        &nm,
        ns,
        "Demo.Echo",
        "Echo",
        &[arg("input", DataTypeId::String)],
        &[arg("output", DataTypeId::String)],
        |inputs: &[Variant]| match inputs.first() {
            Some(Variant::String(s)) => Ok(vec![Variant::String(s.clone())]),
            _ => Err(StatusCode::BadInvalidArgument),
        },
    ));

    // Add: Double + Double -> Double
    ids.push(register_method(
        &nm,
        ns,
        "Demo.Add",
        "Add",
        &[arg("a", DataTypeId::Double), arg("b", DataTypeId::Double)],
        &[arg("sum", DataTypeId::Double)],
        |inputs: &[Variant]| {
            let a = match inputs.first() {
                Some(Variant::Double(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let b = match inputs.get(1) {
                Some(Variant::Double(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            Ok(vec![Variant::Double(a + b)])
        },
    ));

    // RandomValue: Double (max, 0 = default 100) -> Double
    ids.push(register_method(
        &nm,
        ns,
        "Demo.RandomValue",
        "RandomValue",
        &[arg("max", DataTypeId::Double)],
        &[arg("value", DataTypeId::Double)],
        |inputs: &[Variant]| {
            let max = match inputs.first() {
                Some(Variant::Double(v)) if *v > 0.0 => *v,
                _ => 100.0,
            };
            Ok(vec![Variant::Double(rand::random::<f64>() * max)])
        },
    ));

    // SetNodeValue: String (node id) + Double -> String (status)
    let nm_for_set = nm.clone();
    let ns_for_set = ns;
    ids.push(register_method(
        &nm_for_set,
        ns_for_set,
        "Demo.SetNodeValue",
        "SetNodeValue",
        &[arg("node_id", DataTypeId::String), arg("value", DataTypeId::Double)],
        &[arg("status", DataTypeId::String)],
        move |inputs: &[Variant]| {
            let node_id_str = match inputs.first() {
                Some(Variant::String(s)) => s.to_string(),
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let value = match inputs.get(1) {
                Some(Variant::Double(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let Ok(nid) = node_id_str.parse::<NodeId>() else {
                return Ok(vec![Variant::String(UAString::from(format!("BadNodeIdUnknown: {node_id_str}")))]);
            };
            let now = opcua_types::DateTime::now();
            let dv = opcua_types::DataValue::new_now(Variant::Double(value))
                .with_source_timestamp(now)
                .with_server_timestamp(now);
            match nm_for_set.set_value(&server_subscriptions_or_default(), &nid, None, dv) {
                Ok(()) => Ok(vec![Variant::String(UAString::from("Good"))]),
                Err(e) => Ok(vec![Variant::String(UAString::from(format!("{e}"))) ]),
            }
        },
    ));

    Ok(ids)
}

fn arg(name: &str, data_type: DataTypeId) -> Argument {
    Argument {
        name: UAString::from(name),
        data_type: data_type.into(),
        value_rank: -1,
        array_dimensions: None,
        description: LocalizedText::from(""),
    }
}

#[allow(clippy::too_many_arguments)]
fn register_method(
    nm: &opcua_server::node_manager::memory::InMemoryNodeManager<super::history_node_manager::HistoryNodeManagerImpl>,
    ns: u16,
    node_id_str: &str,
    display_name: &str,
    in_args: &[Argument],
    out_args: &[Argument],
    cb: impl Fn(&[Variant]) -> Result<Vec<Variant>, StatusCode> + Send + Sync + 'static,
) -> NodeId {
    let method_id = NodeId::new(ns, node_id_str);
    let in_args_id = NodeId::new(ns, format!("{node_id_str}.InputArguments"));
    let out_args_id = NodeId::new(ns, format!("{node_id_str}.OutputArguments"));

    {
        let mut addr = nm.address_space().write();
        let _ = MethodBuilder::new(&method_id, display_name, display_name)
            .component_of(opcua_types::ObjectId::ObjectsFolder.into())
            .executable(true)
            .user_executable(true)
            .input_args(&mut *addr, &in_args_id, in_args)
            .output_args(&mut *addr, &out_args_id, out_args)
            .insert(&mut *addr);
    }

    nm.inner().add_method_callback(method_id.clone(), cb);
    method_id
}
```

**注意:**
- `SetNodeValue` 回调需要访问订阅缓存以通知变化——`set_value` 签名是 `set_value(&self, subscriptions: &SubscriptionCache, id, index_range, value)`。**方案:闭包捕获 `Arc<SubscriptionCache>`**,`OpcUaServer::start()` 注册时从 `BuildResult.subscriptions` 传入。调整 `register_demo_methods` 签名:加 `subscriptions: Arc<SubscriptionCache>` 参数,SetNodeValue 闭包捕获之
- `DataValue::with_source_timestamp/with_server_timestamp` 若不存在,用 `dv.source_timestamp = Some(now)` 字段赋值(DataValue 字段 pub,已确认)
- `nm.address_space()` 返回 `&Arc<RwLock<AddressSpace>>`(mod.rs:108 已确认),`.write()` 是库锁的 guard
- `nm.inner()` 返回 `&HistoryNodeManagerImpl`(Task 2 已提供 `add_method_callback` 转发)
- `register_method` 的 `nm` 类型参数太长——用 `use ... as HistNm` 别名或直接接受 `&Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>`
- `rand` 依赖已在 opcuasim-core(rand = "0.8")

- [ ] **Step 2: server.rs start() 注册方法 + 删除 test_methods**

`OpcUaServer::start()` 中(sim_engine 启动后、server.run() 前):

```rust
// Register preset demo methods
let _ = super::methods::register_demo_methods(self).await;
```

(`register_demo_methods` 需要 subscriptions——从 `BuildResult.subscriptions` 传给函数或经 server 内部字段;按 Step 1 调整签名后在 start() 内构造)

删除 `crates/opcuasim-core/src/server/test_methods.rs`,`server/mod.rs` 移除 `pub mod test_methods;` 加 `pub mod methods;`、`pub mod history_node_manager;`(若 Task 2 未加)、`pub mod history_store;`(若 Task 1 未加)。

- [ ] **Step 3: 验证编译**

Run: `cargo build --workspace`
Expected: 编译通过;若 e2e.rs 引用 test_methods,更新或删除引用。

- [ ] **Step 4: 提交**

```bash
git add crates/opcuasim-core/src/server/methods.rs crates/opcuasim-core/src/server/server.rs crates/opcuasim-core/src/server/mod.rs
git rm crates/opcuasim-core/src/server/test_methods.rs
git commit -m "feat(core): register preset demo methods at server startup"
```

---

### Task 7: server egui — 历史容量设置 UI

**Files:**
- Modify: `crates/opcuaserver-egui/src/events.rs`(Config 相关 DTO 若含 ServerConfig 直通则无需改;若手写字段则加)
- Modify: `crates/opcuaserver-egui/src/model.rs`(设置表单)
- Modify: `crates/opcuaserver-egui/src/backend/dispatcher.rs`(Config 下发/接收)
- Modify: `crates/opcuaserver-egui/src/panels/*.rs`(设置面板)

**Interfaces:**
- Consumes: Task 5 `ServerConfig.history_buffer_size`
- Produces: 设置面板"历史缓冲容量(条/节点)"输入框,0 = 禁用

- [ ] **Step 1: 定位设置面板**

grep `ServerConfig` / "设置" 定位 opcuaserver-egui 的设置 UI(预计 `panels/settings.rs` 或类似,也可能在 toolbar/status_bar)。读该文件确认现有配置项渲染模式(如端口、安全策略)。

- [ ] **Step 2: 加输入框**

沿现有模式加:

```rust
ui.horizontal(|ui| {
    ui.label(RichText::new("历史缓冲容量(条/节点):").small().color(theme::TEXT_MUTED()));
    let mut cap = model.config.history_buffer_size;
    if ui.add(egui::DragValue::new(&mut cap).range(0..=1_000_000)).changed() {
        model.config.history_buffer_size = cap;
        // 沿现有 SaveConfig/Config 事件流下发(与端口等字段一致)
    }
});
```

- [ ] **Step 3: 验证编译 + fmt**

Run: `cargo fmt && cargo build --workspace`
Expected: 编译通过。

- [ ] **Step 4: 提交**

```bash
git add crates/opcuaserver-egui/src/
git commit -m "feat(server-ui): history buffer size setting"
```

---

### Task 8: 集成测试 server_history.rs + server_methods.rs

**Files:**
- Create: `crates/opcuasim-core/tests/server_history.rs`
- Create: `crates/opcuasim-core/tests/server_methods.rs`
- Modify: `crates/opcuasim-core/tests/reconnect_e2e.rs` 等(若 ServerConfig 构造需补字段,Task 5 已处理)

**Interfaces:**
- Consumes: `OpcUaServer`、`history_read_raw`(client)、`call_method`(client)、Task 1-6 实现

- [ ] **Step 1: server_history.rs**

```rust
//! End-to-end: server records simulated + externally written values into
//! history, readable via the client history_read_raw loop (with paging).

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::history::history_read_raw;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

const PORT: u16 = 48430;

fn server_config() -> ServerConfig {
    ServerConfig {
        name: "HistoryE2E".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        port: PORT,
        security_policies: vec!["None".into()],
        security_modes: vec!["None".into()],
        users: Vec::new(),
        anonymous_enabled: true,
        max_sessions: 10,
        max_subscriptions_per_session: 10,
        history_buffer_size: 10_000,
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

fn writable_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Setpoint".into(),
        display_name: "Setpoint".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: true,
        simulation: SimulationMode::Static { value: "0".into() },
        update_seq: 0,
        current_value: None,
        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn history_records_simulation_and_external_writes() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(), &[], &[sine_node(), writable_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "h1".into(),
        name: "h1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    // 1. Simulated node: wait ~1s, then read history — expect several samples.
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let sine_id: opcua_types::NodeId = "ns=2;s=Demo.Sine".parse().unwrap();
    let now = opcua_types::DateTime::now();
    let start = now - chrono::Duration::seconds(30).to_std().unwrap(); // 或 DateTime 减法 API,以库为准
    let points = history_read_raw(&session, &sine_id, start, now, 1000, false)
        .await
        .expect("history read");
    assert!(
        points.len() >= 3,
        "expected >=3 simulated history samples, got {}",
        points.len()
    );
    // timestamps monotonic
    let ts: Vec<&String> = points.iter().map(|p| &p.source_timestamp).collect();
    assert!(ts.windows(2).all(|w| w[0] <= w[1]), "timestamps must be monotonic");

    // 2. External write: write Setpoint, then history must contain it.
    opcuasim_core::browse::write_node_value(&session, "ns=2;s=Demo.Setpoint", "42.5", "Double")
        .await
        .expect("write");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let sp_id: opcua_types::NodeId = "ns=2;s=Demo.Setpoint".parse().unwrap();
    let points2 = history_read_raw(&session, &sp_id, start, opcua_types::DateTime::now(), 100, false)
        .await
        .expect("history read setpoint");
    assert!(
        points2.iter().any(|p| p.value.contains("42.5")),
        "external write 42.5 must appear in history, got {:?}",
        points2.iter().map(|p| &p.value).collect::<Vec<_>>()
    );

    // 3. Paging: max_values=2 loop must exhaust without error (history_read_raw
    //    internally follows continuation points).
    let points3 = history_read_raw(&session, &sine_id, start, opcua_types::DateTime::now(), 2, false)
        .await
        .expect("paged history read");
    assert_eq!(points3.len(), 2, "max_values=2 must cap the result");

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
```

**注意:**
- `DateTime` 减法/范围构造以库 API 为准(`date_time.rs`):可用 `DateTime::from_timestamp(...)` 或 `DateTime::now() - Duration`;测试内用固定宽窗口(如 from 2026-01-01)避免 API 不确定——仿真样本时间戳都是 now,用 `DateTime::from_ymd(2026,1,1,...)` 作 start 最稳
- `points3.len() == 2` 验证客户端 CP 分页循环正确截断(服务端返回 CP,客户端继续直到 null)——若服务端分页实现正确,`max_values=2` 恰好 2 条

- [ ] **Step 2: server_methods.rs**

```rust
//! End-to-end: preset demo methods are registered and callable.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::method::call_method;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;
use opcua_types::Variant;

const PORT: u16 = 48431;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preset_methods_are_callable() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(
            &ServerConfig {
                name: "MethodsE2E".into(),
                endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
                port: PORT,
                security_policies: vec!["None".into()],
                security_modes: vec!["None".into()],
                users: Vec::new(),
                anonymous_enabled: true,
                max_sessions: 10,
                max_subscriptions_per_session: 10,
                history_buffer_size: 10_000,
            },
            &[],
            &[],
        )
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "m1".into(),
        name: "m1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    // Echo
    let out = call_method(
        &session,
        &"ns=2;s=Demo.Echo".parse().unwrap(),
        vec![Variant::String("hello".into())],
    )
    .await
    .expect("echo");
    assert!(matches!(out.first(), Some(Variant::String(s)) if s.as_ref() == "hello"));

    // Add
    let out = call_method(
        &session,
        &"ns=2;s=Demo.Add".parse().unwrap(),
        vec![Variant::Double(2.0), Variant::Double(3.0)],
    )
    .await
    .expect("add");
    assert!(matches!(out.first(), Some(Variant::Double(v)) if (*v - 5.0).abs() < 1e-9));

    // RandomValue in [0, 100)
    let out = call_method(
        &session,
        &"ns=2;s=Demo.RandomValue".parse().unwrap(),
        vec![Variant::Double(0.0)],
    )
    .await
    .expect("random");
    assert!(matches!(out.first(), Some(Variant::Double(v)) if *v >= 0.0 && *v < 100.0));

    // SetNodeValue writes then reads back
    let out = call_method(
        &session,
        &"ns=2;s=Demo.SetNodeValue".parse().unwrap(),
        vec![
            Variant::String("ns=2;s=Demo.Missing".into()),
            Variant::Double(1.0),
        ],
    )
    .await
    .expect("setnodevalue");
    assert!(matches!(out.first(), Some(Variant::String(s)) if !s.is_empty()));

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
```

`call_method` 签名以 `crates/opcuasim-core/src/method.rs` 为准(读该文件确认参数顺序:`call_method(session, node_id, input_args)` 与返回 `Vec<Variant>`)。

- [ ] **Step 3: 运行测试**

Run: `cargo test -p opcuasim-core --test server_history --test server_methods`
Expected: 2 个测试 PASS。失败时按日志排查(时序问题加 sleep;实现 bug 报告)。

- [ ] **Step 4: 提交**

```bash
git add crates/opcuasim-core/tests/server_history.rs crates/opcuasim-core/tests/server_methods.rs
git commit -m "test(core): e2e coverage for server history and preset methods"
```

---

### Task 9: 全量验证 + 合并

**Files:**
- 全 workspace

- [ ] **Step 1: fmt + clippy + 全量测试**

Run:
```bash
cargo fmt --check
cargo test --workspace
```
Expected: fmt 干净;全部测试 PASS(原有 20 + 新增 server_history/server_methods + HistoryStore 4 单测)。

- [ ] **Step 2: 修复遗留**

如有失败,按 systematic-debugging 定位修复(注意 16 个 pre-existing clippy float 警告不改)。

- [ ] **Step 3: CHANGELOG + 提交**

`CHANGELOG.md` Unreleased 段追加:

```markdown
### Added
- Server: in-memory history buffer with HistoryRead support (simulated and externally written values, paged continuation points)
- Server: preset demo methods (Echo / Add / RandomValue / SetNodeValue) callable from any client
- Server UI: configurable per-node history buffer capacity (0 = disabled)
```

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for server history store and methods"
```

- [ ] **Step 4: 收尾**

Run: `git status --short`
Expected: 干净(仅未跟踪工具目录)。

---

## 执行顺序

Task 1 → 2(依赖 1)→ 3(依赖 2)→ 4(依赖 1,3)→ 5(独立)→ 6(依赖 2,3)→ 7(依赖 5)→ 8(依赖全部)→ 9。Task 5 可与 1-4 并行。

## 验收标准

- [ ] `cargo fmt --check` 通过
- [ ] `cargo test --workspace` 全部 PASS(新增 HistoryStore 4 单测 + server_history + server_methods)
- [ ] 子站对 HistoryRead 返回真实历史数据(仿真 + 外部写入),客户端 CP 分页循环正确取尽
- [ ] 4 个预置方法在任意客户端可调用(Echo/Add/RandomValue/SetNodeValue)
- [ ] `ServerConfig.history_buffer_size` 可配置(0 禁用),旧项目文件兼容
- [ ] 服务端重启历史清空(内存缓冲语义)
