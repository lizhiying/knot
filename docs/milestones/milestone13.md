# Milestone 13: 统一解析器 — pageindex-rs → knot-parser 重构（扁平结构）

## 目标

将 `pageindex-rs` 的全部代码迁移为独立的 `knot-parser` crate，同时：
- 将 Markdown 解析提取为独立的 `knot-markdown` crate
- 将 `knot-pdf` 从 `knot-parser/knot-pdf` 移到顶层 `knot-pdf/`
- 所有解析器 crate 扁平放置在 workspace 根目录

完成后：
- ❌ `pageindex-rs/` 目录将被删除
- ❌ `knot-parser/knot-pdf/` 嵌套结构将被解除
- ✅ `knot-parser/` 成为统一的文档解析入口（替代原 `pageindex-rs`）
- ✅ `knot-pdf/` 独立存在于 workspace 顶层
- ✅ `knot-markdown/` 独立存在于 workspace 顶层

## 成功标准

1. `pageindex-rs/` 目录被完全移除
2. `knot-parser` crate 对外暴露与原 `pageindex-rs` 相同的公共 API
3. `knot-markdown` 提供 Markdown 解析能力，被 `knot-parser` 依赖
4. `knot-pdf` 位于 workspace 顶层，内容不变
5. `knot-app`、`knot-core`、`knot-cli` 等下游依赖全部更新
6. 整个 workspace 编译通过，所有现有测试通过

## 当前结构

```
knot-workspaces/
├── pageindex-rs/           ← 🔴 即将被移除
│   ├── Cargo.toml          (name = "pageindex-rs")
│   └── src/
│       ├── lib.rs           (PageNode, traits, config 等核心类型)
│       ├── core/
│       │   ├── dispatcher.rs (IndexDispatcher)
│       │   └── tree_builder.rs (SemanticTreeBuilder)
│       ├── formats/
│       │   ├── md.rs        (MarkdownParser)  ← 🟡 提取到 knot-markdown
│       │   ├── pdf.rs       (PdfParser)
│       │   └── docx.rs      (DocxParser)
│       └── vision/
│           └── mod.rs       (placeholder)
├── knot-parser/            ← 🟡 目前只是 knot-pdf 的父目录
│   └── knot-pdf/           ← � 将移到顶层
└── Cargo.toml               (workspace)
```

## 目标结构

```
knot-workspaces/
├── knot-parser/             ← 🟢 新 crate（替代 pageindex-rs）
│   ├── Cargo.toml           (name = "knot-parser")
│   ├── src/
│   │   ├── lib.rs
│   │   ├── core/
│   │   ├── formats/
│   │   └── vision/
│   ├── examples/
│   └── tests/
├── knot-pdf/                ← 🟢 移到顶层，内容不变
│   ├── Cargo.toml
│   └── src/
├── knot-markdown/           ← 🟢 新建
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── knot-core/
├── knot-app/
├── knot-cli/
└── Cargo.toml               (workspace，成员更新)
```

## 下游依赖影响

| 下游 crate           | 当前依赖                                  | 更新后                                   |
| -------------------- | ----------------------------------------- | ---------------------------------------- |
| `knot-app/src-tauri` | `pageindex-rs` (features: office, vision) | `knot-parser` (features: office, vision) |
| `knot-cli`           | `pageindex-rs`                            | `knot-parser`                            |
| `knot-core`          | `pageindex-rs` (features: office)         | `knot-parser` (features: office)         |

代码中所有 `use pageindex_rs::*` 需改为 `use knot_parser::*`。

## 迭代概览

| 迭代        | 名称                               | 核心目标                                         |
| ----------- | ---------------------------------- | ------------------------------------------------ |
| Iteration 1 | 移动 knot-pdf & 创建 knot-markdown | 将 knot-pdf 提升到顶层，创建 knot-markdown crate |
| Iteration 2 | 创建 knot-parser 替代 pageindex-rs | 将 pageindex-rs 代码迁入顶层 knot-parser         |
| Iteration 3 | 更新下游依赖 & 清理                | 更新所有引用、删除 pageindex-rs、验证测试        |
