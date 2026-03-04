//! # knot-pdf
//!
//! Rust-native offline PDF parser for RAG applications.
//!
//! ## Quick Start
//!
//! ```no_run
//! use knot_pdf::{parse_pdf, Config};
//!
//! let doc = parse_pdf("example.pdf", &Config::default()).unwrap();
//! println!("Pages: {}", doc.pages.len());
//! ```
//!
//! ## Batch Processing (recommended)
//!
//! For batch processing, create a [`Pipeline`] once and reuse it to avoid
//! reloading models (OCR, layout, formula) on every call:
//!
//! ```no_run
//! use knot_pdf::{Pipeline, Config};
//!
//! let pipeline = Pipeline::new(Config::default());
//! for file in &["a.pdf", "b.pdf", "c.pdf"] {
//!     let doc = pipeline.parse_file(file).unwrap();
//!     println!("{}: {} pages", file, doc.pages.len());
//! }
//! ```

pub mod backend;
pub mod config;
pub mod error;
pub mod figure;
pub mod formula;
pub mod hf_detect;
pub mod hybrid;
pub mod ir;
pub mod layout;
pub mod mem_track;
pub mod ocr;
pub mod pipeline;
pub mod postprocess;
pub mod render;
pub mod scoring;
pub mod store;
pub mod table;
#[cfg(feature = "vision")]
pub mod vision;

pub use config::Config;
pub use error::PdfError;
pub use ir::DocumentIR;
pub use pipeline::Pipeline;
pub use render::{MarkdownRenderer, RagExporter};

use std::path::Path;

/// 同步 API：解析单个 PDF 文件，返回完整的 DocumentIR
///
/// **注意**：此函数每次调用都会创建新的 Pipeline 并重新加载所有模型（OCR、版面检测等）。
/// 如果需要批量处理多个 PDF，请使用 [`Pipeline`] 复用实例以避免重复加载：
///
/// ```no_run
/// use knot_pdf::{Pipeline, Config};
///
/// let pipeline = Pipeline::new(Config::default());
/// let doc1 = pipeline.parse_file("a.pdf").unwrap();
/// let doc2 = pipeline.parse_file("b.pdf").unwrap(); // 模型不会重新加载
/// ```
pub fn parse_pdf<P: AsRef<Path>>(path: P, config: &Config) -> Result<DocumentIR, PdfError> {
    let pipeline = Pipeline::new(config.clone());
    pipeline.parse(path.as_ref())
}

/// 迭代器 API：逐页解析 PDF，返回 PageIR 迭代器
///
/// **注意**：同 [`parse_pdf`]，每次调用都会重新加载模型。
/// 批量处理请使用 [`Pipeline`]。
pub fn parse_pdf_pages<P: AsRef<Path>>(
    path: P,
    config: &Config,
) -> Result<impl Iterator<Item = Result<ir::PageIR, PdfError>>, PdfError> {
    let pipeline = Pipeline::new(config.clone());
    pipeline.parse_pages(path.as_ref())
}

// === 异步 API（feature = "async"）===

#[cfg(feature = "async")]
pub use pipeline::async_pipeline::{
    parse_pdf_async, parse_pdf_stream, parse_pdf_with_handler, AsyncParseHandle, AsyncParseStats,
};
