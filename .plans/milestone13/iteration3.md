Milestone: milestone13
Iteration: iteration3 - 更新下游依赖、删除 pageindex-rs、全量验证

Goal:
更新所有下游 crate 的依赖引用，将 `pageindex-rs` 改为 `knot-parser`，
删除 `pageindex-rs/` 目录，确保整个 workspace 编译和测试通过。

Assumptions:
1. Iteration 1 和 Iteration 2 已完成
2. `knot-parser` crate 已可用且测试通过
3. 下游 crate（knot-app、knot-core、knot-cli）只需修改依赖声明和 `use` 语句

Scope:
- 更新 `knot-app/src-tauri/Cargo.toml`
- 更新 `knot-cli/Cargo.toml`
- 更新 `knot-core/Cargo.toml`
- 批量替换所有 `use pageindex_rs::*` 为 `use knot_parser::*`
- 更新 workspace Cargo.toml：移除 `pageindex-rs` 成员
- 删除 `pageindex-rs/` 目录
- 全量编译和测试验证

Tasks:
- [x] 1. 更新 `knot-app/src-tauri/Cargo.toml`：
      ```toml
      # 原：pageindex-rs = { path = "../../pageindex-rs", features = ["office", "vision"] }
      # 新：knot-parser = { path = "../../knot-parser", features = ["office", "vision"] }
      ```
- [x] 2. 更新 `knot-cli/Cargo.toml`：
      ```toml
      # 原：pageindex-rs = { path = "../pageindex-rs" }
      # 新：knot-parser = { path = "../knot-parser" }
      ```
- [x] 3. 更新 `knot-core/Cargo.toml`：
      ```toml
      # 原：pageindex-rs = { path = "../pageindex-rs", features = ["office"] }
      # 新：knot-parser = { path = "../knot-parser", features = ["office"] }
      ```
- [x] 4. 批量替换代码中的引用：
      - `knot-app/src-tauri/src/main.rs`：约 8 处 `pageindex_rs`
      - `knot-app/src-tauri/src/eval_api.rs`：约 3 处 `pageindex_rs`
      - `knot-core/src/index.rs`：约 4 处 `pageindex_rs`
      - `knot-core/src/mock_embedding.rs`：1 处 `pageindex_rs`
      - `knot-core/src/embedding.rs`：约 2 处 `pageindex_rs`
      - `knot-core/src/llm.rs`：1 处 `pageindex_rs`
      - `knot-cli/src/main.rs`：约 3 处 `pageindex_rs`
      所有 `use pageindex_rs::` → `use knot_parser::`
      所有 `pageindex_rs::` → `knot_parser::`
- [x] 5. 更新 workspace `Cargo.toml`：
      ```toml
      [workspace]
      members = [
          "knot-parser",
          "knot-pdf",
          "knot-markdown",
          "knot-app/src-tauri",
          "undoc",
          "knot-core",
          "knot-cli",
      ]
      ```
- [x] 6. 删除 `pageindex-rs/` 目录
- [x] 7. 全量编译验证：`cargo build --workspace` 通过
- [x] 8. 全量测试验证：`cargo test --workspace` 通过

Exit criteria:
1. ❌ `pageindex-rs/` 目录已删除
2. ✅ 整个 workspace 中无任何对 `pageindex-rs` / `pageindex_rs` 的引用
3. ✅ `cargo build --workspace` 编译通过
4. ✅ `cargo test -p knot-parser` 测试通过
5. ✅ `cargo test -p knot-core` 测试通过

## 影响清单

### 需要修改的文件列表

| 文件                                 | 修改类型            |
| ------------------------------------ | ------------------- |
| `knot-app/src-tauri/Cargo.toml`      | 依赖名变更          |
| `knot-app/src-tauri/src/main.rs`     | use 语句替换 (~8处) |
| `knot-app/src-tauri/src/eval_api.rs` | use 语句替换 (~3处) |
| `knot-cli/Cargo.toml`                | 依赖名变更          |
| `knot-cli/src/main.rs`               | use 语句替换 (~3处) |
| `knot-core/Cargo.toml`               | 依赖名变更          |
| `knot-core/src/index.rs`             | use 语句替换 (~4处) |
| `knot-core/src/mock_embedding.rs`    | use 语句替换 (1处)  |
| `knot-core/src/embedding.rs`         | use 语句替换 (~2处) |
| `knot-core/src/llm.rs`               | use 语句替换 (1处)  |
| `Cargo.toml` (workspace)             | 成员变更            |
| `pageindex-rs/`                      | 删除整个目录        |

### 风险点

1. **knot-app 编译**：Tauri 项目的编译链较复杂，可能需要特别注意
2. **feature flag 兼容**：确保 `knot-parser` 的 features（office, vision）与原 `pageindex-rs` 一致
3. **git history**：删除 pageindex-rs 后 git blame 可能受影响，但代码历史保留在 git 中
