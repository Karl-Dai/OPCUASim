# 子站事件/告警 + 复杂数据类型(A3/A4 + B3 提前)设计规格

> 日期:2026-08-10 · 状态:已批准 · 子项目:SP2(承接 SP1:历史存储 A1 + 方法注册 A2,已合并 9da1b3f)

## 1. 背景与问题

OPCUASim 子站(Server)已具备:节点仿真(Static/Random/Sine/Linear/Script)、历史存储(A1)、预置方法(A2)。当前缺口:

1. **无事件/告警能力**:子站不能产生 OPC UA 事件(BaseEventType),客户端(主站)无法订阅事件——仿真系统的异常状态(越限、心跳、连接变化)没有表达途径。
2. **无复杂数据类型**:`DataType` 枚举仅标量(Boolean~ByteString),无法表达数组、结构体、枚举——仿真场景(如传感器读数结构体、多维坐标)无法建模。
3. **主站无事件订阅 UI**:主站只能值采集(DataChange/Polling),无法查看子站事件流。
4. **事件历史缺失**:服务端 `history_read_events` 返回 Unsupported,事件无法回溯。

## 2. 需求

### 2.1 功能需求

**A3 事件/告警(子站服务端)**

- F1: 子站产生 4 种事件,事件类型为标准 `BaseEventType`(async-opcua-nodes):
  1. **阈值告警**:仿真引擎检测节点值超出 `eu_range_low/high` 时发告警(severity 500,消息含节点与限制值);恢复时发"back to normal"(severity 100)。告警去重:仅在状态翻转时发(每个节点维护 alarm_active 状态)。
  2. **方法触发**:预置方法 `Demo.RaiseEvent(severity: UInt16, message: String)`,客户端调用即发事件。
  3. **周期心跳**:服务端后台任务每 5s 发 `Heartbeat {seq}`(severity 100),source = DemoEvents 对象。
  4. **连接状态**:后台任务每 1s 轮询地址空间中 `ServerDiagnosticsSummary.CurrentSessionCount` 变量,变化时发 `Client connected ({n} sessions)`(severity 200)/ `Client disconnected`(severity 300)。
- F2: 事件源对象 `DemoEvents`(ns=2;s=DemoEvents),`EventNotifier::SUBSCRIBE_TO_EVENTS | HISTORY_READ`。
- F3: 事件推送:`SubscriptionCache::notify_events(&dyn Event, &NodeId)`(经 `ServerHandle::subscriptions()`);无订阅者时 no-op,不报错。
- F4: **事件历史存储**:事件同时写入 `EventStore`(环形缓冲,容量 `event_history_size`,默认 1000,0 禁用);`HistoryNodeManagerImpl::history_read_events` 覆盖实现,从 EventStore 查询 `[start, end]` 区间,支持 `ReadEventDetails.filter` 的 select_clauses(where_clause 简化:支持 `EventFilterOperator::Equals` 单条件,其余视为全选),CP 分页(skip 语义同 A1)。
- F5: `DemoEvents` 对象配置 `HISTORY_READ` 位(validate_history_read_nodes 的 is_for_events 路径要求,已验证库源码)。

**A4 复杂数据类型(子站服务端 + 子站 UI 读写)**

- F6: `DataType` 枚举扩展:
  ```rust
  Array { element_type: Box<DataType> },                    // 一维数组
  Array2D { element_type: Box<DataType>, dims: [u32; 2] },  // 二维数组(多维简化)
  Enum { name: String, fields: Vec<(i64, String)> },        // 枚举:Int32 值 + 定义
  Structure { name: String, fields: Vec<StructField> },     // 结构体(字段可递归嵌套)
  ```
  `StructField { name: String, data_type: DataType }`。
- F7: 服务端启动时注册自定义 DataType 节点(`DataTypeBuilder` + `StructureDefinition`/`EnumDefinition` + encoding 对象,ns=2),变量节点 `data_type` 指向自定义类型。
- F8: 复杂类型值:
  - 数组:`Variant::Array`(元素同类型标量);二维:`Array { dimensions: Some(vec![d1,d2]) }`(`Array::new_multi`)
  - 枚举:`Variant::Int32`(数值)+ DataType 节点定义
  - 结构体:`Variant::ExtensionObject`(经 `custom::DynamicStructure` 动态编码;实现阶段验证服务端构造路径,若不可行退化为字段子变量节点平铺)
- F9: 仿真生成:复杂类型节点用 `SimulationMode` 现有模式驱动,`f64_to_variant` 扩展为按类型生成(数组/结构体字段逐个转换)。
- F10: 读写解析:`string_to_variant` 扩展——数组 `"1,2,3"`、二维 `"1,2;3,4"`、枚举按名字或数值、结构体 `"x=1,y=2"`;显示格式化对称。

**B3 主站事件订阅 UI(提前纳入)**

- F11: core 客户端:扩展 `SubscriptionManager::subscribe_to_events(session, event_source_id, filter)`——创建事件订阅(EventFilter 全选标准字段)、`EventCallback` 收 `event_fields`、存入 `EventLog`(环形,容量 500)。
- F12: 主站 egui 新面板"事件"(events_panel.rs):表格显示 time / severity / source / message,按连接过滤;backend 经 `BackendEvent::Events` 上报。

**跨进程结构体解码验证**

- F13: e2e 用客户端 `DataTypeTreeBuilder` + `session.add_type_loader(DynamicTypeLoader)` 构建类型树,读取结构体节点,断言解码字段与服务端写入一致。

### 2.2 非功能需求

- N1: 无新 crate(仅扩展 workspace 内现有 async-opcua 0.18 依赖使用)。
- N2: `cargo fmt --check` 干净;`cargo test --workspace` 全部 PASS。
- N3: `ServerConfig` 新字段 `event_history_size`(serde default 1000),旧 `.opcuaproj` 兼容。
- N4: 事件推送/历史查询失败不 panic;无订阅者推送 no-op。
- N5: 连接状态轮询读不到变量时跳过该轮,不崩溃。

## 3. 范围

### 3.1 纳入(已批准)

A3 全部(4 种事件源 + 事件历史 + RaiseEvent)+ A4 全部(数组/多维/枚举/结构体/嵌套 + 注册 + 仿真/读写)+ B3 主站事件订阅 UI + 跨进程结构体解码 e2e + `event_history_size` 独立配置。

### 3.2 明确不做

- where_clause 完整表达式引擎(仅 Equals 单条件)
- 事件订阅 UI 之外的主站复杂类型展示(结构体字段树面板,后续 SP3)
- 多维数组 3 维+(`Array2D` 覆盖 2 维,更高维后续)
- 服务端 `history_update`(事件历史只读)
- Union 类型(StructureType::Union 后续;本轮 Structure + Enum)

## 4. 技术方案

### 4.1 模块结构

| 文件 | 责任 | 操作 |
|---|---|---|
| `crates/opcuasim-core/src/server/event_store.rs` | EventStore 环形缓冲 + 查询 | 新建 |
| `crates/opcuasim-core/src/server/events.rs` | DemoEvents 对象、notify、RaiseEvent、心跳/连接/告警任务 | 新建 |
| `crates/opcuasim-core/src/server/history_node_manager.rs` | 覆盖 history_read_events | 修改 |
| `crates/opcuasim-core/src/server/models.rs` | DataType 复杂变体、ServerConfig.event_history_size | 修改 |
| `crates/opcuasim-core/src/server/address_space.rs` | 复杂类型注册、f64_to_variant 扩展 | 修改 |
| `crates/opcuasim-core/src/server/simulation.rs` | 告警检测、复杂类型生成 | 修改 |
| `crates/opcuasim-core/src/server/methods.rs` | RaiseEvent 方法 | 修改 |
| `crates/opcuasim-core/src/server/server.rs` | 任务 spawn、类型注册接线 | 修改 |
| `crates/opcuasim-core/src/browse.rs` | string_to_variant 复杂类型解析 | 修改 |
| `crates/opcuasim-core/src/subscription.rs` | 事件订阅 + EventLog | 修改 |
| `crates/opcuasim-core/src/events.rs` | EventItem/EventLog 数据结构 | 新建 |
| `crates/opcuasim-core/src/history.rs` | history_read_events 客户端包装 | 修改 |
| `crates/opcuaserver-egui/src/panels/*.rs` | 复杂类型值显示/编辑 | 修改 |
| `crates/opcuamaster-egui/src/panels/events_panel.rs` | 主站事件面板 | 新建 |
| `crates/opcuamaster-egui/src/backend/*.rs` | Events 事件流 | 修改 |
| `crates/opcuasim-core/tests/server_events.rs` | 事件 e2e | 新建 |
| `crates/opcuasim-core/tests/server_complex_types.rs` | 复杂类型 e2e + 跨进程解码 | 新建 |
| `crates/opcuamaster-egui/tests/e2e.rs` | 主站事件订阅 e2e | 修改 |

### 4.2 关键技术点

1. **notify_events**:`handle.subscriptions().notify_events([(&event, &source_node_id)])`——`ServerHandle` 经 `OpcUaServer` 内部字段获取。
2. **事件历史**:`HistoryNode::set_result(HistoryEvent { events: Some(vec![HistoryEventFieldList { event_fields }]) })`;CP 编码复用 A1 的 `Box<dyn Any>` skip 方案(事件序列按时间排序,skip 计数)。
3. **复杂类型注册**:`DataTypeBuilder::new(&id, name, name).data_type_definition(StructureDefinition { ... })` + encoding 对象;Variant::ExtensionObject 编码验证 `DynamicStructure` 服务端可用性——**最大风险点**,实现 Task 1 时验证。
4. **EventFilter 客户端构造**:`EventFilter { select_clauses: Some(vec![SimpleAttributeOperand { browse_path: 字段路径 }]), where_clause: ContentFilter::default() }`。
5. **连接状态轮询**:读地址空间 `ServerDiagnosticsSummary.CurrentSessionCount`(VariableId::Server_ServerDiagnostics_ServerDiagnosticsSummary_CurrentSessionCount),与上次值比较。

## 5. 风险与缓解

| 风险 | 等级 | 缓解 |
|---|---|---|
| 结构体 ExtensionObject 动态编码服务端不可用 | 高 | 实现 Task 1 先验证 `DynamicStructure` 构造;不可行则退化为字段子变量平铺(每个字段一个子变量节点),UI 按树展示 |
| 连接状态轮询读不到计数变量 | 低 | 跳过该轮,测试用宽松断言 |
| where_clause 简化 | 低 | 仅 Equals 单条件,测试覆盖 |
| EventFilter 字段选择与 notify 字段不匹配 | 中 | select_clauses 用标准 BaseEventType 字段路径(Time/Severity/SourceNode/SourceName/Message/EventId/EventType),e2e 断言 |

## 6. 验收标准

- [ ] 子站 4 种事件源产生标准 BaseEventType;客户端订阅 DemoEvents 收到(含消息/severity)
- [ ] RaiseEvent 方法可触发事件;心跳每 5s;越限/恢复各发一次;连接变化发事件
- [ ] `history_read_events` 返回事件历史(时间区间 + 字段选择 + CP 分页)
- [ ] 数组/二维数组/枚举/结构体(含嵌套)节点可配置、仿真生成、读写一致
- [ ] 主站"事件"面板展示事件流(时间/severity/source/message)
- [ ] 跨进程 e2e:客户端 DynamicTypeLoader 解码结构体字段与服务端一致
- [ ] `cargo test --workspace` 全部 PASS;fmt 干净;旧项目文件兼容
