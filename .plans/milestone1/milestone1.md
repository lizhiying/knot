# Milestone: milestone1

**Goal:** Create a high-performance, disk-based Local-First RAG Engine capable of handling 10k+ documents.
**Success Metric:** System can index a directory of documents, persist them to disk (LanceDB), and answer user queries with <3s latency and high accuracy on a standard laptop.

## Constraints
- **Local-First:** No reliance on cloud vector DBs.
- **Memory Efficient:** Must not load full index into RAM (use Disk-based LanceDB).
- **Rust Native:** Core logic in Rust for integration with Tauri.

## Iterations
- **[iteration1](./iteration1.md): End-to-End Vertical Slice** - Basic Scan -> Index -> Search flow.
- **[iteration2](./iteration2.md): Incremental Updates & Context** - Smart updates & Richer Metadata.
- **[iteration3](./iteration3.md): Quality & Optimization** - Hybrid Search, Rerank, & Perf Tuning.
