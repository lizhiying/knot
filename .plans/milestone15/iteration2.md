Milestone: milestone15 - GraphRAG
Iteration: Iteration 2 — 提升质量与覆盖度

Goal:
用 LLM 替代规则提取，提升实体识别的准确率和关系类型的丰富度。
支持增量更新（文件修改时只重提取受影响的实体）。

Assumptions:
- 本地 LLM（Qwen3 0.6B）的实体提取能力可能有限，需要设计好 prompt
- 如果本地 LLM 不够好，可以降级为 "规则 + LLM 混合" 模式
- 增量更新基于 file_path 粒度（整个文件重提取）

Scope:

Tasks:
- [ ] 2.1 LLM 实体提取 prompt 设计
  - 设计 few-shot prompt 模板，让 LLM 输出结构化的 (entity, type, relation) JSON
  - 测试不同 prompt 在 Qwen3 上的效果
  - 确定降级策略（LLM 提取失败时回退到规则提取）
  - 修改: `knot-core/src/entity.rs`

- [ ] 2.2 实现 `extract_entities_llm()` 方法
  - 调用本地 LLM 进行实体和关系提取
  - 解析 LLM 输出的 JSON 结果
  - 错误处理和超时控制
  - 修改: `knot-core/src/entity.rs`

- [ ] 2.3 丰富关系类型
  - 从简单的 "共现" 扩展为: 开发者、使用技术、属于分类、对比、因果、时序 等
  - 定义 RelationType 枚举
  - 修改: `knot-core/src/entity.rs`

- [ ] 2.4 实体去重与合并
  - 同名实体（不同大小写、别名）的去重和合并
  - 实体出现频次统计
  - 修改: `knot-core/src/entity.rs`, `knot-core/src/store.rs`

- [ ] 2.5 增量更新支持
  - 文件修改时，先删除该文件旧的实体和关系，再重新提取
  - 复用现有的文件变更检测机制
  - 修改: `knot-core/src/store.rs`, `knot-core/src/index.rs`

- [ ] 2.6 补充测试
  - LLM 提取的 mock 测试
  - 实体合并和去重测试
  - 增量更新正确性测试
  - 修改: `knot-core/src/entity.rs`, `knot-core/src/store.rs`

Exit criteria:
- LLM 提取对常见文档能输出有意义的实体和关系
- 关系类型覆盖 ≥ 5 种
- 文件更新后实体图能正确增量刷新
- 所有新增测试通过
