//! 公式检测与识别模块
//!
//! M12：公式区域检测（Phase A）+ 公式 OCR → LaTeX（Phase B）

pub mod detect;
#[cfg(feature = "formula_model")]
pub mod onnx_recognize;
pub mod recognize;

pub use detect::*;
#[cfg(feature = "formula_model")]
pub use onnx_recognize::*;
pub use recognize::*;
