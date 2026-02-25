//! PDF 后端抽象与默认实现

mod pdf_rs;
mod traits;

#[cfg(feature = "pdfium")]
mod pdfium;

pub use pdf_rs::*;
pub use traits::*;

#[cfg(feature = "pdfium")]
pub use self::pdfium::PdfiumBackend;
