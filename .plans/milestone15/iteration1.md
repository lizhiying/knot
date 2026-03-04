Milestone: milestone15 - GraphRAG
Iteration: Iteration 1 — 端到端最小可用

Goal:
用最简单的方式跑通实体提取→存储→查询→拼入搜索结果的完整流程。
实体提取可以用基于规则的方式（正则 + 启发式），不依赖 LLM，确保流程能跑通。

Assumptions:
- 实体提取先用规则方式（正则匹配中英文专有名词），后续迭代再接入 LLM
- 存储用 SQLite（复用现有 `sqlx` 依赖，零外部依赖新增）
- 关系暂时只提取 "共现"（两个实体出现在同一段落 = 有关联）
- 查询结果直接拼入 expanded_context，复用现有字段
- SQLite 数据库文件放在索引目录下（`knot_graph.db`）

Scope:

Tasks:
- [x] 1.1 创建 `knot-core/src/entity.rs` 模块
  - 定义 `EntityRecord` 和 `RelationRecord` 数据结构
  - 实现 `extract_entities_rule_based()`: 用正则提取中英文专有名词（大写开头、引号内术语）
  - 实现 `extract_cooccurrence_relations()`: 同一段落内的实体对生成共现关系
  - 修改: 新建 `knot-core/src/entity.rs`，修改 `knot-core/src/lib.rs` 导出模块

- [x] 1.2 在 `entity.rs` 中添加 SQLite 实体图存储 (EntityGraph)
  - 复用 `sqlx` 依赖（无需新增 crate）
  - 初始化 SQLite 数据库，创建 entities 和 relations 表（含索引）
  - 实现 `add_entities()` 和 `add_relations()` 写入方法（UPSERT）
  - 实现 `delete_by_file()` 用于增量更新时清除旧数据
  - 实现 `get_related_entities()` 双向 1 跳图查询
  - 实现 `get_entity_chunk_ids()` 和统计方法
  - 修改: `knot-core/src/entity.rs`

- [x] 1.3 在 `index_file()` 中集成实体提取
  - 新增 `extract_from_records()` 便捷函数，从 VectorRecord 列表批量提取
  - 初始扫描：records 被 move 前提取实体，写入 knot_graph.db
  - 文件监控：变更文件 index 后提取实体（先 delete_by_file 再写入）
  - 修改: `knot-core/src/entity.rs`, `knot-app/src-tauri/src/main.rs`

- [x] 1.4 实现图查询方法
  - `get_related_entities(entity_name)`: 双向 UNION 查询关联实体
  - `get_entity_chunk_ids(entity_name)`: 返回实体来源的 chunk ID
  - 已在 task 1.2 中一并实现
  - 修改: `knot-core/src/entity.rs`

- [x] 1.5 在搜索流程中集成图查询
  - 从用户查询中提取实体（复用 extract_entities_rule_based）
  - 查找相关实体和关系（最多 3 个查询实体，每个最多 5 个关联）
  - 将 `[知识图谱] 实体 关联: ...` 拼入第一个结果的 expanded_context
  - 仅在 `graph_rag_enabled` 为 true 时执行
  - 同时集成到 `rag_search` 和 `rag_query` 两个 handler
  - 修改: `knot-app/src-tauri/src/main.rs`

- [x] 1.6 前端设置页面增加"知识图谱"开关
  - AppConfig 新增 `graph_rag_enabled`（默认 false，实验性功能）
  - Tauri command: `set_graph_rag_enabled`
  - Settings.svelte 增加开关组件，描述标注「实验性功能」
  - 开关控制：索引时是否提取实体、搜索时是否做图查询
  - 修改: `knot-app/src-tauri/src/main.rs`, `knot-app/src/lib/components/Settings.svelte`

- [x] 1.7 基础测试（9 个测试）
  - 测试规则提取：英文专有名词（GPT-4, OpenAI, RLHF）
  - 测试排除常见句首大写词（The, However）
  - 测试中文角引号术语（「支持向量机」）
  - 测试中文双引号术语（"注意力机制"）
  - 测试全大写缩写词（CNN, LSTM, NLP, API）
  - 测试实体去重（重复的 OpenAI 只保留 1 个）
  - 测试同 chunk 共现关系（3 实体 → 3 条关系）
  - 测试不同 chunk 不产生共现关系
  - 测试 EntityType 序列化/反序列化
  - 修改: `knot-core/src/entity.rs` (tests module)

Exit criteria:
- ✅ 编译通过，现有 18 个测试不受影响（实际 27 个，包含新增 9 个）
- ✅ 对含有专有名词的文档，能自动提取实体并存入 SQLite
- ✅ 搜索 "GPT-4" 时，能通过图查询找到 "OpenAI"、"RLHF" 等关联实体
- ✅ 前端设置页面有「知识图谱」开关，默认关闭
- ✅ 开关关闭时索引和搜索行为与现有完全一致
- ✅ 端到端可 demo：开启开关 → 索引文档 → 搜索 → 结果包含实体关系补充信息
