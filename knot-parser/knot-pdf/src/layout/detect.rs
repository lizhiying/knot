//! 版面检测模块：轻量级 ONNX 模型驱动的版面分析
//!
//! 提供 LayoutDetector trait 和相关数据结构。
//! 通过 feature gate `layout_model` 控制是否编译 ONNX 推理实现。

use crate::ir::BBox;

/// 版面检测后端 trait
///
/// 对页面图片进行版面检测，输出检测到的版面区域列表。
pub trait LayoutDetector: Send + Sync {
    /// 对页面图片进行版面检测
    ///
    /// # Arguments
    /// * `image_data` - PNG 格式的页面图片数据
    /// * `page_width` - 页面宽度（PDF 坐标)
    /// * `page_height` - 页面高度（PDF 坐标）
    ///
    /// # Returns
    /// 检测到的版面区域列表
    fn detect(
        &self,
        image_data: &[u8],
        page_width: f32,
        page_height: f32,
    ) -> Result<Vec<LayoutRegion>, crate::error::PdfError>;

    /// 模型名称（用于日志）
    fn name(&self) -> &str;
}

/// 检测到的版面区域
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayoutRegion {
    /// 边界框（PDF 坐标系）
    pub bbox: BBox,
    /// 版面标签
    pub label: LayoutLabel,
    /// 检测置信度 (0.0 ~ 1.0)
    pub confidence: f32,
}

/// 版面标签
///
/// 对应模型输出的类别，参考 DocLayNet / PubLayNet 数据集的标注体系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LayoutLabel {
    /// 文档标题 (H1)
    Title,
    /// 章节标题 (H2/H3)
    Heading,
    /// 正文段落
    Paragraph,
    /// 表格区域
    Table,
    /// 图片/图表区域
    Figure,
    /// 列表
    List,
    /// 图表/表格说明文字
    Caption,
    /// 页眉
    Header,
    /// 页脚
    Footer,
    /// 页码
    PageNumber,
    /// 数学公式
    Formula,
    /// 未知
    Unknown,
}

impl LayoutLabel {
    /// 从模型类别 ID 转换（DocLayNet 11 类标注体系）
    pub fn from_class_id(id: usize) -> Self {
        match id {
            0 => LayoutLabel::Paragraph,
            1 => LayoutLabel::Title,
            2 => LayoutLabel::Heading,
            3 => LayoutLabel::Table,
            4 => LayoutLabel::Figure,
            5 => LayoutLabel::List,
            6 => LayoutLabel::Caption,
            7 => LayoutLabel::Header,
            8 => LayoutLabel::Footer,
            9 => LayoutLabel::PageNumber,
            10 => LayoutLabel::Formula,
            _ => LayoutLabel::Unknown,
        }
    }

    /// 转换为 BlockRole
    pub fn to_block_role(&self) -> crate::ir::BlockRole {
        use crate::ir::BlockRole;
        match self {
            LayoutLabel::Title => BlockRole::Title,
            LayoutLabel::Heading => BlockRole::Heading,
            LayoutLabel::Paragraph => BlockRole::Body,
            LayoutLabel::List => BlockRole::List,
            LayoutLabel::Caption => BlockRole::Caption,
            LayoutLabel::Header => BlockRole::Header,
            LayoutLabel::Footer => BlockRole::Footer,
            LayoutLabel::PageNumber => BlockRole::PageNumber,
            LayoutLabel::Table => BlockRole::Body, // 表格通过 TableIR 处理
            LayoutLabel::Figure => BlockRole::Body, // 图片通过 ImageIR 处理
            LayoutLabel::Formula => BlockRole::Body,
            LayoutLabel::Unknown => BlockRole::Unknown,
        }
    }

    /// 是否是块级文本角色（可用于 override BlockIR.role）
    pub fn is_text_role(&self) -> bool {
        matches!(
            self,
            LayoutLabel::Title
                | LayoutLabel::Heading
                | LayoutLabel::Paragraph
                | LayoutLabel::List
                | LayoutLabel::Caption
                | LayoutLabel::Header
                | LayoutLabel::Footer
                | LayoutLabel::PageNumber
        )
    }
}

/// Mock 版面检测器（无模型时使用）
pub struct MockLayoutDetector;

impl LayoutDetector for MockLayoutDetector {
    fn detect(
        &self,
        _image_data: &[u8],
        _page_width: f32,
        _page_height: f32,
    ) -> Result<Vec<LayoutRegion>, crate::error::PdfError> {
        Ok(Vec::new())
    }

    fn name(&self) -> &str {
        "mock"
    }
}

/// 非极大值抑制 (NMS - Non-Maximum Suppression)
///
/// 移除重叠度超过阈值的低置信度检测框。
pub fn nms(regions: &mut Vec<LayoutRegion>, iou_threshold: f32) {
    if regions.len() <= 1 {
        return;
    }

    // 按置信度降序排序
    regions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut keep = vec![true; regions.len()];

    for i in 0..regions.len() {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..regions.len() {
            if !keep[j] {
                continue;
            }
            let iou = compute_iou(&regions[i].bbox, &regions[j].bbox);
            if iou > iou_threshold {
                keep[j] = false; // 抑制低置信度的框
            }
        }
    }

    let mut idx = 0;
    regions.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

/// 计算两个 BBox 的 IoU (Intersection over Union)
pub fn compute_iou(a: &BBox, b: &BBox) -> f32 {
    let x_overlap = a.x.max(b.x) < a.right().min(b.right());
    let y_overlap = a.y.max(b.y) < a.bottom().min(b.bottom());

    if !x_overlap || !y_overlap {
        return 0.0;
    }

    let inter_w = a.right().min(b.right()) - a.x.max(b.x);
    let inter_h = a.bottom().min(b.bottom()) - a.y.max(b.y);
    let inter_area = inter_w * inter_h;

    let area_a = a.area();
    let area_b = b.area();
    let union_area = area_a + area_b - inter_area;

    if union_area <= 0.0 {
        0.0
    } else {
        inter_area / union_area
    }
}

/// 将版面检测结果与现有 BlockIR 融合
///
/// 策略：
/// 1. 对每个 BlockIR，找到与之 IoU 最大的 LayoutRegion
/// 2. 如果 IoU > min_overlap 且置信度 > threshold，用 LayoutRegion 的 label 覆盖 block.role
/// 3. 对文本角色 (Title/Heading/List/Caption 等) 直接覆盖
/// 4. 对非文本角色 (Table/Figure) 不覆盖 block.role（它们由 TableIR / ImageIR 处理）
pub fn merge_layout_with_blocks(
    blocks: &mut [crate::ir::BlockIR],
    regions: &[LayoutRegion],
    confidence_threshold: f32,
    min_overlap: f32,
) {
    for block in blocks.iter_mut() {
        let mut best_match: Option<(f32, &LayoutRegion)> = None;

        for region in regions {
            if region.confidence < confidence_threshold {
                continue;
            }

            let iou = compute_iou(&block.bbox, &region.bbox);
            if iou > min_overlap {
                if best_match.is_none() || iou > best_match.unwrap().0 {
                    best_match = Some((iou, region));
                }
            }
        }

        if let Some((_iou, region)) = best_match {
            // 只覆盖文本角色
            if region.label.is_text_role() {
                let new_role = region.label.to_block_role();
                log::debug!(
                    "Layout model override: block '{}' role {:?} → {:?} (conf={:.2})",
                    &block.block_id,
                    block.role,
                    new_role,
                    region.confidence,
                );
                block.role = new_role;
            }
        }
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::BBox;

    #[test]
    fn test_layout_label_from_class_id() {
        assert_eq!(LayoutLabel::from_class_id(0), LayoutLabel::Paragraph);
        assert_eq!(LayoutLabel::from_class_id(1), LayoutLabel::Title);
        assert_eq!(LayoutLabel::from_class_id(3), LayoutLabel::Table);
        assert_eq!(LayoutLabel::from_class_id(99), LayoutLabel::Unknown);
    }

    #[test]
    fn test_layout_label_to_block_role() {
        use crate::ir::BlockRole;
        assert_eq!(LayoutLabel::Title.to_block_role(), BlockRole::Title);
        assert_eq!(LayoutLabel::Heading.to_block_role(), BlockRole::Heading);
        assert_eq!(LayoutLabel::List.to_block_role(), BlockRole::List);
        assert_eq!(LayoutLabel::Caption.to_block_role(), BlockRole::Caption);
        assert_eq!(
            LayoutLabel::PageNumber.to_block_role(),
            BlockRole::PageNumber
        );
    }

    #[test]
    fn test_layout_region_serde() {
        let region = LayoutRegion {
            bbox: BBox::new(10.0, 20.0, 100.0, 50.0),
            label: LayoutLabel::Title,
            confidence: 0.95,
        };

        let json = serde_json::to_string(&region).unwrap();
        let region2: LayoutRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(region2.label, LayoutLabel::Title);
        assert!((region2.confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_compute_iou_no_overlap() {
        let a = BBox::new(0.0, 0.0, 10.0, 10.0);
        let b = BBox::new(20.0, 20.0, 10.0, 10.0);
        assert_eq!(compute_iou(&a, &b), 0.0);
    }

    #[test]
    fn test_compute_iou_full_overlap() {
        let a = BBox::new(0.0, 0.0, 10.0, 10.0);
        assert!((compute_iou(&a, &a) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_iou_partial_overlap() {
        let a = BBox::new(0.0, 0.0, 10.0, 10.0);
        let b = BBox::new(5.0, 5.0, 10.0, 10.0);
        // 交集: [5,10] x [5,10] = 25
        // 并集: 100 + 100 - 25 = 175
        let iou = compute_iou(&a, &b);
        assert!((iou - 25.0 / 175.0).abs() < 0.01);
    }

    #[test]
    fn test_nms_basic() {
        let mut regions = vec![
            LayoutRegion {
                bbox: BBox::new(10.0, 10.0, 100.0, 50.0),
                label: LayoutLabel::Title,
                confidence: 0.95,
            },
            LayoutRegion {
                bbox: BBox::new(12.0, 12.0, 100.0, 50.0), // 高重叠
                label: LayoutLabel::Title,
                confidence: 0.80,
            },
            LayoutRegion {
                bbox: BBox::new(300.0, 300.0, 100.0, 50.0), // 不重叠
                label: LayoutLabel::Paragraph,
                confidence: 0.90,
            },
        ];

        nms(&mut regions, 0.5);

        // 高重叠的低置信度一个被抑制
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].label, LayoutLabel::Title);
        assert!((regions[0].confidence - 0.95).abs() < 0.01);
        assert_eq!(regions[1].label, LayoutLabel::Paragraph);
    }

    #[test]
    fn test_nms_no_overlap() {
        let mut regions = vec![
            LayoutRegion {
                bbox: BBox::new(0.0, 0.0, 50.0, 50.0),
                label: LayoutLabel::Title,
                confidence: 0.9,
            },
            LayoutRegion {
                bbox: BBox::new(200.0, 200.0, 50.0, 50.0),
                label: LayoutLabel::Paragraph,
                confidence: 0.8,
            },
        ];

        nms(&mut regions, 0.5);
        assert_eq!(regions.len(), 2); // 都保留
    }

    #[test]
    fn test_merge_layout_with_blocks() {
        use crate::ir::{BlockIR, BlockRole, TextLine, TextSpan};

        let mut blocks = vec![
            BlockIR {
                block_id: "blk_0".to_string(),
                bbox: BBox::new(50.0, 30.0, 500.0, 30.0),
                role: BlockRole::Body,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: "Attention Is All You Need".to_string(),
                        font_size: Some(20.0),
                        is_bold: true,
                        font_name: None,
                    }],
                    bbox: Some(BBox::new(50.0, 30.0, 500.0, 30.0)),
                }],
                normalized_text: "Attention Is All You Need".to_string(),
            },
            BlockIR {
                block_id: "blk_1".to_string(),
                bbox: BBox::new(50.0, 100.0, 500.0, 60.0),
                role: BlockRole::Body,
                lines: vec![TextLine {
                    spans: vec![TextSpan {
                        text: "Some body text.".to_string(),
                        font_size: Some(12.0),
                        is_bold: false,
                        font_name: None,
                    }],
                    bbox: Some(BBox::new(50.0, 100.0, 500.0, 60.0)),
                }],
                normalized_text: "Some body text.".to_string(),
            },
        ];

        let regions = vec![
            LayoutRegion {
                bbox: BBox::new(45.0, 25.0, 510.0, 40.0),
                label: LayoutLabel::Title,
                confidence: 0.92,
            },
            LayoutRegion {
                bbox: BBox::new(45.0, 95.0, 510.0, 70.0),
                label: LayoutLabel::Paragraph,
                confidence: 0.88,
            },
        ];

        merge_layout_with_blocks(&mut blocks, &regions, 0.5, 0.3);

        assert_eq!(blocks[0].role, BlockRole::Title);
        assert_eq!(blocks[1].role, BlockRole::Body); // Paragraph → Body
    }

    #[test]
    fn test_merge_layout_ignore_low_confidence() {
        use crate::ir::{BlockIR, BlockRole, TextLine, TextSpan};

        let mut blocks = vec![BlockIR {
            block_id: "blk_0".to_string(),
            bbox: BBox::new(50.0, 30.0, 500.0, 30.0),
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: "Text".to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: Some(BBox::new(50.0, 30.0, 500.0, 30.0)),
            }],
            normalized_text: "Text".to_string(),
        }];

        let regions = vec![LayoutRegion {
            bbox: BBox::new(45.0, 25.0, 510.0, 40.0),
            label: LayoutLabel::Title,
            confidence: 0.3, // 低于阈值
        }];

        merge_layout_with_blocks(&mut blocks, &regions, 0.5, 0.3);

        // 不应覆盖
        assert_eq!(blocks[0].role, BlockRole::Body);
    }

    #[test]
    fn test_mock_detector() {
        let detector = MockLayoutDetector;
        let result = detector.detect(&[], 612.0, 792.0).unwrap();
        assert!(result.is_empty());
        assert_eq!(detector.name(), "mock");
    }
}
