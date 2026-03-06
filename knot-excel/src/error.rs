//! 错误类型定义

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExcelError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Excel parse error: {0}")]
    Parse(String),

    #[error("Sheet not found: {0}")]
    SheetNotFound(String),

    #[error("Empty file: no data found")]
    EmptyFile,

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Calamine error: {0}")]
    Calamine(String),
}
