Milestone: milestone1
Iteration: iteration3

Goal: 提升检索准确率（Hybrid + Rerank）和系统运行效率（Resource Control）。
Assumptions: 基础 RAG 功能完备，需要解决搜不到、搜不 준、卡顿问题。
Scope: Hybrid Search, Rerank, Throttling, Caching.

Tasks:
- [ ] 集成 Full-Text Search (Tantivy 或 LanceDB FTS)
- [ ] 实现 Hybrid Search Logic (加权融合 Vector + Keyword 分数)
- [ ] 集成 Reranker (Cross-encoder 量化模型) 对 Top-K 结果重排序
- [ ] 实现 Dynamic Threshold Logic (自动拒答)
- [ ] 实现 Indexing Throttling (限制并发数)
- [ ] 实现 Embedding Caching (基于 chunk_hash)

Exit criteria:
- 搜索专有名词（如 Error Code）能命中。
- 只有相关性高的结果才会被送给 LLM。
- 大量文件索引时 CPU/内存占用稳定。
