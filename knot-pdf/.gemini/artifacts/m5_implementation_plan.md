# M5 实施计划：OCR Fallback + Store 断点续传

## 概览

M5 需要实现两个可选模块：
1. **OCR Fallback** — 对扫描页/文字不可用页进行 OCR，输出可检索文本
2. **Store 断点续传** — 基于 Store 抽象的缓存机制，支持大文档中断恢复

两者均受 feature gate 保护（`ocr_tesseract` / `store_sled`），不影响核心解析。

## 任务分解

### Task 1: 配置扩展 + IR 扩展
- 在 `Config` 中新增 OCR 和 Store 相关字段
- 新增 `OcrMode` 枚举
- `PageDiagnostics` 增加 OCR 相关字段
- **文件**: `src/config.rs`, `src/ir/types.rs`

### Task 2: OcrBackend Trait 定义 + OcrBlock 类型
- 定义 `OcrBackend` trait（`ocr_region`, `ocr_full_page`, `supported_languages`）
- 定义 `OcrBlock` 结构体
- trait 需要 `Send + Sync`
- **文件**: `src/ocr/mod.rs`, `src/ocr/traits.rs`

### Task 3: OCR 触发条件 + Pipeline 集成
- 基于 `PageScore` 判断是否需要 OCR
- 在 Pipeline 中集成 OCR 处理流程
- OCR 结果回填 `BlockIR`，更新 `PageIR`（source = Ocr/Mixed）
- **文件**: `src/ocr/trigger.rs`, `src/pipeline/mod.rs`

### Task 4: MockOcrBackend + TesseractBackend 默认实现
- Mock 实现（用于测试）
- Tesseract 实现（feature = `ocr_tesseract`，集成 leptess crate）
- **文件**: `src/ocr/mock.rs`, `src/ocr/tesseract.rs`

### Task 5: Store Trait 定义
- 定义 `Store` trait（`save_page`, `load_page`, `get_status`, etc.）
- 定义 `PageStatus` 枚举
- **文件**: `src/store/mod.rs`, `src/store/traits.rs`

### Task 6: SledStore 实现
- 基于 `sled` embedded DB 的 Store 实现
- Key 设计: `{doc_id}:{page_index}`
- feature = `store_sled`
- **文件**: `src/store/sled_store.rs`

### Task 7: 断点续传逻辑 + Pipeline 集成
- Pipeline 启动时检查 Store 状态
- 每页完成后写入 Store
- 失败页处理
- **文件**: `src/pipeline/mod.rs`

### Task 8: 错误类型扩展 + lib.rs 更新
- 增加 OCR / Store 相关错误变体
- 更新 `lib.rs` 导出

### Task 9: 测试
- OCR 单元测试（mock、触发条件、回填）
- Store 单元测试（save/load、状态流转、断点续传）
- **文件**: `tests/m5_tests.rs`

## 实施顺序

1 → 2 → 8 → 5 → 6 → 4 → 3 → 7 → 9
