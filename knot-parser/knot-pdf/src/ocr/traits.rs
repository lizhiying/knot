//! OCR Trait 定义

use crate::ir::BBox;
use serde::{Deserialize, Serialize};

/// OCR 识别出的文本块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBlock {
    /// 识别出的文本
    pub text: String,
    /// 边界框
    pub bbox: BBox,
    /// 置信度 (0.0 ~ 1.0)
    pub confidence: f32,
}

/// OCR 后端 Trait
pub trait OcrBackend: Send + Sync {
    /// 对给定图片的特定区域进行 OCR
    fn ocr_region(
        &self,
        image_data: &[u8],
        bbox: BBox,
    ) -> Result<Vec<OcrBlock>, crate::error::PdfError>;

    /// 对整页图片进行 OCR
    fn ocr_full_page(&self, image_data: &[u8]) -> Result<Vec<OcrBlock>, crate::error::PdfError>;

    /// 获取支持的语言列表
    fn supported_languages(&self) -> Vec<String>;
}
