Milestone: milestone1
Iteration: iteration3

Goal: 提升检索准确率（Hybrid + Rerank）和系统运行效率（Resource Control）。
Assumptions: 基础 RAG 功能完备，需要解决搜不到、搜不 준、卡顿问题。
Scope: Hybrid Search, Rerank, Throttling, Caching.

Tasks:
- [x] 集成 Full-Text Search (Keyword Boosting Substitution due to lancedb issue)
- [x] 实现 Hybrid Search Logic (加权融合 Vector + Keyword 分数)
- [x] 集成 Reranker (Client-side Keyword Scoring) 对 Top-K 结果重排序
- [x] 实现 Dynamic Threshold Logic (Score Boosting)
- [x] 实现 Indexing Throttling (Buffer Unordered Parallelism)
- [x] 实现 Embedding Caching (Based on File Hash Registry)


Exit criteria:
- [x] 搜索专有名词（如 Error Code）能命中。
- [x] 只有相关性高的结果才会被送给 LLM (Simulated by scoring).
- [x] 大量文件索引时 CPU/内存占用稳定 (Throttled parallel).

## Verification Steps

### 1. 验证 Hybrid Search & Reranking
```bash
# 确保 zebra.md 存在且包含 "Zebra"
echo "New File with unique keyword: Zebra" > sample_docs/zebra.md
# 重建索引
cargo run --bin knot-cli -- index --input ./sample_docs
# 搜索 Zebra
cargo run --bin knot-cli -- query --text "Zebra"
# 预期结果: zebra.md 分数显著高于其他 (>100.0，因 keyword boost), 即使 Vector 相似度一般。
```

### 2. 验证 Parallel Indexing
```bash
# 创建多个新文件
echo "Lion" > sample_docs/lion.md
echo "Tiger" > sample_docs/tiger.md
# 运行索引
cargo run --bin knot-cli -- index --input ./sample_docs
# 预期日志: 看到 "Start Indexing: ..." 几乎同时出现，或被 buffer 处理。
# 且 "Skipping unchanged" 对旧文件生效。
```
