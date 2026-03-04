//! OCR 抽象与实现

pub mod integration;
pub mod mock;
pub mod traits;
pub mod trigger;

#[cfg(feature = "ocr_tesseract")]
pub mod tesseract;

#[cfg(feature = "ocr_paddle")]
pub mod paddle;

pub use integration::*;
pub use mock::*;
pub use traits::*;
pub use trigger::*;

#[cfg(feature = "ocr_tesseract")]
pub use tesseract::*;

#[cfg(feature = "ocr_paddle")]
pub use paddle::*;
