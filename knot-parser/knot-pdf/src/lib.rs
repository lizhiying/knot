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

pub mod backend;
pub mod config;
pub mod error;
pub mod figure;
pub mod formula;
pub mod hf_detect;
pub mod ir;
pub mod layout;
pub mod mem_track;
pub mod ocr;
pub mod pipeline;
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

/// 同步 API：解析 PDF 文件，返回完整的 DocumentIR
pub fn parse_pdf<P: AsRef<Path>>(path: P, config: &Config) -> Result<DocumentIR, PdfError> {
    let pipeline = Pipeline::new(config.clone());
    pipeline.parse(path.as_ref())
}

/// 迭代器 API：逐页解析 PDF，返回 PageIR 迭代器
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
