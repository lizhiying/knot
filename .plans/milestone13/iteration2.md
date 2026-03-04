Milestone: milestone13
Iteration: iteration2 - 创建 knot-parser 替代 pageindex-rs

Goal:
在 workspace 顶层创建 `knot-parser` crate，将 pageindex-rs 的全部逻辑迁入，
使用 `knot-markdown` 和 `knot-pdf` 作为依赖。完成后 knot-parser 与 pageindex-rs 功能等价。

Assumptions:
1. Iteration 1 已完成：knot-pdf 位于顶层，knot-markdown 可用
2. knot-parser 的 crate 名为 `knot-parser`（Rust 中为 `knot_parser`）
3. 迁移过程中不修改核心业务逻辑，仅做结构调整和引用更新
4. pageindex-rs 在此迭代中暂时保留（下一迭代删除）

Scope:
- 在 workspace 顶层创建 `knot-parser/` 目录和 `Cargo.toml`
- 将 `pageindex-rs/src/` 下的所有文件复制到 `knot-parser/src/`
- 更新 `formats/md.rs`：使用 `knot-markdown` 替代本地 pulldown-cmark 解析
- 更新 `core/tree_builder.rs`：使用 `knot-markdown::build_from_pages` 替代本地实现
- 更新 `formats/pdf.rs`：更新 knot-pdf 的路径引用
- 复制 examples/ 和 tests/
- 添加到 workspace members

Tasks:
- [x] 1. 创建 `knot-parser/Cargo.toml`：
      ```toml
      [package]
      name = "knot-parser"
      version = "0.1.0"
      edition = "2021"

      [dependencies]
      serde.workspace = true
      pulldown-cmark = "0.10"
      undoc = { version = "0.1", path = "../undoc", optional = true }
      thiserror = "2.0.18"
      async-trait = "0.1.89"
      regex = "1.10"
      serde_json = "1.0"
      uuid = { version = "1.0", features = ["v4"] }
      knot-pdf = { path = "../knot-pdf", features = [...] }
      knot-markdown = { path = "../knot-markdown" }

      [features]
      default = []
      office = ["dep:undoc"]
      vision = []
      ```
- [x] 2. 复制 `pageindex-rs/src/` 到 `knot-parser/src/`
- [x] 3. 修改 `knot-parser/src/formats/md.rs`：
      - 移除本地的 `parse_text()` 实现
      - 导入 `knot_markdown::parse_text` 和 `knot_markdown::MarkdownNode`
      - 添加 `MarkdownNode → PageNode` 转换逻辑
- [x] 4. 修改 `knot-parser/src/core/tree_builder.rs`：
      - 移除本地的 `build_from_pages()` 实现
      - 使用 `knot_markdown::build_from_pages`
      - 添加 `MarkdownNode → PageNode` 转换
- [x] 5. 确认 `knot-parser/src/formats/pdf.rs` 中 `use knot_pdf::*` 依然正常工作
- [x] 6. 复制 `pageindex-rs/examples/` 和 `pageindex-rs/tests/` 到 `knot-parser/`
      - 更新其中的 `use pageindex_rs::*` 为 `use knot_parser::*`
- [x] 7. 将 `knot-parser` 添加到 workspace Cargo.toml 的 members 列表
- [x] 8. 编译验证：`cargo build -p knot-parser` 通过
- [x] 9. 测试验证：`cargo test -p knot-parser` 通过

Exit criteria:
1. `knot-parser/` 位于 workspace 顶层，crate name = "knot-parser"
2. `knot-parser` 包含原 pageindex-rs 的完整逻辑
3. `formats/md.rs` 依赖 `knot-markdown` 而不是本地 pulldown-cmark 解析
4. `core/tree_builder.rs` 依赖 `knot-markdown` 的 `build_from_pages`
5. `cargo build -p knot-parser` 编译通过
6. `cargo test -p knot-parser` 所有测试通过

## 注意事项

### 路径引用

knot-parser 引用其他 crate 的路径（都在同级目录）：
- `knot-pdf = { path = "../knot-pdf" }`
- `knot-markdown = { path = "../knot-markdown" }`
- `undoc = { path = "../undoc" }`

### MarkdownNode → PageNode 转换

需要在 `knot-parser` 中实现一个转换函数：

```rust
impl From<knot_markdown::MarkdownNode> for PageNode {
    fn from(mn: knot_markdown::MarkdownNode) -> Self {
        PageNode {
            node_id: mn.node_id,
            title: mn.title,
            level: mn.level,
            content: mn.content,
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: String::new(),
                page_number: None,
                line_number: None,
                token_count: mn.token_count,
                extra: HashMap::new(),
            },
            children: mn.children.into_iter().map(PageNode::from).collect(),
        }
    }
}
```
