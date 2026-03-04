Milestone: milestone15 - GraphRAG
Iteration: Iteration 1 — 端到端最小可用

Goal:
用最简单的方式跑通实体提取→存储→查询→拼入搜索结果的完整流程。
实体提取可以用基于规则的方式（正则 + 启发式），不依赖 LLM，确保流程能跑通。

Assumptions:
- 实体提取先用规则方式（正则匹配中英文专有名词），后续迭代再接入 LLM
- 存储用 Tantivy 索引（轻量级，不引入新依赖）
- 关系暂时只提取 "共现"（两个实体出现在同一段落 = 有关联）
- 查询结果直接拼入 expanded_context，复用现有字段

Scope:

Tasks:
- [ ] 1.1 创建 `knot-core/src/entity.rs` 模块
  - 定义 `EntityRecord` 和 `RelationRecord` 数据结构
  - 实现 `extract_entities_rule_based()`: 用正则提取中英文专有名词（大写开头、引号内术语）
  - 实现 `extract_cooccurrence_relations()`: 同一段落内的实体对生成共现关系
  - 修改: 新建 `knot-core/src/entity.rs`，修改 `knot-core/src/lib.rs` 导出模块

- [ ] 1.2 在 `KnotStore` 中添加实体存储
  - 创建 entities Tantivy 索引 schema (entity_id, entity_type, name, source_file, chunk_id)
  - 创建 relations Tantivy 索引 schema (from_entity, to_entity, relation_type, source_file)
  - 实现 `add_entities()` 和 `add_relations()` 写入方法
  - 修改: `knot-core/src/store.rs`

- [ ] 1.3 在 `index_file()` 中集成实体提取
  - 在 flatten_tree 之后，对每个 VectorRecord 的 text 调用 entity 提取
  - 将提取结果通过 store.add_entities/add_relations 入库
  - 修改: `knot-core/src/index.rs`

- [ ] 1.4 实现图查询方法
  - `get_related_entities(entity_name)`: 查找与给定实体相关的其他实体
  - `get_entity_context(entity_name)`: 返回实体的来源文本片段
  - 修改: `knot-core/src/store.rs`

- [ ] 1.5 在搜索流程中集成图查询
  - 从用户查询中提取实体（复用 extract_entities）
  - 查找相关实体和关系
  - 将实体关系信息拼入 expanded_context
  - AppConfig 新增 `graph_rag_enabled`（默认 false，因为是实验性功能）
  - 修改: `knot-app/src-tauri/src/main.rs`

- [ ] 1.6 添加基础测试
  - 测试规则提取：英文专有名词、中文引号术语
  - 测试共现关系提取
  - 测试实体去重
  - 修改: `knot-core/src/entity.rs` (tests module)

Exit criteria:
- 编译通过，现有 18 个测试不受影响
- 对含有专有名词的文档，能自动提取实体并存入索引
- 搜索 "GPT-4" 时，能通过图查询找到 "OpenAI"、"RLHF" 等关联实体
- graph_rag_enabled 开关可通过 AppConfig 控制
- 端到端可 demo：索引文档 → 搜索 → 结果包含实体关系补充信息
