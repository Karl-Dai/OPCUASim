# 手测:HistoryRead

子项目 #5(Tier-1 第 5 项)的验证脚本。`opcuasim-core` 自身不带 historian,所以 e2e 测试不能用内嵌 server,只能对外部 historian 验证。

> 注意:本手册最初按 egui 版 UI 编写。当前 master 前端已迁移到 Tauri 2 + Vue 3,界面布局与操作措辞以下文为准。

## 前置条件

装好任一带 historian 的 OPC UA Server:

- **Prosys OPC UA Simulation Server**(免费 GUI,自带 Counter / Sinusoid 节点和 historian),推荐
- **KEPServerEX**(商业,试用即可),勾上 channel 的 "Enable historian"
- **open62541** 自带 `examples/server_history` 示例

启动 server,在其 UI/配置中开启历史归档。

## 步骤

1. `cd crates/opcuamaster-app && cargo tauri dev`
2. 顶栏 → 新建连接,填入服务器 URL,安全策略选 None(匿名),保存后在左侧连接树选中该连接,点顶栏「连接」
3. 在左侧连接树展开节点,找到一个有历史数据的 Variable(例如 Prosys 的 `Objects/Simulation/Counter` 或 `Sinusoid`)
4. 等订阅几分钟,让 historian 攒下数据
5. **在连接树该 Variable 行点 📈 按钮(悬浮提示「查看历史」)**
6. 中央区域自动切到「历史」Tab,观察:
   - 默认时间范围:过去 5 分钟
   - 折线图应有上升 / 周期波形(Counter 单调递增,Sinusoid 正弦)
   - 表格列出每个采样点(Source Timestamp / Value / Status)
7. 点 "1h" 快捷按钮,刷新自动触发,看到更长时间范围的数据
8. 自定义起止时间(改 RFC3339 字符串,如 `2026-04-28T08:00:00Z`),点 "🔄 刷新",数据应反映新范围
9. **在「数据表」Tab 中点已订阅节点行的历史按钮** —— 同样切到历史 Tab
10. 切回「数据表」Tab 再切回「历史」Tab,历史目标保留,数据不丢失
11. 换另一个 Variable 点 📈,历史面板目标切换,曲线与表格随之更新(单 Tab 单目标,数据不串扰)

## 已知限制(本期)

- 仅 Float / Double / Int* / UInt* / Bool 等数值类型才会画折线;String/Bool 显示为文本表
- 时间输入是 RFC3339 字符串,没有日历选择器
- 单 Tab 单节点;不支持多节点叠加图
- 默认 5 分钟范围、上限 5000 点;改更大需 server 配合
- 第一次打开 Tab 自动触发首次刷新;若服务器 5 分钟内无数据,会看到空表(无错误提示)

## 失败模式排查

- "history_read failed: Bad…" → 检查服务器是否已启用历史归档(很多 server 默认不开)
- 折线图为空但表格有数据 → 该节点是非数值类型(String/ByteString 等),`numeric` 列均为 None
- "invalid time '...'" 错误 → RFC3339 格式不合法,起止字段必须含时区(如 `Z` 或 `+08:00`)
