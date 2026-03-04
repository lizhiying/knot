//! 页面级解析策略选择
//!
//! 根据 Config.parse_mode 和页面 text_score 自动决定解析策略。

use crate::config::{Config, ParseMode};

/// 具体的解析策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseStrategy {
    /// 仅 Fast Track（纯文本提取 + 规则）
    FastTrackOnly,
    /// Fast Track + 模型增强（版面/表格/公式 + 后处理）
    FastTrackPlusModels,
    /// 完整混合模式：Fast Track + 模型 + VLM 外部调用
    FullWithVlm,
}

impl ParseStrategy {
    /// 是否需要模型增强
    pub fn needs_models(&self) -> bool {
        matches!(self, Self::FastTrackPlusModels | Self::FullWithVlm)
    }

    /// 是否需要 VLM
    pub fn needs_vlm(&self) -> bool {
        matches!(self, Self::FullWithVlm)
    }

    /// 显示名称
    pub fn display_name(&self) -> &str {
        match self {
            Self::FastTrackOnly => "FastTrack",
            Self::FastTrackPlusModels => "Enhanced",
            Self::FullWithVlm => "Full+VLM",
        }
    }
}

/// 根据页面文本质量分数和配置选择解析策略
///
/// # Arguments
/// - `text_score`: 页面文本质量分数（0.0 ~ 1.0）
/// - `config`: 全局配置
///
/// # Returns
/// 选定的解析策略
pub fn select_parse_strategy(text_score: f32, config: &Config) -> ParseStrategy {
    match config.parse_mode {
        ParseMode::FastTrack => ParseStrategy::FastTrackOnly,

        ParseMode::Enhanced => ParseStrategy::FastTrackPlusModels,

        ParseMode::Full => {
            if text_score > 0.7 {
                // 文字质量好，模型增强就够了
                ParseStrategy::FastTrackPlusModels
            } else if config.vlm_enabled {
                ParseStrategy::FullWithVlm
            } else {
                // VLM 未配置，回退到模型增强
                ParseStrategy::FastTrackPlusModels
            }
        }

        ParseMode::Auto => auto_select(text_score, config),
    }
}

/// Auto 模式的自动策略选择
fn auto_select(text_score: f32, config: &Config) -> ParseStrategy {
    if text_score >= 0.7 {
        // 文字质量高，Fast Track 足够
        ParseStrategy::FastTrackOnly
    } else if text_score < config.vlm_score_threshold && config.vlm_enabled {
        // 文字质量极低 + VLM 可用 → Full 模式
        ParseStrategy::FullWithVlm
    } else {
        // 中等质量 → 模型增强
        ParseStrategy::FastTrackPlusModels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_display_names() {
        assert_eq!(ParseStrategy::FastTrackOnly.display_name(), "FastTrack");
        assert_eq!(
            ParseStrategy::FastTrackPlusModels.display_name(),
            "Enhanced"
        );
        assert_eq!(ParseStrategy::FullWithVlm.display_name(), "Full+VLM");
    }

    #[test]
    fn test_strategy_needs() {
        assert!(!ParseStrategy::FastTrackOnly.needs_models());
        assert!(!ParseStrategy::FastTrackOnly.needs_vlm());
        assert!(ParseStrategy::FastTrackPlusModels.needs_models());
        assert!(!ParseStrategy::FastTrackPlusModels.needs_vlm());
        assert!(ParseStrategy::FullWithVlm.needs_models());
        assert!(ParseStrategy::FullWithVlm.needs_vlm());
    }
}
