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
- [x] 2.1 LLM 实体提取 prompt 设计
  - 设计 few-shot ChatML prompt 模板（`build_entity_extraction_prompt`）
  - 支持 7 种实体类型和 7 种关系类型
  - 文本超过 1500 字符自动截断
  - 修改: `knot-core/src/entity.rs`

- [x] 2.2 实现 LLM 混合提取方法
  - `extract_from_records_with_llm()`: 接受泛型 async 函数作为 LLM 调用器
  - LLM 失败时自动降级到规则提取，并打印统计日志
  - `parse_llm_entity_response()`: 从 LLM 响应中提取 JSON（支持直接 JSON、markdown 代码块、嵌入式 JSON）
  - `extract_json_from_response()`: 容错的 JSON 提取（括号匹配）
  - 修改: `knot-core/src/entity.rs`

- [x] 2.3 丰富关系类型
  - `RelationType` 枚举：7 种关系（co-occurrence, developed-by, uses, belongs-to, compared-with, caused-by, followed-by）
  - 支持别名映射（如 "created-by" → developed-by, "vs" → compared-with）
  - 修改: `knot-core/src/entity.rs`

- [x] 2.4 实体去重与合并
  - `dedup_entities()`: 同 entity_id 合并，类型升级（Concept → 具体类型）
  - `EntityGraph::top_entities()`: 按关系数量排名的热门实体
  - `EntityGraph::relation_type_stats()`: 关系类型统计
  - 初始扫描和文件监控都在写入前 dedup
  - 修改: `knot-core/src/entity.rs`, `knot-app/src-tauri/src/main.rs`

- [x] 2.5 增量更新支持
  - 已在 iteration1 中实现（watch 循环中 `delete_by_file` + 重新提取）
  - iteration2 增强：LLM 提取时从 AppState 获取 parsing_client

- [x] 2.6 补充测试（11 个新测试）
  - RelationType 枚举：roundtrip 和 alias 测试
  - LLM JSON 解析：有效 JSON、markdown code block、无效响应、空实体、带噪声的 JSON
  - 实体去重：合并升级、保留不同实体
  - Prompt 构建：内容正确性、长文本截断
  - 修改: `knot-core/src/entity.rs` (tests module)

Exit criteria:
- ✅ LLM 提取对常见文档能输出有意义的实体和关系（有 LLM 时自动使用）
- ✅ 关系类型覆盖 7 种（≥ 5 种要求）
- ✅ 文件更新后实体图能正确增量刷新（delete + re-extract）
- ✅ 所有新增测试通过（38 个总计）
