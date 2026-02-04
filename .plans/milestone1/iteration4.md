Milestone: milestone1
Iteration: iteration4

Goal: 集成真实模型 (Embedding + LLM) 并跑通完整的 RAG 回答流程。
Assumptions: Mock Embedding 已验证流程，现在需要替换为真实语义向量。本地需要有 LLM 运行环境 (Ollama) 或 API Key。
Scope: Real Embedding, LLM Integration, Prompt Engineering, Schema Refinement.

Tasks:
- [x] **架构重构 (Architecture Refactor)**:
    - [x] Update `knot-core/Cargo.toml`: Add `ort`, `tokenizers`, `reqwest`, `serde`, `serde_json`.
    - [x] Move `knot-app/src-tauri/src/engine/embedding.rs` to `knot-core/src/embedding.rs`.
    - [x] Move `knot-app/src-tauri/src/engine/llm.rs` to `knot-core/src/llm.rs`.
    - [x] Make `knot-app` depend on `knot-core`.
- [x] **逻辑下沉 (Logic Submersion)**:
    - [x] Refactor `knot-core::store` and `indexer`:
        - [x] Accept `data_dir` in `new()`.
        - [x] Store `knot.db` and `knot_index.lance` inside `data_dir`.
    - [x] Expose `EmbeddingEngine` and `LlamaClient` from `knot-core`.
- [x] **验证 (Verification)**:
    - [x] `knot-cli` should use real models from `knot-core` (Architecture ready).
    - [x] `knot-app` should compile using `knot-core`.
- [ ] **完善 Schema 字段**:
    - [ ] 向 `VectorRecord` 添加 `workspace_id`, `file_type`, `chunk_hash`, `index_version` 等缺失字段.
    - [ ] 确保 `store.rs` 适配新 Schema.
- [ ] **Watcher 优化**:
    - [ ] 实现 Debounce 防抖逻辑 (避免频繁触发索引).
- [ ] **完善 Schema 字段**:
    - [ ] 向 `VectorRecord` 添加 `workspace_id`, `file_type`, `chunk_hash`, `index_version` 等缺失字段.
    - [ ] 确保 `store.rs` 适配新 Schema.
- [ ] **Watcher 优化**:
    - [ ] 实现 Debounce 防抖逻辑 (避免频繁触发索引).

Exit criteria:
- `index` 命令生成的向量不再是全0或随机，而是真实的 float array.
- `query` 命令能针对问题生成一段流畅的自然语言回答 (Answer based on Context).
- 向量数据库 Schema 包含所有规划字段.
## Verification Steps

### 1. 架构与依赖验证
检查 `knot-core` 是否包含移动后的逻辑，以及 `knot-app` 是否移除了旧代码。
```bash
# 1. 检查文件移动
ls knot-core/src/embedding.rs knot-core/src/llm.rs knot-core/src/manager.rs
# 预期: 文件存在

# 2. 检查旧文件移除
ls knot-app/src-tauri/src/engine
# 预期: "No such file or directory"

# 3. 检查依赖关系
cat knot-app/src-tauri/Cargo.toml | grep "knot-core"
# 预期: knot-core = { path = "../../knot-core" }
```

### 2. 编译验证
确保重构后，核心库和应用都能正常编译。
```bash
# 验证 Core
cargo check -p knot-core

# 验证 CLI (集成测试)
cargo check -p knot-cli

# 验证 App (关键路径)
cargo check -p knot-app
```

### 3. 数据路径验证 (Runtime)
验证 `knot-cli` 的 `--data-dir` 参数是否生效。
```bash
# 创建测试数据目录
mkdir -p test_data_v4

# 运行索引 (指定 data-dir)
cargo run --bin knot-cli -- --data-dir ./test_data_v4 index --input ./sample_docs

# 检查输出文件
ls -l test_data_v4/knot.db
ls -d test_data_v4/knot_index.lance
# 预期: 数据库文件生成在指定目录，而不是当前目录。
```
