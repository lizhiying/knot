Milestone: milestone15 - GraphRAG
Iteration: Iteration 3 — 质量打磨与体验优化

Goal:
处理边界情况，优化性能，添加知识图谱可视化，完善用户体验。

Assumptions:
- Iteration 1 和 2 的基础已经稳定
- 前端可视化使用轻量级图库（如 D3.js force-directed graph）
- 性能优化主要在索引阶段（实体提取是主要瓶颈）

Scope:

Tasks:
- [ ] 3.1 性能优化：批量实体提取
  - 将多个文本片段合并批量发送给 LLM，减少调用次数
  - 实体提取异步化，不阻塞主索引流程
  - 添加索引阶段的实体提取耗时日志
  - 修改: `knot-core/src/index.rs`, `knot-core/src/entity.rs`

- [ ] 3.2 边界情况处理
  - 空文档、纯图片文档的实体提取
  - 超长文本的分段提取
  - 多语言实体的处理（中英混合）
  - 特殊字符实体名的转义
  - 修改: `knot-core/src/entity.rs`

- [ ] 3.3 知识图谱可视化组件
  - 创建 `KnowledgeGraph.svelte` 组件
  - 使用 D3.js 或 vis.js 渲染力导向图
  - 支持点击实体查看详情和来源
  - 添加 Tauri command 获取图数据
  - 修改: `knot-app/src/lib/components/KnowledgeGraph.svelte` (新建),
    `knot-app/src-tauri/src/main.rs`

- [ ] 3.4 搜索结果中的实体高亮
  - 在搜索结果展示中，识别并高亮实体词
  - 悬浮显示实体类型和关联
  - 修改: `knot-app/src/lib/components/`

- [ ] 3.5 图查询优化
  - 缓存常用实体查询结果
  - 限制图遍历深度（最多 2 跳）
  - 添加 relation.confidence 权重过滤
  - 修改: `knot-core/src/store.rs`

- [ ] 3.6 完善文档和测试
  - 更新 docs/rag-limitations-analysis.md 标记 P4 进展
  - 端到端集成测试
  - 性能基准测试（索引 100 个文档的实体提取耗时）
  - 修改: 多个文件

Exit criteria:
- 实体提取不拖慢正常索引速度超过 50%
- 知识图谱可视化可交互、可缩放
- 边界情况不导致 panic 或崩溃
- 有端到端测试验证完整流程
- 所有文档更新完毕
