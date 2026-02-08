Milestone: cli-independence
Iteration: 1 - 端到端流程验证

Goal:
让 `knot-cli` 可以使用真实 Embedding 模型完成 index → query 流程，
验证核心架构可行性。

Assumptions:
- Embedding 模型文件已存在于 ~/.knot/models/ (手动放置或通过 knot-app 下载)
- 暂不实现 download 命令，手动放置模型
- 暂不实现 ask 命令 (LLM 部分)

Scope:
- 集成 EmbeddingEngine 到 knot-cli
- 实现 status 命令显示基本状态
- 修复 query 向量维度问题
- 验证 index + query 端到端流程

Tasks:
- [x] 添加 ort 依赖到 knot-cli/Cargo.toml
- [x] 创建 CLI 专用的 EmbeddingProvider 实现 (复用 knot-core 的 ThreadSafeEmbeddingEngine)
- [x] 修改 index 命令使用真实 Embedding (替换 MockEmbedding)
- [x] 修复 query 命令向量维度 (384 → 512)
- [x] 实现 status 命令 (显示模型状态、索引统计)
- [ ] 端到端测试: index + query 返回有意义结果

Exit criteria:
- `knot-cli status` 正确显示模型和索引状态
- `knot-cli index -i ~/test` 成功索引文档
- `knot-cli query -t "测试查询"` 返回相关结果 (Score > 0)
