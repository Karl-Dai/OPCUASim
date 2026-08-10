# 子站事件/告警 + 复杂数据类型实施计划(SP2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 A3 事件/告警(4 种事件源 + 事件历史 + RaiseEvent)、A4 复杂数据类型(数组/多维/枚举/结构体/嵌套 + 服务端注册 + 仿真读写)、B3 主站事件订阅 UI(提前)、跨进程结构体解码验证。

**Architecture:** 服务端 `EventStore` 环形缓冲记录事件 + `SubscriptionCache::notify_events` 推送;`DemoEvents` 对象(SUBSCRIBE_TO_EVENTS|HISTORY_READ)作事件源;`HistoryNodeManagerImpl` 覆盖 `history_read_events`;复杂类型经 `DataTypeBuilder` 注册节点 + `custom::DynamicStructure` 动态编码 ExtensionObject;主站 `SubscriptionManager` 扩展事件订阅 + egui 事件面板。无新 crate。

**Tech Stack:** Rust + Tokio,async-opcua 0.18,egui 0.34。已确认库事实:`HistoryEvent`/`HistoryResult for HistoryEvent` 存在;`history_read_events` 服务端调用链完整(attribute.rs:274);validate 要求 Object + `EventNotifier::HISTORY_READ`;`notify_events(&dyn Event, &NodeId)` 经 `ServerHandle::subscriptions()`;`DynamicStructure` 实现 `BinaryEncodable`(custom_struct.rs:254);`StructTypeInfo::from_field`(type_tree.rs:34);`DataTypeTree::new` 独立构造;服务端 `DefaultTypeTree::add_type_node` 可注册类型节点。

## Global Constraints

- 无新增 crate(workspace 现有 async-opcua 0.18 / egui 0.34 足够)
- `cargo fmt` + `cargo test --workspace` 必须通过(16 个 pre-existing float 警告不改)
- `ServerConfig` 新字段 `event_history_size`(serde default 1000,0 禁用),所有构造点同步
- 提交前缀 `feat:`/`test:`/`docs:`,英文消息;每任务单独提交
- 服务端重启事件历史清空(内存缓冲)
- 事件推送无订阅者 no-op,不报错

---

## File Structure

| 文件 | 责任 | 操作 |
|---|---|---|
| `crates/opcuasim-core/src/server/event_store.rs` | EventStore 环形缓冲 + 查询 | 新建 |
| `crates/opcuasim-core/src/server/events.rs` | DemoEvents、notify、RaiseEvent、心跳/连接任务 | 新建 |
| `crates/opcuasim-core/src/server/history_node_manager.rs` | 覆盖 history_read_events | 修改 |
| `crates/opcuasim-core/src/server/models.rs` | DataType 复杂变体、ServerConfig.event_history_size | 修改 |
| `crates/opcuasim-core/src/server/address_space.rs` | 复杂类型注册、f64_to_variant 扩展 | 修改 |
| `crates/opcuasim-core/src/server/simulation.rs` | 告警检测、复杂类型生成 | 修改 |
| `crates/opcuasim-core/src/server/methods.rs` | RaiseEvent 方法 | 修改 |
| `crates/opcuasim-core/src/server/server.rs` | 任务 spawn、类型注册接线 | 修改 |
| `crates/opcuasim-core/src/browse.rs` | string_to_variant 复杂类型解析 | 修改 |
| `crates/opcuasim-core/src/subscription.rs` | 事件订阅 + EventLog | 修改 |
| `crates/opcuasim-core/src/events.rs` | EventItem 数据结构 | 新建 |
| `crates/opcuasim-core/src/history.rs` | history_read_events 客户端包装 | 修改 |
| `crates/opcuaserver-egui/src/panels/*.rs` | 复杂类型值显示/编辑 | 修改 |
| `crates/opcuamaster-egui/src/panels/events_panel.rs` | 主站事件面板 | 新建 |
| `crates/opcuamaster-egui/src/backend/*.rs` | Events 事件流 | 修改 |
| `crates/opcuasim-core/tests/server_events.rs` | 事件 e2e | 新建 |
| `crates/opcuasim-core/tests/server_complex_types.rs` | 复杂类型 e2e + 跨进程解码 | 新建 |
| `crates/opcuamaster-egui/tests/e2e.rs` | 主站事件订阅 e2e | 修改 |

---

### Task 1: core — EventStore + EventItem 模型

**Files:**
- Create: `crates/opcuasim-core/src/server/event_store.rs`
- Create: `crates/opcuasim-core/src/events.rs`
- Modify: `crates/opcuasim-core/src/server/mod.rs`

**Interfaces:**
- Consumes: `opcua_types::{Variant, DateTime}`
- Produces:
  - `crates/opcuasim-core/src/events.rs`:`pub struct EventItem { pub time: String, pub severity: u16, pub source: String, pub message: String, pub event_type: String }` + `pub struct EventLog { items: Arc<RwLock<VecDeque<EventItem>>>, capacity: usize }` + `impl EventLog { new/add/items/clear }`
  - `event_store.rs`:`pub struct EventStore` + `record(&self, time: DateTime, fields: Vec<Variant>)` + `query(&self, start, end, max_values, skip) -> (Vec<(DateTime, Vec<Variant>)>, Option<usize>)` + `len()`

- [ ] **Step 1: 写失败测试(EventStore 单测)**

`crates/opcuasim-core/src/server/event_store.rs` 底部 `#[cfg(test)] mod tests`(与 SP1 HistoryStore 测试同风格):
- ring 淘汰:容量 3 插 5 剩 3,查询得最新 3 条、时间有序
- 时间区间过滤:[start,end] 闭区间
- 分页:2 条/页三页取尽,next_skip 正确
- 零容量禁用

- [ ] **Step 2: 运行测试确认失败** → `cargo test -p opcuasim-core event_store` 编译失败

- [ ] **Step 3: 实现 EventStore + EventItem**

```rust
// event_store.rs
use std::collections::{HashMap, VecDeque};
use tokio::sync::RwLock;
use opcua_types::{DateTime, Variant};

/// Per-source ring buffer of event field lists.
pub struct EventStore {
    buffers: RwLock<HashMap<opcua_types::NodeId, VecDeque<(DateTime, Vec<Variant>)>>>,
    capacity: usize,
}

impl EventStore {
    pub fn new(capacity: usize) -> Self { ... }
    pub async fn record(&self, node_id: &NodeId, time: DateTime, fields: Vec<Variant>) { /* 容量 0 no-op;满 pop_front */ }
    pub async fn query(&self, node_id: &NodeId, start: DateTime, end: DateTime, max_values: u32, skip: usize)
        -> (Vec<(DateTime, Vec<Variant>)>, Option<usize>) { /* 时间过滤闭区间;skip 分页;同 HistoryStore::query 模式 */ }
    pub async fn len(&self, node_id: &NodeId) -> usize { ... }
}
```

- [ ] **Step 4: 测试 PASS + 提交**

```bash
cargo test -p opcuasim-core event_store
git add crates/opcuasim-core/src/server/event_store.rs crates/opcuasim-core/src/events.rs crates/opcuasim-core/src/server/mod.rs
git commit -m "feat(core): event store ring buffer and event item model"
```

---

### Task 2: core — DemoEvents 对象 + notify 基础设施 + RaiseEvent

**Files:**
- Create: `crates/opcuasim-core/src/server/events.rs`
- Modify: `crates/opcuasim-core/src/server/server.rs`
- Modify: `crates/opcuasim-core/src/server/methods.rs`

**Interfaces:**
- Consumes: Task 1 EventStore;库 `ObjectBuilder`、`BaseEventType`、`SubscriptionCache::notify_events`、`ServerHandle`
- Produces:
  - `pub const DEMO_EVENTS_ID: &str = "DemoEvents";`
  - `pub fn build_events_object(address_space: &mut AddressSpace, ns: u16) -> Result<NodeId, OpcUaSimError>`——`ObjectBuilder::new(&id, "DemoEvents", "DemoEvents").event_notifier(EventNotifier::SUBSCRIBE_TO_EVENTS | EventNotifier::HISTORY_READ).organized_by(ObjectId::ObjectsFolder).has_type_definition(ObjectTypeId::BaseObjectType).insert(&mut *address_space)`
  - `pub fn notify_event(handle: &ServerHandle, source: &NodeId, message: &str, severity: u16)`——构造 BaseEventType(event_id 随机、event_type=BaseEventType、source_node、source_name、time/receive_time=now、message、severity),`handle.subscriptions().notify_events([(&event, source)])`;同时写 EventStore(若已挂接)
  - `pub fn register_raise_event_method(nm: &Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>, subscriptions: Arc<SubscriptionCache>, event_store: Option<Arc<EventStore>>)`——`Demo.RaiseEvent(severity: UInt16, message: String)` 方法回调 → 构造事件 + notify + record

- [ ] **Step 1: 读库源码确认 API**
  - `async-opcua-nodes-0.18.0/src/object.rs`(ObjectBuilder::event_notifier)
  - `async-opcua-nodes-0.18.0/src/events/event.rs`(BaseEventType 字段)
  - `async-opcua-server-0.18.0/src/subscriptions/mod.rs:397`(notify_events 签名)
  - `server_handle.rs:67`(subscriptions())
  - `ObjectTypeId::BaseObjectType`、`EventNotifier` bitflags(lib.rs:115)
  - `random::byte_string(6)` 生成 event_id(async-opcua-crypto)

- [ ] **Step 2: 实现 events.rs**

BaseEventType 构造注意:event_type 字段应为 `ObjectTypeId::BaseEventType.into()`;source_node 为 DemoEvents NodeId;source_name 为 "DemoEvents";message 为 `LocalizedText::from(message)`。

notify_event 同时把事件字段写入 EventStore(通过 `OpcUaServer` 内部持有的 `event_store: Arc<Option<Arc<EventStore>>>` 或直接闭包捕获)——**接线方式**:`OpcUaServer` 增加 `event_store: Arc<RwLock<Option<Arc<EventStore>>>>` 字段(SP1 的 history_store 模式),start() 时构造并注入。

- [ ] **Step 3: server.rs 接线 + methods.rs 注册**

- start() 中:
  ```rust
  let event_store = Arc::new(EventStore::new(config.event_history_size));
  let demos = super::events::build_events_object(&mut addr, ns).expect("events object");
  // 存入 self.event_store
  // 注册 RaiseEvent(复用 Task 6 SP1 的 register_method 模式,或 methods.rs 内已有辅助)
  ```
- `register_raise_event_method` 需访问 subscriptions(从 BuildResult)与 event_store

- [ ] **Step 4: 编译 + 提交**

```bash
cargo build --workspace
git add crates/opcuasim-core/src/server/events.rs crates/opcuasim-core/src/server/server.rs crates/opcuasim-core/src/server/methods.rs
git commit -m "feat(core): events object, notify infrastructure and RaiseEvent method"
```

---

### Task 3: core — 心跳 + 连接状态后台任务

**Files:**
- Modify: `crates/opcuasim-core/src/server/events.rs`
- Modify: `crates/opcuasim-core/src/server/server.rs`

**Interfaces:**
- Consumes: Task 2 notify_event;库 `ServerHandle::session_manager()`?→ 不,连接计数走地址空间变量
- Produces:
  - `pub fn spawn_heartbeat_task(handle: Arc<ServerHandle>, source: NodeId, event_store: Arc<EventStore>, interval: Duration) -> JoinHandle<()>`——每 5s `notify_event(&handle, &source, &format!("Heartbeat {seq}"), 100)`,seq 递增
  - `pub fn spawn_connection_monitor_task(handle: Arc<ServerHandle>, source: NodeId, address_space: Arc<RwLock<AddressSpace>>, interval: Duration) -> JoinHandle<()>`——每 1s 读 `VariableId::Server_ServerDiagnostics_ServerDiagnosticsSummary_CurrentSessionCount` 变量值,与上次比较,变化则发事件

- [ ] **Step 1: 确认连接计数读取路径**

读库源码 `async-opcua-server-0.18.0/src/diagnostics/server.rs`:
- `ServerDiagnostics` 是否在 ServerHandle/ServerInfo 可访问?或从地址空间读变量:NodeId = `VariableId::Server_ServerDiagnostics_ServerDiagnosticsSummary_CurrentSessionCount`(`opcua_types::VariableId` 枚举,async-opcua-types)
- `address_space.read_value(&node_id)` 或 `find_node` + `get_attribute` 读当前值——以库 API 为准
- 若地址空间读取不便,改用 `ServerStatusWrapper`/`ServerDiagnostics` 公开 getter(若存在)——实现时二选一,优先地址空间读变量

- [ ] **Step 2: 实现 + 接线**

server.rs start() 中 spawn 两个任务(取消用 `handle.token()` 或单独 CancellationToken,随 server stop 取消)。

- [ ] **Step 3: 编译 + 提交**

```bash
cargo build --workspace
git commit -am "feat(core): heartbeat and connection-state event tasks"
```

---

### Task 4: core — 阈值告警(仿真引擎检测 + 去重)

**Files:**
- Modify: `crates/opcuasim-core/src/server/simulation.rs`
- Modify: `crates/opcuasim-core/src/server/events.rs`(或直接注入)

**Interfaces:**
- Consumes: Task 2 notify_event;现有 `eu_range_low/high`
- Produces:
  - `SimulationEngine` 增加 `alarm_states: Arc<RwLock<HashMap<String, bool>>>`(node_id → alarm_active)
  - 值更新循环中:对数值型节点(非复杂类型),生成值后比较 eu_range:越限且未激活 → 发告警事件 + 置激活;未越限且激活 → 发恢复事件 + 清激活

- [ ] **Step 1: 读 simulation.rs 值更新循环(127-160 行,SP1 后版本)**

- [ ] **Step 2: 实现告警检测**

```rust
// 在 updates.push 后、record 历史后(或同处):
if node_state.simulation.alarm_enabled() {  // 新增字段?不——用 eu_range 是否配置
    let raw = raw_value;
    let high = node_state.eu_range_high;
    let low = node_state.eu_range_low;
    let active = *alarm_states.read().await.get(&node_state.opcua_node_id.to_string()).unwrap_or(&false);
    let is_out = raw > high || raw < low;
    match (is_out, active) {
        (true, false) => { notify_event(&handle, &source, &format!("{} exceeded limit ({}..{})", display, low, high), 500); alarm_states.write().await.insert(id, true); }
        (false, true) => { notify_event(&handle, &source, &format!("{} back to normal", display), 100); alarm_states.write().await.insert(id, false); }
        _ => {}
    }
}
```

- **设计注意**:SimulationEngine 需要 `ServerHandle` 或 notify 回调——**接线**:`OpcUaServer::start()` 把 `Arc<ServerHandle>`(或闭包)传入 SimulationEngine;SP1 已传 sim_nm/subscriptions,扩展传 handle
- 阈值告警的 source = DemoEvents(统一事件源)

- [ ] **Step 3: 编译 + 提交**

```bash
cargo build --workspace
git commit -am "feat(core): alarm events on eu-range limit breach with dedup"
```

---

### Task 5: core — 事件历史 history_read_events

**Files:**
- Modify: `crates/opcuasim-core/src/server/history_node_manager.rs`
- Modify: `crates/opcuasim-core/src/history.rs`(客户端包装)
- Modify: `crates/opcuasim-core/src/server/server.rs`(EventStore 注入 impl)

**Interfaces:**
- Consumes: Task 1 EventStore、Task 2 接线;库 `HistoryEvent`/`HistoryEventFieldList`/`ReadEventDetails`/`EventFilter`/`SimpleAttributeOperand`/`EventFilterOperator`
- Produces:
  - `HistoryNodeManagerImpl` 增加 `event_store: Arc<EventStore>` 字段(`new()` 签名扩展)
  - `history_read_events(&self, context, details: &ReadEventDetails, nodes, timestamps) -> Result<(), StatusCode>` 覆盖:
    - 对每个 HistoryNode:`node_id` 为事件源(DemoEvents)→ `event_store.query(node_id, start, end, num_values_per_node, skip)`(skip 从 CP 解码,同 A1 的 Box<dyn Any> 方案)
    - 字段选择:`details.filter.select_clauses` 非空 → 按 browse_path 选取字段(标准字段路径:Time/Severity/SourceNode/SourceName/Message/EventId/EventType);空 → 全部字段
    - where_clause:仅支持 `EventFilterOperator::Equals` 单条件(字段路径 + 字面量),不匹配的过滤掉;其他运算符/复杂结构 → 视为全选
    - `node.set_result(HistoryEvent { events: Some(vec![HistoryEventFieldList { event_fields: Some(fields) }]) })`;`set_next_continuation_point`(skip 编码,同 A1);`set_status(Good)`
  - `history.rs` 客户端:`pub async fn history_read_events(session, node_id, start, end, max_events, filter: EventFilter) -> Result<Vec<Vec<String>>, OpcUaSimError>`——HistoryReadAction::ReadEventDetails 循环 CP(参照 history_read_raw 模式),返回每个事件的字段值(字符串化)

- [ ] **Step 1: 读库源码确认**
  - `attribute.rs:268-276`(ReadEventDetails 分发)
  - `history.rs:13-50`(HistoryNode API,SP1 已读)
  - `event_filter.rs`/`simple_attribute_operand.rs`/`content_filter.rs`(EventFilter 结构,SP2 探索已确认)
  - `HistoryReadAction::ReadEventDetails`(async-opcua-client)

- [ ] **Step 2: 实现服务端覆盖 + 客户端包装**

- EventStore 注入:`HistoryNodeManagerImpl::new(inner, history, event_store)`——server.rs 构造闭包处同步更新
- CP 编码复用 SP1 的 `encode_skip`/`parse_skip`(usize Box)

- [ ] **Step 3: 编译 + 提交**

```bash
cargo build --workspace
git commit -am "feat(core): event history read with field selection and paging"
```

---

### Task 6: core — DataType 复杂类型模型 + 服务端注册(最大风险)

**Files:**
- Modify: `crates/opcuasim-core/src/server/models.rs`(DataType 扩展)
- Modify: `crates/opcuasim-core/src/server/address_space.rs`(注册 + f64_to_variant)
- Modify: `crates/opcuasim-core/src/server/server.rs`(接线)

**Interfaces:**
- Consumes: 库 `DataTypeBuilder`、`StructureDefinition`、`EnumDefinition`、`custom::DataTypeTree`/`StructTypeInfo`/`DynamicStructure`、`DefaultTypeTree::add_type_node`
- Produces:
  - `DataType` 枚举扩展(见 spec F6)
  - `pub fn register_custom_types(address_space: &mut AddressSpace, type_tree: &mut DefaultTypeTree, ns: u16, types: &[ServerNode]) -> Result<HashMap<String, NodeId>, OpcUaSimError>`——收集配置中的复杂类型(数组元素类型不需要 DataType 节点;枚举/结构体需要),注册:
    - 枚举:`DataTypeBuilder::new(&enum_id, name, name).data_type_definition(DataTypeDefinition::Enum(EnumDefinition { fields })).is_abstract(false).insert(&mut *address_space)`;type_tree.add_type_node
    - 结构体:`DataTypeBuilder` + `StructureDefinition { default_encoding_id: encoding_id, base_data_type: Structure.into(), structure_type: StructureType::Structure, fields }`;encoding 对象节点(DataTypeEncodingBuilder 或 ObjectBuilder subtype)——**实现时验证 encoding 节点注册 API**;type_tree.add_type_node
  - `pub fn type_variant(value: f64, dt: &DataType) -> Variant`(替代/扩展 f64_to_variant):
    - 数组:生成 N 个元素(Demo 场景固定 4 或可配)→ `Variant::Array(Box::new(Array::new(...)))`
    - 二维:`Array::new_multi`(dims [2,2])
    - 枚举:由值映射到字段索引 → `Variant::Int32`
    - 结构体:构造 `DataTypeTree`(types::custom)+ `StructTypeInfo` + `DynamicStructure::new_struct` → `BinaryEncodable::encode` → `ExtensionObject::from_encoded(...)`——**验证 encode 输出构造 ExtensionObject 的 API**
    - 标量:现有 f64_to_variant 逻辑
- 变量节点创建:复杂类型节点的 `VariableBuilder::data_type(custom_type_node_id)`(而非标量 DataTypeId)

- [ ] **Step 1(关键验证): 结构体编码路径**

写一个一次性验证(或直接在实现中带最小单测):
1. `DataTypeTree::new()` + `add_type`(StructTypeInfo::from_field per field)
2. `DynamicStructure::new_struct(type_def, type_tree, vec![Variant::Double(1.0), ...])`
3. `ds.encode()` → bytes;查 `ExtensionObject` 构造:`ExtensionObject::from_encoded(node_id, bytes)`(以库 API 为准;若不存在用 `ExtensionObject { node_id, body: BinaryBody(bytes) }`——验证字段可见性)
4. `Variant::ExtensionObject(eo)` 可作为 VariableBuilder value

若此路径不可行 → **降级方案**:结构体字段平铺为子变量节点(`add_variable_node` 每字段一个),UI 按树展示;TypeId 用标量类型;spec 风险表已授权此降级。降级时在报告中明确说明,并跳过跨进程结构体解码 e2e 的结构体部分(数组/枚举仍测)。

- [ ] **Step 2: models.rs DataType 扩展 + serde**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
}

pub enum DataType {
    // 现有...
    Array { #[serde(rename = "elementType")] element_type: Box<DataType> },
    Array2D { #[serde(rename = "elementType")] element_type: Box<DataType>, dims: [u32; 2] },
    Enum { name: String, fields: Vec<(i64, String)> },
    Structure { name: String, fields: Vec<StructField> },
}
```

`type_id()` 扩展:Array→Array/Array2D→(返回结构体占位或具体;以使用点为准)、Enum→Int32(枚举底层值)、Structure→自定义注册 id(注册后查表)。**注意**:`type_id()` 现有返回 u32 标量 DataTypeId——复杂类型返回 ns=2 自定义节点 id 需返回 NodeId 而非 u32——**重构 type_id 为 `fn type_node_id(&self, custom: &HashMap<String, NodeId>) -> NodeId`**,或保持 type_id 供仿真用 + 新增 type_node_id 供注册用。以最小改动为准。

- [ ] **Step 3: 注册接线**

server.rs build_server/start:收集 ServerNode 的 data_type,去重复杂类型 → `register_custom_types`;复杂类型变量节点创建时用注册的 NodeId。

- [ ] **Step 4: 编译 + 提交**

```bash
cargo build --workspace
git commit -am "feat(core): complex data types (array/matrix/enum/structure) with server registration"
```

---

### Task 7: core — 复杂类型仿真生成 + 读写解析 + 显示

**Files:**
- Modify: `crates/opcuasim-core/src/server/address_space.rs`(f64_to_variant 扩展为 type_variant)
- Modify: `crates/opcuasim-core/src/browse.rs`(string_to_variant 扩展)
- Modify: `crates/opcuasim-core/src/server/simulation.rs`(生成路径)
- Modify: `crates/opcuaserver-egui/src/panels/*.rs`(显示/编辑)

**Interfaces:**
- Consumes: Task 6 类型系统
- Produces:
  - `type_variant(value: f64, dt: &DataType, type_tree/registry...) -> Variant`(Task 6 已定义,此处完成各分支)
  - `string_to_variant` 扩展:
    - 数组:`"1,2,3"` → 元素逐个按 element_type 解析 → `Variant::Array`
    - 二维:`"1,2;3,4"` → `Array::new_multi`
    - 枚举:名字(匹配 fields)或数值 → `Variant::Int32`
    - 结构体:`"x=1,y=2"` → 编码 ExtensionObject
  - 显示:`fn variant_to_display_string(v: &Variant) -> String`(数组 `[1, 2, 3]`、二维 `[1,2;3,4]`、枚举名、结构体字段)——**注意**:主站展示复杂值在 SP3,子站 UI 只做编辑框文本格式
  - 子站 egui:复杂类型节点的值编辑用文本输入(接受上述格式),仿真预览同

- [ ] **Step 1: 实现解析/显示 + 接线**

- write_node_value 的 data_type 参数现在是字符串("Double" 等)——复杂类型写入路径:`string_to_variant(value, data_type_str)` 需要类型上下文——**改动**:`write_node_value` 增加 `data_type: &DataType` 重载或新函数 `write_node_value_typed(session, node_id, value, dt)`;browse 层解析 DataType 字符串为 DataType(现有 `DataType::from_str` 或 UI 传完整类型)——以最小改动:UI 传 `DataType` 序列化串,服务端类型注册表反查
- **简化**:本任务先做服务端类型注册 + 仿真生成 + 客户端读(不强制子站 UI 编辑复杂类型——UI 编辑放 Task 8 顺带;若 UI 改动过大,标注 YAGNI 并在验收时以 e2e 写值覆盖)

- [ ] **Step 2: 编译 + 提交**

```bash
cargo build --workspace
git commit -am "feat(core): complex type simulation, parse and display support"
```

---

### Task 8: core + master-ui — 主站事件订阅 + 事件面板

**Files:**
- Modify: `crates/opcuasim-core/src/subscription.rs`(subscribe_to_events + EventLog)
- Create: `crates/opcuamaster-egui/src/panels/events_panel.rs`
- Modify: `crates/opcuamaster-egui/src/backend/dispatcher.rs`、`state.rs`、`events.rs`、`app.rs`/`mod.rs`

**Interfaces:**
- Consumes: 库 `EventFilter`/`SimpleAttributeOperand`/`MonitoringParameters`、`EventCallback`;Task 1 EventLog/EventItem
- Produces:
  - `SubscriptionManager::subscribe_to_events(&self, session: &Arc<Session>, source_id: &NodeId) -> Result<(), OpcUaSimError>`:
    - 构造 EventFilter:select_clauses = 标准字段(Time/Severity/SourceNode/SourceName/Message/EventId/EventType),where_clause 空
    - 复用现有 create_subscription 流程,`MonitoredItemCreateRequest { item_to_monitor: ReadValueId { node_id: source_id, attribute_id: EventNotifier }, monitoring_mode: Reporting, requested_parameters: MonitoringParameters { filter: EventFilter.into(), ... } }`
    - `EventCallback::new(move |event_fields, _item| { ... })`——回调里把字段对号入座存 EventLog(需解析 Variant 数组按 select 顺序)
  - 主站 backend:`BackendEvent::Events { conn_id, items: Vec<EventItem> }`;dispatcher 把 EventLog 增量推给 UI(或 UI 轮询)
  - `events_panel.rs`:`pub fn show(ui, state)`——表格(time/severity/source/message),顶部连接选择 + 订阅/取消按钮 + 清空

- [ ] **Step 1: 读库确认 EventCallback 回调签名与字段顺序**

`async-opcua-client-0.18.0/src/session/services/subscriptions/callbacks.rs:82`(`EventCallbackFun = dyn FnMut(Option<Vec<Variant>>, &MonitoredItem)`);确认 event_fields 与 select_clauses 顺序对应。

- [ ] **Step 2: 实现 core 订阅 + UI**

- 订阅生命周期:连接建立后 UI 触发订阅;断开清理(参考 pending_subscription 模式,SP1 前已有)
- egui 面板注册到主窗口 tab/面板列表(参考 history_tab 的接入方式)

- [ ] **Step 3: 编译 + 提交**

```bash
cargo build --workspace
git add crates/opcuasim-core/src/subscription.rs crates/opcuasim-core/src/events.rs crates/opcuamaster-egui/src/
git commit -m "feat(master): event subscription and events panel"
```

---

### Task 9: 集成测试 + 跨进程解码 + 全量验证合并

**Files:**
- Create: `crates/opcuasim-core/tests/server_events.rs`
- Create: `crates/opcuasim-core/tests/server_complex_types.rs`
- Modify: `crates/opcuamaster-egui/tests/e2e.rs`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: server_events.rs**

起 server(port 48440,含 sine 越限节点 eu_range 0..1 幅度 10)→ 连接 → 订阅 DemoEvents(EventFilter 全选)→ 断言:
1. 等 6s 收到 ≥1 条心跳(`Heartbeat` 前缀)
2. call RaiseEvent(severity=750, message="test-alarm") → 收到匹配事件
3. history_read_events 查最近 30s → 含 "test-alarm"
4. 越限告警:sine 幅度超出 eu_range → 收到 limit 消息(宽松断言:存在 severity 500 事件)
5. 连接状态:连接建立后 → 收到 Client connected(宽松)

- [ ] **Step 2: server_complex_types.rs**

起 server(port 48441,含数组/二维/枚举/结构体节点)→ 连接 →
1. 数组节点:read 值 → Variant::Array 元素数/类型正确
2. 二维:`Array::new_multi` dims 校验(读回 dimensions)
3. 枚举:read → Variant::Int32 且注册定义一致
4. 结构体(若主路径可用):客户端 `DataTypeTreeBuilder::build(&session)` + `add_type_loader` → read → 解码字段与写入一致(跨进程验证);若降级路径:验证字段子变量树存在且值一致
5. write 数组/枚举 → 读回一致

- [ ] **Step 3: master e2e 事件订阅**

`crates/opcuamaster-egui/tests/e2e.rs` 追加 `event_subscription` 测试:起子站 server → 主站 backend 订阅 → 调 RaiseEvent → 断言收到 BackendEvent::Events 含消息。

- [ ] **Step 4: CHANGELOG + 全量验证**

```markdown
### Added
- Server: events/alarms (threshold, method-triggered, heartbeat, connection-state) with event history read
- Server: complex data types (arrays, 2D arrays, enums, nested structures)
- Master: event subscription panel
```

```bash
cargo fmt && cargo test --workspace
git add CHANGELOG.md && git commit -m "docs: changelog for events and complex types"
```

- [ ] **Step 5: 最终 review + 合并(参照 SP1 流程)**

---

## 执行顺序

Task 1 → 2 → 3 → 4(依赖 2)→ 5(依赖 1,2)→ 6(独立,最大风险先行验证)→ 7(依赖 6)→ 8(依赖 1,2)→ 9(依赖全部)。Task 6 的 Step 1 关键验证在任务开头执行,若降级则影响 Task 7/9。

## 验收标准

- [ ] 4 种事件源产生标准 BaseEventType;客户端订阅 DemoEvents 收到(消息/severity 匹配)
- [ ] RaiseEvent 触发;心跳 5s;越限/恢复去重;连接变化
- [ ] history_read_events 返回事件历史(区间 + 字段选择 + CP 分页)
- [ ] 数组/二维/枚举/结构体(含嵌套)配置、注册、仿真、读写一致
- [ ] 主站"事件"面板展示事件流
- [ ] 跨进程 e2e:结构体解码与服务端一致(或降级路径字段树验证)
- [ ] `cargo test --workspace` 全部 PASS;fmt 干净;旧项目文件兼容
