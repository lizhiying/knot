//! Mock OCR 后端，用于测试

use crate::error::PdfError;
use crate::ir::BBox;
use crate::ocr::traits::{OcrBackend, OcrBlock};

pub struct MockOcrBackend;

impl OcrBackend for MockOcrBackend {
    fn ocr_region(&self, _image_data: &[u8], bbox: BBox) -> Result<Vec<OcrBlock>, PdfError> {
        Ok(vec![OcrBlock {
            text: "Mocked region text".to_string(),
            bbox,
            confidence: 0.9,
        }])
    }

    fn ocr_full_page(&self, _image_data: &[u8]) -> Result<Vec<OcrBlock>, PdfError> {
        Ok(vec![OcrBlock {
            text: "Mocked full page text".to_string(),
            bbox: BBox::new(0.0, 0.0, 500.0, 700.0),
            confidence: 0.85,
        }])
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["eng".to_string(), "chi_sim".to_string()]
    }
}
