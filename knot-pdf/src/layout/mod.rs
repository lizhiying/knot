//! 布局分析模块：阅读顺序重建、XY-Cut 递归分割、版面检测

pub mod detect;
#[cfg(feature = "layout_model")]
pub mod onnx_detect;
mod reading_order;
pub mod xy_cut;

pub use detect::{
    compute_iou, merge_layout_with_blocks, nms, LayoutDetector, LayoutLabel, LayoutRegion,
    MockLayoutDetector,
};
#[cfg(feature = "layout_model")]
pub use onnx_detect::{ClassSchema, OnnxLayoutDetector};
pub use reading_order::*;
pub use xy_cut::{xy_cut_order, XyCutConfig};
