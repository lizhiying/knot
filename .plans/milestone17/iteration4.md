Milestone: milestone17
Iteration: 智能数据区探测 — 多数据块切割与跨表查询

Goal:
处理单 Sheet 内包含多个数据块（表格混排）的情况。
实现数据块边界探测和独立索引，支持 DuckDB 同时挂载多个 DataFrame 进行跨表查询。

Assumptions:
- Iteration 1-3 已完成：标准和复杂 Excel 解析、Text-to-SQL 查询已可用
- 数据块之间通常以连续空行/空列分隔（"楚河汉界"模式）
- 部分场景数据块通过"数据类型跳变"区分（如从全文本区突变为数值/日期区）
- DuckDB 能同时挂载多个临时表进行 JOIN 或 UNION ALL
- LLM 能在 Prompt 中列出多表 Schema 后生成正确的 JOIN SQL

Scope:
- 实现数据块切割算法（两种启发式规则）
- 实现数据块独立索引（每个数据块一个 TableProfile + VectorRecord）
- 支持 DuckDB 多表挂载
- 增强 SQL Prompt 支持多表查询
- 过滤图表和图片元素

Tasks:
- [ ] 4.1 实现数据块切割算法 — 基于两种启发式规则识别数据块边界：① **空白楚河汉界** — 连续的全空行/列作为分隔；② **数据类型跳变** — 从全文本区突变为数值/日期区。输出每个数据块的行列范围
- [ ] 4.2 实现数据块独立索引 — 每个数据块生成独立的 `TableProfile`，元数据标记 `[文件路径]_[Sheet名]_[数据块ID]`，各数据块单独存入向量库
- [ ] 4.3 支持 DuckDB 多表挂载 — 同一文件的多个数据块和跨文件的多个 DataFrame 可同时注册为 DuckDB 临时表，表名自动生成（如 `sheet1_block0`, `sheet1_block1`）
- [ ] 4.4 增强 SQL Prompt 支持多表 — LLM Prompt 中列出所有关联表的 Schema 和表名，允许生成 `JOIN` 或 `UNION ALL` 语句进行跨表查询
- [ ] 4.5 图表和图片元素过滤 — 忽略 `xl/charts/` 和 `xl/media/` 中的视觉元素，只关注单元格数据，避免非数据内容干扰解析

Exit criteria:
- 单 Sheet 包含 2 个以上数据表时，能被正确切割为独立的 DataFrame
- 每个数据块独立生成 TableProfile 并可被向量搜索命中
- 跨数据块的 JOIN 查询能正确执行
- 图表/图片元素不干扰数据提取
- 多个 DataFrame 同时挂载到 DuckDB 后性能可接受
