//! M14：Hybrid 解析模式 & 多后端统一
//!
//! 实现 Fast Track / Enhanced / Full 三级解析策略：
//! - FastTrack：纯文本提取 + 规则（最快）
//! - Enhanced：Fast Track + 可选模型增强（版面/表格/公式）
//! - Full：Fast Track + 模型 + VLM 外部调用（最高质量）
//! - Auto：按页面特征自动选择

pub mod fusion;
pub mod strategy;
pub mod vlm;

pub use strategy::{select_parse_strategy, ParseStrategy};
pub use vlm::{MockVlmBackend, VlmBackend, VlmParseResult};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ParseMode};

    #[test]
    fn test_fast_track_strategy() {
        let mut config = Config::default();
        config.parse_mode = ParseMode::FastTrack;
        let strategy = select_parse_strategy(0.9, &config);
        assert_eq!(strategy, ParseStrategy::FastTrackOnly);
    }

    #[test]
    fn test_enhanced_strategy() {
        let mut config = Config::default();
        config.parse_mode = ParseMode::Enhanced;
        let strategy = select_parse_strategy(0.5, &config);
        assert_eq!(strategy, ParseStrategy::FastTrackPlusModels);
    }

    #[test]
    fn test_full_strategy_good_text() {
        let mut config = Config::default();
        config.parse_mode = ParseMode::Full;
        config.vlm_enabled = true;
        let strategy = select_parse_strategy(0.8, &config);
        assert_eq!(strategy, ParseStrategy::FastTrackPlusModels);
    }

    #[test]
    fn test_full_strategy_poor_text() {
        let mut config = Config::default();
        config.parse_mode = ParseMode::Full;
        config.vlm_enabled = true;
        config.vlm_score_threshold = 0.3;
        let strategy = select_parse_strategy(0.2, &config);
        assert_eq!(strategy, ParseStrategy::FullWithVlm);
    }

    #[test]
    fn test_auto_strategy_high_score() {
        let config = Config::default();
        let strategy = select_parse_strategy(0.9, &config);
        assert_eq!(strategy, ParseStrategy::FastTrackOnly);
    }

    #[test]
    fn test_auto_strategy_medium_score() {
        let config = Config::default();
        let strategy = select_parse_strategy(0.5, &config);
        assert_eq!(strategy, ParseStrategy::FastTrackPlusModels);
    }

    #[test]
    fn test_auto_strategy_low_score_no_vlm() {
        let config = Config::default(); // vlm_enabled = false
        let strategy = select_parse_strategy(0.1, &config);
        assert_eq!(strategy, ParseStrategy::FastTrackPlusModels);
    }

    #[test]
    fn test_auto_strategy_low_score_with_vlm() {
        let mut config = Config::default();
        config.vlm_enabled = true;
        let strategy = select_parse_strategy(0.1, &config);
        assert_eq!(strategy, ParseStrategy::FullWithVlm);
    }
}
