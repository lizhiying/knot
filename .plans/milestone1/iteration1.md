Milestone: milestone1
Iteration: iteration1

Goal: 实现一个即刻可运行的端到端 RAG 流程，验证核心组件（pageindex-rs -> LanceDB -> Search）的连通性。
Assumptions: 暂时全量重建索引，不考虑增量。主要关注 Happy Path。
Scope: 文件扫描、基础解析、向量入库、基础向量检索、LLM 简单回答。

Tasks:
- [x] 定义 Core Data Structures (Record, Config)
- [x] 实现 Basic Watcher (一次性扫描指定目录)
- [x] 集成 `pageindex-rs` 解析 Markdown/代码文件为 PageNode
- [x] 实现 Flatten Logic: 将 PageNode 树转为 VectorRecord 列表 (仅 Leaf Nodes)
- [x] 集成 `lancedb` crate: 创建表并写入 Vectors
- [x] 实现 Basic Retriever: Embedding -> Vector Search
- [x] 此迭代验证: 这是一个由 CLI 驱动的 Demo，输入问题能得到基于文档的回答

Exit criteria:
- `knot-cli index <dir>` 成功生成 `.lance` 数据。
- `knot-cli query <q>` 能返回相关的文本片段。

Command:
cargo run --bin knot-cli -- index --input ./sample_docs
cargo run --bin knot-cli -- query --text "hello"
