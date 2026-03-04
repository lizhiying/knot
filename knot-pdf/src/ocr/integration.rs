//! OCR 集成逻辑：渲染与结果回填

use crate::error::PdfError;
use crate::ir::*;
use crate::ocr::traits::{OcrBackend, OcrBlock};
use std::time::Instant;

/// OCR 任务结果
pub struct OcrResult {
    pub blocks: Vec<OcrBlock>,
    pub duration_ms: u64,
}

/// 执行页面 OCR 并回填 IR
///
/// `force_replace`: 如果为 true，OCR 结果将完全替换原有文本块
/// （用于 `force_all` 模式或原始文本质量过低的情况）
pub fn run_ocr_and_update_page(
    page_ir: &mut PageIR,
    ocr_backend: &dyn OcrBackend,
    image_data: &[u8],
    force_replace: bool,
) -> Result<(), PdfError> {
    let start = Instant::now();

    // 1. 执行 OCR
    let ocr_blocks = ocr_backend.ocr_full_page(image_data)?;
    let duration_ms = start.elapsed().as_millis() as u64;

    // 2. 将 OcrBlock 转换为 BlockIR 并回填
    let mut new_blocks = Vec::new();
    for (i, ocr_b) in ocr_blocks.iter().enumerate() {
        let block_id = format!("ocr_p{}_{}", page_ir.page_index, i);

        // 构建行和 Span
        let line = TextLine {
            spans: vec![TextSpan {
                text: ocr_b.text.clone(),
                font_size: None,
                is_bold: false,
                font_name: None,
            }],
            bbox: Some(ocr_b.bbox),
        };

        new_blocks.push(BlockIR {
            block_id,
            bbox: ocr_b.bbox,
            role: BlockRole::Body,
            lines: vec![line],
            normalized_text: ocr_b.text.clone(),
        });
    }

    // 3. 更新 PageIR
    if force_replace || page_ir.blocks.is_empty() {
        // force_all 模式或原先无文本：完全使用 OCR 结果
        page_ir.source = PageSource::Ocr;
        page_ir.blocks = new_blocks;
    } else {
        // auto 模式且有部分文本：混合追加
        page_ir.source = PageSource::Mixed;
        page_ir.blocks.extend(new_blocks);
    }

    page_ir.timings.ocr_ms = Some(duration_ms);

    // 计算平均置信度作为质量分
    if !ocr_blocks.is_empty() {
        let total_conf: f32 = ocr_blocks.iter().map(|b| b.confidence).sum();
        page_ir.diagnostics.ocr_quality_score = Some(total_conf / ocr_blocks.len() as f32);
    }

    Ok(())
}
