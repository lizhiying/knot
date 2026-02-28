//! Stream 表格抽取引擎（无框表格）
//!
//! 流程：
//! 1. 行聚类（y 坐标 + 行距分组）
//! 2. 列聚类（x 坐标分布 → 列边界检测）
//! 3. 表头推断（首行 / 字体特征）
//! 4. 单元格文本投影（row × column 网格）
//! 5. CellType 推断
//! 6. fallback_text 生成

use super::cell_type::detect_cell_type;
use super::fallback::generate_fallback_text;
use crate::backend::RawChar;
use crate::ir::{BBox, CellType, ExtractionMode, TableCell, TableIR, TableRow};

/// 行 y 坐标聚类容差
const ROW_Y_TOLERANCE: f32 = 3.0;

/// 列 x 坐标对齐容差
const COL_X_TOLERANCE: f32 = 8.0;

/// 词间距阈值（字符间距超过字号此倍率视为空格）
const WORD_SPACING_RATIO: f32 = 0.3;

/// 从候选区域字符中抽取 stream 表格
pub fn extract_stream_table(
    chars: &[RawChar],
    bbox: &BBox,
    table_id: &str,
    page_index: usize,
) -> Option<TableIR> {
    if chars.is_empty() {
        return None;
    }

    // 1. 行聚类
    let rows = cluster_table_rows(chars);
    // 过滤噪声行（只含标点/逗号/空白的行，通常是数字千位分隔符被渲染为独立 y 行）
    let rows: Vec<TableRowChars> = rows
        .into_iter()
        .filter(|row| {
            if row.chars.len() > 10 {
                return true;
            }
            !row.chars.iter().all(|c| {
                let ch = c.unicode;
                ch == ','
                    || ch == '.'
                    || ch == ' '
                    || ch == '\u{00a0}'
                    || ch == ';'
                    || ch == ':'
                    || ch == '-'
            })
        })
        .collect();
    if rows.len() < 2 {
        return None;
    }

    // 2. 列边界检测
    let col_boundaries = detect_column_boundaries(&rows);
    log::debug!(
        "Stream table: {} rows, {} col_boundaries: {:?}",
        rows.len(),
        col_boundaries.len(),
        col_boundaries
    );
    if col_boundaries.len() < 2 {
        return None;
    }

    // 3. 投影到网格
    let grid = project_to_grid(&rows, &col_boundaries);

    // 4. 表头推断
    let (headers, data_start_row) = infer_headers(&rows, &grid);

    // 5. CellType 推断（按列）
    let col_count = col_boundaries.len();
    let column_types = infer_column_types(&grid, data_start_row, col_count);

    // 6. 构建 TableRow / TableCell
    let mut table_rows = Vec::new();
    for (row_idx, grid_row) in grid.iter().enumerate().skip(data_start_row) {
        let mut cells = Vec::new();
        for (col_idx, cell_text) in grid_row.iter().enumerate() {
            let ct = if col_idx < column_types.len() {
                column_types[col_idx]
            } else {
                detect_cell_type(cell_text)
            };
            cells.push(TableCell {
                row: row_idx - data_start_row,
                col: col_idx,
                text: cell_text.clone(),
                cell_type: ct,
                rowspan: 1,
                colspan: 1,
            });
        }
        table_rows.push(TableRow {
            row_index: row_idx - data_start_row,
            cells,
        });
    }

    // 7. fallback_text 生成
    let fallback_text = generate_fallback_text(&headers, &table_rows, table_id, page_index);

    Some(TableIR {
        table_id: table_id.to_string(),
        page_index,
        bbox: *bbox,
        extraction_mode: ExtractionMode::Stream,
        headers,
        rows: table_rows,
        column_types,
        fallback_text,
        confidence: None,
    })
}

/// 聚类后的表格行
#[derive(Debug, Clone)]
struct TableRowChars {
    y_center: f32,
    chars: Vec<RawChar>,
}

/// 按 y 坐标将字符聚类成行
fn cluster_table_rows(chars: &[RawChar]) -> Vec<TableRowChars> {
    let mut sorted: Vec<&RawChar> = chars.iter().collect();
    sorted.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rows: Vec<TableRowChars> = Vec::new();

    for ch in sorted {
        let y_center = ch.bbox.y + ch.bbox.height / 2.0;
        let mut found = false;
        for row in rows.iter_mut() {
            if (y_center - row.y_center).abs() < ROW_Y_TOLERANCE {
                row.chars.push(ch.clone());
                let n = row.chars.len() as f32;
                row.y_center = row.y_center * (n - 1.0) / n + y_center / n;
                found = true;
                break;
            }
        }
        if !found {
            rows.push(TableRowChars {
                y_center,
                chars: vec![ch.clone()],
            });
        }
    }

    // 每行内按 x 排序
    for row in rows.iter_mut() {
        row.chars.sort_by(|a, b| {
            a.bbox
                .x
                .partial_cmp(&b.bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // 行按 y 排序
    rows.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    rows
}

/// 检测列边界：收集所有行中段的起始 x 坐标，聚类得到列左边界
fn detect_column_boundaries(rows: &[TableRowChars]) -> Vec<f32> {
    // 对每行按间隙拆分成段，收集所有段的 x_start
    let mut all_x_starts: Vec<f32> = Vec::new();

    for row in rows {
        let segments = segment_row_chars(&row.chars);
        for seg in &segments {
            all_x_starts.push(seg.0); // x_start
        }
    }

    if all_x_starts.is_empty() {
        return Vec::new();
    }

    // 聚类 x_start 坐标
    all_x_starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut clusters: Vec<(f32, usize)> = Vec::new(); // (center, count)
    for &x in &all_x_starts {
        let mut found = false;
        for (center, count) in clusters.iter_mut() {
            if (x - *center).abs() < COL_X_TOLERANCE {
                *center = (*center * *count as f32 + x) / (*count + 1) as f32;
                *count += 1;
                found = true;
                break;
            }
        }
        if !found {
            clusters.push((x, 1));
        }
    }

    // 只保留在多行中出现的列位置（至少出现在 40% 的行中）
    let min_count = (rows.len() as f32 * 0.3).max(2.0) as usize;
    let mut boundaries: Vec<f32> = clusters
        .into_iter()
        .filter(|(_, count)| *count >= min_count)
        .map(|(center, _)| center)
        .collect();

    boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    boundaries
}

/// 将行中的字符按间隙拆分成段，返回 (x_start, x_end, text)
fn segment_row_chars(chars: &[RawChar]) -> Vec<(f32, f32, String)> {
    if chars.is_empty() {
        return Vec::new();
    }

    let mut segments: Vec<(f32, f32, Vec<char>)> = Vec::new();
    let mut seg_start = chars[0].bbox.x;
    let mut seg_end = chars[0].bbox.x + chars[0].bbox.width;
    let mut seg_chars: Vec<char> = vec![chars[0].unicode];

    for i in 1..chars.len() {
        let prev = &chars[i - 1];
        let curr = &chars[i];
        let gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);
        let font_size = curr.font_size.max(1.0);

        if gap > font_size * WORD_SPACING_RATIO * 3.0 {
            // 大间隙：新段
            segments.push((seg_start, seg_end, seg_chars));
            seg_start = curr.bbox.x;
            seg_end = curr.bbox.x + curr.bbox.width;
            seg_chars = vec![curr.unicode];
        } else {
            if gap > font_size * WORD_SPACING_RATIO {
                seg_chars.push(' ');
            }
            seg_chars.push(curr.unicode);
            seg_end = curr.bbox.x + curr.bbox.width;
        }
    }

    segments.push((seg_start, seg_end, seg_chars));

    segments
        .into_iter()
        .map(|(s, e, cs)| (s, e, cs.into_iter().collect()))
        .collect()
}

/// 将字符投影到 row × column 网格，返回二维文本数组
fn project_to_grid(rows: &[TableRowChars], col_boundaries: &[f32]) -> Vec<Vec<String>> {
    let col_count = col_boundaries.len();
    let mut grid: Vec<Vec<String>> = Vec::new();

    for row in rows {
        let mut row_cells = vec![String::new(); col_count];
        let segments = segment_row_chars(&row.chars);

        for (seg_x, _, seg_text) in &segments {
            // 找到最近的列
            let col_idx = find_nearest_column(*seg_x, col_boundaries);
            if col_idx < col_count {
                if !row_cells[col_idx].is_empty() {
                    row_cells[col_idx].push(' ');
                }
                row_cells[col_idx].push_str(seg_text.trim());
            }
        }

        grid.push(row_cells);
    }

    grid
}

/// 找到与 x 最近的列索引
fn find_nearest_column(x: f32, col_boundaries: &[f32]) -> usize {
    let mut best_idx = 0;
    let mut best_dist = f32::MAX;

    for (i, &boundary) in col_boundaries.iter().enumerate() {
        let dist = (x - boundary).abs();
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }

    best_idx
}

/// 推断表头：返回 (headers, data_start_row)
fn infer_headers(rows: &[TableRowChars], grid: &[Vec<String>]) -> (Vec<String>, usize) {
    if grid.is_empty() {
        return (Vec::new(), 0);
    }

    // 策略1：检查第一行是否具有不同的字体特征（加粗/字号更大）
    let first_row = &rows[0];
    let rest_rows = &rows[1..];

    let first_avg_font = avg_font_size(&first_row.chars);
    let first_has_bold = first_row.chars.iter().any(|c| c.is_bold);

    let rest_avg_font = if rest_rows.is_empty() {
        first_avg_font
    } else {
        rest_rows
            .iter()
            .flat_map(|r| r.chars.iter())
            .map(|c| c.font_size)
            .sum::<f32>()
            / rest_rows
                .iter()
                .map(|r| r.chars.len())
                .sum::<usize>()
                .max(1) as f32
    };

    let is_header_by_font = first_has_bold || first_avg_font > rest_avg_font * 1.1;

    // 策略2：第一行内容看起来像列名（非数字为主）
    let first_row_cells = &grid[0];
    let non_numeric_count = first_row_cells
        .iter()
        .filter(|cell| {
            let trimmed = cell.trim();
            !trimmed.is_empty() && !looks_like_number(trimmed)
        })
        .count();
    let is_header_by_content = non_numeric_count as f32 / first_row_cells.len().max(1) as f32 > 0.5;

    if is_header_by_font || is_header_by_content {
        (
            first_row_cells
                .iter()
                .map(|s| s.trim().to_string())
                .collect(),
            1,
        )
    } else {
        // 没有表头，使用默认列名
        let col_count = grid[0].len();
        let headers: Vec<String> = (0..col_count).map(|i| format!("col_{}", i)).collect();
        (headers, 0)
    }
}

/// 判断文本是否看起来像数字
fn looks_like_number(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // 去除货币符号和百分号
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !matches!(c, '$' | '¥' | '€' | '£' | '%' | ',' | ' '))
        .collect();
    if cleaned.is_empty() {
        return false;
    }
    // 尝试解析为数字
    cleaned.parse::<f64>().is_ok() || cleaned.trim_start_matches('-').parse::<f64>().is_ok()
}

/// 计算平均字体大小
fn avg_font_size(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 0.0;
    }
    chars.iter().map(|c| c.font_size).sum::<f32>() / chars.len() as f32
}

/// 按列推断 CellType
fn infer_column_types(grid: &[Vec<String>], data_start: usize, col_count: usize) -> Vec<CellType> {
    let mut column_types = vec![CellType::Unknown; col_count];

    for (col_idx, col_type) in column_types.iter_mut().enumerate() {
        let mut type_counts = std::collections::HashMap::new();
        for row in grid.iter().skip(data_start) {
            if let Some(cell_text) = row.get(col_idx) {
                let ct = detect_cell_type(cell_text);
                if ct != CellType::Unknown {
                    *type_counts.entry(ct).or_insert(0usize) += 1;
                }
            }
        }

        // 选择出现最多的类型
        if let Some((&best_type, _)) = type_counts.iter().max_by_key(|(_, count)| *count) {
            *col_type = best_type;
        }
    }

    column_types
}
