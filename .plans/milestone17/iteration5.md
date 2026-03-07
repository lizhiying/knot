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
- [ ] 5.1 集成 `duckdb` crate — 在 `knot-excel` 的 Cargo.toml 中添加 `duckdb = { version = "1.4", features = ["bundled"] }`，确保编译通过。创建 `src/query/mod.rs` 模块
- [ ] 5.2 实现 `QueryEngine` 核心 — 封装 DuckDB 内存连接（`:memory:`），实现：① `register_datablock(block: &DataBlock) -> table_name` 将 DataBlock 数据通过 INSERT 语句注册为临时表，② `execute_sql(sql: &str) -> QueryResult` 执行 SQL 并返回结果，③ `unregister_all()` 清理所有临时表
- [ ] 5.3 实现 `SqlGenerator` — Prompt 构建模块：① 接收用户 Query + 所有已注册表的 Schema（表名、列名、列类型、数据示例），② 组装 System Prompt（引导 CTE 优先、DuckDB 语法、中文列名双引号），③ 返回完整 Prompt 字符串供 LLM 调用
- [ ] 5.4 实现多步 SQL 执行 — `execute_multi_step(sql_text: &str)`：① 按分号拆分多条 SQL，② 前 N-1 条自动包装为 `CREATE TEMP TABLE step_N AS (...)` 注册为中间临时表，③ 最后一条执行并返回结果
- [ ] 5.5 实现 `ResultSummarizer` — 结果膨胀控制：① ≤20 行直接返回全量 Markdown 表格，② >20 行生成统计摘要（行数、各列 min/max/avg/distinct_count、前 5 行样本），③ 摘要格式化为 LLM 可理解的文本
- [ ] 5.6 实现 SQL 执行容错 + 重试 — 当 SQL 执行失败时：① 解析 DuckDB 错误信息，② 将错误信息 + 原 SQL + 表 Schema 重新发送给 LLM 要求修复，③ 最多重试 2 次
- [ ] 5.7 修改 `HybridQueryRouter` — 将当前的"直接注入 Markdown 表格"升级为可选的 Text-to-SQL 路径：① 小表格（≤50 行）保持直接注入方式，② 大表格（>50 行）走 DuckDB SQL 查询路径
- [ ] 5.8 新增 `query_excel_table` Tauri command — 单文件 Excel 聊天接口：① 接收 (file_path, query) 参数，② 加载文件所有 DataBlock → 注册到 DuckDB，③ 生成 SQL Prompt → LLM 生成 SQL → 执行 → 返回结果
- [ ] 5.9 前端：查询状态多阶段指示 — 完善 FileChat 的 phase 状态机显示："正在搜索..." → "正在生成 SQL..." → "正在执行查询..." → "正在生成回答..."
- [ ] 5.10 前端：SQL 查询结果展示 — 在回答中展示执行的 SQL 语句（可折叠）和查询结果表格，大结果集显示"数据量较大（N 行），已自动汇总展示"提示

Exit criteria:
- `QueryEngine` 能将 DataBlock 注册为 DuckDB 临时表，并成功执行 SQL 查询
- `SqlGenerator` 生成的 Prompt 能引导 LLM 生成正确的 DuckDB SQL（至少对简单聚合查询如 SUM/AVG/COUNT）
- `ResultSummarizer` 对 >20 行结果生成可读的统计摘要
- SQL 执行失败时能自动重试并修复（至少 50% 的简单错误能自修复）
- 大表格（>50 行）走 Text-to-SQL 路径时，性能可接受（<3s）
- 单文件 Excel 聊天能工作：用户选择一个 Excel 文件后，能通过自然语言查询数据
- 前端正确显示多阶段状态和 SQL 查询结果
