//! 表格结构检测 trait 和数据结构
//!
//! 定义表格结构识别的接口：
//! - `TableStructureDetector` trait — 表格结构检测后端
//! - `TableGridLine` — 行/列分割线
//! - `TableStructureResult` — 检测结果
//! - `MockTableStructureDetector` — 测试用 mock

use serde::{Deserialize, Serialize};

use crate::error::PdfError;
use crate::ir::BBox;

/// 分割线方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridLineDirection {
    /// 水平线（行分隔）
    Horizontal,
    /// 垂直线（列分隔）
    Vertical,
}

/// 表格结构元素类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableElementLabel {
    /// 行区域
    Row,
    /// 列区域
    Column,
    /// 列表头区域
    ColumnHeader,
    /// 行表头（投影）
    ProjectedRowHeader,
    /// 合并单元格
    SpanningCell,
}

/// 检测到的表格结构元素
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableElement {
    /// 边界框
    pub bbox: BBox,
    /// 元素类别
    pub label: TableElementLabel,
    /// 置信度
    pub confidence: f32,
}

/// 表格网格线（从行/列 bbox 推导）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableGridLine {
    /// 方向
    pub direction: GridLineDirection,
    /// 位置（水平线为 Y 坐标，垂直线为 X 坐标）
    pub position: f32,
    /// 起始坐标（水平线为 X 起始，垂直线为 Y 起始）
    pub start: f32,
    /// 结束坐标
    pub end: f32,
    /// 置信度
    pub confidence: f32,
}

/// 表格结构检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStructureResult {
    /// 检测到的行分割线（Y 坐标，从上到下排序）
    pub row_separators: Vec<TableGridLine>,
    /// 检测到的列分割线（X 坐标，从左到右排序）
    pub col_separators: Vec<TableGridLine>,
    /// 检测到的表格结构元素（原始）
    pub elements: Vec<TableElement>,
    /// 列表头区域（如有）
    pub header_bbox: Option<BBox>,
    /// 合并单元格区域
    pub spanning_cells: Vec<BBox>,
}

impl TableStructureResult {
    /// 推导行数
    pub fn num_rows(&self) -> usize {
        if self.row_separators.is_empty() {
            0
        } else {
            self.row_separators.len() + 1
        }
    }

    /// 推导列数
    pub fn num_cols(&self) -> usize {
        if self.col_separators.is_empty() {
            0
        } else {
            self.col_separators.len() + 1
        }
    }
}

/// 表格结构检测后端 trait
pub trait TableStructureDetector: Send + Sync {
    /// 从表格区域的图片数据中检测行列结构
    ///
    /// - `image_data`: 表格区域的 PNG 图片
    /// - `table_bbox`: 表格在页面中的位置（用于坐标转换）
    fn detect(
        &self,
        image_data: &[u8],
        table_bbox: &BBox,
    ) -> Result<TableStructureResult, PdfError>;

    /// 检测器名称
    fn name(&self) -> &str;
}

/// Mock 表格结构检测器（测试用）
pub struct MockTableStructureDetector;

impl TableStructureDetector for MockTableStructureDetector {
    fn detect(
        &self,
        _image_data: &[u8],
        _table_bbox: &BBox,
    ) -> Result<TableStructureResult, PdfError> {
        Ok(TableStructureResult {
            row_separators: Vec::new(),
            col_separators: Vec::new(),
            elements: Vec::new(),
            header_bbox: None,
            spanning_cells: Vec::new(),
        })
    }

    fn name(&self) -> &str {
        "mock"
    }
}

// ============================================================
// 行/列 bbox → 分割线转换
// ============================================================

/// 从行区域 bbox 列表提取行分割线
///
/// 策略：对行 bbox 按 Y 排序，取相邻行的中间线作为分割线
pub fn rows_to_separators(row_elements: &[TableElement]) -> Vec<TableGridLine> {
    if row_elements.is_empty() {
        return Vec::new();
    }

    let mut rows: Vec<&TableElement> = row_elements
        .iter()
        .filter(|e| e.label == TableElementLabel::Row)
        .collect();
    rows.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut separators = Vec::new();

    for i in 0..rows.len().saturating_sub(1) {
        let current_bottom = rows[i].bbox.y + rows[i].bbox.height;
        let next_top = rows[i + 1].bbox.y;
        let mid_y = (current_bottom + next_top) / 2.0;

        // 分割线的 X 范围取两行的并集
        let x_start = rows[i].bbox.x.min(rows[i + 1].bbox.x);
        let x_end = rows[i].bbox.right().max(rows[i + 1].bbox.right());

        let confidence = (rows[i].confidence + rows[i + 1].confidence) / 2.0;

        separators.push(TableGridLine {
            direction: GridLineDirection::Horizontal,
            position: mid_y,
            start: x_start,
            end: x_end,
            confidence,
        });
    }

    separators
}

/// 从列区域 bbox 列表提取列分割线
pub fn cols_to_separators(col_elements: &[TableElement]) -> Vec<TableGridLine> {
    if col_elements.is_empty() {
        return Vec::new();
    }

    let mut cols: Vec<&TableElement> = col_elements
        .iter()
        .filter(|e| e.label == TableElementLabel::Column)
        .collect();
    cols.sort_by(|a, b| {
        a.bbox
            .x
            .partial_cmp(&b.bbox.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut separators = Vec::new();

    for i in 0..cols.len().saturating_sub(1) {
        let current_right = cols[i].bbox.x + cols[i].bbox.width;
        let next_left = cols[i + 1].bbox.x;
        let mid_x = (current_right + next_left) / 2.0;

        let y_start = cols[i].bbox.y.min(cols[i + 1].bbox.y);
        let y_end = cols[i].bbox.bottom().max(cols[i + 1].bbox.bottom());

        let confidence = (cols[i].confidence + cols[i + 1].confidence) / 2.0;

        separators.push(TableGridLine {
            direction: GridLineDirection::Vertical,
            position: mid_x,
            start: y_start,
            end: y_end,
            confidence,
        });
    }

    separators
}

// ============================================================
// 模型结果与规则方法融合
// ============================================================

/// 将模型检测到的分割线与规则方法的分割线合并
///
/// 策略：
/// 1. 如果两条线距离 < tolerance → 取置信度更高的
/// 2. 否则保留两条线
/// 3. 模型置信度 < min_confidence 时忽略模型结果
pub fn merge_grid_lines(
    rule_lines: &[TableGridLine],
    model_lines: &[TableGridLine],
    tolerance: f32,
    min_model_confidence: f32,
) -> Vec<TableGridLine> {
    let mut merged: Vec<TableGridLine> = rule_lines.to_vec();

    for model_line in model_lines {
        if model_line.confidence < min_model_confidence {
            continue;
        }

        // 查找规则线中最接近的
        let closest = merged.iter().enumerate().find(|(_, rl)| {
            rl.direction == model_line.direction
                && (rl.position - model_line.position).abs() < tolerance
        });

        match closest {
            Some((idx, existing)) => {
                // 取置信度更高的
                if model_line.confidence > existing.confidence {
                    merged[idx] = model_line.clone();
                }
            }
            None => {
                // 新线，直接添加
                merged.push(model_line.clone());
            }
        }
    }

    // 按位置排序
    merged.sort_by(|a, b| {
        a.position
            .partial_cmp(&b.position)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    merged
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_detector() {
        let detector = MockTableStructureDetector;
        let result = detector
            .detect(&[], &BBox::new(0.0, 0.0, 100.0, 100.0))
            .unwrap();
        assert!(result.row_separators.is_empty());
        assert!(result.col_separators.is_empty());
        assert_eq!(detector.name(), "mock");
    }

    #[test]
    fn test_rows_to_separators() {
        let elements = vec![
            TableElement {
                bbox: BBox::new(10.0, 10.0, 200.0, 20.0),
                label: TableElementLabel::Row,
                confidence: 0.9,
            },
            TableElement {
                bbox: BBox::new(10.0, 32.0, 200.0, 20.0),
                label: TableElementLabel::Row,
                confidence: 0.85,
            },
            TableElement {
                bbox: BBox::new(10.0, 54.0, 200.0, 20.0),
                label: TableElementLabel::Row,
                confidence: 0.88,
            },
        ];

        let seps = rows_to_separators(&elements);
        assert_eq!(seps.len(), 2, "3 行应产生 2 条分割线");
        assert!(seps[0].position < seps[1].position, "应按 Y 排序");
        assert_eq!(seps[0].direction, GridLineDirection::Horizontal);

        // 第一条线在第一行底部和第二行顶部之间
        let expected_y = (10.0 + 20.0 + 32.0) / 2.0; // (30 + 32) / 2 = 31
        assert!((seps[0].position - expected_y).abs() < 1.0);
    }

    #[test]
    fn test_cols_to_separators() {
        let elements = vec![
            TableElement {
                bbox: BBox::new(10.0, 10.0, 80.0, 100.0),
                label: TableElementLabel::Column,
                confidence: 0.9,
            },
            TableElement {
                bbox: BBox::new(95.0, 10.0, 80.0, 100.0),
                label: TableElementLabel::Column,
                confidence: 0.85,
            },
        ];

        let seps = cols_to_separators(&elements);
        assert_eq!(seps.len(), 1, "2 列应产生 1 条分割线");
        assert_eq!(seps[0].direction, GridLineDirection::Vertical);

        // 分割线在 (90 + 95) / 2 = 92.5
        assert!((seps[0].position - 92.5).abs() < 1.0);
    }

    #[test]
    fn test_merge_grid_lines_dedup() {
        let rule_lines = vec![TableGridLine {
            direction: GridLineDirection::Horizontal,
            position: 100.0,
            start: 0.0,
            end: 500.0,
            confidence: 0.6,
        }];
        let model_lines = vec![
            TableGridLine {
                direction: GridLineDirection::Horizontal,
                position: 102.0, // 接近 100
                start: 0.0,
                end: 500.0,
                confidence: 0.9, // 更高置信度
            },
            TableGridLine {
                direction: GridLineDirection::Horizontal,
                position: 200.0, // 新线
                start: 0.0,
                end: 500.0,
                confidence: 0.8,
            },
        ];

        let merged = merge_grid_lines(&rule_lines, &model_lines, 5.0, 0.5);
        assert_eq!(merged.len(), 2, "一条去重 + 一条新增 = 2");
        // 第一条应被模型线替换（置信度更高）
        assert!((merged[0].position - 102.0).abs() < 0.1);
        assert!((merged[0].confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_merge_grid_lines_low_confidence_ignored() {
        let rule_lines = vec![];
        let model_lines = vec![TableGridLine {
            direction: GridLineDirection::Horizontal,
            position: 100.0,
            start: 0.0,
            end: 500.0,
            confidence: 0.2, // 低于阈值
        }];

        let merged = merge_grid_lines(&rule_lines, &model_lines, 5.0, 0.5);
        assert!(merged.is_empty(), "低置信度模型线应被忽略");
    }

    #[test]
    fn test_table_structure_result_counts() {
        let result = TableStructureResult {
            row_separators: vec![
                TableGridLine {
                    direction: GridLineDirection::Horizontal,
                    position: 50.0,
                    start: 0.0,
                    end: 100.0,
                    confidence: 0.9,
                },
                TableGridLine {
                    direction: GridLineDirection::Horizontal,
                    position: 100.0,
                    start: 0.0,
                    end: 100.0,
                    confidence: 0.85,
                },
            ],
            col_separators: vec![TableGridLine {
                direction: GridLineDirection::Vertical,
                position: 50.0,
                start: 0.0,
                end: 150.0,
                confidence: 0.88,
            }],
            elements: Vec::new(),
            header_bbox: None,
            spanning_cells: Vec::new(),
        };

        assert_eq!(result.num_rows(), 3, "2 分割线 → 3 行");
        assert_eq!(result.num_cols(), 2, "1 分割线 → 2 列");
    }

    #[test]
    fn test_table_element_label_serde() {
        let elem = TableElement {
            bbox: BBox::new(10.0, 20.0, 100.0, 50.0),
            label: TableElementLabel::Row,
            confidence: 0.95,
        };
        let json = serde_json::to_string(&elem).unwrap();
        let elem2: TableElement = serde_json::from_str(&json).unwrap();
        assert_eq!(elem2.label, TableElementLabel::Row);
        assert!((elem2.confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_grid_line_direction_serde() {
        let line = TableGridLine {
            direction: GridLineDirection::Vertical,
            position: 123.4,
            start: 0.0,
            end: 500.0,
            confidence: 0.9,
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("Vertical"));
        let line2: TableGridLine = serde_json::from_str(&json).unwrap();
        assert_eq!(line2.direction, GridLineDirection::Vertical);
    }

    #[test]
    fn test_empty_rows_produces_no_separators() {
        let seps = rows_to_separators(&[]);
        assert!(seps.is_empty());

        let seps = cols_to_separators(&[]);
        assert!(seps.is_empty());
    }

    #[test]
    fn test_single_row_produces_no_separator() {
        let elements = vec![TableElement {
            bbox: BBox::new(10.0, 10.0, 200.0, 30.0),
            label: TableElementLabel::Row,
            confidence: 0.9,
        }];
        let seps = rows_to_separators(&elements);
        assert!(seps.is_empty(), "单行不产生分割线");
    }
}
