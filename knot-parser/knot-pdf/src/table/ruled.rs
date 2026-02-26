//! Ruled 表格抽取引擎（有线表格）
//!
//! 流程：
//! 1. 线段预处理（过滤噪声、合并共线、分类水平/垂直）
//! 2. 网格生成（交叉点检测 → 行列边界 → cell bbox 矩阵）
//! 3. 合并单元格检测（rowspan / colspan）
//! 4. 文本投影到 cell（按 bbox 重叠面积）
//! 5. 表头推断（ruled 特化）
//! 6. CellType 推断 + fallback_text 生成
//! 7. 输出 TableIR

use crate::backend::{RawChar, RawLine, RawRect};
use crate::ir::{BBox, CellType, ExtractionMode, TableCell, TableIR, TableRow};

use super::cell_type::detect_cell_type;
use super::fallback::generate_fallback_text;

// ─── 常量 ───

/// 最小线段长度（pt），过短的线段视为噪声
const MIN_LINE_LENGTH: f32 = 5.0;

/// 最小线宽（pt），过细的线段视为噪声
const MIN_LINE_WIDTH: f32 = 0.05;

/// 坐标对齐容差（snap to grid）
const SNAP_TOLERANCE: f32 = 2.0;

/// 合并共线线段的容差（垂直方向偏差）
const COLLINEAR_TOLERANCE: f32 = 3.0;

/// 交叉点检测容差
const INTERSECTION_TOLERANCE: f32 = 3.0;

/// 最少行数
const MIN_GRID_ROWS: usize = 2;

/// 最少列数
const MIN_GRID_COLS: usize = 2;

/// 线段密度阈值 — 区域内至少需要这么多条水平+垂直线段才尝试 ruled 抽取
const MIN_RULED_LINES: usize = 4;

// ─── 公开接口 ───

/// 判断给定的线段/矩形是否足以构成 ruled 表格
pub fn has_enough_lines(lines: &[RawLine], rects: &[RawRect]) -> bool {
    let mut all_lines = normalize_lines(lines, rects);
    filter_noise(&mut all_lines);
    let (h, v) = classify_lines(&all_lines);
    // 传统 ruled（有水平线+垂直线）
    let traditional = h.len() >= 2 && v.len() >= 2 && (h.len() + v.len()) >= MIN_RULED_LINES;
    // 三线表 booktabs（只有水平线，无垂直线）
    let booktabs = h.len() >= 3 && v.is_empty();
    traditional || booktabs
}

/// 从线段/矩形 + 字符中抽取 ruled 表格
pub fn extract_ruled_table(
    lines: &[RawLine],
    rects: &[RawRect],
    chars: &[RawChar],
    page_index: usize,
    table_id: &str,
) -> Option<TableIR> {
    // 1. 线段预处理
    let mut all_lines = normalize_lines(lines, rects);
    filter_noise(&mut all_lines);
    let (mut h_lines, mut v_lines) = classify_lines(&all_lines);

    if h_lines.len() < 2 {
        return None;
    }

    // 如果没有垂直线但有 ≥3 条水平线，尝试三线表 (booktabs) 抽取
    if v_lines.is_empty() && h_lines.len() >= 3 {
        return extract_booktabs_table(&h_lines, chars, page_index, table_id);
    }

    if v_lines.len() < 2 {
        return None;
    }

    // 合并共线线段
    merge_collinear(&mut h_lines, true);
    merge_collinear(&mut v_lines, false);

    // 坐标对齐（snap to grid）
    snap_lines(&mut h_lines, true);
    snap_lines(&mut v_lines, false);

    // 排序
    h_lines.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    v_lines.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    // 2. 网格生成
    let grid = build_grid(&h_lines, &v_lines)?;
    if grid.row_count < MIN_GRID_ROWS || grid.col_count < MIN_GRID_COLS {
        return None;
    }

    // 3. 合并单元格检测
    let cell_spans = detect_merged_cells(&grid, &h_lines, &v_lines);

    // 4. 文本投影到 cell
    let cell_texts = project_text_to_cells(&grid, &cell_spans, chars);

    // 5. 表头推断
    let (headers, data_start_row) = infer_ruled_headers(&grid, &cell_texts, chars, &h_lines);

    // 6. 构建 TableIR
    let col_count = grid.col_count;

    // CellType 推断（按列）
    let column_types = infer_column_types(&cell_texts, data_start_row, col_count);

    // 构建 TableRow / TableCell
    let mut table_rows = Vec::new();
    for row_idx in data_start_row..grid.row_count {
        let mut cells = Vec::new();
        let mut col = 0;
        while col < col_count {
            let span = &cell_spans[row_idx][col];
            if span.is_covered {
                col += 1;
                continue;
            }

            let ct = if col < column_types.len() {
                column_types[col]
            } else {
                detect_cell_type(&cell_texts[row_idx][col])
            };

            cells.push(TableCell {
                row: row_idx - data_start_row,
                col,
                text: cell_texts[row_idx][col].clone(),
                cell_type: ct,
                rowspan: span.rowspan,
                colspan: span.colspan,
            });
            col += 1;
        }
        table_rows.push(TableRow {
            row_index: row_idx - data_start_row,
            cells,
        });
    }

    // fallback_text
    let fallback_text = generate_fallback_text(&headers, &table_rows, table_id, page_index);

    // bbox
    let table_bbox = compute_table_bbox(&grid);

    Some(TableIR {
        table_id: table_id.to_string(),
        page_index,
        bbox: table_bbox,
        extraction_mode: ExtractionMode::Ruled,
        headers,
        rows: table_rows,
        column_types,
        fallback_text,
            confidence: None,
    })
}

// ─── 内部结构体 ───

/// 归一化后的线段（水平或垂直）
#[derive(Debug, Clone)]
struct NormalizedLine {
    /// 对于水平线：y 坐标；对于垂直线：x 坐标
    x: f32,
    y: f32,
    /// 起始坐标（沿线段方向）
    start: f32,
    /// 结束坐标（沿线段方向）
    end: f32,
    /// 线宽
    width: f32,
    /// 是否为水平线
    is_horizontal: bool,
}

/// 网格结构
#[derive(Debug)]
struct Grid {
    /// 行边界（y 坐标列表，从上到下）
    row_bounds: Vec<f32>,
    /// 列边界（x 坐标列表，从左到右）
    col_bounds: Vec<f32>,
    /// 行数（= row_bounds.len() - 1）
    row_count: usize,
    /// 列数（= col_bounds.len() - 1）
    col_count: usize,
}

#[allow(dead_code)]
impl Grid {
    /// 获取 cell 的 bbox
    fn cell_bbox(&self, row: usize, col: usize) -> BBox {
        let x = self.col_bounds[col];
        let y = self.row_bounds[row];
        let w = self.col_bounds[col + 1] - x;
        let h = self.row_bounds[row + 1] - y;
        BBox::new(x, y, w, h)
    }
}

/// 单元格跨度信息
#[derive(Debug, Clone)]
struct CellSpan {
    rowspan: usize,
    colspan: usize,
    /// 是否被其他单元格的 span 覆盖（即不是左上角）
    is_covered: bool,
}

// ─── 线段预处理 ───

/// 将 RawLine 和 RawRect 归一化为统一格式
fn normalize_lines(lines: &[RawLine], rects: &[RawRect]) -> Vec<NormalizedLine> {
    let mut result = Vec::new();

    for line in lines {
        match line.orientation {
            crate::backend::LineOrientation::Horizontal => {
                let x_min = line.start.x.min(line.end.x);
                let x_max = line.start.x.max(line.end.x);
                let y = (line.start.y + line.end.y) / 2.0;
                result.push(NormalizedLine {
                    x: x_min,
                    y,
                    start: x_min,
                    end: x_max,
                    width: line.width,
                    is_horizontal: true,
                });
            }
            crate::backend::LineOrientation::Vertical => {
                let y_min = line.start.y.min(line.end.y);
                let y_max = line.start.y.max(line.end.y);
                let x = (line.start.x + line.end.x) / 2.0;
                result.push(NormalizedLine {
                    x,
                    y: y_min,
                    start: y_min,
                    end: y_max,
                    width: line.width,
                    is_horizontal: false,
                });
            }
            crate::backend::LineOrientation::Diagonal => {
                // 对角线不用于表格检测
            }
        }
    }

    // 将窄矩形转换为线段
    for rect in rects {
        let w = rect.bbox.width;
        let h = rect.bbox.height;

        if h < 3.0 && w > MIN_LINE_LENGTH {
            // 水平线
            result.push(NormalizedLine {
                x: rect.bbox.x,
                y: rect.bbox.y + h / 2.0,
                start: rect.bbox.x,
                end: rect.bbox.x + w,
                width: h.max(rect.width),
                is_horizontal: true,
            });
        } else if w < 3.0 && h > MIN_LINE_LENGTH {
            // 垂直线
            result.push(NormalizedLine {
                x: rect.bbox.x + w / 2.0,
                y: rect.bbox.y,
                start: rect.bbox.y,
                end: rect.bbox.y + h,
                width: w.max(rect.width),
                is_horizontal: false,
            });
        }
    }

    result
}

/// 过滤噪声线段
fn filter_noise(lines: &mut Vec<NormalizedLine>) {
    lines.retain(|l| {
        let length = l.end - l.start;
        length >= MIN_LINE_LENGTH && l.width >= MIN_LINE_WIDTH
    });
}

/// 分类水平线和垂直线
fn classify_lines(lines: &[NormalizedLine]) -> (Vec<NormalizedLine>, Vec<NormalizedLine>) {
    let mut h = Vec::new();
    let mut v = Vec::new();

    for line in lines {
        if line.is_horizontal {
            h.push(line.clone());
        } else {
            v.push(line.clone());
        }
    }

    (h, v)
}

/// 合并近似共线的线段
fn merge_collinear(lines: &mut Vec<NormalizedLine>, is_horizontal: bool) {
    if lines.len() < 2 {
        return;
    }

    // 按主轴坐标排序
    if is_horizontal {
        lines.sort_by(|a, b| {
            a.y.partial_cmp(&b.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.start
                        .partial_cmp(&b.start)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
    } else {
        lines.sort_by(|a, b| {
            a.x.partial_cmp(&b.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.start
                        .partial_cmp(&b.start)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
    }

    let mut merged = Vec::new();
    let mut current = lines[0].clone();

    for line in lines.iter().skip(1) {
        let same_axis = if is_horizontal {
            (line.y - current.y).abs() < COLLINEAR_TOLERANCE
        } else {
            (line.x - current.x).abs() < COLLINEAR_TOLERANCE
        };

        // 检查是否可以合并（共线 + 有重叠或间隙很小）
        if same_axis && line.start <= current.end + COLLINEAR_TOLERANCE {
            current.end = current.end.max(line.end);
            current.width = current.width.max(line.width);
        } else {
            merged.push(current);
            current = line.clone();
        }
    }
    merged.push(current);

    *lines = merged;
}

/// 坐标对齐（snap to grid）
fn snap_lines(lines: &mut [NormalizedLine], is_horizontal: bool) {
    // 收集主轴坐标
    let coords: Vec<f32> = if is_horizontal {
        lines.iter().map(|l| l.y).collect()
    } else {
        lines.iter().map(|l| l.x).collect()
    };

    // 对坐标进行聚类
    let snapped = snap_coords(&coords);

    // 应用对齐
    for (line, &new_coord) in lines.iter_mut().zip(snapped.iter()) {
        if is_horizontal {
            line.y = new_coord;
        } else {
            line.x = new_coord;
        }
    }
}

/// 对坐标列表进行聚类对齐
fn snap_coords(coords: &[f32]) -> Vec<f32> {
    if coords.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<(usize, f32)> = coords.iter().enumerate().map(|(i, &v)| (i, v)).collect();
    sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut clusters: Vec<(f32, Vec<usize>)> = Vec::new();
    for (idx, val) in sorted {
        let mut found = false;
        for (center, indices) in clusters.iter_mut() {
            if (val - *center).abs() < SNAP_TOLERANCE {
                *center = (*center * indices.len() as f32 + val) / (indices.len() + 1) as f32;
                indices.push(idx);
                found = true;
                break;
            }
        }
        if !found {
            clusters.push((val, vec![idx]));
        }
    }

    let mut result = vec![0.0_f32; coords.len()];
    for (center, indices) in &clusters {
        for &idx in indices {
            result[idx] = *center;
        }
    }
    result
}

// ─── 网格生成 ───

/// 从水平线和垂直线构建网格
fn build_grid(h_lines: &[NormalizedLine], v_lines: &[NormalizedLine]) -> Option<Grid> {
    // 提取唯一的行边界（y 坐标）
    let mut row_bounds: Vec<f32> = Vec::new();
    for line in h_lines {
        let y = line.y;
        if !row_bounds.iter().any(|&r| (r - y).abs() < SNAP_TOLERANCE) {
            row_bounds.push(y);
        }
    }
    row_bounds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // 提取唯一的列边界（x 坐标）
    let mut col_bounds: Vec<f32> = Vec::new();
    for line in v_lines {
        let x = line.x;
        if !col_bounds.iter().any(|&c| (c - x).abs() < SNAP_TOLERANCE) {
            col_bounds.push(x);
        }
    }
    col_bounds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let row_count = if row_bounds.len() > 1 {
        row_bounds.len() - 1
    } else {
        return None;
    };
    let col_count = if col_bounds.len() > 1 {
        col_bounds.len() - 1
    } else {
        return None;
    };

    Some(Grid {
        row_bounds,
        col_bounds,
        row_count,
        col_count,
    })
}

// ─── 合并单元格检测 ───

/// 检测合并单元格（基于内部分隔线是否存在）
fn detect_merged_cells(
    grid: &Grid,
    h_lines: &[NormalizedLine],
    v_lines: &[NormalizedLine],
) -> Vec<Vec<CellSpan>> {
    let mut spans = vec![
        vec![
            CellSpan {
                rowspan: 1,
                colspan: 1,
                is_covered: false,
            };
            grid.col_count
        ];
        grid.row_count
    ];

    // 检查每个 cell 边界处是否有分隔线
    for row in 0..grid.row_count {
        for col in 0..grid.col_count {
            if spans[row][col].is_covered {
                continue;
            }

            // 向右检查 colspan
            let mut cspan = 1;
            for c in (col + 1)..grid.col_count {
                let separator_x = grid.col_bounds[c];
                let y_top = grid.row_bounds[row];
                let y_bot = grid.row_bounds[row + 1];
                if !has_vertical_line(v_lines, separator_x, y_top, y_bot) {
                    cspan += 1;
                } else {
                    break;
                }
            }

            // 向下检查 rowspan
            let mut rspan = 1;
            for r in (row + 1)..grid.row_count {
                let separator_y = grid.row_bounds[r];
                let x_left = grid.col_bounds[col];
                let x_right = grid.col_bounds[col + cspan];
                if !has_horizontal_line(h_lines, separator_y, x_left, x_right) {
                    rspan += 1;
                } else {
                    break;
                }
            }

            // 标记跨越的单元格
            if rspan > 1 || cspan > 1 {
                spans[row][col].rowspan = rspan;
                spans[row][col].colspan = cspan;

                for r in row..(row + rspan) {
                    for c in col..(col + cspan) {
                        if r == row && c == col {
                            continue;
                        }
                        if r < grid.row_count && c < grid.col_count {
                            spans[r][c].is_covered = true;
                        }
                    }
                }
            }
        }
    }

    spans
}

/// 检查在指定位置是否存在垂直分隔线
fn has_vertical_line(v_lines: &[NormalizedLine], x: f32, y_top: f32, y_bot: f32) -> bool {
    let mid_y = (y_top + y_bot) / 2.0;
    let range_y = y_bot - y_top;

    for line in v_lines {
        if (line.x - x).abs() < INTERSECTION_TOLERANCE {
            // 线段需要覆盖分隔区域的大部分
            if line.start <= mid_y + INTERSECTION_TOLERANCE
                && line.end >= mid_y - INTERSECTION_TOLERANCE
            {
                let overlap_start = line.start.max(y_top);
                let overlap_end = line.end.min(y_bot);
                let overlap = overlap_end - overlap_start;
                if overlap > range_y * 0.5 {
                    return true;
                }
            }
        }
    }
    false
}

/// 检查在指定位置是否存在水平分隔线
fn has_horizontal_line(h_lines: &[NormalizedLine], y: f32, x_left: f32, x_right: f32) -> bool {
    let mid_x = (x_left + x_right) / 2.0;
    let range_x = x_right - x_left;

    for line in h_lines {
        if (line.y - y).abs() < INTERSECTION_TOLERANCE {
            if line.start <= mid_x + INTERSECTION_TOLERANCE
                && line.end >= mid_x - INTERSECTION_TOLERANCE
            {
                let overlap_start = line.start.max(x_left);
                let overlap_end = line.end.min(x_right);
                let overlap = overlap_end - overlap_start;
                if overlap > range_x * 0.5 {
                    return true;
                }
            }
        }
    }
    false
}

// ─── 文本投影 ───

/// 将字符投影到网格单元格
fn project_text_to_cells(
    grid: &Grid,
    cell_spans: &[Vec<CellSpan>],
    chars: &[RawChar],
) -> Vec<Vec<String>> {
    let mut texts = vec![vec![String::new(); grid.col_count]; grid.row_count];

    // 为每个 cell 创建扩展的 bbox（考虑合并单元格）
    let mut cell_bboxes: Vec<(usize, usize, BBox)> = Vec::new();
    for row in 0..grid.row_count {
        for col in 0..grid.col_count {
            if cell_spans[row][col].is_covered {
                continue;
            }
            let span = &cell_spans[row][col];
            let x = grid.col_bounds[col];
            let y = grid.row_bounds[row];
            let end_col = (col + span.colspan).min(grid.col_count);
            let end_row = (row + span.rowspan).min(grid.row_count);
            let w = grid.col_bounds[end_col] - x;
            let h = grid.row_bounds[end_row] - y;
            cell_bboxes.push((row, col, BBox::new(x, y, w, h)));
        }
    }

    // 对每个字符，找到重叠面积最大的 cell
    for ch in chars {
        let char_cx = ch.bbox.x + ch.bbox.width / 2.0;
        let char_cy = ch.bbox.y + ch.bbox.height / 2.0;

        let mut best_cell: Option<(usize, usize)> = None;
        let mut best_overlap = 0.0_f32;

        for &(row, col, ref cell_bb) in &cell_bboxes {
            // 先检查中心点是否在 cell 内
            if char_cx >= cell_bb.x
                && char_cx <= cell_bb.x + cell_bb.width
                && char_cy >= cell_bb.y
                && char_cy <= cell_bb.y + cell_bb.height
            {
                best_cell = Some((row, col));
                break;
            }

            // 否则检查重叠面积
            let overlap = ch.bbox.overlap_area(cell_bb);
            if overlap > best_overlap {
                best_overlap = overlap;
                best_cell = Some((row, col));
            }
        }

        if let Some((row, col)) = best_cell {
            // 收集字符（暂先按照出现顺序追加）
            if row < texts.len() && col < texts[row].len() {
                texts[row][col].push(ch.unicode);
            }
        }
    }

    // 对每个 cell 的文本进行 trim
    for row in texts.iter_mut() {
        for cell in row.iter_mut() {
            *cell = cell.trim().to_string();
        }
    }

    texts
}

// ─── 表头推断 ───

/// Ruled 表格表头推断
fn infer_ruled_headers(
    grid: &Grid,
    cell_texts: &[Vec<String>],
    chars: &[RawChar],
    h_lines: &[NormalizedLine],
) -> (Vec<String>, usize) {
    if cell_texts.is_empty() || grid.row_count == 0 {
        return (Vec::new(), 0);
    }

    let first_row = &cell_texts[0];

    // 策略1：首行下方有较粗分隔线
    let first_row_bottom = grid.row_bounds[1];
    let thick_line_below = h_lines
        .iter()
        .any(|l| (l.y - first_row_bottom).abs() < INTERSECTION_TOLERANCE && l.width > 1.0);

    // 策略2：首行字体加粗/字号更大
    let first_row_chars: Vec<&RawChar> = chars
        .iter()
        .filter(|c| {
            let cy = c.bbox.y + c.bbox.height / 2.0;
            cy >= grid.row_bounds[0] && cy <= grid.row_bounds[1]
        })
        .collect();
    let rest_chars: Vec<&RawChar> = chars
        .iter()
        .filter(|c| {
            let cy = c.bbox.y + c.bbox.height / 2.0;
            cy > grid.row_bounds[1]
        })
        .collect();

    let first_has_bold = first_row_chars.iter().any(|c| c.is_bold);
    let first_avg_font = if first_row_chars.is_empty() {
        0.0
    } else {
        first_row_chars.iter().map(|c| c.font_size).sum::<f32>() / first_row_chars.len() as f32
    };
    let rest_avg_font = if rest_chars.is_empty() {
        first_avg_font
    } else {
        rest_chars.iter().map(|c| c.font_size).sum::<f32>() / rest_chars.len() as f32
    };
    let is_header_by_font = first_has_bold || first_avg_font > rest_avg_font * 1.1;

    // 策略3：首行内容以非数字为主
    let non_numeric = first_row
        .iter()
        .filter(|s| {
            let t = s.trim();
            !t.is_empty() && !looks_like_number(t)
        })
        .count();
    let is_header_by_content = non_numeric as f32 / first_row.len().max(1) as f32 > 0.5;

    if thick_line_below || is_header_by_font || is_header_by_content {
        let headers: Vec<String> = first_row.iter().map(|s| s.trim().to_string()).collect();
        (headers, 1)
    } else {
        let col_count = grid.col_count;
        let headers: Vec<String> = (0..col_count).map(|i| format!("col_{}", i)).collect();
        (headers, 0)
    }
}

/// 判断文本是否像数字
fn looks_like_number(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !matches!(c, '$' | '¥' | '€' | '£' | '%' | ',' | ' '))
        .collect();
    if cleaned.is_empty() {
        return false;
    }
    cleaned.parse::<f64>().is_ok() || cleaned.trim_start_matches('-').parse::<f64>().is_ok()
}

// ─── CellType 推断 ───

/// 按列推断 CellType
fn infer_column_types(
    cell_texts: &[Vec<String>],
    data_start: usize,
    col_count: usize,
) -> Vec<CellType> {
    let mut column_types = vec![CellType::Unknown; col_count];

    for (col_idx, col_type) in column_types.iter_mut().enumerate() {
        let mut type_counts = std::collections::HashMap::new();
        for row in cell_texts.iter().skip(data_start) {
            if let Some(cell_text) = row.get(col_idx) {
                let ct = detect_cell_type(cell_text);
                if ct != CellType::Unknown {
                    *type_counts.entry(ct).or_insert(0usize) += 1;
                }
            }
        }
        if let Some((&best_type, _)) = type_counts.iter().max_by_key(|(_, count)| *count) {
            *col_type = best_type;
        }
    }

    column_types
}

// ─── 辅助 ───

/// 计算表格整体 bbox
fn compute_table_bbox(grid: &Grid) -> BBox {
    let x = grid.col_bounds[0];
    let y = grid.row_bounds[0];
    let w = grid.col_bounds.last().unwrap_or(&0.0) - x;
    let h = grid.row_bounds.last().unwrap_or(&0.0) - y;
    BBox::new(x, y, w, h)
}

// ─── 三线表 (Booktabs) 抽取 ───

/// 词间距比例阈值（用于单元格内文本空格判定）
const BOOKTABS_WORD_SPACING_RATIO: f32 = 0.15;

/// 将一组按 x 排序的字符拼接为文本，根据字符间距自动插入空格
fn chars_to_text_with_spacing(chars: &[&RawChar]) -> String {
    if chars.is_empty() {
        return String::new();
    }
    let mut text = String::new();
    text.push(chars[0].unicode);

    for i in 1..chars.len() {
        let prev = &chars[i - 1];
        let curr = &chars[i];
        let gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);
        let font_size = prev.font_size.max(curr.font_size).max(1.0);
        if gap > font_size * BOOKTABS_WORD_SPACING_RATIO {
            text.push(' ');
        }
        text.push(curr.unicode);
    }
    text
}

/// 检测双栏布局中的栏沟（column gutter）
///
/// 通过分析表格区域之外的短行起始 x 坐标分布，找到双栏之间的间隙。
fn detect_column_gutter(
    all_chars: &[RawChar],
    table_y_top: f32,
    table_y_bottom: f32,
    _content_x_min: f32,
    _content_x_max: f32,
) -> Option<(f32, f32)> {
    // 只使用表格区域之外的字符
    let outside_chars: Vec<&RawChar> = all_chars
        .iter()
        .filter(|c| {
            let cy = c.bbox.y + c.bbox.height / 2.0;
            cy < table_y_top - 5.0 || cy > table_y_bottom + 5.0
        })
        .collect();

    if outside_chars.len() < 30 {
        return None;
    }

    // 按 y 聚类成行（3pt 容差），收集 (line_x_min, line_x_max)
    let mut sorted_by_y: Vec<&RawChar> = outside_chars.clone();
    sorted_by_y.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut text_lines: Vec<(f32, f32)> = Vec::new();
    let mut cur_xmin = sorted_by_y[0].bbox.x;
    let mut cur_xmax = sorted_by_y[0].bbox.x + sorted_by_y[0].bbox.width;
    let mut cur_y = sorted_by_y[0].bbox.y;
    let mut cur_count = 1;

    for &ch in sorted_by_y.iter().skip(1) {
        if (ch.bbox.y - cur_y).abs() < 3.0 {
            cur_xmin = cur_xmin.min(ch.bbox.x);
            cur_xmax = cur_xmax.max(ch.bbox.x + ch.bbox.width);
            cur_count += 1;
        } else {
            if cur_count >= 3 {
                text_lines.push((cur_xmin, cur_xmax));
            }
            cur_xmin = ch.bbox.x;
            cur_xmax = ch.bbox.x + ch.bbox.width;
            cur_y = ch.bbox.y;
            cur_count = 1;
        }
    }
    if cur_count >= 3 {
        text_lines.push((cur_xmin, cur_xmax));
    }

    if text_lines.len() < 4 {
        return None;
    }

    // 页面 x 范围
    let page_x_min = text_lines.iter().map(|t| t.0).fold(f32::MAX, f32::min);
    let page_x_max = text_lines.iter().map(|t| t.1).fold(f32::MIN, f32::max);
    let x_range = page_x_max - page_x_min;

    if x_range < 100.0 {
        return None;
    }

    // 只保留短行（x_span < 60% 页面宽度），收集起始 x
    let max_col_span = x_range * 0.60;
    let mut line_starts: Vec<f32> = text_lines
        .iter()
        .filter(|(xmin, xmax)| (xmax - xmin) < max_col_span)
        .map(|(xmin, _)| *xmin)
        .collect();

    if line_starts.len() < 4 {
        return None;
    }

    // 排序后找最大间隙
    line_starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_gap = 0.0_f32;
    let mut gap_pos = 0;
    for i in 1..line_starts.len() {
        let gap = line_starts[i] - line_starts[i - 1];
        if gap > max_gap {
            max_gap = gap;
            gap_pos = i;
        }
    }

    if max_gap < 20.0 || gap_pos == 0 {
        return None;
    }

    let split_x = (line_starts[gap_pos - 1] + line_starts[gap_pos]) / 2.0;

    // gutter 边界可以直接用 gap 的两端点
    // gap_left_start = 最后一个左栏行的起始 x
    // gap_right_start = 第一个右栏行的起始 x
    // 但我们需要左栏行的结束 x 来确定 gutter_left
    // 收集左栏行的 x_max
    let max_col_span = x_range * 0.60;
    let mut left_col_ends: Vec<f32> = Vec::new();

    for &(xmin, xmax) in &text_lines {
        let span = xmax - xmin;
        if span < max_col_span && xmin < split_x {
            left_col_ends.push(xmax);
        }
    }

    if left_col_ends.is_empty() {
        return None;
    }

    left_col_ends.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // gutter_left = 左栏行结束 x 的 75 百分位
    let gutter_left = left_col_ends[left_col_ends.len() * 3 / 4];
    // gutter_right = 第一个右栏行的起始 x
    let gutter_right = line_starts[gap_pos];

    let gutter_width = gutter_right - gutter_left;
    if gutter_width < 3.0 || gutter_width > 80.0 {
        return None;
    }

    Some((gutter_left, gutter_right))
}

/// 过滤双栏布局中混入表格区域的正文段落字符
///
/// 策略：对字符按 y 再按 x 排序后，按 x 间隙 > 8pt 分段。
/// 每段如果完全在某一栏内（不跨越 gutter）且字符数 > 25，
/// 则判定为正文段落文字，移除。否则保留。
fn filter_multicolumn_body_text<'a>(
    chars: &[&'a RawChar],
    gutter_left: f32,
    gutter_right: f32,
    _table_x_min: f32,
    _table_x_max: f32,
) -> Vec<&'a RawChar> {
    if chars.is_empty() {
        return Vec::new();
    }

    // 按 y 再按 x 排序
    let mut sorted: Vec<&'a RawChar> = chars.to_vec();
    sorted.sort_by(|a, b| {
        let ya = a.bbox.y;
        let yb = b.bbox.y;
        ya.partial_cmp(&yb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.bbox
                    .x
                    .partial_cmp(&b.bbox.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    // 按 x 间隙 > 8pt 分段（同时在 y 跳变 > 4pt 时也分段）
    const RUN_GAP: f32 = 8.0;
    const Y_BREAK: f32 = 4.0;
    const BODY_TEXT_MIN_CHARS: usize = 25;

    let mut result: Vec<&'a RawChar> = Vec::new();
    let mut run_start = 0;

    for i in 1..=sorted.len() {
        let should_break = if i == sorted.len() {
            true
        } else {
            let prev = &sorted[i - 1];
            let curr = &sorted[i];
            let x_gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);
            let y_gap = (curr.bbox.y - prev.bbox.y).abs();
            x_gap > RUN_GAP || y_gap > Y_BREAK
        };

        if should_break {
            let run = &sorted[run_start..i];

            // 检查这个段是否完全在一栏内
            let run_x_min = run.iter().map(|c| c.bbox.x).fold(f32::MAX, f32::min);
            let run_x_max = run
                .iter()
                .map(|c| c.bbox.x + c.bbox.width)
                .fold(f32::MIN, f32::max);

            let in_left_only = run_x_max < gutter_left + 5.0;
            let in_right_only = run_x_min > gutter_right - 5.0;
            let is_long = run.len() > BODY_TEXT_MIN_CHARS;

            if (in_left_only || in_right_only) && is_long {
                // 正文段落段 → 跳过
            } else {
                // 表格内容或短段 → 保留
                result.extend(run);
            }

            run_start = i;
        }
    }

    result
}

/// 从只有水平线的三线表中抽取表格
///
/// 学术论文中最常见的表格样式：只有 top/header-sep/bottom 三条水平线，
/// 无垂直线。列边界通过文本对齐推断。
///
/// 改进策略：
/// 1. 列边界仅通过数据行（第2条水平线之后的区域）推断，避免表头跨列文字干扰
/// 2. 表头区域（第1~2条水平线之间）的多个子行纵向合并为单个表头行
/// 3. 单元格内文本根据词间距自动插入空格
fn extract_booktabs_table(
    h_lines: &[NormalizedLine],
    chars: &[RawChar],
    page_index: usize,
    table_id: &str,
) -> Option<TableIR> {
    let mut h_sorted = h_lines.to_vec();
    merge_collinear(&mut h_sorted, true);
    snap_lines(&mut h_sorted, true);
    h_sorted.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));


    if h_sorted.len() < 3 {
        return None;
    }

    // 行边界 = 水平线的 y 坐标
    let mut row_bounds: Vec<f32> = Vec::new();
    for line in &h_sorted {
        if !row_bounds
            .iter()
            .any(|&r| (r - line.y).abs() < SNAP_TOLERANCE)
        {
            row_bounds.push(line.y);
        }
    }
    row_bounds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    log::debug!(
        "Booktabs page {}: {} h_lines after merge, row_bounds={:?}",
        page_index,
        h_sorted.len(),
        row_bounds
    );
    for (i, line) in h_sorted.iter().enumerate() {
    }

    if row_bounds.len() < 3 {
        return None;
    }

    let region_count = row_bounds.len() - 1;

    // 确定表格的 x 范围（取最长水平线的 start/end）
    let table_x_min = h_sorted.iter().map(|l| l.start).fold(f32::MAX, f32::min);
    let table_x_max = h_sorted.iter().map(|l| l.end).fold(f32::MIN, f32::max);

    // 筛选表格区域内的字符
    let y_top = row_bounds[0] - 2.0;
    let y_bottom = *row_bounds.last().unwrap() + 2.0;
    let table_chars: Vec<&RawChar> = chars
        .iter()
        .filter(|c| {
            let cy = c.bbox.y + c.bbox.height / 2.0;
            let cx = c.bbox.x + c.bbox.width / 2.0;
            cy >= y_top && cy <= y_bottom && cx >= table_x_min - 10.0 && cx <= table_x_max + 10.0
        })
        .collect();

    if table_chars.is_empty() {
        return None;
    }

    // ── 双栏布局检测与过滤 ──
    // 分析表格区域外的字符 x 分布，检测栏沟（column gutter）
    let table_chars = {
        let gutter = detect_column_gutter(chars, y_top, y_bottom, table_x_min, table_x_max);
        if let Some((gutter_left, gutter_right)) = gutter {
            let before_count = table_chars.len();
            let filtered = filter_multicolumn_body_text(
                &table_chars,
                gutter_left,
                gutter_right,
                table_x_min,
                table_x_max,
            );
            filtered
        } else {
            table_chars
        }
    };

    if table_chars.is_empty() {
        return None;
    }

    // 按水平线间区域分组
    let mut region_chars: Vec<Vec<&RawChar>> = vec![Vec::new(); region_count];
    for ch in &table_chars {
        let cy = ch.bbox.y + ch.bbox.height / 2.0;
        for r in 0..region_count {
            if cy >= row_bounds[r] - 2.0 && cy < row_bounds[r + 1] + 2.0 {
                region_chars[r].push(ch);
                break;
            }
        }
    }

    // ── 步骤1：在每个区域内按 y 坐标递增扫描聚类成子行 ──
    // 记录每个子行属于哪个 region (用于区分表头和数据区域)
    struct SubRow<'a> {
        chars: Vec<&'a RawChar>,
        region_idx: usize,
    }

    let mut sub_rows: Vec<SubRow> = Vec::new();
    for (region_idx, region) in region_chars.iter().enumerate() {
        if region.is_empty() {
            continue;
        }
        let mut sorted: Vec<&RawChar> = region.clone();
        sorted.sort_by(|a, b| {
            a.bbox
                .y
                .partial_cmp(&b.bbox.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut current_row: Vec<&RawChar> = vec![sorted[0]];
        let mut row_y = sorted[0].bbox.y;

        for &ch in sorted.iter().skip(1) {
            let delta = ch.bbox.y - row_y;
            if delta > 6.0 {
                sub_rows.push(SubRow {
                    chars: std::mem::take(&mut current_row),
                    region_idx,
                });
                row_y = ch.bbox.y;
            }
            current_row.push(ch);
        }
        if !current_row.is_empty() {
            sub_rows.push(SubRow {
                chars: current_row,
                region_idx,
            });
        }
    }

    // 对每个子行内的字符按 x 排序
    for sub_row in sub_rows.iter_mut() {
        sub_row.chars.sort_by(|a, b| {
            a.bbox
                .x
                .partial_cmp(&b.bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // ── 步骤2：仅用数据行（region_idx >= 1）检测列间隙 ──
    const MIN_COL_GAP: f32 = 20.0;
    let mut all_gap_positions: Vec<f32> = Vec::new();

    // 统计每个数据行的间隙数量，用于一致性验证
    // 判定逻辑：按 MIN_COL_GAP 拆段，表格行应有 ≥3 个短段，段落行通常是 1-2 个长段
    let mut per_row_gap_counts: Vec<usize> = Vec::new();

    for sub_row in &sub_rows {
        if sub_row.region_idx < 1 {
            continue; // 跳过表头区域
        }

        let row = &sub_row.chars;
        // 按 MIN_COL_GAP 拆段，统计段数和各段字符数
        let mut segments: Vec<usize> = Vec::new(); // 每段的字符数
        let mut current_seg_len: usize = 1;

        for i in 1..row.len() {
            let prev_right = row[i - 1].bbox.x + row[i - 1].bbox.width;
            let curr_left = row[i].bbox.x;
            let gap = curr_left - prev_right;
            if gap >= MIN_COL_GAP {
                segments.push(current_seg_len);
                current_seg_len = 1;
            } else {
                current_seg_len += 1;
            }
        }
        if !row.is_empty() {
            segments.push(current_seg_len);
        }

        let gap_count = if segments.len() >= 1 {
            segments.len() - 1
        } else {
            0
        };

        // 表格行判定：≥3 段（≥2 个间隙），且各段平均长度 ≤ 30 字符
        let avg_seg_len = if !segments.is_empty() {
            segments.iter().sum::<usize>() as f32 / segments.len() as f32
        } else {
            0.0
        };

        if gap_count >= 2 && avg_seg_len <= 30.0 {
            per_row_gap_counts.push(gap_count);
        } else {
            per_row_gap_counts.push(usize::MAX); // 标记为无效（段落行）
        }
    }

    // 找出有效行（非 usize::MAX）中最常见的间隙数量（众数）
    let mut gap_count_freq: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for &gc in &per_row_gap_counts {
        if gc != usize::MAX && gc >= 2 {
            // 只统计合理的间隙数量（≥2 意味着 ≥3 列）
            *gap_count_freq.entry(gc).or_insert(0) += 1;
        }
    }

    let (mode_gap_count, mode_freq) = gap_count_freq
        .iter()
        .max_by_key(|(_, &freq)| freq)
        .map(|(&gc, &freq)| (gc, freq))
        .unwrap_or((0, 0));

    // 至少需要 2 行具有一致的间隙模式
    if mode_freq < 2 || mode_gap_count < 2 {
        return None;
    }

    // 只使用间隙数量等于众数的数据行来推断列边界
    // 同时过滤 sub_rows：只保留有效的表格行
    let mut valid_data_indices: Vec<usize> = Vec::new();
    let mut data_row_idx = 0;
    for (i, sub_row) in sub_rows.iter().enumerate() {
        if sub_row.region_idx < 1 {
            continue;
        }
        if data_row_idx < per_row_gap_counts.len()
            && per_row_gap_counts[data_row_idx] == mode_gap_count
        {
            valid_data_indices.push(i);

            let row = &sub_row.chars;
            for j in 1..row.len() {
                let prev_right = row[j - 1].bbox.x + row[j - 1].bbox.width;
                let curr_left = row[j].bbox.x;
                let gap = curr_left - prev_right;
                if gap >= MIN_COL_GAP {
                    let gap_center = (prev_right + curr_left) / 2.0;
                    all_gap_positions.push(gap_center);
                }
            }
        }
        data_row_idx += 1;
    }

    if all_gap_positions.is_empty() {
        return None;
    }

    // 聚类间隙位置 → 列分隔 x 坐标
    all_gap_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut col_separators: Vec<f32> = Vec::new();
    for &pos in &all_gap_positions {
        if let Some(last) = col_separators.last() {
            if (pos - last).abs() < MIN_COL_GAP {
                let n = col_separators.len();
                col_separators[n - 1] = (col_separators[n - 1] + pos) / 2.0;
                continue;
            }
        }
        col_separators.push(pos);
    }

    // 构建列边界
    let mut col_bounds = vec![table_x_min];
    col_bounds.extend(&col_separators);
    col_bounds.push(table_x_max);

    let col_count = col_bounds.len() - 1;
    if col_count < 2 {
        return None;
    }

    // ── 步骤3：处理表头区域 ──
    // 表头区域 = region_idx == 0 的所有子行（过滤掉明显非表格的行）
    // 用有效数据行的最大字符数 ×3 作为表头行的字符数上限
    let max_data_row_chars = valid_data_indices
        .iter()
        .map(|&i| sub_rows[i].chars.len())
        .max()
        .unwrap_or(50);
    let header_char_limit = (max_data_row_chars * 3).max(60);

    // 如果有多个子行（如 "Sequential" + "Operations"），纵向合并
    let header_sub_rows: Vec<&SubRow> = sub_rows
        .iter()
        .filter(|r| r.region_idx == 0 && r.chars.len() <= header_char_limit)
        .collect();
    // 有效数据行 = 通过一致性验证的行
    let valid_data_sub_rows: Vec<&SubRow> =
        valid_data_indices.iter().map(|&i| &sub_rows[i]).collect();

    // 将表头子行的字符投影到列，然后纵向合并
    let mut header_texts: Vec<String> = vec![String::new(); col_count];
    if header_sub_rows.len() <= 1 {
        // 单行表头：直接投影
        if let Some(hr) = header_sub_rows.first() {
            let mut col_chars: Vec<Vec<&RawChar>> = vec![Vec::new(); col_count];
            for ch in &hr.chars {
                let cx = ch.bbox.x + ch.bbox.width / 2.0;
                for c in 0..col_count {
                    if cx >= col_bounds[c] - 2.0 && cx < col_bounds[c + 1] + 2.0 {
                        col_chars[c].push(ch);
                        break;
                    }
                }
            }
            for (c, cchars) in col_chars.iter().enumerate() {
                header_texts[c] = chars_to_text_with_spacing(cchars).trim().to_string();
            }
        }
    } else {
        // 多行表头：每个子行投影到列后，纵向用 " " (空格) 拼接
        let mut col_lines: Vec<Vec<String>> = vec![Vec::new(); col_count];
        for hr in &header_sub_rows {
            let mut col_chars: Vec<Vec<&RawChar>> = vec![Vec::new(); col_count];
            for ch in &hr.chars {
                let cx = ch.bbox.x + ch.bbox.width / 2.0;
                for c in 0..col_count {
                    if cx >= col_bounds[c] - 2.0 && cx < col_bounds[c + 1] + 2.0 {
                        col_chars[c].push(ch);
                        break;
                    }
                }
            }
            for (c, cchars) in col_chars.iter().enumerate() {
                let line_text = chars_to_text_with_spacing(cchars).trim().to_string();
                if !line_text.is_empty() {
                    col_lines[c].push(line_text);
                }
            }
        }
        for (c, lines) in col_lines.iter().enumerate() {
            header_texts[c] = lines.join(" ");
        }
    }

    log::info!(
        "Booktabs table detected on page {}: {} data rows x {} cols, {} h-lines, headers: {:?}",
        page_index,
        valid_data_sub_rows.len(),
        col_count,
        h_sorted.len(),
        header_texts,
    );

    // ── 步骤4：将有效数据行字符投影到网格（带空格检测）──
    let data_row_count = valid_data_sub_rows.len();
    let mut cell_texts: Vec<Vec<String>> = vec![vec![String::new(); col_count]; data_row_count];

    for (r, sub_row) in valid_data_sub_rows.iter().enumerate() {
        // 先按列分组字符
        let mut col_chars: Vec<Vec<&RawChar>> = vec![Vec::new(); col_count];
        for ch in &sub_row.chars {
            let cx = ch.bbox.x + ch.bbox.width / 2.0;
            for c in 0..col_count {
                if cx >= col_bounds[c] - 2.0 && cx < col_bounds[c + 1] + 2.0 {
                    col_chars[c].push(ch);
                    break;
                }
            }
        }
        for (c, cchars) in col_chars.iter().enumerate() {
            cell_texts[r][c] = chars_to_text_with_spacing(cchars).trim().to_string();
        }
    }

    // CellType 推断
    let column_types = infer_column_types(&cell_texts, 0, col_count);

    // 构建 TableRow/TableCell
    let mut table_rows = Vec::new();
    for row_idx in 0..data_row_count {
        let mut cells = Vec::new();
        for col in 0..col_count {
            cells.push(TableCell {
                row: row_idx,
                col,
                text: cell_texts[row_idx][col].clone(),
                cell_type: if col < column_types.len() {
                    column_types[col]
                } else {
                    CellType::Unknown
                },
                rowspan: 1,
                colspan: 1,
            });
        }
        table_rows.push(TableRow {
            row_index: row_idx,
            cells,
        });
    }

    let fallback_text = generate_fallback_text(&header_texts, &table_rows, table_id, page_index);

    let table_bbox = BBox::new(
        table_x_min,
        row_bounds[0],
        table_x_max - table_x_min,
        row_bounds.last().unwrap() - row_bounds[0],
    );


    Some(TableIR {
        table_id: table_id.to_string(),
        page_index,
        bbox: table_bbox,
        extraction_mode: ExtractionMode::Ruled,
        headers: header_texts,
        rows: table_rows,
        column_types,
        fallback_text,
            confidence: None,
    })
}

// ─── 测试 ───

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{LineOrientation, Point};

    fn make_h_line(x1: f32, x2: f32, y: f32, width: f32) -> RawLine {
        RawLine {
            start: Point { x: x1, y },
            end: Point { x: x2, y },
            width,
            orientation: LineOrientation::Horizontal,
        }
    }

    fn make_v_line(x: f32, y1: f32, y2: f32, width: f32) -> RawLine {
        RawLine {
            start: Point { x, y: y1 },
            end: Point { x, y: y2 },
            width,
            orientation: LineOrientation::Vertical,
        }
    }

    fn make_char(unicode: char, x: f32, y: f32, w: f32, h: f32) -> RawChar {
        RawChar {
            unicode,
            bbox: BBox::new(x, y, w, h),
            font_size: h,
            font_name: None,
            is_bold: false,
        }
    }

    #[test]
    fn test_filter_noise() {
        let mut lines = vec![
            NormalizedLine {
                x: 10.0,
                y: 100.0,
                start: 10.0,
                end: 12.0,
                width: 1.0,
                is_horizontal: true,
            }, // 太短
            NormalizedLine {
                x: 10.0,
                y: 200.0,
                start: 10.0,
                end: 200.0,
                width: 1.0,
                is_horizontal: true,
            }, // OK
        ];
        filter_noise(&mut lines);
        assert_eq!(lines.len(), 1);
        assert!((lines[0].y - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_merge_collinear() {
        let mut lines = vec![
            NormalizedLine {
                x: 10.0,
                y: 100.0,
                start: 10.0,
                end: 100.0,
                width: 1.0,
                is_horizontal: true,
            },
            NormalizedLine {
                x: 100.0,
                y: 100.5,
                start: 100.0,
                end: 200.0,
                width: 1.0,
                is_horizontal: true,
            },
        ];
        merge_collinear(&mut lines, true);
        assert_eq!(lines.len(), 1);
        assert!((lines[0].start - 10.0).abs() < 0.01);
        assert!((lines[0].end - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_build_grid() {
        // 3×3 网格（2行2列）
        let h_lines = vec![
            NormalizedLine {
                x: 0.0,
                y: 10.0,
                start: 10.0,
                end: 310.0,
                width: 1.0,
                is_horizontal: true,
            },
            NormalizedLine {
                x: 0.0,
                y: 50.0,
                start: 10.0,
                end: 310.0,
                width: 1.0,
                is_horizontal: true,
            },
            NormalizedLine {
                x: 0.0,
                y: 90.0,
                start: 10.0,
                end: 310.0,
                width: 1.0,
                is_horizontal: true,
            },
        ];
        let v_lines = vec![
            NormalizedLine {
                x: 10.0,
                y: 0.0,
                start: 10.0,
                end: 90.0,
                width: 1.0,
                is_horizontal: false,
            },
            NormalizedLine {
                x: 160.0,
                y: 0.0,
                start: 10.0,
                end: 90.0,
                width: 1.0,
                is_horizontal: false,
            },
            NormalizedLine {
                x: 310.0,
                y: 0.0,
                start: 10.0,
                end: 90.0,
                width: 1.0,
                is_horizontal: false,
            },
        ];

        let grid = build_grid(&h_lines, &v_lines).unwrap();
        assert_eq!(grid.row_count, 2);
        assert_eq!(grid.col_count, 2);
    }

    #[test]
    fn test_simple_ruled_table() {
        // 构建一个简单的 2列3行 有线表格
        let lines = vec![
            // 水平线
            make_h_line(10.0, 310.0, 10.0, 1.0),  // 顶边
            make_h_line(10.0, 310.0, 40.0, 2.0),  // 表头分隔线（粗）
            make_h_line(10.0, 310.0, 70.0, 1.0),  // 第一行
            make_h_line(10.0, 310.0, 100.0, 1.0), // 底边
            // 垂直线
            make_v_line(10.0, 10.0, 100.0, 1.0),  // 左边
            make_v_line(160.0, 10.0, 100.0, 1.0), // 中间
            make_v_line(310.0, 10.0, 100.0, 1.0), // 右边
        ];

        let chars = vec![
            // 表头行：Name | Score
            make_char('N', 20.0, 20.0, 8.0, 12.0),
            make_char('a', 28.0, 20.0, 8.0, 12.0),
            make_char('m', 36.0, 20.0, 8.0, 12.0),
            make_char('e', 44.0, 20.0, 8.0, 12.0),
            make_char('9', 170.0, 20.0, 8.0, 12.0),
            make_char('5', 178.0, 20.0, 8.0, 12.0),
            // 数据行1
            make_char('A', 20.0, 50.0, 8.0, 12.0),
            make_char('1', 170.0, 50.0, 8.0, 12.0),
            make_char('0', 178.0, 50.0, 8.0, 12.0),
            // 数据行2
            make_char('B', 20.0, 80.0, 8.0, 12.0),
            make_char('2', 170.0, 80.0, 8.0, 12.0),
            make_char('0', 178.0, 80.0, 8.0, 12.0),
        ];

        let result = extract_ruled_table(&lines, &[], &chars, 0, "t0_0");
        assert!(result.is_some(), "Should extract a ruled table");

        let table = result.unwrap();
        assert_eq!(table.extraction_mode, ExtractionMode::Ruled);
        assert_eq!(table.headers.len(), 2);
        assert_eq!(table.rows.len(), 2);
        assert!(!table.fallback_text.is_empty());
    }
}
