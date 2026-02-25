//! 图片 IR

use serde::{Deserialize, Serialize};

use super::{BBox, ImageFormat, ImageSource};

/// 图片 IR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageIR {
    /// 图片 ID
    pub image_id: String,
    /// 所在页码
    pub page_index: usize,
    /// 边界框
    pub bbox: BBox,
    /// 图片格式
    #[serde(default)]
    pub format: ImageFormat,
    /// 图片数据引用（可选，延迟加载避免内存复制）
    #[serde(skip)]
    pub bytes_ref: Option<Vec<u8>>,
    /// 关联的标题/说明文本块 ID
    pub caption_refs: Vec<String>,
    /// 图片来源类型
    #[serde(default)]
    pub source: ImageSource,
    /// OCR 提取的文字描述（图表区域渲染后 OCR 得到的文字）
    #[serde(default)]
    pub ocr_text: Option<String>,
}
