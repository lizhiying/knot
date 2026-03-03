Milestone: milestone12
Iteration: iteration2 - OCR 支持与混合解析模式

Goal:
在 Iteration 1 的基础上，让 PdfParser 能够根据配置启用 OCR（扫描件回退）和混合解析模式。
- 低质量页面（text_score < 阈值）自动通过 knot-pdf 的 OCR 回退链处理
- 通过 feature gate 控制 OCR 依赖，不影响轻量编译

Assumptions:
1. ✅ knot-pdf 已内建多级回退链（pdfium → VLM → OCR → 空页面），只需正确传递配置即可。
2. ✅ pageindex-rs 的 PageIndexConfig 可以扩展字段。
3. ✅ knot-pdf Pipeline 的 Send/Sync 问题已在 Iteration 1 中确认，继续使用 parse_pdf() 方案。

Scope:
- ✅ 在 pageindex-rs 中添加 OCR 和 Vision 相关 feature gate。
- ✅ 扩展 PdfParser 支持可配置的 PdfConfig（OCR、Vision、页面过滤等）。
- ✅ 在 PageIndexConfig 中新增 PDF 专属配置选项。
- ✅ 从 PageIndexConfig 桥接到 knot-pdf 的 Vision LLM 和 OCR 配置。
- ✅ 编译验证 + 测试。

Tasks:
- [x] 1. pageindex-rs/Cargo.toml：添加 `pdf_ocr` 和 `pdf_vision` feature gate。
- [x] 2. pageindex-rs/src/lib.rs：在 PageIndexConfig 中新增 `pdf_ocr_enabled`、`pdf_vision_api_url`、`pdf_vision_model`、`pdf_page_indices` 字段 + builder 方法。
- [x] 3. pageindex-rs/src/formats/pdf.rs：实现 `build_pdf_config()` 从 PageIndexConfig 映射到 knot-pdf Config（OCR 模式、Vision API、页码过滤、超时和内存限制）。
- [x] 4. 修复所有直接构造 PageIndexConfig 的位置（md.rs 测试、dispatcher.rs 测试、test_parse.rs 示例、knot-app main.rs）。
- [x] 5. 编译验证 + 测试通过（pageindex-rs 7/7, knot-core 9/9）。

Exit criteria:
1. ✅ 无 OCR feature 时编译通过，PdfParser 正常工作。
2. ✅ 启用 `pdf_ocr` / `pdf_vision` feature 时编译通过，Config 正确传递。
3. ✅ 现有测试全部通过。

## 配置映射关系

| PageIndexConfig 字段 | knot-pdf Config 字段       | 说明                    |
| -------------------- | -------------------------- | ----------------------- |
| `pdf_ocr_enabled`    | `ocr_enabled` + `ocr_mode` | 为 true 时启用 Auto OCR |
| `pdf_vision_api_url` | `vision_api_url`           | OpenAI 兼容 API         |
| `pdf_vision_model`   | `vision_model`             | 如 "gpt-4o"             |
| `pdf_page_indices`   | `page_indices`             | 页码过滤（0-indexed）   |
| —                    | `page_timeout_secs: 30`    | 固定值，单页超时保护    |
| —                    | `max_memory_mb: 500`       | 固定值，内存峰值保护    |

## 使用示例

```rust
// 默认模式（快速纯文本抽取）
let config = PageIndexConfig::new();

// 启用 OCR（扫描件支持）
let config = PageIndexConfig::new().with_pdf_ocr();

// 启用 Vision LLM（复杂排版支持）
let config = PageIndexConfig::new()
    .with_pdf_vision("http://localhost:11434/v1/chat/completions", "gpt-4o");

// 仅解析前 5 页
let config = PageIndexConfig::new()
    .with_pdf_pages(vec![0, 1, 2, 3, 4]);
```
