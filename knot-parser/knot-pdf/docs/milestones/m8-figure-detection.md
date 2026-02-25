# M8：图表区域检测与 OCR 描述

## 目标

为 `knot-pdf` 增加对**矢量绘制图表**（如架构图、流程图、示意图等）的识别能力。这类图表不是嵌入的位图（Image XObject），而是由大量 PDF Path objects（矩形、线段、曲线）和零散文字标签构成。当前解析器会将图中的文字标签当作普通文本块输出，导致结果碎片化、无语义。

本里程碑的目标是：

1. **检测图表区域**：识别页面中 Path objects 密集的区域，判定为"图表"
2. **渲染为位图**：将图表区域裁剪渲染为 PNG 图片
3. **OCR 文字描述**：对渲染后的图片调用 OCR，提取图中文字作为描述
4. **过滤碎片文字**：将图表区域内的零散文字标签从正文 blocks 中剔除
5. **关联 Caption**：自动检测图表下方的 "Figure X: ..." 标题并关联

## 依赖

- M1 ~ M7（全部核心功能就绪）
- PDFium backend（Path object 遍历 + 页面渲染）

## 交付物

- [ ] `FigureRegion` 检测器（基于 Path object 密度分析）
- [ ] `OcrRenderer` 扩展：支持裁剪渲染（指定 BBox 区域）
- [ ] 图表区域 OCR 文字提取 + `ImageIR` 回填
- [ ] 图区域内文字块过滤（从 `blocks` 中剔除）
- [ ] Caption 自动关联
- [ ] `Config` 新增图表检测相关配置
- [ ] Markdown 渲染器支持图表输出
- [ ] 单元测试 + 集成测试

---

## Checklist

### 1. Backend 层：Path Object 统计

在 `PdfBackend` trait 新增方法，统计页面中 Path objects 的 bbox 信息：

```rust
/// 从 PDF 中提取的原始 Path object 信息（用于图表区域检测）
pub struct RawPathObject {
    pub bbox: BBox,
    /// Path 类型：线段/曲线/矩形/填充区域
    pub kind: PathObjectKind,
}

pub enum PathObjectKind {
    Line,
    Rect,
    Curve,
    Fill,
}

// PdfBackend trait 新增方法
fn extract_path_objects(&self, page_index: usize) -> Result<Vec<RawPathObject>, PdfError> {
    let _ = page_index;
    Ok(Vec::new()) // 默认空实现
}
```

- [ ] 在 `traits.rs` 中新增 `RawPathObject`、`PathObjectKind` 类型
- [ ] 在 `PdfBackend` trait 中新增 `extract_path_objects()` 默认实现
- [ ] 在 `PdfiumBackend` 中实现：遍历 `page.objects()`，收集 `as_path_object()` 的 bbox

### 2. 图表区域检测器（核心算法）

新增模块 `src/figure/mod.rs`，实现图表区域检测：

```
src/figure/
├── mod.rs          // 模块导出
├── detector.rs     // 图表区域检测算法
└── types.rs        // FigureRegion 类型定义
```

**检测算法概要**：

1. **网格化**：将页面划分为 N×M 的网格（例如 20×30）
2. **密度计算**：统计每个网格单元中 PathObject 的数量
3. **连通区域合并**：将相邻的高密度网格合并为连续的"图表候选区域"
4. **过滤规则**：
   - 最小面积阈值：区域面积 > 页面面积的 5%
   - 最小 Path objects 数量：> 10
   - 排除已识别的表格区域（通过 bbox 重叠检测）
   - 排除页面边缘的装饰线/边框
5. **文字标签检测**：统计候选区域内的文字块数量，如果区域内有多个零散小文字块（非连续段落），加分

```rust
/// 图表区域
pub struct FigureRegion {
    /// 区域 ID
    pub figure_id: String,
    /// 边界框
    pub bbox: BBox,
    /// 区域内 Path objects 数量
    pub path_count: usize,
    /// 区域内文字块 ID 列表（需要从正文 blocks 中剔除）
    pub contained_block_ids: Vec<String>,
    /// 置信度 (0.0 ~ 1.0)
    pub confidence: f32,
    /// 关联的 Caption 文本
    pub caption: Option<String>,
}
```

- [ ] 创建 `src/figure/types.rs`：`FigureRegion` 结构体
- [ ] 创建 `src/figure/detector.rs`：
  - [ ] `detect_figure_regions()` 主函数
  - [ ] 网格密度分析算法
  - [ ] 连通区域合并算法
  - [ ] 过滤规则（面积/数量/排除表格区域）
- [ ] 创建 `src/figure/mod.rs`：模块导出
- [ ] 在 `src/lib.rs` 中注册 `figure` 模块

### 3. OcrRenderer 扩展：裁剪渲染

在 `OcrRenderer` trait 中新增裁剪渲染方法：

```rust
/// 将指定页面的指定区域渲染为图片字节数据
fn render_region_to_image(
    &self,
    page_index: usize,
    bbox: BBox,
    render_width: u32,
) -> Result<Vec<u8>, PdfError>;
```

在 `PdfiumOcrRenderer` 中实现：
- 渲染整页 → 根据 bbox 坐标裁剪 → 输出 PNG
- 使用 `image` crate 的 `crop_imm()` 进行裁剪

- [ ] `OcrRenderer` trait 增加 `render_region_to_image()` 方法（带默认实现）
- [ ] `PdfiumOcrRenderer` 实现裁剪渲染
- [ ] `MockOcrRenderer` 提供空实现

### 4. ImageIR 扩展

在 `ImageIR` 中新增字段以支持图表描述：

```rust
pub struct ImageIR {
    // ... 现有字段 ...
    
    /// 图片来源类型
    pub source: ImageSource,
    /// OCR 提取的文字描述（图内文字标签聚合）
    pub ocr_text: Option<String>,
    /// 图内的原始文字标签（保留结构信息）
    pub text_labels: Vec<TextLabel>,
}

pub enum ImageSource {
    /// 嵌入的位图（Image XObject）
    Embedded,
    /// 矢量图表区域渲染
    FigureRegion,
}

pub struct TextLabel {
    pub text: String,
    pub bbox: BBox,
}
```

- [ ] 在 `ir/image.rs` 中新增 `ImageSource`、`TextLabel` 类型
- [ ] `ImageIR` 新增 `source`、`ocr_text`、`text_labels` 字段
- [ ] 保持向后兼容（新字段用 `#[serde(default)]`）

### 5. Pipeline 集成

在 `process_page()` 中集成图表检测流程：

```
extract_chars() → build_blocks_and_grids()
     ↓
extract_path_objects() → detect_figure_regions()
     ↓
render_region_to_image() → ocr_region() → 生成 ImageIR
     ↓
从 blocks 中剔除图区域内的文字块
     ↓
Caption 关联（检测图下方的 "Figure X: ..." 文本块）
```

- [ ] 在 `process_page()` 中调用 `extract_path_objects()`
- [ ] 调用 `detect_figure_regions()` 检测图区域
- [ ] 对每个检测到的图区域：
  - [ ] 调用 `render_region_to_image()` 渲染为位图
  - [ ] 调用 `ocr_region()` 提取图内文字
  - [ ] 构建 `ImageIR`（source = FigureRegion）
  - [ ] 从 `blocks` 中剔除 `contained_block_ids` 引用的文字块
- [ ] Caption 检测：在图区域正下方寻找以 "Figure" / "Fig." / "图" 开头的文字块

### 6. Config 新增配置项

```rust
/// 是否启用图表区域检测
pub figure_detection_enabled: bool,  // 默认 true

/// 图表区域最小面积（占页面面积比例）
pub figure_min_area_ratio: f32,  // 默认 0.05

/// 图表区域最小 Path objects 数量
pub figure_min_path_count: usize,  // 默认 10

/// 图表渲染分辨率（宽度像素）
pub figure_render_width: u32,  // 默认 800
```

- [ ] `Config` 新增 4 个图表相关字段
- [ ] 默认值函数
- [ ] `validate()` 中添加校验

### 7. Markdown 渲染器适配

```rust
// 渲染图表（FigureRegion 来源的 ImageIR）
for image in &page.images {
    match image.source {
        ImageSource::FigureRegion => {
            // 输出图表占位 + OCR 文字
            output.push_str(&format!("![Figure {}](page_{}_fig_{})\n\n", ...));
            if let Some(ocr_text) = &image.ocr_text {
                output.push_str(&format!("<!-- Figure text: {} -->\n\n", ocr_text));
            }
            // 关联 caption
            for cap_ref in &image.caption_refs {
                // 输出斜体 caption
            }
        }
        ImageSource::Embedded => {
            // 现有逻辑
        }
    }
}
```

- [ ] `MarkdownRenderer` 区分 `ImageSource::FigureRegion` 和 `Embedded`
- [ ] FigureRegion 类型输出 OCR 文字作为 HTML 注释或段落
- [ ] Caption 以斜体输出

### 8. RAG 渲染器适配

- [ ] `RagRenderer` 将图表的 OCR 文字作为独立的 RAG 条目输出
- [ ] JSONL 格式中新增 `type: "figure"` 类型

### 9. 测试

- [ ] 单元测试：
  - [ ] 网格密度算法（手动构造 PathObject 数据，验证检测结果）
  - [ ] 连通区域合并（多个独立图表区域）
  - [ ] 过滤规则（排除小面积 / 低密度 / 表格区域重叠）
  - [ ] Caption 关联匹配
- [ ] 集成测试：
  - [ ] 构造包含矢量图表的测试 PDF
  - [ ] 验证图表被检测并生成 ImageIR
  - [ ] 验证图区域内文字被正确剔除
  - [ ] 验证 Markdown 输出包含图表信息
  - [ ] 验证 RAG 输出包含 figure 类型条目

---

## 算法示例

以 "Attention Is All You Need" 论文第 3 页为例：

1. **Path object 提取**：该页面可能有 ~100 个 Path objects（矩形框、箭头线段、圆弧）
2. **网格密度分析**：页面上半部分（y < 60% 区域）网格密度显著高于下半部分
3. **连通区域合并**：上半部分的高密度网格合并为一个连续区域 ≈ BBox(100, 50, 400, 500)
4. **过滤**：该区域面积约占页面 40%，Path objects > 50 个 → 确认为图表
5. **渲染**：裁剪该 BBox 区域渲染为 800px 宽的 PNG 图片
6. **OCR**：识别图中文字标签 → "Output Probabilities, Softmax, Linear, Add & Norm, Feed Forward, Multi-Head Attention, ..."
7. **文字剔除**：将这些零散标签文字从 `blocks` 中移除
8. **Caption 关联**：图下方的 "Figure 1: The Transformer - model architecture." 设为 caption

**Markdown 输出结果**：

```markdown
<!-- Page 3 -->

![Figure p2_0](page_3_fig_p2_0)

<!-- Figure text: Output Probabilities, Softmax, Linear, Add & Norm, Feed Forward, Multi-Head Attention, Masked Multi-Head Attention, Positional Encoding, Input Embedding, Output Embedding, Inputs, Outputs (shifted right), N× -->

*Figure 1: The Transformer - model architecture.*

The Transformer follows this overall architecture using stacked self-attention and point-wise, fully connected layers for both the encoder and decoder, shown in the left and right halves of Figure 1, respectively.

## 3.1 Encoder and Decoder Stacks

Encoder: The encoder is composed of a stack of N = 6 identical layers. ...
```

---

## 完成标准

- [ ] 矢量图表区域检测准确率 > 80%（20 个测试 PDF，含论文/报告/技术文档）
- [ ] 图区域内零散文字被正确从正文中剔除
- [ ] 图表 OCR 文字包含主要标签内容
- [ ] Caption 关联正确（"Figure X: ..." 格式）
- [ ] 不影响现有文本/表格解析性能（处理时间增加 < 20%）
- [ ] Markdown / RAG 输出格式正确
- [ ] 全部测试通过
