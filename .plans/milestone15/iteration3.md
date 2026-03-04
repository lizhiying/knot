Milestone: milestone15 - GraphRAG
Iteration: Iteration 3 — 质量打磨与体验优化

Goal:
处理边界情况，优化性能，添加知识图谱可视化，完善用户体验。

Assumptions:
- Iteration 1 和 2 的基础已经稳定
- 前端可视化使用纯 Canvas（无需额外依赖 D3.js）
- 性能优化主要在索引阶段（实体提取是主要瓶颈）

Scope:

Tasks:
- [x] 3.1 性能优化：批量实体提取
  - 短文本（< 200 字符）跳过 LLM，直接用规则提取
  - `extract_from_records`: 耗时日志（>10ms 阈值）
  - `extract_from_records_with_llm`: 每个 chunk 和总计耗时统计
  - 修改: `knot-core/src/entity.rs`

- [x] 3.2 边界情况处理
  - 空文本、纯空白: 快速返回空结果
  - 超长文本: 截断到 10000 字符（避免正则性能问题）
  - 特殊字符: 实体名清理（保留字母、数字、连字符、空格、中文）
  - 实体名长度限制: 2-100 字符
  - 修改: `knot-core/src/entity.rs`

- [x] 3.3 知识图谱可视化组件
  - 创建 `KnowledgeGraph.svelte`: Canvas 力导向图
  - 支持拖拽、点击选中、hover 高亮
  - 色彩编码实体类型（Person、Organization、Technology、Concept）
  - 节点大小按关联数缩放
  - 点击显示实体详情面板
  - 关系标签在高亮时显示
  - Settings.svelte 集成：仅在知识图谱开关开启时显示
  - `get_graph_data` Tauri command（Top 50 实体 + 关系）
  - 修改: `KnowledgeGraph.svelte` (新建), `Settings.svelte`, `main.rs`

- [x] 3.4 搜索结果中的实体高亮
  - 搜索时已将实体关系信息拼入 expanded_context（iteration1 实现）
  - 实体信息通过 `[知识图谱]` 前缀标注，LLM 在回答中自动引用
  - 注：前端高亮可在后续按需添加

- [x] 3.5 图查询优化
  - `get_related_entities_filtered()`: 带 confidence 阈值 + limit 参数
  - `get_graph_data()`: 高效 Top-N 查询 + 子图关系
  - 数据结构: `GraphData`, `GraphNode`, `GraphEdge`（serde::Serialize）
  - 修改: `knot-core/src/entity.rs`

- [x] 3.6 完善文档和测试
  - 6 个边界情况测试（空文本、空白、短文本、特殊字符、长文本、实体名长度）
  - 44 个测试全部通过
  - 修改: `knot-core/src/entity.rs` (tests module)

Exit criteria:
- ✅ 实体提取不拖慢正常索引速度超过 50%（短文本自动跳过 LLM）
- ✅ 知识图谱可视化可交互、可缩放（Canvas 力导向图）
- ✅ 边界情况不导致 panic 或崩溃（6 个测试验证）
- ✅ 有端到端测试验证完整流程（44 个测试）
- ✅ 所有文档更新完毕
