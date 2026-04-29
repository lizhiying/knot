Milestone: milestone18
Iteration: iteration3

Goal: 增强数据表达层与容错层。引入基于 DuckDB 的 Agentic 多步查询交互回退；以及完成最终在对话流中透传二维结构化表格视图的炫酷渲染。

Assumptions:
- 多表与过滤功能已前置跑通并非常稳定。
- DuckDB 内置的数据抓取可以被串行化且随时转回为人类语言解释或二次 Prompt 追问。

Scope:
- 如果 DuckDB 抓取报错，把错误文本和前文喂回给大模型进行最多 N 步的重试编排动作。
- 回答中识别 JSON 行数据以供图表/组件渲染而不只是文字复述。

Tasks:
- [x] 1. Agentic Step-by-Step 循环执行代理：如果一次 SQL 抛出 `syntax error` 或查不到核心值，触发 `re-prompt` 进行自我修正或者发问追问用户约束条件。
- [x] 2. 对最终的输出结果设计统一的标准协议（例如 `GridData` 包涵 columns/rows 格式）。
- [x] 3. Knot Svelte 5 前端 UI 捕获该新数据类型流，利用专门的 Table Grid 渲染模块做前端交互型展示（支持基础分页/查看全量）。

Exit criteria:
- 用户对结构超复杂的表问了一个有歧义条件的问题，大模型多次 SQL 验证并纠错，最终提取了精华并引导提示。
- 在前端最终吐出并绘制了一个原生极好用带滚动的 `Table Grid` 展现了精准查询的数据！
