//! Tesseract OCR 后端实现

use crate::error::PdfError;
use crate::ir::BBox;
use crate::ocr::traits::{OcrBackend, OcrBlock};
use leptess::{LepTess, Variable};

pub struct TesseractBackend {
    lt: LepTess,
}

impl TesseractBackend {
    pub fn new(languages: &[String]) -> Result<Self, PdfError> {
        let lang_str = languages.join("+");
        let lt = LepTess::new(None, &lang_str)
            .map_err(|e| PdfError::Ocr(format!("Failed to initialize Tesseract: {:?}", e)))?;
        Ok(Self { lt })
    }
}

impl OcrBackend for TesseractBackend {
    fn ocr_region(&self, image_data: &[u8], bbox: BBox) -> Result<Vec<OcrBlock>, PdfError> {
        let mut lt = LepTess::new(None, "eng") // Simple re-init or use internal state
            .map_err(|e| PdfError::Ocr(e.to_string()))?;

        lt.set_image_from_mem(image_data)
            .map_err(|e| PdfError::Ocr(e.to_string()))?;

        // Set rectangle
        lt.set_rectangle(
            bbox.x as i32,
            bbox.y as i32,
            bbox.width as i32,
            bbox.height as i32,
        );

        let text = lt
            .get_utf8_text()
            .map_err(|e| PdfError::Ocr(e.to_string()))?;

        Ok(vec![OcrBlock {
            text,
            bbox,
            confidence: 0.8, // LepTess doesn't easily expose block-level confidence without iterator
        }])
    }

    fn ocr_full_page(&self, image_data: &[u8]) -> Result<Vec<OcrBlock>, PdfError> {
        let mut lt = LepTess::new(None, "eng").map_err(|e| PdfError::Ocr(e.to_string()))?;

        lt.set_image_from_mem(image_data)
            .map_err(|e| PdfError::Ocr(e.to_string()))?;

        let text = lt
            .get_utf8_text()
            .map_err(|e| PdfError::Ocr(e.to_string()))?;

        // Simple heuristic for full page bbox
        Ok(vec![OcrBlock {
            text,
            bbox: BBox::new(0.0, 0.0, 0.0, 0.0), // Should ideally get actual dimensions from image
            confidence: 0.8,
        }])
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["eng".to_string()]
    }
}
