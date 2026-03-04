//! ONNX 版面检测器实现
//!
//! 基于 tract-onnx 的 Pure Rust ONNX 推理引擎，
//! 支持 YOLO 系列模型（DocLayout-YOLO / YOLOv8 等）。
//!
//! # 模型格式
//!
//! 支持标准 YOLO ONNX 模型：
//! - 输入: `(1, 3, H, W)` — NCHW 格式的 RGB float32 图片，像素值 0~1
//! - 输出: `(1, C, N)` — C = 4 + num_classes, N = 检测框数量
//!   - 前 4 维: `[cx, cy, w, h]` (中心坐标 + 宽高，归一化到输入尺寸)
//!   - 后 num_classes 维: 各类别置信度
//!
//! # 支持的类别 (DocStructBench 11 类)
//!
//! 0=Title, 1=Text, 2=Abandon, 3=Figure, 4=FigureCaption,
//! 5=Table, 6=TableCaption, 7=TableFootnote, 8=Isolate_Formula,
//! 9=Formula_Caption, 10=(reserved)
//!
//! （也支持 DocLayNet 11 类，通过 `class_mapping` 配置）

#[cfg(feature = "layout_model")]
use tract_onnx::prelude::*;

use super::{LayoutDetector, LayoutLabel, LayoutRegion};
use crate::error::PdfError;
use crate::ir::BBox;

/// DocStructBench 类别映射
const DOCSTRUCT_CLASSES: &[LayoutLabel] = &[
    LayoutLabel::Title,     // 0: Title
    LayoutLabel::Paragraph, // 1: Text (plain_text)
    LayoutLabel::Unknown,   // 2: Abandon
    LayoutLabel::Figure,    // 3: Figure
    LayoutLabel::Caption,   // 4: Figure_caption
    LayoutLabel::Table,     // 5: Table
    LayoutLabel::Caption,   // 6: Table_caption
    LayoutLabel::Footer,    // 7: Table_footnote
    LayoutLabel::Formula,   // 8: Isolate_Formula
    LayoutLabel::Caption,   // 9: Formula_Caption
    LayoutLabel::Unknown,   // 10: (reserved)
];

/// DocLayNet 类别映射
const DOCLAYNET_CLASSES: &[LayoutLabel] = &[
    LayoutLabel::Caption,    // 0: Caption
    LayoutLabel::Footer,     // 1: Footnote
    LayoutLabel::Formula,    // 2: Formula
    LayoutLabel::List,       // 3: List-item
    LayoutLabel::PageNumber, // 4: Page-footer
    LayoutLabel::Header,     // 5: Page-header
    LayoutLabel::Figure,     // 6: Picture
    LayoutLabel::Heading,    // 7: Section-header
    LayoutLabel::Table,      // 8: Table
    LayoutLabel::Paragraph,  // 9: Text
    LayoutLabel::Title,      // 10: Title
];

/// 模型类别体系
#[derive(Debug, Clone, Copy)]
pub enum ClassSchema {
    /// DocStructBench 11 类
    DocStructBench,
    /// DocLayNet 11 类
    DocLayNet,
}

impl Default for ClassSchema {
    fn default() -> Self {
        Self::DocStructBench
    }
}

/// ONNX 版面检测器
///
/// 使用 tract-onnx 加载 YOLO 格式的 ONNX 模型进行推理。
#[cfg(feature = "layout_model")]
pub struct OnnxLayoutDetector {
    /// tract 推理模型
    model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
    /// 模型输入尺寸 (H, W)
    input_size: (usize, usize),
    /// 类别映射
    class_schema: ClassSchema,
    /// 检测置信度阈值
    confidence_threshold: f32,
    /// NMS IoU 阈值
    nms_threshold: f32,
}

#[cfg(feature = "layout_model")]
impl OnnxLayoutDetector {
    /// 从文件加载 ONNX 模型
    pub fn from_file(
        model_path: &std::path::Path,
        input_size: u32,
        class_schema: ClassSchema,
        confidence_threshold: f32,
    ) -> Result<Self, PdfError> {
        let size = input_size as usize;

        let model = tract_onnx::onnx()
            .model_for_path(model_path)
            .map_err(|e| PdfError::Backend(format!("Failed to load ONNX model: {}", e)))?
            .with_input_fact(
                0,
                InferenceFact::dt_shape(f32::datum_type(), tvec![1, 3, size, size]),
            )
            .map_err(|e| PdfError::Backend(format!("Failed to set input shape: {}", e)))?
            .into_optimized()
            .map_err(|e| PdfError::Backend(format!("Failed to optimize model: {}", e)))?
            .into_runnable()
            .map_err(|e| PdfError::Backend(format!("Failed to make model runnable: {}", e)))?;

        log::info!(
            "Loaded ONNX layout model from {:?} (input={}x{}, schema={:?})",
            model_path,
            size,
            size,
            class_schema,
        );

        Ok(Self {
            model,
            input_size: (size, size),
            class_schema,
            confidence_threshold,
            nms_threshold: 0.5,
        })
    }

    /// 从 bytes 加载 ONNX 模型
    pub fn from_bytes(
        model_bytes: &[u8],
        input_size: u32,
        class_schema: ClassSchema,
        confidence_threshold: f32,
    ) -> Result<Self, PdfError> {
        let size = input_size as usize;
        let mut cursor = std::io::Cursor::new(model_bytes);

        let model = tract_onnx::onnx()
            .model_for_read(&mut cursor)
            .map_err(|e| {
                PdfError::Backend(format!("Failed to load ONNX model from bytes: {}", e))
            })?
            .with_input_fact(
                0,
                InferenceFact::dt_shape(f32::datum_type(), tvec![1, 3, size, size]),
            )
            .map_err(|e| PdfError::Backend(format!("Failed to set input shape: {}", e)))?
            .into_optimized()
            .map_err(|e| PdfError::Backend(format!("Failed to optimize model: {}", e)))?
            .into_runnable()
            .map_err(|e| PdfError::Backend(format!("Failed to make model runnable: {}", e)))?;

        Ok(Self {
            model,
            input_size: (size, size),
            class_schema,
            confidence_threshold,
            nms_threshold: 0.5,
        })
    }

    /// 图片预处理：将 PNG bytes → 归一化 float32 tensor
    fn preprocess(&self, image_data: &[u8]) -> Result<Tensor, PdfError> {
        use image::GenericImageView;

        let img = image::load_from_memory(image_data)
            .map_err(|e| PdfError::Backend(format!("Failed to decode image: {}", e)))?;

        let (h, w) = self.input_size;
        let resized = img.resize_exact(w as u32, h as u32, image::imageops::FilterType::Triangle);

        // 转换为 NCHW float32 tensor，像素值归一化到 0~1
        let mut tensor = Tensor::zero::<f32>(&[1, 3, h, w])
            .map_err(|e| PdfError::Backend(format!("Failed to create tensor: {}", e)))?;

        {
            let data = tensor
                .as_slice_mut::<f32>()
                .map_err(|e| PdfError::Backend(format!("Failed to get tensor slice: {}", e)))?;

            for y in 0..h {
                for x in 0..w {
                    let pixel = resized.get_pixel(x as u32, y as u32);
                    let r = pixel[0] as f32 / 255.0;
                    let g = pixel[1] as f32 / 255.0;
                    let b = pixel[2] as f32 / 255.0;

                    data[0 * h * w + y * w + x] = r;
                    data[1 * h * w + y * w + x] = g;
                    data[2 * h * w + y * w + x] = b;
                }
            }
        }

        Ok(tensor)
    }

    /// 后处理：将模型输出转换为 LayoutRegion
    ///
    /// YOLO 输出格式: (1, C, N) 其中 C = 4 + num_classes
    /// 前 4 维: [cx, cy, w, h]
    /// 后 num_classes 维: 类别置信度
    fn postprocess(
        &self,
        output: &Tensor,
        page_width: f32,
        page_height: f32,
    ) -> Result<Vec<LayoutRegion>, PdfError> {
        let shape = output.shape();
        // 期望输出 shape: (1, C, N) 或 (1, N, C)
        if shape.len() != 3 || shape[0] != 1 {
            return Err(PdfError::Backend(format!(
                "Unexpected model output shape: {:?}",
                shape
            )));
        }

        let data = output
            .as_slice::<f32>()
            .map_err(|e| PdfError::Backend(format!("Failed to read output: {}", e)))?;

        let class_map = match self.class_schema {
            ClassSchema::DocStructBench => DOCSTRUCT_CLASSES,
            ClassSchema::DocLayNet => DOCLAYNET_CLASSES,
        };
        let num_classes = class_map.len();

        // 判断输出布局: (1, C, N) vs (1, N, C)
        let (n_detections, n_channels, is_transposed) = if shape[1] == 4 + num_classes {
            // (1, C, N) — 标准 YOLO 格式
            (shape[2], shape[1], true)
        } else if shape[2] == 4 + num_classes {
            // (1, N, C)
            (shape[1], shape[2], false)
        } else {
            // 尝试猜测
            if shape[1] < shape[2] {
                (shape[2], shape[1], true)
            } else {
                (shape[1], shape[2], false)
            }
        };

        let (inp_h, inp_w) = self.input_size;
        let scale_x = page_width / inp_w as f32;
        let scale_y = page_height / inp_h as f32;

        let mut regions = Vec::new();

        for i in 0..n_detections {
            // 提取 bbox 和 class scores
            let (cx, cy, bw, bh, class_scores): (f32, f32, f32, f32, Vec<f32>) = if is_transposed {
                // (1, C, N): data 按 [c * N + i] 访问
                let cx = data[0 * n_detections + i];
                let cy = data[1 * n_detections + i];
                let bw = data[2 * n_detections + i];
                let bh = data[3 * n_detections + i];
                let scores: Vec<f32> = (0..num_classes)
                    .map(|c| data[(4 + c) * n_detections + i])
                    .collect();
                (cx, cy, bw, bh, scores)
            } else {
                // (1, N, C): data 按 [i * C + c] 访问
                let offset = i * n_channels;
                let cx = data[offset + 0];
                let cy = data[offset + 1];
                let bw = data[offset + 2];
                let bh = data[offset + 3];
                let scores: Vec<f32> = (0..num_classes).map(|c| data[offset + 4 + c]).collect();
                (cx, cy, bw, bh, scores)
            };

            // 找到最高置信度
            let (best_class, &best_score) = class_scores
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, &0.0));

            if best_score < self.confidence_threshold {
                continue;
            }

            // 将 [cx, cy, w, h] 转为 [x, y, w, h] 并映射到页面坐标
            let x = (cx - bw / 2.0) * scale_x;
            let y = (cy - bh / 2.0) * scale_y;
            let w = bw * scale_x;
            let h = bh * scale_y;

            // 裁剪到页面范围
            let x = x.max(0.0);
            let y = y.max(0.0);
            let w = w.min(page_width - x);
            let h = h.min(page_height - y);

            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            let label = if best_class < class_map.len() {
                class_map[best_class]
            } else {
                LayoutLabel::Unknown
            };

            regions.push(LayoutRegion {
                bbox: BBox::new(x, y, w, h),
                label,
                confidence: best_score,
            });
        }

        Ok(regions)
    }
}

#[cfg(feature = "layout_model")]
impl LayoutDetector for OnnxLayoutDetector {
    fn detect(
        &self,
        image_data: &[u8],
        page_width: f32,
        page_height: f32,
    ) -> Result<Vec<LayoutRegion>, PdfError> {
        let input = self.preprocess(image_data)?;

        let outputs = self
            .model
            .run(tvec![input.into()])
            .map_err(|e| PdfError::Backend(format!("ONNX inference failed: {}", e)))?;

        let output = outputs[0]
            .to_array_view::<f32>()
            .map_err(|e| PdfError::Backend(format!("Failed to read output tensor: {}", e)))?;

        let output_tensor = output.into_owned().into_tensor();
        let mut regions = self.postprocess(&output_tensor, page_width, page_height)?;

        // NMS
        super::nms(&mut regions, self.nms_threshold);

        Ok(regions)
    }

    fn name(&self) -> &str {
        "onnx-layout"
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docstruct_class_mapping() {
        assert_eq!(DOCSTRUCT_CLASSES[0], LayoutLabel::Title);
        assert_eq!(DOCSTRUCT_CLASSES[1], LayoutLabel::Paragraph);
        assert_eq!(DOCSTRUCT_CLASSES[3], LayoutLabel::Figure);
        assert_eq!(DOCSTRUCT_CLASSES[5], LayoutLabel::Table);
    }

    #[test]
    fn test_doclaynet_class_mapping() {
        assert_eq!(DOCLAYNET_CLASSES[0], LayoutLabel::Caption);
        assert_eq!(DOCLAYNET_CLASSES[3], LayoutLabel::List);
        assert_eq!(DOCLAYNET_CLASSES[7], LayoutLabel::Heading);
        assert_eq!(DOCLAYNET_CLASSES[8], LayoutLabel::Table);
        assert_eq!(DOCLAYNET_CLASSES[10], LayoutLabel::Title);
    }

    #[test]
    fn test_class_schema_default() {
        let schema = ClassSchema::default();
        assert!(matches!(schema, ClassSchema::DocStructBench));
    }
}
