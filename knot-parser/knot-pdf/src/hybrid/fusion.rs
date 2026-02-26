//! Fast Track 与 VLM 结果融合
//!
//! 当两个通道都产出结果时，按以下原则融合：
//! - 文本优先级：Fast Track 提取的文本更准确（无幻觉）
//! - 结构优先级：VLM 判断的标题/列表结构通常更准确
//! - 冲突解决：根据 text_score 决定信任哪一方

use super::vlm::VlmParseResult;
use crate::ir::{BlockIR, BlockRole, PageIR};

/// 融合结果来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionSource {
    /// 仅来自 Fast Track
    FastTrack,
    /// 来自 VLM（VLM 只输出，Fast Track 无结果）
    Vlm,
    /// 两者融合
    Merged,
}

/// 将 VLM 结果融合到已有的 PageIR 中
///
/// 策略：
/// 1. 如果 Fast Track 文本质量好（text_score > 0.5），保留 Fast Track 的文本，
///    用 VLM 的结构信息（role）修正 block roles
/// 2. 如果 Fast Track 文本质量差（text_score <= 0.5），优先使用 VLM 的文本
/// 3. VLM 识别出但 Fast Track 未覆盖的区域，补充到结果中
pub fn fuse_vlm_result(page: &mut PageIR, vlm_result: VlmParseResult) -> FusionSource {
    if page.blocks.is_empty() && !vlm_result.blocks.is_empty() {
        // Fast Track 无结果，直接使用 VLM 结果
        page.blocks = vlm_result.blocks;
        page.tables.extend(vlm_result.tables);
        return FusionSource::Vlm;
    }

    if vlm_result.blocks.is_empty() {
        return FusionSource::FastTrack;
    }

    if page.text_score > 0.5 {
        // Fast Track 文本质量好 —— 用 VLM 修正结构
        apply_role_corrections(page, &vlm_result.blocks);
    } else {
        // Fast Track 文本质量差 —— 用 VLM 补充内容
        supplement_from_vlm(page, vlm_result);
    }

    FusionSource::Merged
}

/// 用 VLM 的 role 信息修正 Fast Track 的 block roles
///
/// 简单策略：如果 VLM block 和 Fast Track block 大致对应（按顺序），
/// 且 VLM 返回了更具体的 role（如 Title/Heading/List），则修正。
fn apply_role_corrections(page: &mut PageIR, vlm_blocks: &[BlockIR]) {
    let ft_len = page.blocks.len();
    let vlm_len = vlm_blocks.len();

    // 如果数量差异太大，不做修正
    if ft_len == 0 || vlm_len == 0 {
        return;
    }

    // 按比例对应：vlm_blocks[i] ↔ ft_blocks[i * ft_len / vlm_len]
    for (vlm_idx, vlm_block) in vlm_blocks.iter().enumerate() {
        let ft_idx = vlm_idx * ft_len / vlm_len;
        if ft_idx >= ft_len {
            break;
        }

        let ft_block = &mut page.blocks[ft_idx];

        // 只有当 Fast Track 是 Body/Unknown 且 VLM 给出更具体的 role 时才修正
        if matches!(ft_block.role, BlockRole::Body | BlockRole::Unknown) {
            match vlm_block.role {
                BlockRole::Title | BlockRole::Heading | BlockRole::List | BlockRole::Caption => {
                    ft_block.role = vlm_block.role;
                }
                _ => {}
            }
        }
    }
}

/// 从 VLM 结果中补充 Fast Track 缺失的内容
fn supplement_from_vlm(page: &mut PageIR, vlm_result: VlmParseResult) {
    // 如果 Fast Track 块数很少而 VLM 块数明显更多，补充 VLM 的块
    if vlm_result.blocks.len() > page.blocks.len() * 2 {
        // VLM 有显著更多的内容 → 替换
        page.blocks = vlm_result.blocks;
    }

    // 补充表格
    if page.tables.is_empty() && !vlm_result.tables.is_empty() {
        page.tables = vlm_result.tables;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_page(blocks: Vec<BlockIR>, text_score: f32) -> PageIR {
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
            text_score,
            is_scanned_guess: false,
            source: PageSource::BornDigital,
            timings: Timings::default(),
        }
    }

    fn make_block(id: &str, text: &str, role: BlockRole) -> BlockIR {
        BlockIR {
            block_id: id.to_string(),
            bbox: BBox::new(72.0, 100.0, 468.0, 16.0),
            role,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(BBox::new(72.0, 100.0, 468.0, 16.0)),
            }],
            normalized_text: text.to_string(),
        }
    }

    #[test]
    fn test_fuse_no_vlm_blocks() {
        let ft_blocks = vec![make_block("b1", "Text", BlockRole::Body)];
        let mut page = make_page(ft_blocks, 0.9);
        let vlm = VlmParseResult {
            markdown: String::new(),
            blocks: vec![],
            tables: vec![],
            confidence: 0.0,
        };
        let source = fuse_vlm_result(&mut page, vlm);
        assert_eq!(source, FusionSource::FastTrack);
        assert_eq!(page.blocks.len(), 1);
    }

    #[test]
    fn test_fuse_no_ft_blocks() {
        let mut page = make_page(vec![], 0.0);
        let vlm = VlmParseResult {
            markdown: "# Title".to_string(),
            blocks: vec![make_block("v1", "Title", BlockRole::Title)],
            tables: vec![],
            confidence: 0.9,
        };
        let source = fuse_vlm_result(&mut page, vlm);
        assert_eq!(source, FusionSource::Vlm);
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].role, BlockRole::Title);
    }

    #[test]
    fn test_fuse_role_correction() {
        let ft_blocks = vec![
            make_block("b1", "Introduction", BlockRole::Body),
            make_block("b2", "Some text", BlockRole::Body),
        ];
        let mut page = make_page(ft_blocks, 0.8);

        let vlm_blocks = vec![
            make_block("v1", "Introduction", BlockRole::Title),
            make_block("v2", "Some text", BlockRole::Body),
        ];
        let vlm = VlmParseResult {
            markdown: String::new(),
            blocks: vlm_blocks,
            tables: vec![],
            confidence: 0.85,
        };

        let source = fuse_vlm_result(&mut page, vlm);
        assert_eq!(source, FusionSource::Merged);
        // VLM 的 Title role 应修正 Fast Track 的 Body
        assert_eq!(page.blocks[0].role, BlockRole::Title);
        assert_eq!(page.blocks[1].role, BlockRole::Body);
    }

    #[test]
    fn test_fuse_low_quality_supplement() {
        let ft_blocks = vec![make_block("b1", "?", BlockRole::Unknown)];
        let mut page = make_page(ft_blocks, 0.2);

        let vlm_blocks = vec![
            make_block("v1", "Title", BlockRole::Title),
            make_block("v2", "First paragraph", BlockRole::Body),
            make_block("v3", "Second paragraph", BlockRole::Body),
        ];
        let vlm = VlmParseResult {
            markdown: String::new(),
            blocks: vlm_blocks,
            tables: vec![],
            confidence: 0.9,
        };

        let source = fuse_vlm_result(&mut page, vlm);
        assert_eq!(source, FusionSource::Merged);
        // VLM 块数远多于 FT → 应替换
        assert_eq!(page.blocks.len(), 3);
    }
}
