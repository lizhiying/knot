//! PaddleOCR 后端实现（Pure Rust，零系统依赖）
//!
//! 基于 `pure_onnx_ocr` crate，使用 PaddleOCR PP-OCRv5 模型进行文字检测与识别。
//! 无需安装 Tesseract 或任何系统 C/C++ 库，跨平台支持 macOS/Linux/Windows。
//!
//! 注意：`OcrEngine` (tract-onnx) 内部使用了 `Rc`（非 Send+Sync），
//! 因此 `PaddleOcrBackend` 使用 `Mutex` 包装以满足 `OcrBackend: Send + Sync` 约束。

use std::path::Path;
use std::sync::Mutex;

use pure_onnx_ocr::{OcrEngine, OcrEngineBuilder};

use crate::error::PdfError;
use crate::ir::BBox;
use crate::ocr::traits::{OcrBackend, OcrBlock};

/// PaddleOCR 后端 (Pure Rust)
///
/// 使用 Mutex 包装 OcrEngine 以满足 Send + Sync 约束。
/// 多线程场景下通过互斥锁串行化 OCR 调用。
pub struct PaddleOcrBackend {
    engine: Mutex<OcrEngine>,
}

// SAFETY: OcrEngine 本身不是 Sync（内含 Rc），
// 但我们用 Mutex 包装后，同一时刻只有一个线程访问它。
unsafe impl Send for PaddleOcrBackend {}
unsafe impl Sync for PaddleOcrBackend {}

impl PaddleOcrBackend {
    /// 从指定模型目录创建 PaddleOCR 后端
    ///
    /// `model_dir` 需包含以下文件：
    /// - `det.onnx`：文字检测模型
    /// - `rec.onnx`：文字识别模型
    /// - `ppocrv5_dict.txt`：字典文件
    pub fn new(model_dir: &Path) -> Result<Self, PdfError> {
        let det_path = model_dir.join("det.onnx");
        let rec_path = model_dir.join("rec.onnx");
        let dict_path = model_dir.join("ppocrv5_dict.txt");

        // 检查模型文件是否存在
        if !det_path.exists() {
            return Err(PdfError::Ocr(format!(
                "检测模型不存在: {}",
                det_path.display()
            )));
        }
        if !rec_path.exists() {
            return Err(PdfError::Ocr(format!(
                "识别模型不存在: {}",
                rec_path.display()
            )));
        }
        if !dict_path.exists() {
            return Err(PdfError::Ocr(format!(
                "字典文件不存在: {}",
                dict_path.display()
            )));
        }

        let engine = OcrEngineBuilder::default()
            .det_model_path(det_path.to_str().unwrap())
            .rec_model_path(rec_path.to_str().unwrap())
            .dictionary_path(dict_path.to_str().unwrap())
            .build()
            .map_err(|e| PdfError::Ocr(format!("PaddleOCR 初始化失败: {:?}", e)))?;

        Ok(Self {
            engine: Mutex::new(engine),
        })
    }
}

impl OcrBackend for PaddleOcrBackend {
    fn ocr_region(&self, image_data: &[u8], bbox: BBox) -> Result<Vec<OcrBlock>, PdfError> {
        // 加载图片
        let img = image::load_from_memory(image_data)
            .map_err(|e| PdfError::Ocr(format!("图片加载失败: {}", e)))?;

        // 裁剪 region
        let cropped = img.crop_imm(
            bbox.x as u32,
            bbox.y as u32,
            bbox.width as u32,
            bbox.height as u32,
        );

        // 执行 OCR
        let engine = self
            .engine
            .lock()
            .map_err(|e| PdfError::Ocr(format!("OCR engine lock failed: {}", e)))?;

        let results = engine
            .run_from_image(&cropped)
            .map_err(|e| PdfError::Ocr(format!("OCR 区域识别失败: {:?}", e)))?;

        let blocks: Vec<OcrBlock> = results
            .into_iter()
            .map(|r| {
                let poly = &r.bounding_box;
                let points: Vec<(f32, f32)> = poly
                    .exterior()
                    .points()
                    .map(|p| (p.x() as f32, p.y() as f32))
                    .collect();

                let (min_x, min_y, max_x, max_y) = compute_bounds(&points);

                OcrBlock {
                    text: r.text.clone(),
                    bbox: BBox::new(bbox.x + min_x, bbox.y + min_y, max_x - min_x, max_y - min_y),
                    confidence: r.confidence,
                }
            })
            .collect();

        Ok(blocks)
    }

    fn ocr_full_page(&self, image_data: &[u8]) -> Result<Vec<OcrBlock>, PdfError> {
        let img = image::load_from_memory(image_data)
            .map_err(|e| PdfError::Ocr(format!("图片加载失败: {}", e)))?;

        let engine = self
            .engine
            .lock()
            .map_err(|e| PdfError::Ocr(format!("OCR engine lock failed: {}", e)))?;

        let results = engine
            .run_from_image(&img)
            .map_err(|e| PdfError::Ocr(format!("OCR 全页识别失败: {:?}", e)))?;

        let blocks: Vec<OcrBlock> = results
            .into_iter()
            .map(|r| {
                let poly = &r.bounding_box;
                let points: Vec<(f32, f32)> = poly
                    .exterior()
                    .points()
                    .map(|p| (p.x() as f32, p.y() as f32))
                    .collect();

                let (min_x, min_y, max_x, max_y) = compute_bounds(&points);

                OcrBlock {
                    text: r.text.clone(),
                    bbox: BBox::new(min_x, min_y, max_x - min_x, max_y - min_y),
                    confidence: r.confidence,
                }
            })
            .collect();

        Ok(blocks)
    }

    fn supported_languages(&self) -> Vec<String> {
        // PaddleOCR PP-OCRv5 模型原生支持的语言
        vec![
            "zh".to_string(), // 中文
            "en".to_string(), // 英文
            "ja".to_string(), // 日文
            "ko".to_string(), // 韩文
        ]
    }
}

/// 从多边形点集计算包围盒
fn compute_bounds(points: &[(f32, f32)]) -> (f32, f32, f32, f32) {
    if points.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (min_x, min_y, max_x, max_y)
}
