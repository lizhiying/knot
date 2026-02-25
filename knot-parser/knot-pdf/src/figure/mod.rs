//! 图表区域检测模块
//!
//! 识别 PDF 页面中由矢量 Path objects 构成的图表区域（架构图、流程图等），
//! 将其渲染为位图并通过 OCR 提取文字描述。

pub mod detector;
pub mod types;

pub use detector::*;
pub use types::*;
