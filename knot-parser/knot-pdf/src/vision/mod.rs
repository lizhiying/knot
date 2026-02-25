//! LLM Vision API 集成
//!
//! 通过外部 LLM API 对图表/图形生成语义描述，而非简单 OCR 文字识别。

mod openai;

pub use openai::OpenAiVisionDescriber;

use crate::error::PdfError;

/// 图片语义描述 trait
///
/// 通过 LLM Vision API 对图片生成自然语言描述。
/// 用于图表、架构图等需要"理解"而非"文字识别"的图片。
pub trait VisionDescriber: Send + Sync {
    /// 对图片生成语义描述
    ///
    /// - `image_png`: PNG 格式的图片字节
    /// - `context_hint`: 可选的上下文提示（如 caption 文字），帮助 LLM 更好理解图片
    fn describe_image(
        &self,
        image_png: &[u8],
        context_hint: Option<&str>,
    ) -> Result<String, PdfError>;
}
