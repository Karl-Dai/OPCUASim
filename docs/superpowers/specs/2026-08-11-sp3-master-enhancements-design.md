# 主站增强:聚合历史 + 结构体字段树 + where_clause 表达式 + CI(SP3)设计规格

> 日期:2026-08-11 · 状态:草稿 · 子项目:SP3(承接 SP2:事件/告警 + 复杂数据类型 + 主站事件订阅,已合并 8fe931b)

## 1. 背景与问题

OPCUASim 已完成:SP1 子站历史存储(A1)+ 方法注册(A2);SP2 子站事件/告警(A3)+ 复杂数据类型(A4)+ 主站事件订阅 UI(B3)。当前缺口:

1. **无聚合历史**:`history_read_processed`(ReadProcessedDetails/Aggregates)服务端返回 `BadHistoryOperationUnsupported`——历史数据只能看原始样本,无法按时间间隔聚合成平均/最大/最小/计数等统计值(如"过去 1 小时每分钟的平均值")。
2. **where_clause 仅单条件**:事件历史过滤只支持 `FilterOperator::Equals` 单条件(SP2 简化),无法表达 `Severity >= 400 AND Message LIKE '%alarm%'` 等复合表达式。
3. **主站复杂类型单文本**:子站 SP2 已支持数组/多维数组/枚举/结构体(含嵌套),但主站收到复杂值后仅显示扁平字符串(如 `{x: 1.0, y: 2.0}`),无法按结构体字段树/多维数组网格浏览。
4. **无 CI**:仅 release.yml(标签触发打包),无 PR/分支的 fmt+clippy+test 质量门——回归风险靠人工 `cargo test`。

## 2. 需求

### 2.1 功能需求

**B2 聚合历史(子站服务端 + 主站)**

- F1: 服务端实现对 `ReadProcessedDetails`(`history_read_processed`)的聚合计算:按 `processing_interval` 将时间区间切成等长桶,每桶对采样值计算指定聚合函数,输出桶序列 `HistoryData.data_values`(每桶一个 `DataValue`,source_timestamp = 桶起始时间)。
- F2: 支持聚合函数子集(按 NodeId 常量):
  `Average(2342)`、`TimeAverage(2343)`、`Minimum(2346)`、`Maximum(2347)`、`Count(2352)`、`Total(2344)`、`Delta(2359)`、`PercentGood(2362)`;未实现聚合返回 `BadAggregateNotSupported`。
- F3: 空桶输出 `Variant::Empty`(简化,不引入 BadNoData 状态);数值仅对可转 f64 的样本聚合(非数值样本跳过,`PercentGood` = 好样本数/总样本数)。
- F4: 主站 `history_read_processed` 客户端封装(复用 CP 分页骨架),历史标签页"聚合"模式:选聚合函数 + 采样间隔 → 图表 + 表格显示桶结果。

**B1 主站历史标签页增强(事件历史标签页 + 聚合模式)**

- F5: 历史标签页加模式切换 `Raw | 聚合 | 事件`:
  - **Raw**:现状(原始样本 plot + 表格)
  - **聚合**:聚合函数下拉(平均/最小/最大/计数/TimeAverage/Total/Delta/PercentGood)+ 处理间隔(ms){1s~1h} → plot + 表格
  - **事件**:事件字段表格(时间/Severity/Source/Message,复用 events_panel 风格),走 `history_read_events`

**B3 残留:where_clause 完整表达式(事件历史过滤)**

- F6: 自研轻量 `ContentFilter` 求值器 `filter.rs`:操作数支持 `SimpleAttributeOperand`(browse_path 末段名 → 事件字段索引)与 `LiteralOperand`。
- F7: 支持的运算符:`Equals`、`Not`、`And`、`Or`、`GreaterThan`、`LessThan`、`GreaterThanOrEqual`、`LessThanOrEqual`、`Between`、`InList`、`Like`(通配 `%`/`_` 简易匹配);`Cast`/`InView`/`OfType`/`RelatedTo`/`Bitwise*` → `BadFilterOperatorInvalid`。
- F8: 数值比较跨类型统一按 f64(Int8~Int64/Float/Double/枚举 Int32);字符串按字符串;布尔按布尔。
- F9: `events_history_node_manager` 的 where_clause 过滤改为走 `filter::eval_clauses(elements, fields)`(替换现单 Equals 块);select_clauses 维持现状。

**B4 主站复杂类型展示(结构体字段树)**

- F10: `variant_to_tree(v: &Variant) -> Vec<TreeNode>`(core、纯函数可测):递归构造树——
  - `Variant::Array`:元素展开为子节点;`dimensions` 二维时按行列分组(参考 `variant_to_display_string` 的 `[1,2;3,4]` 布局)
  - `Variant::ExtensionObject`:若库解码为结构体 → 逐字段子节点;否则 fallback hex + 类型说明
  - 枚举按名字显示(附值);标量按现有格式
- F11: 主站挂载:值面板(选中节点详情 detail 区)+ 实时数据表(复杂值单元格显示 "📂" 可点开树)。UI 渲染层 `show_variant_tree(ui, &[TreeNode])`(master-egui)。

**C1 CI 管线**

- F12: `.github/workflows/ci.yml`(新建):PR 与 push master 触发 `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`;现有 release.yml 不动。

### 2.2 非功能需求

- N1: 无新 crate(仅扩展 workspace 内现有 async-opcua 0.18 依赖使用)。
- N2: `cargo fmt --check` 干净;`cargo test --workspace` 全部 PASS;`cargo clippy --workspace --all-targets -- -D warnings` 干净。
- N3: 聚合失败/空桶/非数值样本不 panic;不支持的聚合返回标准状态码。
- N4: 兼容旧 `.opcuaproj`(无新增配置字段;若有新增字段必须 serde default)。

## 3. 范围

### 3.1 纳入(已批准)

B2 聚合历史(服务端 + 主站 UI)+ B1 历史标签页聚合/事件模式 + where_clause 完整表达式引擎 + B4 结构体字段树 + C1 CI 管线。

### 3.2 明确不做

- 订阅事件的 where_clause 实时过滤(仅事件历史读取路径)
- `ServerCapabilities.AggregateFunctions` 特性属性节点注册(跳过,文档注明)
- 聚合配置复杂度(`treat_uncertain_as_bad`/`percent_data_good` 简化:仅按 status Good 计数 PercentGood)
- `history_update`(历史只读)
- 未解码 ExtensionObject 的二进制反序列化(hex fallback)
- 跨进程结构体解码 F-2 遗留(保持 PARTIAL,不重复解码层开发;字段树基于客户端已解码值)

## 4. 技术方案

### 4.1 模块结构

| 文件 | 责任 | 操作 |
|---|---|---|
| `crates/opcuasim-core/src/server/aggregate.rs` | 聚合函数计算(按桶) | 新建 |
| `crates/opcuasim-core/src/server/history_store.rs` | `query_samples`(无分页全量区间查询) | 修改 |
| `crates/opcuasim-core/src/server/history_node_manager.rs` | 覆盖 `history_read_processed` | 修改 |
| `crates/opcuasim-core/src/server/filter.rs` | ContentFilter 求值器 | 新建 |
| `crates/opcuasim-core/src/server/events_history_node_manager.rs` | where_clause 走求值器 | 修改 |
| `crates/opcuasim-core/src/server/mod.rs` | 模块导出 | 修改 |
| `crates/opcuasim-core/src/history.rs` | `history_read_processed` 客户端封装 | 修改 |
| `crates/opcuasim-core/src/values.rs` | `variant_to_tree`(可测纯函数) | 新建 |
| `crates/opcuasim-core/src/lib.rs` | 模块导出 | 修改 |
| `crates/opcuasim-core/tests/server_aggregates.rs` | 聚合 e2e | 新建 |
| `crates/opcuasim-core/tests/content_filter.rs` | where_clause 求值 e2e | 新建 |
| `crates/opcuasim-core/tests/variant_tree.rs` | 字段树纯函数测试 | 新建 |
| `crates/opcuamaster-egui/src/panels/history_tab.rs` | 模式切换/聚合/事件标签页 | 修改 |
| `crates/opcuamaster-egui/src/panels/value_panel.rs` | 结构体字段树挂载 | 修改 |
| `crates/opcuamaster-egui/src/panels/data_table.rs` | 复杂值展开树 | 修改 |
| `crates/opcuamaster-egui/src/model.rs` | HistoryTabState 扩展 | 修改 |
| `crates/opcuamaster-egui/src/backend/dispatcher.rs` | ReadHistory 分发三模式 | 修改 |
| `.github/workflows/ci.yml` | CI 质量门 | 新建 |

### 4.2 关键技术点

1. **聚合分桶**:区间 `[start, end)` 按 `processing_interval`(Duration, 微秒)切桶;每桶收集样本,`aggregate_fn(桶) -> Variant`。`delta` = 末值-首值(区间内首末样本);`Total` 对数值求和(TimeAverage 按时间加权:∑v·Δt/∑Δt)。
2. **客户端聚合封装**:`HistoryReadAction::ReadProcessedDetails(ReadProcessedDetails { start_time, end_time, processing_interval, aggregate_type: Some(vec![NodeId]), aggregate_configuration: AggregateConfiguration { use_server_capabilities_defaults: true, ..Default::default() } })`;响应 `into_inner_as::<HistoryData>()`(与 raw 相同结构,复用 map_data_value)。
3. **CP 分页复用**:聚合结果按桶序列做 skip 分页,沿用 SP1 的 `Box<dyn Any>` skip 方案;`query_samples` 无分页(内存量小)。
4. **求值器**:递归 `eval_element`;元素结果类型为 `FilterResult(bool)`;`And/Or/Not` 组合;`Between` 三操作数(字段 + 低 + 高);`InList` 变长;`Like` 转正则(转义 + `%`→`.*`、`_`→`.`)。字段名 → 索引映射与 select_clauses 一致(SharedTable `field_names` 常量抽到模块共享)。
5. **variant_to_tree**:递归;`TreeNode { name, value_display, children: Vec<TreeNode> }`;ExtensionObject 探测 `into_inner_as::<DynamicStructure>` 失败则 hex。UI 用 `CollapsingHeader` + `Label` 渲染。

## 5. 风险与缓解

| 风险 | 等级 | 缓解 |
|---|---|---|
| 客户端收到结构体值未被解码(二进制 body) | 中 | 实现 T4 先验证库订阅/读取路径的解码行为;未解码则字段树走 hex fallback + 文档注明 |
| 聚合语义与标准有偏差(Bound/空桶/时间语义) | 中 | 明确简化并写入文档;e2e 用确定数据验证均值/计数 |
| `Like` 通配语义超需求 | 低 | 仅 `%`/`_` 简易匹配,测试覆盖 |
| clippy `-D warnings` 全量目标有存量问题 | 低 | 若无建议先修存量(仅限本分支相关),再 CI;历史遗留不阻塞 |
| UI 模式切换状态扩散 | 低 | HistoryTabState 集中加 mode/agg 字段,单一 state struct |

## 6. 验收标准

- [ ] 服务端 `history_read_processed` 返回按 `processing_interval` 分桶的聚合结果(Average/Min/Max/Count/TimeAverage/Total/Delta/PercentGood);未实现聚合 → BadAggregateNotSupported
- [ ] 主站历史标签页三模式:Raw(现状)、聚合(函数+间隔 → plot+表格)、事件(字段表格)
- [ ] 事件历史带复合 where_clause(`Severity >= 400 AND Message LIKE '%x%'` 等)过滤正确
- [ ] 主站值面板与实时表对数组/二维数组/枚举/结构体(嵌套)以字段树/网格展示
- [ ] CI 工作流:本地三命令(fmt/clippy/test)全绿,工作流文件语法正确
- [ ] `cargo test --workspace` 全部 PASS;fmt 干净