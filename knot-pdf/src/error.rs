//! 错误类型定义

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PDF parse error: {0}")]
    Parse(String),

    #[error("PDF is encrypted")]
    Encrypted,

    #[error("PDF is corrupted: {0}")]
    Corrupted(String),

    #[error("Page {0} not found")]
    PageNotFound(usize),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    #[error("OCR error: {0}")]
    Ocr(String),

    #[error("Store error: {0}")]
    Store(String),

    #[error("Operation timed out: {0}")]
    Timeout(String),
}
