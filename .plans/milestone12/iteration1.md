Milestone: milestone12
Iteration: iteration1 - 基础集成：重写 PdfParser 使用 knot-pdf

Goal:
替换 pageindex-rs 中现有的 LLM-OCR PDF 解析方案，使用 knot-pdf 作为默认后端。集成后 PDF 解析不再强依赖 LLM，100 页文档解析从 ~30 分钟降至 ~18 秒。

Assumptions:
1. ✅ knot-pdf 已在 workspace members 中，可直接通过 path 引用。
2. ✅ `SemanticTreeBuilder::build_from_pages()` 可直接复用，不需要修改。
3. ✅ knot-core 层面（KnotIndexer）不需要感知底层变化，因为输出仍是 PageNode 树。

Scope:
- ✅ 修改 pageindex-rs/Cargo.toml，添加 knot-pdf 依赖，移除可由 knot-pdf 传递的冗余依赖。
- ✅ 重写 pageindex-rs/src/formats/pdf.rs，使用 knot-pdf 的 parse_pdf 解析 PDF，经 MarkdownRenderer 渲染后，复用 SemanticTreeBuilder 构建 PageNode 树。
- ✅ 修改 knot-core/src/index.rs 的 index_directory()，支持 .pdf 文件的索引。
- ✅ 确保编译通过，现有测试不破坏。

Tasks:
- [x] 1. 修改 pageindex-rs/Cargo.toml：添加 `knot-pdf` 依赖（features = ["pdfium"]），移除 `lopdf`、`pdfium-render`、`base64`、`image` 直接依赖。
- [x] 2. 重写 pageindex-rs/src/formats/pdf.rs：使用 knot-pdf 的 parse_pdf + MarkdownRenderer 替换 LLM-OCR 方案。（296 行 → ~130 行）
- [x] 3. 修改 knot-core/src/index.rs：在 index_directory() 中添加 `.pdf` 扩展名支持。
- [x] 4. 编译验证：整个 workspace 编译通过（cargo build -p pageindex-rs && cargo build -p knot-core）。
- [x] 5. 运行现有测试：pageindex-rs 7 个测试全部通过，knot-core 9 个测试全部通过。

附加修复:
- [x] 6. 修复 knot-pdf 自身的编译错误：pipeline/mod.rs 第 1197 行 `vision_describer` 缺少 `#[cfg(feature = "vision")]` 保护。

Exit criteria:
1. ✅ `cargo build` 整个 workspace 编译通过。
2. ✅ pageindex-rs 的 PdfParser 使用 knot-pdf 作为后端。
3. ✅ knot-core 的 index_directory() 支持 .pdf 文件。
4. ✅ 现有测试通过（pageindex-rs: 7/7, knot-core: 9/9）。

## 技术决策记录

### 为什么不在 PdfParser 中持有 Pipeline 实例？

knot-pdf 的 `Pipeline` 内部包含 `OcrRenderer` 等组件，这些组件没有实现 `Sync` trait。
而 pageindex-rs 的 `DocumentParser` trait 要求 `Send + Sync`，导致持有 `Pipeline` 的 `PdfParser` 无法满足约束。

**解决方案**：使用 `parse_pdf()` 便捷函数（每次内部创建 Pipeline）。对于单文件解析场景（pageindex 的主要用例），
开销可忽略。批量处理场景可在未来在 KnotIndexer 层面直接复用 Pipeline。

### 依赖精简效果

| 依赖            | 集成前        | 集成后                      |
| --------------- | ------------- | --------------------------- |
| `lopdf`         | v0.32（直接） | 移除（knot-pdf 传递 v0.38） |
| `pdfium-render` | v0.8（直接）  | 移除（knot-pdf 传递）       |
| `base64`        | v0.21（直接） | 移除（knot-pdf 传递）       |
| `image`         | v0.25（直接） | 移除（knot-pdf 传递）       |
| `knot-pdf`      | 无            | **新增** path 引用          |
