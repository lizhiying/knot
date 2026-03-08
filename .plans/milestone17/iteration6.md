Milestone: milestone17
Iteration: 表格数据展示优化 — 渲染增强 + 分页查询

Goal:
优化表格相关数据的展示效果。当前 LLM 生成的包含结构化数据的回答（如血压记录、账目明细等）以纯文本列表形式展示，
信息密度低、可读性差。本迭代实现两个核心改进：
1. **LLM 输出表格渲染**：引导 LLM 输出 Markdown 表格格式，前端正确渲染为可视化表格
2. **SQL 查询结果分页**：对 DuckDB 查询结果实现分页加载，避免大数据集一次性加载导致内存和性能问题

Assumptions:
- Iteration 5 已完成：DuckDB Text-to-SQL 查询引擎可用，单文件 Excel 聊天已实现
- 当前 Markdown 渲染使用 `marked` 库，已支持表格语法（`|col1|col2|`）
- 当前 AI Insight（Spotlight 搜索）和 FileChat（Knowledge 单文件聊天）是两个独立的展示区域
- DuckDB 支持 `LIMIT/OFFSET` 分页语法
- LLM（Qwen 4B）能遵循 Prompt 中的表格输出格式要求

Scope:
- **Part A: LLM 表格输出引导**
  - 修改 RAG System Prompt，引导 LLM 对结构化数据使用 Markdown 表格格式输出
  - 优化前端 Markdown 表格 CSS 样式（AI Insight + FileChat 两个场景）
  - 表格样式统一：深色主题、紧凑行高、斑马纹、overflow 横向滚动

- **Part B: SQL 查询结果分页展示（FileChat 场景）**
  - 后端：`query_excel_table` 支持分页参数（page, page_size），DuckDB SQL 自动追加 LIMIT/OFFSET
  - 后端：返回总行数（COUNT 单独查询或 DuckDB window function）
  - 前端：FileChat SQL 结果区域添加分页控件（上一页/下一页 + 页码指示）
  - 前端：默认 page_size = 20，用户可切换（20/50/100）
  - 分页请求不重新生成 SQL，复用已注册的 DuckDB 表

- **Part C: AI Insight 场景表格增强（Spotlight 搜索）**
  - Spotlight 搜索场景 LLM 回答中的 Markdown 表格样式优化
  - 长表格（>10行）在 AI Insight 区域可折叠展示

Tasks:
- [x] 6.1 修改 RAG System Prompt 引导表格输出 — 在 `rag_generate` 的 System Prompt 中添加规则：当回答包含多条结构化数据时，使用 Markdown 表格格式 (`| 列1 | 列2 |`) 展现。给出 few-shot 示例。测试验证 LLM 对"列出最后5条血压"类查询输出表格
- [x] 6.2 AI Insight Markdown 表格 CSS 样式 — 在 spotlight.css markdown-content CSS 中增强 table 样式：圆角边框、深色背景、斑马纹（odd-even 交替色）、表头加粗高亮 sticky、行悬停高亮、容器 overflow-x:auto（宽表格横向可滚动）
- [x] 6.3 FileChat Markdown 表格 CSS 样式 — FileChat.svelte 中 answer-content 的 :global(table) 样式增强，与 6.2 保持一致
- [ ] 6.4 后端分页支持 — 修改 `query_excel_table` Tauri command：① 新增 `page`（默认 1）和 `page_size`（默认 20）参数，② DuckDB 执行 SQL 后，先用 `SELECT COUNT(*) FROM (original_sql)` 获取总行数，③ 再用 `SELECT * FROM (original_sql) LIMIT {page_size} OFFSET {(page-1)*page_size}` 获取当前页，④ ExcelQueryResponse 新增 `total_count` 和 `current_page` 字段
- [ ] 6.5 后端 DuckDB 会话复用 — 当前 `query_excel_table` 每次调用都重新解析 Excel + 注册 DuckDB。分页翻页时应复用已注册的表。方案：① 在 AppState 中缓存 `QueryEngine`（按 file_path 键），② 翻页请求直接用缓存的 engine 执行分页 SQL，③ 添加过期清理（如 5 分钟无访问自动释放）
- [ ] 6.6 前端分页控件 — FileChat.svelte SQL 结果区域底部添加分页 UI：① 上一页/下一页按钮（Material Icons），② 当前页/总页数显示，③ page_size 切换（20/50/100 下拉选择），④ 翻页时调用 `query_excel_page` command（不重新生成 SQL），⑤ 加载中状态指示
- [ ] 6.7 新增 `query_excel_page` Tauri command — 分页翻页专用接口：① 接收 file_path、sql、page、page_size 参数，② 从 AppState 缓存获取 QueryEngine（不重新解析文件），③ 执行分页 SQL 返回结果，④ 如缓存失效则自动重建
- [ ] 6.8 AI Insight 长表格折叠 — Spotlight 搜索中 LLM 回答如包含 >10 行的 Markdown 表格：① 默认只显示前 5 行 + "展开全部 (N 行)" 按钮，② 点击后展开完整表格，③ 需要在 htmlContent 渲染后做 DOM 后处理（检测 <table> 行数）

Exit criteria:
- 查询"列出最后5条血压"时，LLM 回答使用 Markdown 表格格式，前端正确渲染为可视化表格
- Markdown 表格在深色主题下清晰可读（斑马纹 + 表头高亮 + 紧凑布局）
- FileChat 中 SQL 查询结果集超过 20 行时自动分页，翻页响应 <500ms（不重新解析 Excel）
- 分页 UI 包含页码、翻页按钮、page_size 切换
- 总行数超过 1000 的大数据集，首页加载时间 < 1s
- AI Insight 中超长表格可折叠展示
