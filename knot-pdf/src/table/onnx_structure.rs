//! ONNX 表格结构检测器实现
//!
//! 基于 tract-onnx 的表格结构识别推理引擎。
//!
//! 支持以下模型架构：
//! - Table Transformer (DETR) — microsoft/table-transformer-structure-recognition
//! - YOLO 系列表格检测模型
//!
//! # 模型格式
//!
//! ## Table Transformer (DETR)
//! - 输入: `(1, 3, H, W)` float32, 像素值归一化 0~1
//! - 输出: logits `(1, num_queries, num_classes+1)` + boxes `(1, num_queries, 4)` (cx,cy,w,h 归一化)
//!
//! ## YOLO 格式
//! - 输入: `(1, 3, H, W)` float32, 像素值归一化 0~1
//! - 输出: `(1, C, N)` 其中 C = 4 + num_classes
//!
//! # 类别
//!
//! Table Transformer 5 类:
//! 0: table row
//! 1: table column
//! 2: table column header
//! 3: table projected row header
//! 4: table spanning cell

#[cfg(feature = "table_model")]
use tract_onnx::prelude::*;

use super::structure_detect::*;
use crate::error::PdfError;
use crate::ir::BBox;

/// Table Transformer 类别映射
const TABLE_TRANSFORMER_CLASSES: &[TableElementLabel] = &[
    TableElementLabel::Row,                // 0: table row
    TableElementLabel::Column,             // 1: table column
    TableElementLabel::ColumnHeader,       // 2: table column header
    TableElementLabel::ProjectedRowHeader, // 3: table projected row header
    TableElementLabel::SpanningCell,       // 4: table spanning cell
];

/// 模型架构类型
#[derive(Debug, Clone, Copy)]
pub enum TableModelArch {
    /// Table Transformer (DETR 风格)
    /// 输出: logits + boxes 两个 tensor
    Detr,
    /// YOLO 风格
    /// 输出: 单个 tensor (1, C, N)
    Yolo,
}

impl Default for TableModelArch {
    fn default() -> Self {
        TableModelArch::Yolo
    }
}

/// ONNX 表格结构检测器
#[cfg(feature = "table_model")]
pub struct OnnxTableStructureDetector {
    model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
    input_height: u32,
    input_width: u32,
    arch: TableModelArch,
    confidence_threshold: f32,
    num_classes: usize,
}

#[cfg(feature = "table_model")]
impl OnnxTableStructureDetector {
    /// 从文件加载 ONNX 模型
    pub fn from_file(
        model_path: &std::path::Path,
        input_height: u32,
        input_width: u32,
        arch: TableModelArch,
        confidence_threshold: f32,
    ) -> Result<Self, PdfError> {
        let optimized = tract_onnx::onnx()
            .model_for_path(model_path)
            .map_err(|e| PdfError::Backend(format!("Failed to load table model: {}", e)))?
            .with_input_fact(
                0,
                InferenceFact::dt_shape(
                    f32::datum_type(),
                    tvec![1, 3, input_height as i64, input_width as i64],
                ),
            )
            .map_err(|e| PdfError::Backend(format!("Failed to set input shape: {}", e)))?
            .into_optimized()
            .map_err(|e| PdfError::Backend(format!("Failed to optimize model: {}", e)))?;

        // 从模型输出 shape 自动推断 num_classes
        let num_classes = Self::infer_num_classes(&optimized, &arch)?;

        let model = optimized
            .into_runnable()
            .map_err(|e| PdfError::Backend(format!("Failed to build runnable: {}", e)))?;

        log::info!(
            "Loaded table structure model from {:?} (arch={:?}, classes={}, input={}x{})",
            model_path,
            arch,
            num_classes,
            input_height,
            input_width
        );

        Ok(Self {
            model,
            input_height,
            input_width,
            arch,
            confidence_threshold,
            num_classes,
        })
    }

    /// 从 bytes 加载 ONNX 模型
    pub fn from_bytes(
        model_bytes: &[u8],
        input_height: u32,
        input_width: u32,
        arch: TableModelArch,
        confidence_threshold: f32,
    ) -> Result<Self, PdfError> {
        let cursor = std::io::Cursor::new(model_bytes);
        let optimized = tract_onnx::onnx()
            .model_for_read(&mut cursor.clone())
            .map_err(|e| PdfError::Backend(format!("Failed to load table model: {}", e)))?
            .with_input_fact(
                0,
                InferenceFact::dt_shape(
                    f32::datum_type(),
                    tvec![1, 3, input_height as i64, input_width as i64],
                ),
            )
            .map_err(|e| PdfError::Backend(format!("Failed to set input shape: {}", e)))?
            .into_optimized()
            .map_err(|e| PdfError::Backend(format!("Failed to optimize model: {}", e)))?;

        let num_classes = Self::infer_num_classes(&optimized, &arch)?;

        let model = optimized
            .into_runnable()
            .map_err(|e| PdfError::Backend(format!("Failed to build runnable: {}", e)))?;

        Ok(Self {
            model,
            input_height,
            input_width,
            arch,
            confidence_threshold,
            num_classes,
        })
    }

    /// 从模型输出 shape 自动推断类别数
    fn infer_num_classes(
        optimized: &Graph<TypedFact, Box<dyn TypedOp>>,
        arch: &TableModelArch,
    ) -> Result<usize, PdfError> {
        let outputs = optimized
            .output_outlets()
            .map_err(|e| PdfError::Backend(format!("Cannot get output outlets: {}", e)))?;

        match arch {
            TableModelArch::Detr => {
                // DETR: logits shape (1, num_queries, num_classes+1)
                if let Some(outlet) = outputs.first() {
                    let fact = optimized
                        .outlet_fact(*outlet)
                        .map_err(|e| PdfError::Backend(format!("Cannot get output fact: {}", e)))?;
                    if let Some(shape) = fact.shape.as_concrete() {
                        if shape.len() >= 3 {
                            return Ok(shape[2]); // 包含 no-object 类
                        }
                    }
                }
                Ok(6) // 默认 5+1
            }
            TableModelArch::Yolo => {
                // YOLO: output shape (1, C, N) 其中 C = 4 + num_classes
                if let Some(outlet) = outputs.first() {
                    let fact = optimized
                        .outlet_fact(*outlet)
                        .map_err(|e| PdfError::Backend(format!("Cannot get output fact: {}", e)))?;
                    if let Some(shape) = fact.shape.as_concrete() {
                        if shape.len() >= 3 {
                            // (1, C, N) — C 通常是较小的维度
                            let c = shape[1].min(shape[2]);
                            if c > 4 {
                                log::info!(
                                    "Auto-detected YOLO num_classes={} from output shape {:?}",
                                    c - 4,
                                    shape
                                );
                                return Ok(c - 4);
                            }
                        }
                    }
                }
                Ok(5) // 默认 5 类
            }
        }
    }

    /// 图片预处理：PNG bytes → 归一化 float32 tensor
    fn preprocess(&self, image_data: &[u8]) -> Result<Tensor, PdfError> {
        use image::GenericImageView;

        let img = image::load_from_memory(image_data)
            .map_err(|e| PdfError::Backend(format!("Failed to decode image: {}", e)))?;

        let resized = img.resize_exact(
            self.input_width,
            self.input_height,
            image::imageops::FilterType::Triangle,
        );

        // 转为 NCHW float32，归一化到 0~1
        let mut tensor =
            Tensor::zero::<f32>(&[1, 3, self.input_height as usize, self.input_width as usize])
                .map_err(|e| PdfError::Backend(format!("Failed to create tensor: {}", e)))?;

        let data = tensor
            .as_slice_mut::<f32>()
            .map_err(|e| PdfError::Backend(format!("tensor error: {}", e)))?;

        let h = self.input_height as usize;
        let w = self.input_width as usize;

        for y in 0..h {
            for x in 0..w {
                let pixel = resized.get_pixel(x as u32, y as u32);
                data[0 * h * w + y * w + x] = pixel[0] as f32 / 255.0; // R
                data[1 * h * w + y * w + x] = pixel[1] as f32 / 255.0; // G
                data[2 * h * w + y * w + x] = pixel[2] as f32 / 255.0; // B
            }
        }

        Ok(tensor)
    }

    /// DETR 后处理
    fn postprocess_detr(
        &self,
        outputs: &[TValue],
        table_bbox: &BBox,
    ) -> Result<Vec<TableElement>, PdfError> {
        if outputs.len() < 2 {
            return Err(PdfError::Backend(
                "DETR model should have at least 2 outputs (logits + boxes)".to_string(),
            ));
        }

        let logits = outputs[0]
            .to_array_view::<f32>()
            .map_err(|e| PdfError::Backend(format!("logits error: {}", e)))?;
        let boxes = outputs[1]
            .to_array_view::<f32>()
            .map_err(|e| PdfError::Backend(format!("boxes error: {}", e)))?;

        let num_queries = logits.shape()[1];
        let mut elements = Vec::new();

        for q in 0..num_queries {
            // softmax 找最大类别
            let mut max_score = f32::NEG_INFINITY;
            let mut max_class = 0usize;
            let num_classes_with_no_obj = logits.shape()[2];

            for c in 0..num_classes_with_no_obj {
                let score = logits[[0, q, c]];
                if score > max_score {
                    max_score = score;
                    max_class = c;
                }
            }

            // 最后一个类别是 "no object"
            if max_class >= num_classes_with_no_obj - 1 {
                continue;
            }

            // 简单 sigmoid/softmax 近似置信度
            let confidence = 1.0 / (1.0 + (-max_score).exp());
            if confidence < self.confidence_threshold {
                continue;
            }

            // bbox: (cx, cy, w, h) 归一化到 0~1
            let cx = boxes[[0, q, 0]];
            let cy = boxes[[0, q, 1]];
            let bw = boxes[[0, q, 2]];
            let bh = boxes[[0, q, 3]];

            // 转换为页面坐标
            let x = (cx - bw / 2.0) * table_bbox.width + table_bbox.x;
            let y = (cy - bh / 2.0) * table_bbox.height + table_bbox.y;
            let w = bw * table_bbox.width;
            let h = bh * table_bbox.height;

            let label = if max_class < TABLE_TRANSFORMER_CLASSES.len() {
                TABLE_TRANSFORMER_CLASSES[max_class]
            } else {
                continue;
            };

            elements.push(TableElement {
                bbox: BBox::new(x, y, w, h),
                label,
                confidence,
            });
        }

        Ok(elements)
    }

    /// YOLO 后处理
    fn postprocess_yolo(
        &self,
        output: &Tensor,
        table_bbox: &BBox,
    ) -> Result<Vec<TableElement>, PdfError> {
        let shape = output.shape();
        if shape.len() != 3 {
            return Err(PdfError::Backend(format!(
                "Expected 3D output, got {:?}",
                shape
            )));
        }

        let data = output
            .as_slice::<f32>()
            .map_err(|e| PdfError::Backend(format!("output error: {}", e)))?;

        let dim1 = shape[1];
        let dim2 = shape[2];

        // 检测是 (1, C, N) 还是 (1, N, C)
        let (num_detections, channels, is_transposed) = {
            let expected_c = 4 + self.num_classes;
            if dim1 == expected_c {
                (dim2, dim1, false) // (1, C, N)
            } else if dim2 == expected_c {
                (dim1, dim2, true) // (1, N, C)
            } else {
                return Err(PdfError::Backend(format!(
                    "Cannot determine output layout: shape {:?}, expected C={}",
                    shape, expected_c
                )));
            }
        };

        let mut elements = Vec::new();
        let input_w = self.input_width as f32;
        let input_h = self.input_height as f32;

        for i in 0..num_detections {
            // 读取 (cx, cy, w, h) 和各类别分数
            let get = |c: usize| -> f32 {
                if is_transposed {
                    data[i * channels + c]
                } else {
                    data[c * num_detections + i]
                }
            };

            let cx = get(0);
            let cy = get(1);
            let bw = get(2);
            let bh = get(3);

            // 找最大类别
            let mut max_score = f32::NEG_INFINITY;
            let mut max_class = 0usize;
            for c in 0..self.num_classes {
                let score = get(4 + c);
                if score > max_score {
                    max_score = score;
                    max_class = c;
                }
            }

            if max_score < self.confidence_threshold {
                continue;
            }

            // 坐标还原到页面坐标
            let x = ((cx - bw / 2.0) / input_w) * table_bbox.width + table_bbox.x;
            let y = ((cy - bh / 2.0) / input_h) * table_bbox.height + table_bbox.y;
            let w = (bw / input_w) * table_bbox.width;
            let h = (bh / input_h) * table_bbox.height;

            let label = if max_class < TABLE_TRANSFORMER_CLASSES.len() {
                TABLE_TRANSFORMER_CLASSES[max_class]
            } else {
                continue;
            };

            elements.push(TableElement {
                bbox: BBox::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0)),
                label,
                confidence: max_score,
            });
        }

        Ok(elements)
    }
}

#[cfg(feature = "table_model")]
impl TableStructureDetector for OnnxTableStructureDetector {
    fn detect(
        &self,
        image_data: &[u8],
        table_bbox: &BBox,
    ) -> Result<TableStructureResult, PdfError> {
        // 1. 预处理
        let input = self.preprocess(image_data)?;

        // 2. 推理
        let outputs = self
            .model
            .run(tvec!(input.into()))
            .map_err(|e| PdfError::Backend(format!("Table model inference failed: {}", e)))?;

        // 3. 后处理
        let elements = match self.arch {
            TableModelArch::Detr => {
                let output_refs: Vec<TValue> = outputs.into_iter().collect();
                self.postprocess_detr(&output_refs, table_bbox)?
            }
            TableModelArch::Yolo => {
                let output = outputs[0]
                    .to_array_view::<f32>()
                    .map_err(|e| PdfError::Backend(format!("output error: {}", e)))?;
                let tensor = output.into_owned().into();
                self.postprocess_yolo(&tensor, table_bbox)?
            }
        };

        // 4. 提取行列分割线
        let row_separators = rows_to_separators(&elements);
        let col_separators = cols_to_separators(&elements);

        // 5. 提取表头和合并单元格
        let header_bbox = elements
            .iter()
            .filter(|e| e.label == TableElementLabel::ColumnHeader)
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|e| e.bbox.clone());

        let spanning_cells: Vec<BBox> = elements
            .iter()
            .filter(|e| e.label == TableElementLabel::SpanningCell)
            .map(|e| e.bbox.clone())
            .collect();

        Ok(TableStructureResult {
            row_separators,
            col_separators,
            elements,
            header_bbox,
            spanning_cells,
        })
    }

    fn name(&self) -> &str {
        "onnx-table-structure"
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_transformer_class_count() {
        assert_eq!(TABLE_TRANSFORMER_CLASSES.len(), 5);
        assert_eq!(TABLE_TRANSFORMER_CLASSES[0], TableElementLabel::Row);
        assert_eq!(TABLE_TRANSFORMER_CLASSES[1], TableElementLabel::Column);
        assert_eq!(
            TABLE_TRANSFORMER_CLASSES[4],
            TableElementLabel::SpanningCell
        );
    }

    #[test]
    fn test_table_model_arch_default() {
        let arch = TableModelArch::default();
        matches!(arch, TableModelArch::Detr);
    }
}
