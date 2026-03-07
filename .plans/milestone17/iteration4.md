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
- [x] 4.1 实现数据块切割算法 — `split_sheet_into_ranges` 双策略：① **空白楚河汉界**：连续 ≥4 行全空行作为分隔（避免数据间 1-2 行正常空行误触发），② **数据类型跳变**：当前数据区有数值列 → 1-3 行空行 → 全文本行（疑似新表头） → 后续有数值列 → 确认切割。两策略互补，测试覆盖 3 个场景（分割/跳变/不分割）
- [x] 4.2 实现数据块独立索引 — `parse_sheet_to_block_with_offset`：每个数据块独立执行表头检测、降维拼接、forward_fill、脏数据过滤，生成独立的 TableProfile。source_id 包含 block_index
- [ ] 4.3 支持 DuckDB 多表挂载 — 推迟：当前直接注入 Markdown 表格到 LLM context 已覆盖中小表格场景
- [ ] 4.4 增强 SQL Prompt 支持多表 — 推迟（依赖 4.3）
- [ ] 4.5 图表和图片元素过滤 — calamine 本身只读取单元格数据，xl/charts/ 和 xl/media/ 不会被加载

Exit criteria:
- [x] 单 Sheet 包含 2 个以上数据表时，能被正确切割为独立的 DataFrame — test_multi_block_detection 验证
- [x] 每个数据块独立生成 TableProfile 并可被向量搜索命中 — 每个 block 生成独立 profile
- [ ] 跨数据块的 JOIN 查询能正确执行 — 推迟（依赖 DuckDB）
- [x] 图表/图片元素不干扰数据提取 — calamine 自动跳过
- [ ] 多个 DataFrame 同时挂载到 DuckDB 后性能可接受 — 推迟
