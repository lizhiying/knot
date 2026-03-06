Milestone: milestone17
Iteration: 混合查询路由 + Text-to-SQL 引擎

Goal:
实现混合查询路由器（HybridQueryRouter），根据搜索结果中的文档类型自动选择查询策略。
集成 DuckDB 查询引擎，实现 Text-to-SQL 能力。
支持纯文本 RAG、纯 Text-to-SQL、混合文档并行查询与合并三种场景。
解决多步查询和中间结果膨胀问题（4 层降级策略）。

Assumptions:
- Iteration 1 已完成：Excel 文件已可被索引，搜索能命中 Table Profile Chunk
- `doc_type` 字段已在 VectorRecord 中打标但尚未用于路由
- `duckdb-rs` crate 足够稳定，可用于嵌入式 OLAP 查询
- 本地 LLM (8192 tokens) 能生成基本的 SQL 语句（Quality 可能不稳定，需要重试机制）
- Polars DataFrame 可以通过 Arrow 格式零拷贝注册到 DuckDB

Scope:
- 扩展 VectorRecord/SearchResult 的 `doc_type` 字段到 LanceDB/Tantivy Schema
- 集成 DuckDB，实现 QueryEngine（连接管理、DataFrame 注册、SQL 执行）
- 实现 SqlGenerator（Prompt 构建 + CTE 引导）
- 实现 ResultSummarizer（结果膨胀控制）
- 实现 HybridQueryRouter（搜索结果分流 + 查询策略选择）
- 修改 `rag_search` 集成路由逻辑
- 前端展示结构化查询结果和多阶段状态

Tasks:
- [ ] 2.1 扩展 `VectorRecord` 增加 `doc_type` 字段到 Schema — 在 LanceDB Schema 和 Tantivy Schema 中新增 `doc_type: String` 字段（值为 `"text"` 或 `"tabular"`），兼容存量数据（默认 `"text"`）
- [ ] 2.2 扩展 `SearchResult` 增加 `doc_type` 字段 — 搜索结果携带文档类型信息，供路由器判断
- [ ] 2.3 集成 `duckdb` crate — 在 `knot-excel` 中添加 DuckDB 依赖，实现 Arrow 零拷贝注册 Polars DataFrame 为临时表
- [ ] 2.4 实现 `QueryEngine` — 封装 DuckDB 连接管理、多 DataFrame 注册/注销、SQL 执行（含多步临时表链 `execute_multi_step`）、结果格式化为 Markdown 表格
- [ ] 2.5 实现 `SqlGenerator` Prompt 构建 — 将用户 Query + 所有关联临时表名 + Schema 信息组装为 LLM Prompt，引导 LLM 优先使用 CTE/子查询一步完成，包含 DuckDB 语法约束
- [ ] 2.6 实现 SQL 执行容错 + 多步重试 — SQL 执行失败时重试（最多 2 次）；多步 SQL 自动走临时表链；失败时只传 schema 摘要给 LLM（不传全量数据）
- [ ] 2.7 实现 `ResultSummarizer` — 4 层降级策略：① CTE/子查询 ② DuckDB 临时表链 ③ 结果摘要化（>20 行时生成统计摘要：行数、列统计、前 5 行样本） ④ 硬截断前 20 行 + 警告
- [ ] 2.8 实现 `HybridQueryRouter` — 核心路由逻辑：按 `doc_type` 分流搜索结果，判断场景（纯文本/纯表格/混合/多表格），选择查询策略，合并结果上下文注入同一 Prompt
- [ ] 2.9 修改 `rag_search` 集成路由逻辑 — 搜索完成后，根据 `doc_type` 分流，对 tabular 结果执行 SQL 查询，将查询结果附加到 context 中
- [ ] 2.10 新增 `query_excel_table` Tauri command — 单文件模式：接收用户 Query + 文件路径，直接走 Text-to-SQL 路径
- [ ] 2.11 前端：结构化查询结果展示 — 当 RAG 回答包含表格数据时，以格式化的 Markdown 表格展示
- [ ] 2.12 前端：查询状态多阶段指示 — 显示分阶段状态："正在搜索..." -> "正在分析表格数据..." -> "正在生成回答..."
- [ ] 2.13 前端：数据来源类型标注 — Sources 列表中区分文本来源和数据来源，数据来源显示执行的 SQL
- [ ] 2.14 前端：单文件 Excel 聊天 — Knowledge 页面支持对已索引的 Excel 文件进行单文件聊天（直接走 Text-to-SQL 路径）
- [ ] 2.15 前端：大结果集警告提示 — 当结果被摘要化时，显示"数据量较大（N 行），已自动汇总展示"的提示

Exit criteria:
- 纯表格场景: 提问"XX 表的 YY 列总和是多少"时，系统能自动生成 SQL 并返回正确结果
- 混合文档场景: 同时命中 PDF 和 Excel 时，回答能综合文本描述和数据结果
- 多 Excel 场景: 命中多个 Excel 数据块时，DuckDB 能同时挂载并支持跨表查询
- 多步查询场景: "增长最快的产品库存如何"能通过 CTE 或临时表链一次性完成
- 结果膨胀场景: SQL 返回 1000+ 行时，自动摘要化，不超出 LLM 上下文限制
- SQL 执行失败时能自动重试并修复（最多 2 次）
- 结果以 Markdown 表格形式展示
- 前端分阶段状态指示正确显示
