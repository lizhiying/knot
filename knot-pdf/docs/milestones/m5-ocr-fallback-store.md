# M5：OCR Fallback（可选）+ Store 断点续传

## 目标

实现两个可选特性：1）离线 OCR 兜底，对扫描页/文字不可用页进行 region-level OCR，输出可检索文本；2）基于 Store 抽象的断点续传与缓存机制，支持大文档中断恢复。两者均为可选 feature，不影响核心解析能力。

## 依赖

- M1（IR 定义 + Fast Track）
- M2（PageScore，用于触发 OCR 判断）

## 交付物

- [x] `OcrBackend` trait + 默认实现
- [x] OCR fallback pipeline（渲染 + OCR + 回填 IR）
- [x] `Store` trait + `SledStore` 实现
- [x] 断点续传逻辑
- [x] feature gate：`ocr_paddle` / `store_sled`

---

## OCR 后端技术选型

### 目标

OCR 必须内置到 app 中，**不要求用户额外安装 Tesseract 等系统依赖**，需跨平台支持 macOS / Linux / Windows。

### 候选方案对比

| 维度           | ① `pure-onnx-ocr`                                                              | ② `kreuzberg-tesseract`                                     | ③ `ocr-rs`                              |
| -------------- | ------------------------------------------------------------------------------ | ----------------------------------------------------------- | --------------------------------------- |
| **核心原理**   | Pure Rust 重新实现 PaddleOCR（DBNet + SVTR），使用 `tract-onnx` 推理 ONNX 模型 | 构建时从源码编译 Tesseract/Leptonica 并**静态链接**进二进制 | Pure Rust 基于 PaddleOCR + MNN 推理框架 |
| **系统依赖**   | ❌ 零依赖，`cargo build` 即可                                                   | ❌ 零运行时依赖（构建时自动编译 C++）                        | ❌ 零依赖                                |
| **中文支持**   | ✅ PaddleOCR PP-OCRv5 模型，中/英/日/韩                                         | ✅ Tesseract 原生多语言                                      | ✅ PaddleOCR 模型                        |
| **跨平台**     | ✅ Mac/Linux/Windows/WASM                                                       | ✅ Mac/Linux/Windows                                         | ✅ Mac/Linux/Windows                     |
| **模型文件**   | 需分发 `det.onnx` + `rec.onnx` (~20-50MB)                                      | 需分发 `.traineddata` 语言包                                | 需分发 MNN 模型                         |
| **构建复杂度** | ⭐ 低（纯 Rust）                                                                | ⭐⭐⭐ 高（首次编译 C++ >5分钟）                               | ⭐⭐ 中等                                 |
| **二进制体积** | 中等（模型文件外挂）                                                           | 较大（静态链接整个 Tesseract）                              | 中等                                    |

### 最终选型：`pure-onnx-ocr`（方案①）

理由：
- **Pure Rust 零依赖**：用户只需 `cargo build`，无需安装任何系统库。
- **PaddleOCR 模型**：天然中英文支持，对中文 PDF 场景优于 Tesseract。
- **跨平台无忧**：Mac/Linux/Windows 甚至 WASM 均可编译。
- **API 清晰**：`OcrEngineBuilder` → `OcrEngine` → `run_from_image()`，与 `OcrBackend` trait 适配。
- **模型文件**可通过 app bundle 或 `include_bytes!` 嵌入。

**feature 名称变更**：`ocr_tesseract` → `ocr_paddle`

---

## Checklist

### 1. OcrBackend Trait 定义

- [x] 定义 `OcrBackend` trait：
  - [x] `ocr_region(image: &[u8], bbox: BBox) -> Vec<OcrBlock>`
  - [x] `ocr_full_page(image: &[u8]) -> Vec<OcrBlock>`
  - [x] `supported_languages() -> Vec<String>`
- [x] 定义 `OcrBlock` 结构体：
  - [x] `text: String`
  - [x] `bbox: BBox`（近似位置）
  - [x] `confidence: f32`（若 OCR 引擎支持）
- [x] trait 设计支持 `Send + Sync`（并发安全）

### 2. OCR 触发条件

- [x] 基于 `PageScore` 的自动触发：
  - [x] `PageScore.score < config.scoring.text_threshold` → 触发 OCR
  - [x] `PageScore.is_scanned_guess == true` → 触发 OCR
- [x] 用户强制模式：
  - [x] `config.ocr.mode = Auto | ForceAll | Disabled`
- [x] 按页粒度决策（不做整本 PDF 无脑 OCR）

### 3. Region 渲染策略

- [x] 页面渲染为图片：
  - [x] 默认渲染宽度 512px（可配置 `config.ocr.render_width`）
  - [x] 仅渲染需要 OCR 的页面
- [ ] Region-level OCR（优先）：（后续优化，当前整页 OCR 已满足需求）
  - [ ] 根据文本缺失区域划分 OCR region
  - [ ] 对每个 region 单独 OCR（减少无效识别）
- [x] 整页 OCR（降级）：
  - [x] 无法确定 region 时，整页低分辨率 OCR
- [x] 渲染后立即释放图片内存（不缓存）

### 4. 默认 OCR 后端实现

- [x] ~~`TesseractBackend`（feature = `ocr_tesseract`）~~ **已废弃，迁移至 PaddleOCR**
- [x] `PaddleOcrBackend`（feature = `ocr_paddle`）：
  - [x] 集成 `pure_onnx_ocr` crate（Pure Rust，零系统依赖）
  - [x] 支持中英文识别（PaddleOCR PP-OCRv5 模型）
  - [x] 模型文件通过 app bundle 分发
- [x] OCR 输出标准化为 `OcrBlock`

### 5. OCR 输出回填 IR

- [x] OCR result 写入 `BlockIR`：
  - [x] `role = Body`（OCR 文本默认为正文）
  - [x] `bbox` 为 OCR 引擎返回的近似位置
  - [x] `normalized_text` 填充
- [x] `PageIR` 更新：
  - [x] `source = Ocr | Mixed`
  - [x] `timings.ocr_ms` 记录耗时
  - [x] `is_scanned_guess = true`
- [x] OCR 质量评分：
  - [x] 基于 OCR 输出的可打印率 / 字数 / 置信度
  - [x] 写入 `PageDiagnostics`

### 6. OCR 资源控制

- [x] 并发限流：
  - [x] `max_ocr_workers` 配置控制 OCR 并发数（Pipeline 中实现）
  - [x] 默认 OCR 并发 = 1（同步模式天然串行，async 模式预留 Semaphore 接口）
- [x] 内存限制：
  - [x] 渲染图片使用后立即 drop
  - [x] OCR 队列有界（同步模式下天然有界——逐页处理，不积压队列）

### 7. Store Trait 定义

- [x] 定义 `Store` trait：
  - [x] `save_page(doc_id, page_index, page_ir) -> Result<()>`
  - [x] `load_page(doc_id, page_index) -> Result<Option<PageIR>>`
  - [x] `get_status(doc_id, page_index) -> PageStatus`
  - [x] `save_diagnostics(doc_id, page_index, diagnostics)`
  - [x] `get_last_completed_page(doc_id) -> Option<usize>`
- [x] 定义 `PageStatus` 枚举：
  - [x] `NotStarted`
  - [x] `InProgress`
  - [x] `Done`
  - [x] `Failed(String)`

### 8. SledStore 实现

- [x] `SledStore`（feature = `store_sled`）：
  - [x] 基于 `sled` embedded DB
  - [x] key 设计：`{doc_id}:{page_index}`
  - [x] value：`PageIR` 的 serde 序列化（可选压缩）
- [x] 存储内容：
  - [x] 每页 `PageIR` blob
  - [x] 每页 `PageStatus`
  - [x] 每页 `PageDiagnostics`
  - [x] 每页耗时信息
- [x] 配置：`config.store.store_path`

### 9. 断点续传逻辑

- [x] Pipeline 启动时检查 Store：
  - [x] 查询 `get_last_completed_page(doc_id)`
  - [x] 从下一页开始继续解析
- [x] 每页完成后写入 Store：
  - [x] `save_page` + 更新 `PageStatus = Done`
- [x] 失败页处理：
  - [x] 标记 `PageStatus = Failed`
  - [x] 跳过失败页继续后续页
  - [x] 失败信息写入 diagnostics
- [x] `doc_id` 计算：基于文件 hash（确保同一文件可恢复）

### 10. 配置扩展

- [x] `Config` 新增字段：
  - [x] `ocr_enabled: bool`（默认 false）
  - [x] `ocr_mode: OcrMode`（Auto / ForceAll / Disabled）
  - [x] `ocr_languages: Vec<String>`
  - [x] `ocr_render_width: u32`（默认 512）
  - [x] `store_enabled: bool`（默认 false）
  - [x] `store_path: PathBuf`
  - [x] `ocr_workers: usize`（默认 1）
  - [x] `ocr_model_dir: Option<PathBuf>`

### 11. 测试

- [x] OCR 单元测试：
  - [x] OcrBackend trait mock 实现
  - [x] OCR 触发条件逻辑
  - [x] OCR 结果回填 BlockIR 正确
  - [x] OCR Mixed 源回填验证
  - [x] OCR 质量评分计算
- [x] Store 单元测试：
  - [x] save / load PageIR 往返正确
  - [x] PageStatus 状态流转
  - [x] 断点续传：模拟中断后恢复
- [x] 集成测试：
  - [x] 多页断点续传：前 5 页完成 + 第 6 页失败后从正确位置恢复
  - [x] Store 重启后数据持久化验证
  - [x] Failed 页状态保留验证
- [x] 资源控制测试：
  - [x] OCR 串行执行验证（同步模式最大并发 = 1）
  - [x] 渲染图片使用后内存释放
  - [x] Pipeline ocr_workers 配置下限 clamped to 1

---

## 实现笔记

### Phase 1（已完成）：核心抽象 + TesseractBackend

1. **核心抽象**:
   - `src/ocr/traits.rs`: 定义 `OcrBackend` 接口。
   - `src/store/traits.rs`: 定义 `Store` 接口及 `PageStatus` 枚举。

2. **具体后端 (v1)**:
   - `src/ocr/tesseract.rs`: 基于 `leptess` 的 Tesseract 实现（feature: `ocr_tesseract`）**→ 已废弃**。
   - `src/ocr/mock.rs`: 用于 CI 的 Mock 实现。
   - `src/store/sled_store.rs`: 基于 `sled` 的持久化存储实现（feature: `store_sled`）。

3. **流程集成**:
   - `src/ocr/trigger.rs`: 封装 `should_trigger_ocr` 逻辑。
   - `src/ocr/integration.rs`: OCR 结果回填至 `PageIR`。
   - `src/pipeline/mod.rs`: 断点续传 + OCR 触发 + 状态跟踪。

### Phase 2（✅ 已完成）：迁移至 PaddleOCR（pure-onnx-ocr）

1. **替换依赖**：`leptess` → `pure_onnx_ocr`
2. **新增 `PaddleOcrBackend`**：实现 `OcrBackend` trait（使用 Mutex 包装解决 Send+Sync）
3. **Feature 重命名**：新增 `ocr_paddle`（保留 `ocr_tesseract` 向后兼容）
4. **Pipeline 更新**：优先使用 PaddleOCR → Tesseract → Mock 的降级策略
5. **模型文件管理**：通过 `config.ocr_model_dir` 指定模型目录
6. **模型文件已下载**：`models/ppocrv5/det.onnx`(84MB) + `rec.onnx`(81MB) + `ppocrv5_dict.txt`(72KB)

### Phase 3（✅ 已完成）：集成测试 + 并发限流

1. **Pipeline 添加 `max_ocr_workers`**：受 `config.ocr_workers` 控制，最小为 1
2. **断点续传集成测试**：多页完成 + 失败页 + Store 重启后恢复
3. **OCR 并发控制测试**：验证同步模式串行执行
4. **内存释放测试**：图片数据 drop 语义验证

---

## 完成标准

- [x] OCR 开启时，扫描页能输出可检索文本（即使结构一般）
- [x] OCR 仅对需要的页面启用，不做全文档无脑 OCR
- [x] Store 断点续传：中断后可从指定页继续
- [x] `doc_id` 基于文件 hash，同一文件可恢复
- [x] OCR 和 Store 均为可选 feature，不启用时零开销
- [x] OCR 后端迁移至 pure-onnx-ocr，无需用户安装系统依赖
- [x] 所有单元测试通过，CI 通过

## 遗留优化项（非阻塞）

- [ ] Region-level OCR：根据文本缺失区域划分 OCR region，减少无效识别
- [ ] async 模式下使用 tokio::sync::Semaphore 实现真正的并发限流
- [ ] 5 份真实扫描件 PDF 端到端 OCR 验证
