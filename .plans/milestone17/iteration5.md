Milestone: milestone17
Iteration: DuckDB 集成 — Text-to-SQL 查询引擎

Goal:
集成 DuckDB 嵌入式 OLAP 引擎，实现 Text-to-SQL 能力。
用户提问时，系统自动将 Excel 数据加载到 DuckDB，通过 LLM 生成 SQL 并执行，返回结构化查询结果。
支持单表查询、多表 JOIN、CTE 多步查询，以及结果膨胀控制。

Assumptions:
- Iteration 1-4 已完成：Excel 解析、索引、复杂报表处理、多数据块切割均可用
- `duckdb` crate v1.4.4 稳定可用，支持 Arrow 格式数据注册
- 当前 LLM 为 Qwen 系列（4B 量化版），上下文窗口约 8192 tokens
- 当前方案"直接注入 Markdown 表格"对 ≤50 行数据有效，但 >50 行需要 SQL 查询
- 不依赖 Polars，DataBlock 中的 Vec<Vec<String>> 数据通过 DuckDB API 直接注册

Scope:
- 集成 duckdb crate，实现 QueryEngine（连接管理、数据注册、SQL 执行）
- 实现 SqlGenerator（Prompt 构建 + CTE 引导）
- 实现 ResultSummarizer（结果膨胀控制：小结果全量、大结果摘要）
- 实现 SQL 执行容错（错误反馈 + 重试）
- 修改 HybridQueryRouter，tabular 结果走 Text-to-SQL 路径
- 新增 query_excel_table Tauri command（单文件 Excel 聊天）
- 前端展示 SQL 查询结果和多阶段状态

Tasks:
- [x] 5.1 集成 `duckdb` crate — `duckdb = { version = "1.4", features = ["bundled"] }`，编译通过，创建 `src/query/` 模块（mod.rs + engine.rs + sql.rs + result.rs）
- [x] 5.2 实现 `QueryEngine` 核心 — DuckDB 内存连接，`register_datablock` 按列类型建表 + 参数化 INSERT，`execute_sql` 通过 Rows 迭代读取多类型（String/i64/f64/bool），`unregister_all` 清理临时表。3 个单测覆盖
- [x] 5.3 实现 `SqlGenerator` — System Prompt（CTE 优先、DuckDB 语法、中文列名双引号），User Prompt（Schema + 数据示例 + 用户问题），Fix Prompt（错误修复）。3 个单测覆盖
- [x] 5.4 实现多步 SQL 执行 — `execute_multi_step`：分号拆分，前 N-1 条包装为 `CREATE TEMP TABLE step_N AS (...)`，最后一条返回结果。单测验证中间表链
- [x] 5.5 实现 `ResultSummarizer` — ≤20 行全量 Markdown，>20 行统计摘要（数值列 min/max/avg，文本列 distinct count，前 5 行样本）。2 个单测覆盖
- [x] 5.6 实现 SQL 执行容错 + 重试 — SQL 执行失败时：① 解析错误信息，② 调用 LLM build_fix_prompt 修复 SQL，③ 最多重试 2 次。已集成到 rag_search 和 query_excel_table 中
- [x] 5.7 修改 `HybridQueryRouter` — ① 小表格（≤50行）保持直接 Markdown 注入，② 大表格（>50行）走 DuckDB Text-to-SQL 路径（注册→LLM 生成 SQL→执行→ResultSummarizer 处理），③ 任何环节失败时 fallback 到 Markdown。提取了 `inject_block_as_markdown` 辅助函数
- [x] 5.8 新增 `query_excel_table` Tauri command — 单文件 Excel 聊天接口：加载文件→注册 DuckDB→LLM 生成 SQL→执行（带重试）→返回 ExcelQueryResponse（含 SQL、列名、行数据、摘要）
- [x] 5.9 前端：查询状态多阶段指示 — 新增 `querying_sql` phase，显示紫色 database 图标。Excel 文件查询流程：「搜索表格数据」→「正在生成 SQL 查询」→「正在执行查询」→「正在生成回答」。SQL 结果作为增强 context 注入生成阶段
- [x] 5.10 前端：SQL 查询结果展示 — 可折叠 SQL 代码块（紫色高亮），小结果集（≤20行）直接渲染数据表格，大结果集显示「数据量较大（N 行），已自动汇总展示」提示。重试标记（「已重试」）。所有样式使用紫色 accent 与数据库操作视觉关联

Exit criteria:
- `QueryEngine` 能将 DataBlock 注册为 DuckDB 临时表，并成功执行 SQL 查询
- `SqlGenerator` 生成的 Prompt 能引导 LLM 生成正确的 DuckDB SQL（至少对简单聚合查询如 SUM/AVG/COUNT）
- `ResultSummarizer` 对 >20 行结果生成可读的统计摘要
- SQL 执行失败时能自动重试并修复（至少 50% 的简单错误能自修复）
- 大表格（>50 行）走 Text-to-SQL 路径时，性能可接受（<3s）
- 单文件 Excel 聊天能工作：用户选择一个 Excel 文件后，能通过自然语言查询数据
- 前端正确显示多阶段状态和 SQL 查询结果
