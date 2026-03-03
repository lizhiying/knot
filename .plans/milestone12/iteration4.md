Milestone: milestone12
Iteration: iteration4 - 全功能集成：OCR 模型下载 + Vision LLM 复用

Goal:
1. ✅ pageindex-rs 启用 knot-pdf 全部 features（pdfium, ocr_paddle, vision, layout_model, formula_model）
2. ✅ PaddleOCR 模型加入 knot-app 模型下载队列
3. ✅ knot-pdf Vision 复用 knot-app 已有的 OCRFlux-3B（llama-server 18080 端口）
4. ✅ PdfParser 从 knot-app 传入模型路径和 Vision API 配置

Tasks:
- [x] 1. pageindex-rs/Cargo.toml：knot-pdf 启用全部 features。
- [x] 2. pageindex-rs/src/lib.rs：PageIndexConfig 新增 pdf_ocr_model_dir 字段 + 更新 builder。
- [x] 3. pageindex-rs/src/formats/pdf.rs：build_pdf_config 补充 OCR 模型路径、Vision API 和 VLM 映射。
- [x] 4. knot-app models/config.rs：添加 PaddleOCR 模型的 HuggingFace URL 映射。
- [x] 5. knot-app main.rs start_download_queue：加入 OCR 模型下载（det.onnx, rec.onnx, ppocrv5_dict.txt）。
- [x] 6. knot-app main.rs parse_file：自动检测配置 PDF OCR + Vision 指向 llama-server 18080。
- [x] 7. 修复所有 PageIndexConfig 构造处（md.rs ×2, dispatcher.rs ×2, test_parse.rs）。
- [x] 8. 编译验证：pageindex-rs ✅, knot-core ✅, knot-app ✅。测试：9/9 通过。

Exit criteria:
1. ✅ cargo build 全部通过（pageindex-rs, knot-core, knot-app）。
2. ✅ start_download_queue 包含 PaddleOCR 模型（ppocrv5/det.onnx, rec.onnx, ppocrv5_dict.txt）。
3. ✅ parse_file 时 PdfParser 自动检测并使用 OCRFlux-3B Vision + PaddleOCR。

## 架构总览

```
knot-app (parse_file)
  │
  ├── ModelPathManager → ~/.knot/models/ppocrv5/ (OCR 模型路径)
  ├── parsing_llm (llama-server:18080 + OCRFlux-3B)
  │     └── VisionDescriber API: /v1/chat/completions
  │
  └── PageIndexConfig
        ├── pdf_ocr_enabled: true (if det.onnx exists)
        ├── pdf_ocr_model_dir: "~/.knot/models/ppocrv5"
        ├── pdf_vision_api_url: "http://127.0.0.1:18080/v1/chat/completions"
        └── pdf_vision_model: "OCRFlux-3B"
              │
              └── PdfParser → Pipeline (knot-pdf)
                    ├── PaddleOCR (det.onnx + rec.onnx) → 扫描件文字识别
                    ├── VisionDescriber (OCRFlux-3B) → 图表语义理解
                    ├── VLM (OCRFlux-3B) → 低质量页面回退
                    ├── PdfiumBackend → 结构化文本抽取
                    └── Layout/Formula → 版面检测 + 公式识别（待模型下载）
```

## 模型下载清单

| 模型文件                     | 来源                         | 大小(约) | 用途         |
| ---------------------------- | ---------------------------- | -------- | ------------ |
| `OCRFlux-3B.Q4_K_M.gguf`     | mradermacher/OCRFlux-3B-GGUF | ~2GB     | VLM 本体     |
| `OCRFlux-3B.mmproj-f16.gguf` | mradermacher/OCRFlux-3B-GGUF | ~200MB   | VLM 视觉投影 |
| `Qwen3-1.7B-Q4_K_M.gguf`     | unsloth/Qwen3-1.7B-GGUF      | ~1GB     | Chat LLM     |
| `ppocrv5/det.onnx`           | OpenPPOCR/PP-OCRv5           | ~5MB     | 文字检测     |
| `ppocrv5/rec.onnx`           | OpenPPOCR/PP-OCRv5           | ~14MB    | 文字识别     |
| `ppocrv5/ppocrv5_dict.txt`   | OpenPPOCR/PP-OCRv5           | ~200KB   | OCR 字典     |
