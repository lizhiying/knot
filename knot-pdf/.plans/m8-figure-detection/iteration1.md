# M8 Iteration 1：端到端流程打通

## 目标

打通从 Path object 提取 → 图表区域检测 → 裁剪渲染 → OCR 文字回填 → 文字块剔除 → Markdown 输出的完整流程。
先不追求检测精度，重点是架构正确、数据流通。

## 步骤

### Step 1：Backend 层 - 新增 RawPathObject + extract_path_objects

1. `traits.rs` 新增 `RawPathObject`、`PathObjectKind` 类型
2. `PdfBackend` trait 新增 `extract_path_objects()` 方法（默认空实现）
3. `PdfiumBackend` 实现：遍历 `page.objects()` 收集 `as_path_object()` 的 bbox

### Step 2：IR 层 - 扩展 ImageIR

1. `ir/types.rs` 新增 `ImageSource` 枚举
2. `ir/image.rs` 新增 `source`、`ocr_text` 字段（serde(default)）

### Step 3：Figure 检测模块 - 基础版本

1. 创建 `src/figure/mod.rs`
2. 创建 `src/figure/types.rs`：`FigureRegion` 结构体
3. 创建 `src/figure/detector.rs`：网格密度分析 + 连通区域合并
4. `lib.rs` 注册 `figure` 模块

### Step 4：OcrRenderer 扩展 - 裁剪渲染

1. `OcrRenderer` trait 新增 `render_region_to_image()` 方法
2. `PdfiumOcrRenderer` 实现裁剪渲染
3. `MockOcrRenderer` 空实现

### Step 5：Config - 新增配置项

1. 新增 `figure_detection_enabled`、`figure_min_area_ratio`、`figure_min_path_count`、`figure_render_width`

### Step 6：Pipeline 集成

1. `process_page()` 中调用 `extract_path_objects()` + `detect_figure_regions()`
2. 对图区域裁剪渲染 + OCR → 构建 ImageIR（source = FigureRegion）
3. 从 blocks 中剔除图区域内的文字块
4. Caption 检测（"Figure" / "Fig." / "图"）

### Step 7：Markdown + RAG 渲染器适配

1. `MarkdownRenderer` 区分 FigureRegion 和 Embedded
2. `RagExporter` 新增 Figure 类型

### Step 8：编译验证

1. `cargo build --features pdfium,cli` 编译通过
2. `cargo test` 测试通过

## 完成标准

- 编译通过，无新 warning
- 现有测试不被破坏
- 新增的图表检测流程在 pipeline 中正确触发
