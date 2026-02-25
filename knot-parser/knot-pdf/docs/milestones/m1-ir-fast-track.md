# M1：IR + Fast Track 基础文本 + Markdown/RAG 导出

## 目标

构建项目基础骨架，定义统一 IR 数据结构，实现 born-digital PDF 的基础文本抽取，并提供 Markdown 和 RAG 扁平化导出能力。完成后即可对嵌入文本 PDF 进行基本解析和检索。

## 依赖

无（第一个里程碑）

## 交付物

- 可编译运行的 `knot-pdf` crate
- 完整的 IR 类型定义（含 serde）
- 基础文本抽取 pipeline（born-digital）
- Markdown 渲染器 + RAG 扁平化导出

---

## Checklist

### 1. 基础工程

- [x] 建立 crate：`knot-pdf`
- [x] feature 设计：
  - [x] `default`（纯解析）
  - [x] `async`（tokio）
  - [x] `store_sled`
  - [x] `pdfium`（渲染增强，可选，占位）
  - [x] `ocr_tesseract` 或 `ocr_leptess`（可选）
- [x] CI 配置：
  - [x] fmt / clippy / test
  - [x] 最小样本解析测试（小 PDF，集成测试中程序化生成）

### 2. 统一 IR 定义

- [x] 定义 `DocumentIR`
  - [x] `doc_id`（hash）
  - [x] `metadata`（title / author / created…尽力）
  - [x] `outline`（如果能拿到）
  - [x] `pages`（可 streaming 输出）
  - [x] `diagnostics`（全局）
- [x] 定义 `PageIR`
  - [x] `page_index`
  - [x] `size` / `rotation`
  - [x] `blocks: Vec<BlockIR>`
  - [x] `tables: Vec<TableIR>`（可为空）
  - [x] `images: Vec<ImageIR>`
  - [x] `diagnostics: PageDiagnostics`
  - [x] `text_score`（0-1）
  - [x] `is_scanned_guess`（bool）
  - [x] `source`（born_digital / ocr / mixed）
  - [x] `timings`（extract / render / ocr）
- [x] 定义 `BlockIR`
  - [x] `bbox`
  - [x] `role`（Body / Header / Footer / Title / List / Caption / Unknown）
  - [x] `lines` / `spans`（尽可能保留 font_size / bold）
  - [x] `normalized_text`（用于索引）
- [x] 定义 `TableIR`
  - [x] `table_id`
  - [x] `page_index`
  - [x] `bbox`
  - [x] `extraction_mode`（ruled / stream / unknown）
  - [x] `headers`（可为空但要有）
  - [x] `rows`（行数组）
  - [x] `cells`（二维或稀疏 map）
  - [x] `cell_types`（number / text / percent / currency / date / unknown）
  - [x] `fallback_text`（必须有）
- [x] 定义 `ImageIR`
  - [x] `image_id`
  - [x] `page_index`
  - [x] `bbox`
  - [x] `format`（png / jpg / unknown）
  - [x] `bytes_ref`（可选：延迟加载）
  - [x] `caption_refs`（关联文本块 id）
- [x] ID 体系（`doc_id` / `page_id` / `table_id`）
- [x] serde 序列化 / 反序列化
- [x] IR 单元测试（13 个测试覆盖所有 IR 类型）

### 3. Fast Track 文本抽取

- [x] `PdfBackend` trait 定义
  - [x] `extract_chars`
  - [x] `extract_blocks`（通过 layout 模块 `build_blocks` 间接实现）
  - [x] `extract_images`
- [x] 实现一个默认后端（优先 bbox / char 信息）
- [x] 基础阅读顺序重建：
  - [x] 行聚类（y 坐标）
  - [x] 列检测（x 分布）
  - [x] 段落合并（行距 / 缩进）
- [x] 图片 / 图形区域检测（至少给出 bbox 和引用）

### 4. 渲染与导出

- [x] `MarkdownRenderer`（最低可读输出）
- [x] RAG 扁平化导出：
  - [x] `block_lines`（带 page / bbox 引用）
  - [x] `table_row_lines`（占位，M3 完善）
  - [x] `table_cell_lines`（占位，M3 完善）

### 5. 对外 API（基础版）

- [x] 同步 API：`parse_pdf(path, config) -> DocumentIR`
- [x] 迭代器 API：`parse_pdf_pages(path, config) -> impl Iterator<Item=PageIR>`
- [x] 基础 `Config` 结构体

### 6. 测试

- [x] 单元测试：IR 序列化 / 反序列化（13 个 serde 往返测试）
- [x] 集成测试：至少 2 份 born-digital PDF 解析正确（6 个集成测试）
- [x] 输出 IR JSON 并人工检查（`test_ir_json_output_quality` + `test_document_ir_json_output`）

---

## 完成标准

- [x] `knot-pdf` crate 可编译、CI 通过
- [x] 对 born-digital PDF 能输出完整的 `DocumentIR`（含 blocks）
- [x] IR 可序列化为 JSON
- [x] Markdown 渲染器能输出可读文本
- [x] RAG 扁平化导出能输出 block_lines

---

## 完成总结

> **状态：✅ 全部完成**

### 最后一轮收尾工作

| # | 任务 | 完成情况 |
|---|------|---------|
| 1 | **`pdfium` feature 定义** | 在 `Cargo.toml` 中添加了 `pdfium = []` 占位 feature |
| 2 | **IR 单元测试** | 13 个测试：serde 往返（Document/Page/Block/Table/Image）、BBox 几何逻辑、枚举默认值、Table 导出方法、TextLine 辅助方法 |
| 3 | **集成测试** | 6 个测试：单页 PDF 解析、多页 PDF 解析、迭代器 API、错误处理、Config serde、IR JSON 输出质量 |
| 4 | **IR JSON 输出检查** | `test_document_ir_json_output` + `test_ir_json_output_quality` 验证 JSON 结构完整性和可读性 |
| 5 | **CI 配置** | `.github/workflows/ci.yml`：fmt / clippy / test，支持 ubuntu + macos |
| 6 | **代码格式化** | `cargo fmt` 修复所有格式问题 |
| 7 | **Clippy 警告修复** | 修复 `needless_borrow` 警告，`-D warnings` 零错误 |

### 验证结果

```
✅ cargo fmt --all -- --check     → 通过
✅ cargo clippy -- -D warnings    → 0 警告 0 错误
✅ cargo test --all-features      → 20/20 测试通过
   ├── 13 个单元测试 (ir_tests.rs)
   ├── 6 个集成测试 (integration_tests.rs)
   └── 1 个 doc-test
```

### 新增文件

- `tests/ir_tests.rs` — IR 类型单元测试
- `tests/integration_tests.rs` — born-digital PDF 集成测试
- `.github/workflows/ci.yml` — GitHub Actions CI 配置

