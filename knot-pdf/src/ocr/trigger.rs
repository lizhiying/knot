//! OCR 触发逻辑

use crate::config::{Config, OcrMode};
use crate::ir::PageIR;

/// 判断给定页面是否需要触发 OCR
pub fn should_trigger_ocr(page_ir: &PageIR, config: &Config) -> bool {
    // 1. 如果 OCR 已禁用，直接返回 false
    if !config.ocr_enabled || config.ocr_mode == OcrMode::Disabled {
        return false;
    }

    // 2. 如果是强制全文档 OCR 模式
    if config.ocr_mode == OcrMode::ForceAll {
        return true;
    }

    // 3. 自动模式下的判断逻辑
    if config.ocr_mode == OcrMode::Auto {
        // 3.1 基于 PageScore 评分
        if page_ir.text_score < config.scoring_text_threshold {
            return true;
        }

        // 3.2 基于扫描页猜测
        if page_ir.is_scanned_guess {
            return true;
        }

        // 3.3 如果没有提取到任何文本块，且不是空白页（可以通过图片或其他信息判断，目前简单处理）
        if page_ir.blocks.is_empty() && (!page_ir.images.is_empty()) {
            return true;
        }
    }

    false
}
