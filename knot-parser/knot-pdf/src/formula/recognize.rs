//! 公式识别 Trait 定义
//!
//! M12 Phase B：公式 OCR → LaTeX
//!
//! 定义公式识别器接口，支持多种后端实现（ONNX、外部 API 等）。

use crate::error::PdfError;

/// 公式识别结果
#[derive(Debug, Clone)]
pub struct FormulaRecognition {
    /// 识别得到的 LaTeX 字符串
    pub latex: String,
    /// 识别置信度 (0.0 ~ 1.0)
    pub confidence: f32,
}

/// 公式识别器 Trait
///
/// 输入：公式区域的渲染图片（PNG 字节）
/// 输出：LaTeX 字符串
pub trait FormulaRecognizer: Send + Sync {
    /// 识别单张公式图片，返回 LaTeX 字符串
    fn recognize(&self, image_bytes: &[u8]) -> Result<FormulaRecognition, PdfError>;

    /// 批量识别（默认实现：逐个调用 recognize）
    fn recognize_batch(&self, images: &[Vec<u8>]) -> Vec<Result<FormulaRecognition, PdfError>> {
        images.iter().map(|img| self.recognize(img)).collect()
    }
}

/// Mock 公式识别器（测试用）
///
/// 返回固定的 LaTeX 占位符
pub struct MockFormulaRecognizer;

impl FormulaRecognizer for MockFormulaRecognizer {
    fn recognize(&self, _image_bytes: &[u8]) -> Result<FormulaRecognition, PdfError> {
        Ok(FormulaRecognition {
            latex: r"\text{[formula]}".to_string(),
            confidence: 0.5,
        })
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_recognizer() {
        let recognizer = MockFormulaRecognizer;
        let result = recognizer.recognize(&[0u8; 100]).unwrap();
        assert!(!result.latex.is_empty());
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_batch_recognize() {
        let recognizer = MockFormulaRecognizer;
        let images = vec![vec![0u8; 100], vec![0u8; 200]];
        let results = recognizer.recognize_batch(&images);
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.is_ok());
        }
    }
}
