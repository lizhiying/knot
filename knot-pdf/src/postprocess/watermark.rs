//! 水印检测与过滤
//!
//! 水印特征：
//! - 文字旋转角度大（rotation > 10°）
//! - 跨越整页的大面积文本
//! - 多页重复的相同文字内容
//! - 颜色为浅灰色（如可获取）

use super::PostProcessor;
use crate::config::Config;
use crate::ir::{BlockRole, PageIR};

/// 水印过滤器
pub struct WatermarkFilter;

impl WatermarkFilter {
    pub fn new() -> Self {
        Self
    }

    /// 判断一个 block 是否像水印
    fn is_watermark_block(block: &crate::ir::BlockIR, page_area: f32) -> bool {
        let text = block.full_text();

        // 规则 1：已经是 Header/Footer/PageNumber 的不判断为水印
        if matches!(
            block.role,
            BlockRole::Header | BlockRole::Footer | BlockRole::PageNumber
        ) {
            return false;
        }

        // 规则 2：文本太短不可能是水印
        if text.len() < 2 {
            return false;
        }

        // 规则 3：检测重复性的典型水印文本
        let lower = text.to_lowercase();
        let is_common_watermark = [
            "confidential",
            "draft",
            "sample",
            "watermark",
            "do not copy",
            "internal use only",
            "不得复制",
            "机密",
            "草稿",
            "样本",
            "仅供内部使用",
            "版权所有",
        ]
        .iter()
        .any(|w| lower.contains(w));

        if is_common_watermark {
            // 如果是常见水印文本且面积较大
            let block_area = block.bbox.area();
            if block_area > page_area * 0.03 {
                return true;
            }
        }

        // 规则 4：面积极大（占页面 >20%）且文字很少的块 → 可能是水印
        // 但要排除包含联系方式、品牌信息的块（常见于 PPT 尾页）
        let block_area = block.bbox.area();
        let char_count = text.chars().count();
        if block_area > page_area * 0.2 && char_count < 50 {
            // 检查是否包含联系方式或品牌信息（PPT 尾页常见）
            let has_contact_info = lower.contains("http")
                || lower.contains("www")
                || lower.contains("微博")
                || lower.contains("微信")
                || lower.contains("邮箱")
                || lower.contains("电话")
                || lower.contains("热线")
                || lower.contains("@")
                || lower.contains("qr")
                || lower.contains("二维码")
                || lower.contains(".com")
                || lower.contains(".cn")
                || lower.contains(".org");
            if !has_contact_info {
                return true;
            }
        }

        // 规则 5：对角线方向的文本（通过 bbox 形状判断）
        // 如果宽高比非常极端且面积大，可能是倾斜水印
        let aspect = if block.bbox.height > 0.0 {
            block.bbox.width / block.bbox.height
        } else {
            0.0
        };
        if aspect > 5.0 && block_area > page_area * 0.15 && char_count < 30 {
            return true;
        }

        false
    }
}

impl PostProcessor for WatermarkFilter {
    fn name(&self) -> &str {
        "watermark_filter"
    }

    fn process_page(&self, page: &mut PageIR, config: &Config) {
        if !config.remove_watermark {
            return;
        }

        let page_area = page.size.width * page.size.height;

        // 标记水印块
        for block in &mut page.blocks {
            if Self::is_watermark_block(block, page_area) {
                block.role = BlockRole::Watermark;
                log::debug!(
                    "Watermark detected: block_id={}, text='{}'",
                    block.block_id,
                    block.full_text().chars().take(50).collect::<String>()
                );
            }
        }

        // 从正文块中移除水印块
        page.blocks.retain(|b| b.role != BlockRole::Watermark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_page(blocks: Vec<BlockIR>) -> PageIR {
        PageIR {
            page_index: 0,
            size: PageSize {
                width: 612.0,
                height: 792.0,
            },
            rotation: 0.0,
            blocks,
            tables: vec![],
            images: vec![],
            formulas: vec![],
            diagnostics: PageDiagnostics::default(),
            text_score: 1.0,
            is_scanned_guess: false,
            source: PageSource::BornDigital,
            timings: Timings::default(),
        }
    }

    fn make_block(id: &str, text: &str, bbox: BBox) -> BlockIR {
        BlockIR {
            block_id: id.to_string(),
            bbox,
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(bbox),
            }],
            normalized_text: text.to_string(),
        }
    }

    #[test]
    fn test_common_watermark_text() {
        let block = make_block("b1", "CONFIDENTIAL", BBox::new(100.0, 300.0, 400.0, 100.0));
        let page_area = 612.0 * 792.0;
        assert!(WatermarkFilter::is_watermark_block(&block, page_area));
    }

    #[test]
    fn test_large_area_few_chars() {
        let block = make_block("b1", "DRAFT", BBox::new(50.0, 100.0, 500.0, 600.0));
        let page_area = 612.0 * 792.0;
        assert!(WatermarkFilter::is_watermark_block(&block, page_area));
    }

    #[test]
    fn test_normal_text_not_watermark() {
        let block = make_block(
            "b1",
            "This is a normal paragraph with sufficient text content.",
            BBox::new(72.0, 100.0, 468.0, 20.0),
        );
        let page_area = 612.0 * 792.0;
        assert!(!WatermarkFilter::is_watermark_block(&block, page_area));
    }

    #[test]
    fn test_watermark_filter_removes_blocks() {
        let blocks = vec![
            make_block("b1", "Normal text", BBox::new(72.0, 100.0, 468.0, 20.0)),
            make_block("b2", "CONFIDENTIAL", BBox::new(100.0, 300.0, 400.0, 100.0)),
            make_block(
                "b3",
                "More normal text",
                BBox::new(72.0, 140.0, 468.0, 20.0),
            ),
        ];
        let mut page = make_page(blocks);
        let config = Config::default();
        let filter = WatermarkFilter::new();
        filter.process_page(&mut page, &config);

        assert_eq!(page.blocks.len(), 2);
        assert_eq!(page.blocks[0].block_id, "b1");
        assert_eq!(page.blocks[1].block_id, "b3");
    }

    #[test]
    fn test_chinese_watermark() {
        let block = make_block("b1", "机密文件", BBox::new(100.0, 200.0, 400.0, 150.0));
        let page_area = 612.0 * 792.0;
        assert!(WatermarkFilter::is_watermark_block(&block, page_area));
    }
}
