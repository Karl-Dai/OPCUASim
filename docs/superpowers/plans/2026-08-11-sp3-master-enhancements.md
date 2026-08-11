# 主站增强实施计划(SP3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现聚合历史(B2:服务端 ReadProcessedDetails 计算 + 主站聚合 UI)、主站历史增强(B1:历史标签页 Raw/聚合/事件三模式)、where_clause 完整表达式(B3 残留:事件历史过滤求值器)、主站复杂类型展示(B4:结构体字段树)、CI 管线(C1)。

**Architecture:** 服务端 `aggregate.rs` 按 `processing_interval` 分桶计算聚合函数;`filter.rs` 递归求值 ContentFilter;`HistoryNodeManagerImpl::history_read_processed` 覆盖库 Unsupported 默认;客户端 `history_read_processed` 复用 CP 分页骨架;`variant_to_tree` 纯函数 + 主站 CollapsingHeader 树;`.github/workflows/ci.yml` 质量门。无新 crate。

**Tech Stack:** Rust + Tokio,async-opcua 0.18,egui 0.34。已确认库事实:`ReadProcessedDetails { start_time, end_time, processing_interval, aggregate_type: Option<Vec<NodeId>>, aggregate_configuration }` 存在(read_processed_details.rs:15);服务端 InMemory `history_read_processed` 返回 `BadHistoryOperationUnsupported`(memory_mgr_impl.rs:209)→ 必须自研覆盖;聚合函数 NodeId 常量 `AggregateFunction_Average=2342/TimeAverage=2343/Total=2344/Minimum=2346/Maximum=2347/Range=2350/Count=2352/Delta=2359/PercentGood=2362`(node_ids.rs:7352+);客户端 `HistoryReadAction::ReadProcessedDetails` 已存在(attributes.rs:29),响应 `into_inner_as::<HistoryData>()` 与 raw 一致;`FilterOperator` 18 算符枚举(enums.rs:443),库无现成求值器;`ContentFilterElement { filter_operator, filter_operands: Option<Vec<ExtensionObject>> }`;SP2 现过滤仅单 Equals(history_node_manager.rs:353),`field_names` 常量在 history_node_manager.rs:325。

## Global Constraints

- 无新增 crate(workspace 现有 async-opcua 0.18 / egui 0.34 足够)
- `cargo fmt` + `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` 必须通过
- 提交前缀 `feat:`/`test:`/`docs:`/`ci:`,英文消息;每任务单独提交
- 现有 release.yml 不动(CI 是新增独立文件)
- 不新增 ServerConfig 字段(如有必须 serde default)
- 聚合/过滤/树构建失败不 panic;不支持的聚合返回标准 StatusCode

---

## File Structure

| 文件 | 责任 | 操作 |
|---|---|---|
| `crates/opcuasim-core/src/server/aggregate.rs` | 聚合函数计算(按桶) | 新建 |
| `crates/opcuasim-core/src/server/filter.rs` | ContentFilter 求值器 | 新建 |
| `crates/opcuasim-core/src/values.rs` | `variant_to_tree` 纯函数 | 新建 |
| `crates/opcuasim-core/src/server/history_store.rs` | `query_samples` 全量区间查询 | 修改 |
| `crates/opcuasim-core/src/server/history_node_manager.rs` | 覆盖 `history_read_processed`;field_names 抽共享 | 修改 |
| `crates/opcuasim-core/src/server/events_history_node_manager.rs` | where_clause 走求值器 | 修改 |
| `crates/opcuasim-core/src/server/mod.rs` | 模块导出 | 修改 |
| `crates/opcuasim-core/src/lib.rs` | values 模块导出 | 修改 |
| `crates/opcuasim-core/src/history.rs` | `history_read_processed` 封装 | 修改 |
| `crates/opcuasim-core/tests/server_aggregates.rs` | 聚合 e2e | 新建 |
| `crates/opcuasim-core/tests/content_filter.rs` | 求值器 e2e | 新建 |
| `crates/opcuasim-core/tests/variant_tree.rs` | 字段树测试 | 新建 |
| `crates/opcuamaster-egui/src/model.rs` | HistoryTabState 模式/聚合字段 | 修改 |
| `crates/opcuamaster-egui/src/panels/history_tab.rs` | 三模式 UI | 修改 |
| `crates/opcuamaster-egui/src/panels/value_panel.rs` | 字段树挂载 | 修改 |
| `crates/opcuamaster-egui/src/panels/data_table.rs` | 复杂值展开 | 修改 |
| `crates/opcuamaster-egui/src/backend/dispatcher.rs` | ReadHistory 分发 | 修改 |
| `.github/workflows/ci.yml` | CI 质量门 | 新建 |

---

### Task 1: core — aggregate.rs + history_store.query_samples + 服务端 history_read_processed

**Files:**
- Create: `crates/opcuasim-core/src/server/aggregate.rs`
- Modify: `crates/opcuasim-core/src/server/history_store.rs`
- Modify: `crates/opcuasim-core/src/server/history_node_manager.rs`
- Modify: `crates/opcuasim-core/src/server/mod.rs`

**Interfaces:**
- Consumes: `opcua_types::{DataValue, DateTime, NodeId, Duration}`(注意 Duration 是 i64 微秒)
- Produces:
  - `aggregate.rs`: `pub fn aggregate_samples(samples: &[DataValue], start: DateTime, end: DateTime, processing_interval: Duration, agg_type: &NodeId) -> Result<Vec<DataValue>, StatusCode>`(内部按桶聚合,桶起始时间为 source_timestamp);`pub fn aggregate_supported(agg_type: &NodeId) -> bool`;`pub fn aggregate_bucket(values: &[ValueLike], agg_type: &NodeId) -> Variant`
  - `history_store.rs`: `pub async fn query_samples(&self, node_id: &NodeId, start: DateTime, end: DateTime) -> Vec<DataValue>`(全量区间,无分页)
  - `history_node_manager.rs`: `async fn history_read_processed` 覆盖:解析 details(processing_interval、aggregate_type) → 每节点 query_samples → aggregate_samples → `HistoryData { data_values: Some(buckets) }` 填 `node.set_result`;CP 分页 skip 语义同 raw

- [ ] **Step 1: 写失败测试(aggregate 单测 + history_store.query_samples 单测)**
  - `aggregate.rs` 内 `#[cfg(test)]`:用确定性样本(固定时间戳 + 已知值)断言 Average/Maximum/Minimum/Count/TimeAverage/Total/Delta/PercentGood;空桶;不支持聚合返回 BadAggregateNotSupported
  - `history_store.rs` 追加测试:query_samples 返回全量区间样本(不分页)
- [ ] **Step 2: 实现 aggregate.rs**(aggregate_samples + 桶分切 + 8 聚合函数;`variant_to_f64` 复用或将解析抽公共)
- [ ] **Step 3: history_store.rs 加 query_samples**
- [ ] **Step 4: history_node_manager.rs 覆盖 history_read_processed**(mock details 直测;CP skip 分页)
- [ ] **Step 5: `cargo test -p opcuasim-core` PASS + `cargo fmt`**

### Task 2: core — filter.rs 求值器 + events_history_node_manager 接入

**Files:**
- Create: `crates/opcuasim-core/src/server/filter.rs`
- Modify: `crates/opcuasim-core/src/server/events_history_node_manager.rs`
- Modify: `crates/opcuasim-core/src/server/mod.rs`
- Modify(共享常量): `crates/opcuasim-core/src/server/history_node_manager.rs`

**Interfaces:**
- Produces:
  - `filter.rs`: `pub fn eval_clauses(elements: &[ContentFilterElement], fields: &[Variant]) -> Result<bool, StatusCode>`(顶层 0 或 1 元素;多元素按 And 组合,与 OPC UA 语义一致:元素间隐式 And);`fn eval_element(el, fields) -> Result<bool, StatusCode>`;`fn compare(a: &Variant, b: &Variant, op: FilterOperator) -> Result<bool, StatusCode>`;`fn like_match(s: &str, pat: &str) -> bool`
  - `history_node_manager.rs`(若 events manager 未复用):抽 `pub(crate) const EVENT_FIELD_NAMES: &[&str]` 供两处共享
  - `events_history_node_manager.rs`:删 filter_eq 单 Equals 块 → `match eval_clauses(&details.filter.where_clause.elements.unwrap_or_default(), &fields)`;错误 → `BadFilterOperatorInvalid`

- [ ] **Step 1: 写失败测试(filter.rs 单测)**:Equals/Not/And/Or/GT/LT/GTE/LTE/Between/InList/Like(%/_),数值跨类型(Int32 vs Double)、字符串、布尔;多元素隐式 And;错误算符 → BadFilterOperatorInvalid
- [ ] **Step 2: 实现 filter.rs**(操作数解析 SimpleAttributeOperand/LiteralOperand;比较统一 f64/字符串/布尔;递归 And/Or/Not;Between 三操作数;InList 变长;Like 转正则)
- [ ] **Step 3: events_history_node_manager.rs 替换过滤逻辑**(保留 select_clauses 现状)
- [ ] **Step 4: `cargo test -p opcuasim-core` PASS + `cargo fmt`**

### Task 3: core — history_read_processed 客户端封装 + e2e 测试

**Files:**
- Modify: `crates/opcuasim-core/src/history.rs`
- Create: `crates/opcuasim-core/tests/server_aggregates.rs`(e2e)

**Interfaces:**
- Produces:
  - `history.rs`: `pub async fn history_read_processed(session: &Arc<Session>, node_id: &NodeId, start: DateTime, end: DateTime, processing_interval_ms: u64, agg_type: NodeId, max_values: u32) -> Result<Vec<HistoryDataPoint>, OpcUaSimError>`(CP 分页骨架复用 raw;`ReadProcessedDetails` 构造;`AggregateConfiguration { use_server_capabilities_defaults: true, treat_uncertain_as_bad: false, percent_data_bad: 0, percent_data_good: 100, use_sloped_extrapolation: false }`;结果 `into_inner_as::<HistoryData>()`)
  - `server_aggregates.rs`(e2e):起真实 server(固定 sine 节点 200ms 间隔)→ 等 ~3s → `history_read_processed(Average/Maximum, 2000ms)` → 断言桶数 ≥1、每桶值为数值且在样本范围内;未实现聚合(用非法 NodeId)→ BadAggregateNotSupported 或错误返回

- [ ] **Step 1: 写失败 e2e(server_aggregates.rs)**:目标断言如上
- [ ] **Step 2: 实现 history.rs 客户端封装**
- [ ] **Step 3: 跑通 e2e;`cargo test --workspace` 全绿 + `cargo fmt`**

### Task 4: core — variant_to_tree + 测试

**Files:**
- Create: `crates/opcuasim-core/src/values.rs`
- Modify: `crates/opcuasim-core/src/lib.rs`
- Create: `crates/opcuasim-core/tests/variant_tree.rs`

**Interfaces:**
- Produces:
  - `values.rs`: `pub struct TreeNode { pub name: String, pub value: String, pub children: Vec<TreeNode> }`;`pub fn variant_to_tree(name: &str, v: &Variant) -> Vec<TreeNode>`(递归:数组→每元素子节点;dimensions 二维→按行分组;"多维"仍一维展开+注记);`ExtensionObject` → `into_inner_as::<DynamicStructure>` 成功则逐字段,失败 hex fallback(`format!("{v:x}")` 不适用时 `format!("{v}")` + 类型注解);枚举无位(客户端侧无法区分,值即 Int32)

- [ ] **Step 1: 写失败测试(variant_tree.rs)**:数组/二维数组/标量/ExtensionObject(hex fallback 路径至少)/嵌套组合的属性断言(节点名、值文本、子节点数)
- [ ] **Step 2: 实现 values.rs**
- [ ] **Step 3: `cargo test -p opcuasim-core` PASS + `cargo fmt`**

### Task 5: master-egui — 历史标签页三模式 + 聚合/事件 UI

**Files:**
- Modify: `crates/opcuamaster-egui/src/model.rs`(HistoryTabState: mode 枚举 Raw/Processed/Events、agg_type 选择(显示名→NodeId)、processing_interval_ms、聚合结果复用 points、事件结果 events_points)
- Modify: `crates/opcuamaster-egui/src/panels/history_tab.rs`(模式切换 SegmentedControl;Processed 显示聚合下拉+间隔;Events 显示事件表格;plot 仅 Raw/Processed)
- Modify: `crates/opcuamaster-egui/src/backend/dispatcher.rs`(UiCommand::ReadHistory 加 mode/agg 字段;do_read_history 分发 history_read_raw / history_read_processed / history_read_events)
- Modify: `crates/opcuamaster-egui/src/events.rs`(UiCommand 定义)

**Interfaces:**
- 复用:`history_read_processed`(T3)、`history_read_events`(SP2 已有)、`HistoryDataPoint`、`EventHistoryPoint`

- [ ] **Step 1: model.rs 扩展 HistoryTabState**(mode + agg 状态 + events_points;serde 不涉及——运行时状态)
- [ ] **Step 2: events.rs UiCommand::ReadHistory 加字段**
- [ ] **Step 3: dispatcher.rs 分发三模式**(do_read_history 加超参数)
- [ ] **Step 4: history_tab.rs 三模式 UI**
- [ ] **Step 5: `cargo build` PASS + `cargo fmt`**

### Task 6: master-egui — 结构体字段树挂载

**Files:**
- Modify: `crates/opcuamaster-egui/src/panels/value_panel.rs`(详情区:复杂值 → `variant_to_tree` 递归 CollapsingHeader 树)
- Modify: `crates/opcuamaster-egui/src/panels/data_table.rs`(复杂值单元格 → 显示 "📂" + 点击 popup/内联树)
- 复用:`values::variant_to_tree`(T4)、core 侧现有显示字符串

- [ ] **Step 1: 写失败/参考测试(值面板树渲染输入构造在 variant_tree.rs 已覆盖;此处仅 UI)}
- [ ] **Step 2: value_panel.rs 挂载树**(CollapsingHeader + Label 递归;添加 show_variant_tree 辅助,直接可测性差,逻辑已在 core)
- [ ] **Step 3: data_table.rs 复杂值单元格响应**(点击展开 popup 树)
- [ ] **Step 4: `cargo build` PASS + `cargo fmt`**

### Task 7: CI — .github/workflows/ci.yml

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- 触发:push master + PR
- 步骤:checkout → stable toolchain → `cargo fmt --check` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace`
- 单 job(ubuntu-latest),无缓存(小项目)

- [ ] **Step 1: 本地跑三命令确认全绿**(fmt --check 可能需先修存量格式;clippy 存量问题仅修本分支触碰文件)
- [ ] **Step 2: 写 ci.yml**
- [ ] **Step 3: 语法检查(yaml 解析;在不 push 情况下尽量验证)**

---

## 验证与收尾

- [ ] `cargo fmt --check` 干净
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 干净
- [ ] `cargo test --workspace` 全部 PASS(含新增 e2e)
- [ ] 全部任务已提交(每任务独立 commit,conventional 前缀)
- [ ] whole-branch review → 修复 → 合并 master → 删除分支