Milestone: milestone13
Iteration: iteration1 - 移动 knot-pdf 到顶层 & 创建 knot-markdown crate

Goal:
1. 将 `knot-parser/knot-pdf` 移动到 workspace 顶层 `knot-pdf/`，解除嵌套结构
2. 创建 `knot-markdown` crate，提取 Markdown 解析逻辑
3. 清理旧的 `knot-parser/` 目录（移走 knot-pdf 后该目录为空）

Assumptions:
1. 移动 knot-pdf 只是目录位置变化，crate 内部代码不需要修改
2. 移动后需更新所有引用 knot-pdf 路径的 Cargo.toml
3. `knot-markdown` 只负责 Markdown 解析，定义自己的轻量 `MarkdownNode` 类型
4. `pulldown-cmark` 是 Markdown 解析的唯一核心依赖

Scope:
- 将 `knot-parser/knot-pdf/` 整体移动到 `knot-pdf/`
- 删除空的 `knot-parser/` 目录
- 更新 workspace Cargo.toml 中 knot-pdf 的成员路径
- 更新 pageindex-rs/Cargo.toml 中 knot-pdf 的 path 引用
- 创建 `knot-markdown/` crate
- 确保编译通过

Tasks:
- [x] 1. 移动 knot-pdf 到顶层：
      ```bash
      mv knot-parser/knot-pdf ./knot-pdf
      rm -rf knot-parser/   # 移走后该目录为空
      ```
- [x] 2. 更新 workspace `Cargo.toml`：
      ```toml
      # 原：  "knot-parser/knot-pdf"
      # 新：  "knot-pdf"
      ```
- [x] 3. 更新 `pageindex-rs/Cargo.toml` 中 knot-pdf 的路径：
      ```toml
      # 原：  knot-pdf = { path = "../knot-parser/knot-pdf", ... }
      # 新：  knot-pdf = { path = "../knot-pdf", ... }
      ```
- [x] 4. 编译验证移动成功：`cargo build -p knot-pdf` 和 `cargo build -p pageindex-rs` 通过
- [x] 5. 创建 `knot-markdown/Cargo.toml`：
      ```toml
      [package]
      name = "knot-markdown"
      version = "0.1.0"
      edition = "2021"

      [dependencies]
      pulldown-cmark = "0.10"
      serde = { version = "1.0", features = ["derive"] }
      thiserror = "2.0"
      ```
- [x] 6. 创建 `knot-markdown/src/lib.rs`：
      - 定义 `MarkdownNode` 结构体（node_id, title, level, content, children, token_count）
      - 定义 `MarkdownError` 错误类型
      - 迁移 `parse_text()` 函数（从 pageindex-rs/src/formats/md.rs）
      - 迁移 `build_from_pages()` 函数（从 pageindex-rs/src/core/tree_builder.rs）
      - 迁移辅助函数 `heading_level_to_u32`, `count_tokens`
- [x] 7. 将 `knot-markdown` 添加到 workspace Cargo.toml 的 members 列表
- [x] 8. 编写单元测试：迁移原有的 `test_markdown_hierarchy` 和 `test_h3_only_headings`
- [x] 9. 编译验证：`cargo build -p knot-markdown` 和 `cargo test -p knot-markdown` 通过（3 tests + 1 doc-test）

Exit criteria:
1. `knot-pdf/` 位于 workspace 顶层，编译通过
2. 旧的 `knot-parser/` 目录已删除
3. `knot-markdown/` 位于 workspace 顶层，包含完整的 Markdown 解析逻辑
4. `cargo build -p knot-pdf` 通过
5. `cargo build -p knot-markdown` 通过
6. `cargo test -p knot-markdown` 通过（至少 2 个测试）
7. `cargo build -p pageindex-rs` 通过（路径更新后仍可编译）

## 关键设计决策

### MarkdownNode 的定义

```rust
pub struct MarkdownNode {
    pub node_id: String,
    pub title: String,
    pub level: u32,
    pub content: String,
    pub token_count: usize,
    pub children: Vec<MarkdownNode>,
}
```

不包含 `embedding`、`summary`、`page_number` 等字段。
这些由上层 `knot-parser` 在转换为 `PageNode` 时填充。

### knot-pdf 移动后的路径变化

| 引用方                  | 原路径                    | 新路径        |
| ----------------------- | ------------------------- | ------------- |
| workspace Cargo.toml    | `knot-parser/knot-pdf`    | `knot-pdf`    |
| pageindex-rs/Cargo.toml | `../knot-parser/knot-pdf` | `../knot-pdf` |
