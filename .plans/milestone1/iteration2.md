Milestone: milestone1
Iteration: iteration2

Goal: 引入状态管理和结构化上下文，支持增量更新，大幅提升索引效率和检索上下文质量。
Assumptions: Iteration 1 已完成，LanceDB 读写正常。
Scope: SQLite Registry, Hash Check, Parent-Child Context, Breadcrumbs.

Tasks:
- [x] 引入 `sqlx` + `sqlite`: 创建 `file_registry` 表
- [x] 实现 File Hash 计算与比对逻辑 (Skipping unchanged files)
- [x] 实现 Node-level Diff Logic (基于 `node_id` + `content_hash`)
- [x] 优化 Flatten Logic: 注入 Parent Context 和 Breadcrumbs 到 VectorRecord
- [x] 实现 Delete Logic: 处理文件删除或移动的情况
- [x] 集成 `notify` crate: 监听文件系统变更事件


Exit criteria:
- [x] 修改一个文件，Index 仅处理该文件（看日志）。
- [x] 检索结果中包含 `breadcrumbs` 信息（如 "Doc > Chapter 1 > Section A"）。

## Verification Steps (How to Verify)

### 1. 验证增量更新 (Incremental Updates)
```bash
# 第一次运行（全量索引）
cargo run --bin knot-cli -- index --input ./sample_docs
# 预期输出: "Indexing: ..." (所有文件)

# 第二次运行（无修改）
cargo run --bin knot-cli -- index --input ./sample_docs
# 预期输出: "Skipping unchanged: ..." (所有文件，Found 0 vectors)

# 修改文件后运行
echo "\nChange" >> ./sample_docs/hello.md
cargo run --bin knot-cli -- index --input ./sample_docs
# 预期输出: "Indexing: .../hello.md" (仅该文件)
```

### 2. 验证上下文 (Breadcrumbs)
```bash
cargo run --bin knot-cli -- query --text "hello"
# 预期输出包含: "Context: hello > Hello World"
```

### 3. 验证删除 (Deletion)
```bash
rm ./sample_docs/hello.md
cargo run --bin knot-cli -- index --input ./sample_docs
# 预期输出: "Found 1 files to delete", "Deleting from store..."
```

### 4. 验证 Watch 模式
```bash
cargo run --bin knot-cli -- watch --input ./sample_docs
# 在另一个终端修改或新建文件，观察 CLI 输出 "Change detected" 并自动触发索引更新。
```
