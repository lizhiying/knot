Milestone: milestone17
Iteration: 最小可用 — 标准表读取 + Table Profile 索引

Goal:
跑通「读取标准 Excel -> 生成 Table Profile -> 存入向量库 -> 搜索命中」的完整端到端链路。
本阶段假设每个 Sheet 只有一个规整的标准二维表（第一行为表头，无合并单元格）。
这是一个 vertical slice，覆盖从文件读取到搜索命中的全部路径。

Assumptions:
- 每个 Sheet 只有一个标准二维表（单行表头，无合并单元格）
- Excel 文件规模中等（不处理 100MB+ 极端场景）
- 现有的 `knot-pdf` / `knot-parser` 架构足够作为参考模板
- `FileRegistry` + `DirectoryWatcher` 已支持 `.xlsx` 文件类型识别（仅展示，不索引）
- 暂不需要 Text-to-SQL 查询能力，仅做向量化索引和搜索

Scope:
- 创建 `knot-excel` crate，实现基础解析能力
- 在 `knot-parser` 中新增 Excel format handler
- 修改索引逻辑使 `.xlsx`/`.xls` 可被索引
- 前端 Knowledge 页面更新索引状态和图标

Tasks:
- [x] 1.1 创建 `knot-excel` crate — 在 workspace 中新建独立项目，依赖 `calamine`（不用 polars，手动实现类型推断）；结构参考 `knot-pdf`，包含 `Config`、`Pipeline`、`Error` 等基础模块；更新 workspace `Cargo.toml` 的 members
- [x] 1.2 实现 `ExcelReader` — 封装 Calamine，遍历所有可见 Sheet，读取数据为 `Vec<Vec<String>>`；自动识别第一行为表头，其余为数据体
- [x] 1.3 实现列类型推断 — 自动推断列类型（String/Float/Int/Date/Bool），drop 全空行和列
- [x] 1.4 实现 `TableProfile` 生成 — 为每个 DataBlock 生成结构化摘要文本：元数据（文件路径_Sheet名）、Schema（列名+类型）、数据抽样（前 3 行 Markdown 表格）
- [x] 1.5 在 `knot-parser` 中新增 `excel.rs` format handler — 实现 `DocumentParser` trait，调用 `knot-excel` 解析 Excel 文件，将 `TableProfile` 转换为 `PageNode` 树
- [x] 1.6 修改 `KnotIndexer` 索引逻辑 — 在 `index_directory` 文件类型过滤中添加 `.xlsx`/`.xls`；metadata 中标记 `doc_type: "tabular"` 和 `table_profile` JSON
- [x] 1.7 修改 `monitor.rs` 文件类型支持 — `.xlsx` 已在 `should_index_file` 的支持列表中
- [x] 1.8 Knowledge 页面更新 — 在 `is_indexable_type` 中添加 `KnowledgeFileType::Excel`，从 `Unsupported` 变为动态状态
- [x] 1.9 表格类型文件图标 — 已在 `FileList.svelte` 中映射 `table_chart` 图标和绿色 `#10b981`

Exit criteria:
- 一个标准的 `.xlsx` 文件（如销售报表）能被成功索引
- 在 RAG 搜索中输入"销售数据"能命中该 Excel 的 Table Profile Chunk
- Knowledge 页面正确显示 Excel 文件的索引状态和专属图标
- `knot-excel` crate 能独立编译并通过基本单元测试
