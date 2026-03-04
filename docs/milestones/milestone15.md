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

## 配置开关

知识图谱为**实验性功能**，通过设置页面的开关控制，**默认关闭**。

```rust
// knot-app/src-tauri/src/main.rs
struct AppConfig {
    // ... existing fields ...
    /// 是否启用知识图谱（实验性功能）
    #[serde(default = "default_graph_rag_enabled")]
    graph_rag_enabled: bool,
}

fn default_graph_rag_enabled() -> bool {
    false // 默认关闭，实验性功能
}
```

**开关影响范围：**

| 阶段 | 开关关闭                       | 开关开启                                   |
| ---- | ------------------------------ | ------------------------------------------ |
| 索引 | 不提取实体，不创建 SQLite 数据 | 对每个文档提取实体和关系，写入 SQLite      |
| 搜索 | 完全跳过图查询，行为与现有一致 | 从查询中提取实体 → 图查询 → 补充关联上下文 |

**前端设置页面：**
- 位于 LLM 配置区域，"多跳检索"开关下方
- 标签：「知识图谱」
- 副标签：「实验性功能，自动提取文档中的实体和关系，增强关联查询」
- 配套 Tauri command: `set_graph_rag_enabled`

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

**使用 SQLite（`rusqlite` bundled 模式）**

选型理由：
- Knot 是 PC 端全盘索引工具，实体量可达百万级，纯内存方案（如 petgraph）内存占用过大
- SQLite 是桌面应用标配（Chrome、VS Code 都用），单文件、零外部依赖
- `bundled` 模式将 SQLite C 源码直接编译进二进制，用户无需安装任何东西
- `WITH RECURSIVE` 原生支持多跳图遍历，无需手动拼接
- 增量更新友好：`DELETE WHERE source_file = ?` + `INSERT` 事务性操作

```toml
# knot-core/Cargo.toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
```

**数据库 Schema：**

```sql
-- entities 表
CREATE TABLE IF NOT EXISTS entities (
    entity_id   TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    entity_type TEXT NOT NULL,    -- Person | Organization | Technology | Concept
    source_file TEXT NOT NULL,
    chunk_id    TEXT
);
CREATE INDEX IF NOT EXISTS idx_entity_name ON entities(name);
CREATE INDEX IF NOT EXISTS idx_entity_file ON entities(source_file);
CREATE INDEX IF NOT EXISTS idx_entity_type ON entities(entity_type);

-- relations 表
CREATE TABLE IF NOT EXISTS relations (
    from_entity   TEXT NOT NULL,
    to_entity     TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    source_file   TEXT NOT NULL,
    confidence    REAL DEFAULT 1.0,
    PRIMARY KEY (from_entity, to_entity, relation_type)
);
CREATE INDEX IF NOT EXISTS idx_rel_from ON relations(from_entity);
CREATE INDEX IF NOT EXISTS idx_rel_to   ON relations(to_entity);
CREATE INDEX IF NOT EXISTS idx_rel_file ON relations(source_file);
```

**图查询示例：**

```sql
-- 1 跳：查找 GPT-4 的所有关联实体（<1ms）
SELECT r.to_entity, r.relation_type, e.entity_type
FROM relations r JOIN entities e ON r.to_entity = e.entity_id
WHERE r.from_entity = 'GPT-4';

-- 2 跳：递归查找（<5ms）
WITH RECURSIVE hops AS (
    SELECT to_entity, relation_type, 1 AS depth
    FROM relations WHERE from_entity = 'GPT-4'
    UNION ALL
    SELECT r.to_entity, r.relation_type, h.depth + 1
    FROM relations r JOIN hops h ON r.from_entity = h.to_entity
    WHERE h.depth < 2
)
SELECT DISTINCT to_entity, relation_type, depth FROM hops;

-- 增量更新：文件重索引时清除旧数据
DELETE FROM entities  WHERE source_file = '/path/to/updated.md';
DELETE FROM relations WHERE source_file = '/path/to/updated.md';
-- 然后重新 INSERT 新提取的实体和关系
```

**规模预估：**

| 场景     | 文档数  | 实体数    | 关系数    | SQLite 文件大小 |
| -------- | ------- | --------- | --------- | --------------- |
| 轻度用户 | 1,000   | 10,000    | 50,000    | ~5 MB           |
| 中度用户 | 10,000  | 100,000   | 500,000   | ~50 MB          |
| 重度用户 | 100,000 | 1,000,000 | 5,000,000 | ~500 MB         |

## 涉及文件

| 文件                                                | 变更类型 | Iteration | 说明                       |
| --------------------------------------------------- | -------- | --------- | -------------------------- |
| `knot-core/src/store.rs`                            | 待修改   | 1         | 实体/关系存储和查询方法    |
| `knot-core/src/index.rs`                            | 待修改   | 1         | 实体提取逻辑集成           |
| `knot-core/src/entity.rs`                           | 新建     | 1         | 实体提取、关系提取核心模块 |
| `knot-app/src-tauri/src/main.rs`                    | 待修改   | 1         | 配置开关 + 搜索集成        |
| `knot-app/src/lib/components/Settings.svelte`       | 待修改   | 2         | GraphRAG 开关              |
| `knot-app/src/lib/components/KnowledgeGraph.svelte` | 新建     | 3         | 知识图谱可视化             |
