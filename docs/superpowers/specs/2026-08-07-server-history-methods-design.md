# 子站历史存储 + 方法注册设计文档

日期:2026-08-07
范围:子项目 1——A1 历史数据存储 + A2 方法注册(子站规范补全)

## 概述

OPCUASim 子站(Server)当前对 HistoryRead 返回 `BadHistoryOperationUnsupported`(async-opcua 0.18 内置 `SimpleNodeManager` 所有 history 方法硬编码 Unsupported),方法注册仅存在于未接入生产的 `test_methods.rs`。本设计补齐这两块规范能力,让子站支持真正的历史数据读取与可调用方法。

用户已确认的 4 个决策:

| 决策点 | 选择 |
|--------|------|
| 历史存储深度 | 内存环形缓冲(每节点固定容量,服务重启清空) |
| 历史记录路径 | 仿真生成 + 客户端外部写入都记录 |
| 方法注册范围 | 预置演示方法集(4 个) |
| 缓冲容量配置 | 全局统一容量(ServerConfig 字段,UI 可改,重启生效) |

## 技术栈

| 层级 | 技术 |
|------|------|
| 后端 | Rust + Tokio 异步运行时 |
| OPC UA 库 | async-opcua 0.18(async-opcua-server / async-opcua-types / async-opcua-nodes) |
| GUI | egui 0.34(opcuaserver-egui 配置面板) |

**无新增依赖,无新 crate。** 全部修改落在 `opcuasim-core` 服务端 + `opcuaserver-egui` 配置。

## 关键库事实(已验证)

- `SimpleNodeManager = InMemoryNodeManager<SimpleNodeManagerImpl>`(`async-opcua-server-0.18.0/src/node_manager/memory/simple.rs:49`)
- `InMemoryNodeManagerImpl` 是 trait(`memory_mgr_impl.rs:46`),history 方法均有默认实现返回 `BadHistoryOperationUnsupported`(`memory_mgr_impl.rs:188-258`);`read_values`/`create_value_monitored_items`/`write`/`call` 等为 trait 方法(默认行为需实现阶段核对)
- `validate_history_read_nodes`(`memory/mod.rs:438`):读取历史前校验——非 Variable 节点返回 `BadHistoryOperationUnsupported`,缺少 `AccessLevel::HISTORY_READ` 返回 `BadUserAccessDenied`
- `VariableBuilder::history_readable()` 设置 `AccessLevel::HISTORY_READ`(async-opcua-nodes `variable.rs:85`)
- 客户端 `history_read_raw`(`crates/opcuasim-core/src/history.rs:23`)已有 ContinuationPoint 循环——服务端必须配合分页语义

## 功能范围

### A1 历史数据存储

- 自定义 `HistoryNodeManagerImpl`(实现 `InMemoryNodeManagerImpl` trait)替换 `simple_node_manager`
- `HistoryStore` 环形缓冲:每节点 `VecDeque<DataValue>`,容量 = `ServerConfig.history_buffer_size`(默认 10000)
- 两条记录路径:SimulationEngine 值更新循环 + 自定义 impl 的 `write()`(外部写入,委托成功后记录)
- 覆盖 `history_read_raw_modified`:按 `[start, end]` 过滤 + ContinuationPoint 分页
- 所有变量节点 `history_readable()`(AddressSpace 构建时)

### A2 方法注册

- `server/methods.rs` 预置 4 个方法:Echo、Add、RandomValue、SetNodeValue
- `OpcUaServer::start()` 内注册
- `test_methods.rs` 删除(被 methods.rs 取代)

## 项目结构(修改面)

```
crates/opcuasim-core/
├── src/
│   ├── server/
│   │   ├── mod.rs               # 修改:模块声明(history_store、methods、history_node_manager)
│   │   ├── history_store.rs     # 新建:环形缓冲 + 查询/分页
│   │   ├── history_node_manager.rs # 新建:自定义 InMemoryNodeManagerImpl
│   │   ├── methods.rs           # 新建:预置 4 个方法注册
│   │   ├── test_methods.rs      # 删除:被 methods.rs 取代
│   │   ├── models.rs            # 修改:ServerConfig 加 history_buffer_size
│   │   ├── address_space.rs     # 修改:add_variable_node 加 history_readable()
│   │   ├── simulation.rs        # 修改:值更新循环记录历史
│   │   └── server.rs            # 修改:用 HistoryNodeManagerImpl 替换 simple_node_manager;start() 注册方法
│   └── lib.rs                   # 修改:如暴露新模块
crates/opcuaserver-egui/
├── src/
│   ├── events.rs                # 修改:Config 事件流带 history_buffer_size(如结构字段变化)
│   ├── model.rs                 # 修改:设置表单加容量输入
│   └── panels/                  # 修改:设置面板加"历史缓冲容量"输入框
crates/opcuasim-core/tests/
├── server_history.rs            # 新建:历史记录 + 读取 e2e
└── server_methods.rs            # 新建:方法注册 + 调用 e2e
```

## 详细设计

### 1. HistoryStore(`history_store.rs`)

```rust
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use tokio::sync::RwLock;
use opcua_types::{ByteString, DataValue, DateTime, NodeId};

/// Per-node ring buffer of historical samples, oldest-first.
pub struct HistoryStore {
    buffers: RwLock<HashMap<NodeId, VecDeque<DataValue>>>,
    capacity: usize,
}

impl HistoryStore {
    pub fn new(capacity: usize) -> Self;

    /// Record a sample; drops oldest when at capacity.
    pub async fn record(&self, node_id: &NodeId, dv: DataValue);

    /// Query samples with source_timestamp in [start, end], oldest-first,
    /// skipping `skip` already-returned samples (continuation support).
    /// Returns (samples, next_skip) where next_skip is Some(skip+len) when
    /// more samples remain, None when exhausted.
    pub async fn query(
        &self,
        node_id: &NodeId,
        start: DateTime,
        end: DateTime,
        max_values: u32,
        skip: usize,
    ) -> (Vec<DataValue>, Option<usize>);

    /// Current sample count for a node.
    pub async fn len(&self, node_id: &NodeId) -> usize;
}
```

实现要点:

- `record`:写锁插入尾部;`len() >= capacity` 时 `pop_front()` 后 push
- `query`:读锁;过滤 `dv.source_timestamp` 在 `[start, end]`(缺失时间戳的样本:按 `server_timestamp` 兜底;两者皆无则包含——仿真与写入路径均带时间戳,边界仅防御);跳过前 `skip` 条;取 `max_values` 条;若过滤后剩余样本数 > `skip + max_values`,返回 `Some(skip + 实际返回数)`,否则 `None`
- 时间戳比较:DateTime 实现 Ord,可直接比较
- 容量 0 视为禁用(不记录)

### 2. HistoryNodeManagerImpl(`history_node_manager.rs`)

```rust
/// Custom in-memory node manager: default behavior for most services, plus
/// real history read backed by HistoryStore and write-through history recording.
pub struct HistoryNodeManagerImpl {
    history: Arc<HistoryStore>,
}
```

实现 `InMemoryNodeManagerImpl`:

- **`history_read_raw_modified`**(核心):对每个 `HistoryNode`:
  - 从 `HistoryNode` 取 node_id、`ReadRawModifiedDetails`(start_time/end_time/num_values_per_node)
  - `is_read_modified: false` 分支:从 `history.query(node, start, end, num_values_per_node, skip)` 取样本
  - skip 来源:请求的 `continuation_point` 解码(见下)
  - 将样本写入 `HistoryNode` 的 `HistoryData`(DataValues),设置 `continuation_point`(若非空余量)或 null
  - `is_read_modified: true`:返回 `BadHistoryOperationUnsupported`(本期只支持 raw;modified 不在范围,文档注明)
- **`write`**:委托默认行为(或等效实现);成功后 `history.record(node_id, 写入后的 DataValue)`——记录"外部写入"路径
- **其余方法**:使用 trait 默认实现。**实现阶段必须核对 `InMemoryNodeManagerImpl` 各方法的默认行为**(`read_values`/`create_value_monitored_items` 等默认实现是否为合理空实现或与 SimpleNodeManagerImpl 等价)。若默认实现为空导致功能缺失,参照 `memory_mgr_impl.rs` 中 `SimpleNodeManagerImpl` 的实现补齐必要方法(实现时以库源码为准)。

**ContinuationPoint 编码**(服务端↔客户端 `history.rs` 循环配合):

- CP = `ByteString` 编码的跳过的样本数,如 `format!("{skip}").into_bytes()`
- 解析失败 → `BadContinuationPointInvalid`

### 3. 记录路径挂钩

**SimulationEngine 值更新循环**(`simulation.rs:121-142`):

```rust
// 生成 value_strings 的同时(现循环内),对每个更新:
if let Some(store) = &history_store {
    store.record(&node_state.opcua_node_id, dv.clone()).await;
}
```

- `SimulationEngine` 增加 `history_store: Option<Arc<HistoryStore>>` 字段(构造后设置;None 时跳过记录,容量 0 场景)
- `OpcUaServer::start()` 中创建 `Arc<HistoryStore>` 并传入 sim_engine 与 node manager

**外部写入**(`history_node_manager.rs` 的 `write`):

```rust
async fn write(&self, context, nodes, ...) -> Result<(), StatusCode> {
    // 1. 执行默认/委托写入
    // 2. 对成功写入的节点: history.record(node_id, 新值 DataValue with now timestamps)
    // 失败节点不记录
}
```

**注意**:`set_values`(SimulationEngine 批量写)会走 AddressSpace 而非 node manager 的 `write`——两条路径独立记录会导致**重复记录**(SimulationEngine 手动 record + set_values 不触发 write 回调)。设计上明确:**SimulationEngine 路径自行 record;node manager write 路径仅覆盖"客户端外部写入"**。需确认 `nm.set_values` 不触发 `write()` trait 方法(库实现为直接写 AddressSpace,不经过 impl write)——实现阶段验证,若触发则 SimulationEngine 改为不手动 record 而依赖 write 钩子(二选一,避免双写)。

### 4. 历史读取服务端流程(与客户端循环的配合)

```
客户端 history_read_raw 循环:
  history_read(details, Both, release=false, nodes=[{node, cp}])
    → 服务端: validate → history_read_raw_modified
       → HistoryStore.query(...) → HistoryNode.history_data = HistoryData{data_values}
       → continuation_point = encode(skip_next) 或 null
  客户端收到 CP 非 null → 下一轮带 cp 继续
  取尽(max_values 达)或 CP null → 结束;提前退出时 release=true 释放(客户端已实现)
```

### 5. 方法注册(`methods.rs`)

```rust
/// Preset demo methods registered at server startup.
pub async fn register_demo_methods(server: &OpcUaServer) -> Result<Vec<NodeId>, OpcUaSimError> {
    // Echo / Add / RandomValue / SetNodeValue
}
```

- 复用 `test_methods.rs` 已验证模式:MethodBuilder + input_args/output_args + `nm.inner().add_method_callback`
- 辅助函数 `register_method(nm, ns, node_id, name, in_args, out_args, callback)`
- **SetNodeValue 回调**:捕获 `node_manager`(Arc<SimpleNodeManager> 或新 impl 的 Arc),解析 node_id 后 `nm.set_value(...)`;返回写入状态字符串
- 注册点:`OpcUaServer::start()` 内,build 完成 + simulation 启动后调用(约 `server.rs:228` 之后)
- 方法 NodeId(沿用 ns=2 命名空间):

| 方法 | NodeId | 入参 | 出参 | 逻辑 |
|------|--------|------|------|------|
| Echo | `ns=2;s=Demo.Echo` | `input: String` | `output: String` | 原样返回 |
| Add | `ns=2;s=Demo.Add` | `a: Double, b: Double` | `sum: Double` | 求和 |
| RandomValue | `ns=2;s=Demo.RandomValue` | `max: Double` | `value: Double` | `[0, max)` 随机,缺省 100(入参为 Optional 语义:传 0 表示默认) |
| SetNodeValue | `ns=2;s=Demo.SetNodeValue` | `node_id: String, value: Double` | `status: String` | 解析 node_id → 写值 → 返回 "Good"/错误描述 |

- `RandomValue` 入参 `max: Double` 的"缺省"语义:主站方法面板必填入参,客户端调 `RandomValue(0)` 视为默认 100——文档注明
- `test_methods.rs` 删除,`server/mod.rs` 更新模块声明

### 6. 配置与 UI

- `models.rs` `ServerConfig` 加字段:

```rust
/// Per-node history ring buffer capacity. 0 disables history.
#[serde(default = "default_history_buffer_size")]
pub history_buffer_size: usize,
// fn default_history_buffer_size() -> usize { 10_000 }
```

- 旧项目文件缺字段 → serde 默认 10000(兼容)
- `opcuaserver-egui`:设置面板加"历史缓冲容量(条/节点)"`DragValue`(0 = 禁用),经现有 Config 事件流下发;重启生效(启动时从 config 读)

### 7. 错误处理与边界

| 场景 | 行为 |
|------|------|
| 环形缓冲满 | 丢弃最旧样本,debug 日志 |
| 查询空区间 | 空 HistoryData + Good |
| 非 Variable / 无 HIST_READ | 库 validate 自动返回 BadUserAccessDenied |
| 外部写失败 | 不记录(仅成功节点) |
| CP 解码失败 | BadContinuationPointInvalid |
| `is_read_modified: true` | BadHistoryOperationUnsupported(本期不支持 modified,文档注明) |
| history_buffer_size = 0 | 不记录;history_read 返回空数据(不报错) |
| RandomValue(0) | 按默认 100 处理 |

## 测试策略

### 单元测试(HistoryStore,无网络)

- 环形淘汰:容量 3 插 5 条,只剩最新 3 条
- 时间区间过滤:start/end 边界包含性
- 分页:max_values=2 查询 5 条,`(2, Some(2))` → `(2, Some(4))` → `(1, None)` 三页取尽
- 容量 0 禁用:record 不生效,query 返回空

### 集成测试(`crates/opcuasim-core/tests/`)

- **`server_history.rs`(新建)**:
  - 起真实 server(sine 节点 200ms 间隔,history_buffer_size 默认)
  - 连接 → 等 2s → `history_read_raw(node, now-10s, now, 100, false)` → 断言 ≥1 条、时间戳单调递增
  - 客户端写值(Setpoint 可写节点)→ 再读历史 → 断言外部写入值出现在历史中
  - CP 分页:max_values=2 → 断言三页取尽(复用 history_read_raw 的 CP 循环)
- **`server_methods.rs`(新建)**:
  - 起 server → 连接 → browse/直接构造 4 个方法 NodeId → 逐个 `call_method`
  - 断言 Echo 原样返回、Add 求和、RandomValue ∈ [0,100)、SetNodeValue 写值后 `session.read` 读回一致
- 全量 `cargo test --workspace` 保持全绿(现有 20 项 + 新增)

## 不在范围内(YAGNI)

- 历史落盘持久化(重启保留)——用户选内存缓冲
- 按节点独立容量——用户选全局统一
- 聚合历史(ReadProcessedDetails/Aggregates)——子项目 3(B2)
- 事件/告警历史(HistoryReadEvent)——子项目 2(A3)
- 方法自定义管理器(UI 增删改方法)——用户选预置集
- 服务端历史浏览 UI——子项目 3 主站侧
- `is_read_modified` 历史(HistoryModifiedData)——本期只 raw
- 历史变化订阅通知(Part 11 subscription 扩展)——超范围

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| `InMemoryNodeManagerImpl` 默认方法实现为空导致读/订阅功能缺失 | 实现前读库源码 `memory_mgr_impl.rs` 核对各默认实现;若关键方法为空,参照 SimpleNodeManagerImpl 补齐(实现阶段以库源码为准) |
| `set_values` 是否触发 impl `write()` 导致双写 | 实现阶段验证;若触发,SimulationEngine 不手动 record,统一走 write 钩子(二选一) |
| 服务端 HistoryNode 数据结构/分页 API 形态与预期不符 | 实现前读库源码 `memory/mod.rs` 的 HistoryNode 结构与 `validate_history_read_nodes` 调用路径 |
| 历史样本时间戳缺失 | 过滤用 source_timestamp 兜底 server_timestamp,仿真/写入路径均带时间戳 |
| 环形缓冲内存占用(大容量 × 多节点) | 默认 10000 条/节点 × Double 约 100KB/节点级,可接受;容量可配置归零禁用 |
