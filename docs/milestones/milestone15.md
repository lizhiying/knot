# Milestone 15: GraphRAG — 知识图谱增强 RAG

## 目标

在现有 RAG 系统（向量搜索 + 关键词搜索 + 上下文扩展 + 多跳检索）基础上，引入知识图谱能力，从文档中提取实体和关系，构建结构化知识网络，支持复杂的推理查询（因果链、比较、聚合）。

## 成功指标

- 能对索引文档自动提取实体（人物、组织、概念、技术）和关系
- 搜索时能利用实体关系进行图查询，补充向量搜索结果
- 对"A 和 B 有什么关系"、"哪些技术被用于 X 领域"等关系型查询有显著提升

## 前置条件

- ✅ Milestone 14 已完成（P0 上下文扩展 + P1 文档摘要 + P2 多跳检索）
- 需要本地 LLM 支持实体提取（现有 Qwen3 0.6B 可能能力有限）

## 迭代规划

### Iteration 1: 端到端最小可用 (vertical slice)

目标: 用最简单的方式跑通"实体提取 → 存储 → 查询 → 拼入搜索结果"的完整流程，即使实体提取质量很粗糙。

详见 [iteration1.md](../../.plans/milestone15/iteration1.md)

### Iteration 2: 提升质量与覆盖度

目标: 提升实体提取的质量，完善关系类型，支持增量更新，改善图查询的召回率。

详见 [iteration2.md](../../.plans/milestone15/iteration2.md)

### Iteration 3: 质量打磨与体验优化

目标: 处理边界情况、性能调优、前端可视化、完善测试。

详见 [iteration3.md](../../.plans/milestone15/iteration3.md)

## 架构设计

```
文档解析流程 (index_file):
  PageNode 树
    → 现有: flatten → VectorRecord → LanceDB + Tantivy
    → 新增: extract_entities() → EntityRecord → 实体图存储

搜索流程 (search):
  用户查询
    → 现有: 向量搜索 + 关键词搜索 + RRF + 多跳 + 上下文扩展
    → 新增: 实体识别 → 图查询 → 关系链补充 → 合并入结果
```

### 实体图数据模型

```
EntityRecord:
  entity_id: String     # 唯一标识，如 "GPT-4"
  entity_type: String   # "Person" | "Organization" | "Technology" | "Concept"
  name: String          # 显示名称
  source_file: String   # 来源文件
  source_chunk_id: String  # 来源 VectorRecord ID

RelationRecord:
  from_entity: String   # entity_id
  to_entity: String     # entity_id
  relation_type: String # "开发者" | "使用" | "属于" | "对比" | ...
  source_file: String
  confidence: f32       # 置信度
```

### 存储方案

首选方案: **用 Tantivy 索引存储实体和关系**（轻量级，不引入新依赖）
- 实体表: Tantivy 索引 (entity_id, entity_type, name, source_file)
- 关系表: Tantivy 索引 (from_entity, to_entity, relation_type, source_file)
- 图遍历: 多次 Tantivy 查询实现 1-2 跳

备选方案: SQLite（如果查询模式更复杂）

## 涉及文件

| 文件                                                | 变更类型 | Iteration | 说明                       |
| --------------------------------------------------- | -------- | --------- | -------------------------- |
| `knot-core/src/store.rs`                            | 待修改   | 1         | 实体/关系存储和查询方法    |
| `knot-core/src/index.rs`                            | 待修改   | 1         | 实体提取逻辑集成           |
| `knot-core/src/entity.rs`                           | 新建     | 1         | 实体提取、关系提取核心模块 |
| `knot-app/src-tauri/src/main.rs`                    | 待修改   | 1         | 配置开关 + 搜索集成        |
| `knot-app/src/lib/components/Settings.svelte`       | 待修改   | 2         | GraphRAG 开关              |
| `knot-app/src/lib/components/KnowledgeGraph.svelte` | 新建     | 3         | 知识图谱可视化             |
