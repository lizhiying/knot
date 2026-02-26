//! 图表文字过滤器
//!
//! 检测并过滤掉图表内部的文字块（如柱状图数据标签、轴标签等）。
//!
//! PPT 导出 PDF 中，图表内的数字标签（"97.0%"、"1,747" 等）会被提取为普通文本块，
//! 但这些文本块混入正文会严重干扰内容质量。
//!
//! 检测策略：
//! 1. 识别"数据标签"块：短文本 + 纯数字/百分比/年份格式
//! 2. 当多个数据标签在空间上聚集时，判定为图表区域
//! 3. 将图表区域内的文本块标记为图表注释并过滤

use super::PostProcessor;
use crate::config::Config;
use crate::ir::{BBox, BlockRole, PageIR};

/// 图表文字过滤器
pub struct ChartTextFilter;

impl ChartTextFilter {
    pub fn new() -> Self {
        Self
    }

    /// 判断文本是否像图表数据标签
    fn is_chart_data_label(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.len() > 30 {
            return false;
        }

        // 纯数字：150, 248, 1747, 1,747
        let no_comma = trimmed.replace(',', "");
        if no_comma.chars().all(|c| c.is_ascii_digit())
            && !no_comma.is_empty()
            && no_comma.len() <= 6
        {
            return true;
        }

        // 百分比：97.0%, 65.7%, 28.0%
        if trimmed.ends_with('%') {
            let num_part = &trimmed[..trimmed.len() - 1];
            if num_part.parse::<f32>().is_ok() {
                return true;
            }
        }

        // 年份标签：2022年, 2030年
        if trimmed.ends_with('年') && trimmed.len() <= 7 {
            let num_part = &trimmed[..trimmed.len() - 3]; // UTF-8: "年" = 3 bytes
            if num_part.parse::<u32>().is_ok() {
                return true;
            }
        }

        // 纯数字加点号：35.0, 30.0, 25.0%
        if trimmed.parse::<f32>().is_ok() && trimmed.len() <= 8 {
            return true;
        }

        // 单位标签：亿元、万元
        if trimmed == "亿元" || trimmed == "万元" || trimmed == "%" {
            return true;
        }

        false
    }

    /// 判断文本是否像图表轴标签/图例标签（短文本 + 无动词/句子结构）
    fn is_chart_axis_label(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 10 {
            return false;
        }

        // 类似 "同比变化"、"市场规模"、"单位：亿元"
        if trimmed.starts_with("单位") || trimmed.starts_with("来源") {
            return true;
        }

        false
    }

    /// 聚类分析：检测数据标签的空间聚集
    /// 返回聚集区域的 bounding box
    fn find_chart_regions(
        data_label_bboxes: &[BBox],
        page_width: f32,
        page_height: f32,
    ) -> Vec<BBox> {
        if data_label_bboxes.len() < 4 {
            return vec![];
        }

        // 简单策略：如果 >5 个数据标签集中在页面某个矩形区域内，
        // 用所有标签的外接矩形作为图表区域
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for bbox in data_label_bboxes {
            min_x = min_x.min(bbox.x);
            min_y = min_y.min(bbox.y);
            max_x = max_x.max(bbox.right());
            max_y = max_y.max(bbox.bottom());
        }

        let region_w = max_x - min_x;
        let region_h = max_y - min_y;

        // 区域不应该太大（不应超过页面 70%）
        if region_w > page_width * 0.7 && region_h > page_height * 0.7 {
            return vec![];
        }

        // 区域内标签密度要足够高
        let region_area = region_w * region_h;
        if region_area <= 0.0 {
            return vec![];
        }

        // 加一点 padding（上下左右各扩展 20pt）
        let padding = 20.0;
        vec![BBox::new(
            (min_x - padding).max(0.0),
            (min_y - padding).max(0.0),
            (region_w + padding * 2.0).min(page_width),
            (region_h + padding * 2.0).min(page_height),
        )]
    }
}

impl PostProcessor for ChartTextFilter {
    fn name(&self) -> &str {
        "chart_text_filter"
    }

    fn process_page(&self, page: &mut PageIR, config: &Config) {
        if !config.figure_detection_enabled {
            return;
        }

        // 1. 识别数据标签块
        let data_label_bboxes: Vec<BBox> = page
            .blocks
            .iter()
            .filter(|b| matches!(b.role, BlockRole::Body | BlockRole::Unknown))
            .filter(|b| Self::is_chart_data_label(&b.full_text()))
            .map(|b| b.bbox)
            .collect();

        if data_label_bboxes.len() < 4 {
            return; // 图表至少有 4+ 个数据标签
        }

        // 2. 聚类分析找图表区域
        let chart_regions =
            Self::find_chart_regions(&data_label_bboxes, page.size.width, page.size.height);

        if chart_regions.is_empty() {
            return;
        }

        // 3. 过滤图表区域内的"数据标签"和"轴标签"块
        let mut removed_count = 0;
        for block in &mut page.blocks {
            if block.role != BlockRole::Body && block.role != BlockRole::Unknown {
                continue;
            }

            let text = block.full_text();
            let cx = block.bbox.center_x();
            let cy = block.bbox.center_y();

            // 检查是否在图表区域内
            let in_chart = chart_regions
                .iter()
                .any(|r| cx >= r.x && cx <= r.right() && cy >= r.y && cy <= r.bottom());

            if !in_chart {
                continue;
            }

            // 在图表区域内的数据标签 → 过滤
            if Self::is_chart_data_label(&text) || Self::is_chart_axis_label(&text) {
                block.role = BlockRole::Unknown; // 标记为 Unknown，后续过滤
                removed_count += 1;
            }
        }

        if removed_count > 0 {
            // 移除被标记的块
            page.blocks.retain(|b| {
                if b.role == BlockRole::Unknown {
                    let text = b.full_text();
                    // 只移除数据标签，保留正常的 Unknown 块
                    !(Self::is_chart_data_label(&text) || Self::is_chart_axis_label(&text))
                } else {
                    true
                }
            });
            log::debug!(
                "Chart text filter: removed {} data label blocks from page {}",
                removed_count,
                page.page_index,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    #[test]
    fn test_chart_data_labels() {
        assert!(ChartTextFilter::is_chart_data_label("150"));
        assert!(ChartTextFilter::is_chart_data_label("1,747"));
        assert!(ChartTextFilter::is_chart_data_label("97.0%"));
        assert!(ChartTextFilter::is_chart_data_label("65.7%"));
        assert!(ChartTextFilter::is_chart_data_label("28.0%"));
        assert!(ChartTextFilter::is_chart_data_label("2022年"));
        assert!(ChartTextFilter::is_chart_data_label("2030年"));
        assert!(ChartTextFilter::is_chart_data_label("35.0"));
    }

    #[test]
    fn test_not_chart_labels() {
        assert!(!ChartTextFilter::is_chart_data_label(
            "AI营销投资进入规模化阶段"
        ));
        assert!(!ChartTextFilter::is_chart_data_label(
            "企业将更倾向于在ROI明确见效"
        ));
        assert!(!ChartTextFilter::is_chart_data_label(
            "This is a normal sentence."
        ));
    }

    #[test]
    fn test_chart_axis_labels() {
        assert!(ChartTextFilter::is_chart_axis_label("单位：亿元"));
        assert!(ChartTextFilter::is_chart_axis_label("来源：公开数据"));
        assert!(!ChartTextFilter::is_chart_axis_label(
            "长文本不应该是轴标签因为它太长了"
        ));
    }

    fn make_block(id: &str, text: &str, bbox: BBox) -> BlockIR {
        BlockIR {
            block_id: id.to_string(),
            bbox,
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: text.to_string(),
                    font_size: Some(10.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(bbox),
            }],
            normalized_text: text.to_string(),
        }
    }

    #[test]
    fn test_chart_region_detection() {
        let bboxes = vec![
            BBox::new(100.0, 300.0, 40.0, 15.0), // "150"
            BBox::new(150.0, 300.0, 40.0, 15.0), // "248"
            BBox::new(200.0, 300.0, 40.0, 15.0), // "349"
            BBox::new(250.0, 300.0, 40.0, 15.0), // "471"
            BBox::new(300.0, 300.0, 40.0, 15.0), // "636"
        ];
        let regions = ChartTextFilter::find_chart_regions(&bboxes, 612.0, 792.0);
        assert_eq!(regions.len(), 1);
    }

    #[test]
    fn test_filter_chart_text_in_page() {
        let blocks = vec![
            make_block(
                "b1",
                "AI营销投资进入规模化阶段",
                BBox::new(72.0, 50.0, 400.0, 20.0),
            ),
            // 图表数据标签（聚集在 y=300 区域）
            make_block("b2", "150", BBox::new(100.0, 300.0, 30.0, 12.0)),
            make_block("b3", "248", BBox::new(150.0, 300.0, 30.0, 12.0)),
            make_block("b4", "349", BBox::new(200.0, 300.0, 30.0, 12.0)),
            make_block("b5", "471", BBox::new(250.0, 300.0, 30.0, 12.0)),
            make_block("b6", "636", BBox::new(300.0, 300.0, 30.0, 12.0)),
            make_block("b7", "97.0%", BBox::new(100.0, 280.0, 40.0, 12.0)),
            make_block("b8", "65.7%", BBox::new(150.0, 280.0, 40.0, 12.0)),
            make_block(
                "b9",
                "正文内容在图表区域外",
                BBox::new(72.0, 500.0, 400.0, 20.0),
            ),
        ];
        let mut page = PageIR {
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
        };

        let config = Config::default();
        let filter = ChartTextFilter::new();
        filter.process_page(&mut page, &config);

        // 正文块应该保留
        assert!(page
            .blocks
            .iter()
            .any(|b| b.normalized_text.contains("AI营销")));
        assert!(page
            .blocks
            .iter()
            .any(|b| b.normalized_text.contains("正文内容")));

        // 数据标签块应该被过滤
        assert!(!page.blocks.iter().any(|b| b.normalized_text == "150"));
        assert!(!page.blocks.iter().any(|b| b.normalized_text == "97.0%"));
    }
}
