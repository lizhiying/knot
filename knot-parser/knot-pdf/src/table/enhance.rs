//! 表格增强模块：置信度评估、合并单元格检测、IoU 消歧
//!
//! M11 新增功能：
//! - `TableConfidence` 结构化置信度评估
//! - 合并单元格检测（colspan/rowspan）
//! - 表格区域 IoU 重叠消歧
//! - 列对齐质量评估
//! - 行间距均匀性评估
//! - 数据密度评估

use serde::{Deserialize, Serialize};

use crate::backend::RawChar;
use crate::ir::{BBox, TableCell, TableIR, TableRow};
use crate::layout::compute_iou;

/// 表格置信度评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableConfidence {
    /// 总置信度 (0.0 ~ 1.0)
    pub score: f32,
    /// 列对齐质量 (0.0 ~ 1.0)
    pub alignment_score: f32,
    /// 行间距均匀性 (0.0 ~ 1.0)
    pub spacing_score: f32,
    /// 数据密度评分 (0.0 ~ 1.0)
    pub data_density_score: f32,
    /// 行列一致性 (0.0 ~ 1.0)
    pub consistency_score: f32,
}

impl Default for TableConfidence {
    fn default() -> Self {
        Self {
            score: 0.0,
            alignment_score: 0.0,
            spacing_score: 0.0,
            data_density_score: 0.0,
            consistency_score: 0.0,
        }
    }
}

// ============================================================
// 列对齐一致性检测
// ============================================================

/// 分析字符的 X 起点分布，返回稳定的列对齐中心和对齐质量评分
///
/// 策略：统计每行中 segment 的 x 起点，聚类找到稳定的列边界
pub fn evaluate_column_alignment(chars: &[RawChar]) -> (Vec<f32>, f32) {
    if chars.is_empty() {
        return (Vec::new(), 0.0);
    }

    // 按 Y 聚类成行
    let rows = cluster_rows_by_y(chars, 3.0);
    if rows.len() < 3 {
        return (Vec::new(), 0.0);
    }

    // 收集每行每个 segment 的 x 起点
    let mut all_x_starts: Vec<Vec<f32>> = Vec::new();
    for row in &rows {
        let segments = segment_by_gap(row, 12.0);
        let x_starts: Vec<f32> = segments.iter().map(|s| s.x_start).collect();
        if x_starts.len() >= 2 {
            all_x_starts.push(x_starts);
        }
    }

    if all_x_starts.len() < 3 {
        return (Vec::new(), 0.0);
    }

    // 聚类所有 x 起点
    let flat_x: Vec<f32> = all_x_starts
        .iter()
        .flat_map(|xs| xs.iter().copied())
        .collect();
    let clusters = cluster_values(&flat_x, 8.0);

    // 筛选出现频次 >= 行数的 40% 的聚类中心
    let threshold = (all_x_starts.len() as f32 * 0.4).ceil() as usize;
    let stable_centers: Vec<f32> = clusters
        .into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(center, _)| center)
        .collect();

    if stable_centers.len() < 2 {
        return (stable_centers, 0.2);
    }

    // 计算每行有多少个 x 起点命中了稳定聚类中心
    let mut match_ratios: Vec<f32> = Vec::new();
    for xs in &all_x_starts {
        let matched = xs
            .iter()
            .filter(|&&x| stable_centers.iter().any(|&c| (x - c).abs() < 10.0))
            .count();
        match_ratios.push(matched as f32 / xs.len().max(1) as f32);
    }

    let avg_match: f32 = match_ratios.iter().sum::<f32>() / match_ratios.len().max(1) as f32;

    // 列数越多 + 匹配率越高 → 分数越高
    let col_bonus = ((stable_centers.len() as f32 - 1.0) / 3.0).min(1.0);
    let score = avg_match * 0.7 + col_bonus * 0.3;

    (stable_centers, score.min(1.0))
}

// ============================================================
// 行间距均匀性检测
// ============================================================

/// 评估行间距的均匀性
///
/// 连续行的 Y 间距标准差 / 平均间距 < 0.3 → 高分
pub fn evaluate_row_spacing(chars: &[RawChar]) -> f32 {
    let rows = cluster_rows_by_y(chars, 3.0);
    if rows.len() < 3 {
        return 0.0;
    }

    // 计算相邻行的 Y 间距
    let mut row_y_centers: Vec<f32> = rows
        .iter()
        .map(|r| {
            let sum: f32 = r.iter().map(|c| c.bbox.y).sum();
            sum / r.len() as f32
        })
        .collect();
    row_y_centers.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let spacings: Vec<f32> = row_y_centers
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .collect();

    if spacings.is_empty() {
        return 0.0;
    }

    let avg_spacing = spacings.iter().sum::<f32>() / spacings.len() as f32;
    if avg_spacing < 1.0 {
        return 0.0;
    }

    let variance: f32 = spacings
        .iter()
        .map(|&s| (s - avg_spacing).powi(2))
        .sum::<f32>()
        / spacings.len() as f32;
    let std_dev = variance.sqrt();
    let cv = std_dev / avg_spacing; // 变异系数

    // CV < 0.1 → 1.0, CV > 0.5 → 0.0
    (1.0 - (cv / 0.5)).max(0.0).min(1.0)
}

// ============================================================
// 数据密度评估
// ============================================================

/// 评估区域内的数据密度（数字/数据标记比例）
pub fn evaluate_data_density(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 0.0;
    }

    let mut numeric_count = 0usize;
    let mut data_marker_count = 0usize;
    let total = chars.len();

    for c in chars {
        let ch = c.unicode;
        if ch.is_ascii_digit() || ch == '.' {
            numeric_count += 1;
        }
        if ",%$¥#€£+-±".contains(ch) {
            data_marker_count += 1;
        }
    }

    let numeric_ratio = numeric_count as f32 / total as f32;
    let marker_ratio = data_marker_count as f32 / total as f32;

    // 数字比例 > 50% → 满分, < 10% → 0 分
    let score = (numeric_ratio * 2.0).min(1.0) * 0.8 + (marker_ratio * 10.0).min(1.0) * 0.2;

    score.min(1.0)
}

// ============================================================
// 表格置信度综合评估
// ============================================================

/// 对 TableIR 计算结构化置信度
pub fn evaluate_table_confidence(table: &TableIR, chars: &[RawChar]) -> TableConfidence {
    // 行列一致性
    let col_counts: Vec<usize> = table.rows.iter().map(|r| r.cells.len()).collect();
    let avg_cols = if col_counts.is_empty() {
        0.0
    } else {
        col_counts.iter().sum::<usize>() as f32 / col_counts.len() as f32
    };
    let col_variance: f32 = if col_counts.is_empty() {
        0.0
    } else {
        col_counts
            .iter()
            .map(|&c| (c as f32 - avg_cols).powi(2))
            .sum::<f32>()
            / col_counts.len() as f32
    };
    let consistency_score = 1.0 / (1.0 + col_variance);

    // 使用字符数据评估
    let (_, alignment_score) = evaluate_column_alignment(chars);
    let spacing_score = evaluate_row_spacing(chars);
    let data_density_score = evaluate_data_density(chars);

    // 综合评分
    let score = alignment_score * 0.30
        + spacing_score * 0.20
        + data_density_score * 0.20
        + consistency_score * 0.30;

    TableConfidence {
        score: score.min(1.0),
        alignment_score,
        spacing_score,
        data_density_score,
        consistency_score,
    }
}

// ============================================================
// 合并单元格检测
// ============================================================

/// 检测并标记合并单元格（colspan/rowspan）
///
/// 策略：
/// 1. Colspan: 如果某个 cell 的文本宽度跨越了多个列边界 → 设定 colspan
/// 2. Rowspan: 如果相邻行的同一列有相同文本且上面的 cell 在下方没有边界 → 设定 rowspan
/// 3. 空单元格推断: 如果某行某列没有 cell → 插入空 cell 或视为被合并
pub fn detect_merged_cells(table: &mut TableIR) {
    if table.rows.is_empty() || table.headers.is_empty() {
        return;
    }

    let num_cols = table.headers.len();

    // --- Colspan 检测 ---
    // 如果某行的 cell 数少于列数，检查是否有 cell 的文本本应跨多列
    for row in &mut table.rows {
        if row.cells.len() < num_cols && !row.cells.is_empty() {
            // 简单启发式：如果只有一个 cell 且列数 > 1，它可能跨全部列
            if row.cells.len() == 1 && num_cols > 1 {
                row.cells[0].colspan = num_cols;
            }
        }
    }

    // --- Rowspan 检测 ---
    // 对每列，如果相邻行有完全相同的非空文本 → 合并
    for col_idx in 0..num_cols {
        let mut run_start: Option<usize> = None;
        let mut run_text: Option<String> = None;

        for row_idx in 0..table.rows.len() {
            let cell_text = table.rows[row_idx]
                .cells
                .iter()
                .find(|c| c.col == col_idx)
                .map(|c| c.text.clone());

            match (&run_text, &cell_text) {
                (Some(prev), Some(curr)) if !prev.is_empty() && prev == curr => {
                    // 继续 run
                }
                _ => {
                    // 结束之前的 run，如果长度 > 1 则标记 rowspan
                    if let (Some(start), Some(_text)) = (run_start, &run_text) {
                        let span = row_idx - start;
                        if span > 1 {
                            // 标记第一个 cell 的 rowspan
                            if let Some(cell) = table.rows[start]
                                .cells
                                .iter_mut()
                                .find(|c| c.col == col_idx)
                            {
                                cell.rowspan = span;
                            }
                            // 标记后续 cell 为被合并（可选：清空文本或标记）
                            for r in (start + 1)..row_idx {
                                if let Some(cell) =
                                    table.rows[r].cells.iter_mut().find(|c| c.col == col_idx)
                                {
                                    cell.text = String::new(); // 被合并的行置空
                                }
                            }
                        }
                    }
                    run_start = Some(row_idx);
                    run_text = cell_text;
                }
            }
        }

        // 处理最后一个 run
        if let (Some(start), Some(_text)) = (run_start, &run_text) {
            let span = table.rows.len() - start;
            if span > 1 && !_text.is_empty() {
                if let Some(cell) = table.rows[start]
                    .cells
                    .iter_mut()
                    .find(|c| c.col == col_idx)
                {
                    cell.rowspan = span;
                }
                for r in (start + 1)..table.rows.len() {
                    if let Some(cell) = table.rows[r].cells.iter_mut().find(|c| c.col == col_idx) {
                        cell.text = String::new();
                    }
                }
            }
        }
    }

    // --- 空单元格填充 ---
    // 先收集 rowspan 信息，然后填充空单元格
    let mut rowspan_covers: Vec<(usize, usize)> = Vec::new(); // (row_idx, col_idx)
    for row in &table.rows {
        for cell in &row.cells {
            if cell.rowspan > 1 {
                for r in 1..cell.rowspan {
                    rowspan_covers.push((cell.row + r, cell.col));
                }
            }
        }
    }

    for row in &mut table.rows {
        let existing_cols: Vec<usize> = row.cells.iter().map(|c| c.col).collect();
        for col_idx in 0..num_cols {
            if !existing_cols.contains(&col_idx) {
                let covered = rowspan_covers.contains(&(row.row_index, col_idx));
                if !covered {
                    row.cells.push(TableCell {
                        row: row.row_index,
                        col: col_idx,
                        text: String::new(),
                        cell_type: crate::ir::CellType::Unknown,
                        rowspan: 1,
                        colspan: 1,
                    });
                }
            }
        }
        // 按列排序
        row.cells.sort_by_key(|c| c.col);
    }
}

// ============================================================
// IoU 重叠消歧
// ============================================================

/// 对表格候选区域进行 IoU 重叠消歧
///
/// 如果两个表格区域 IoU > threshold → 保留置信度更高的
pub fn deduplicate_tables_by_iou(tables: &mut Vec<TableIR>, iou_threshold: f32) {
    if tables.len() <= 1 {
        return;
    }

    let n = tables.len();
    let mut keep = vec![true; n];

    for i in 0..n {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..n {
            if !keep[j] {
                continue;
            }
            let iou = compute_iou(&tables[i].bbox, &tables[j].bbox);
            if iou > iou_threshold {
                // 保留 cell 数更多的（更完整的表格）
                let ci = tables[i].cell_count();
                let cj = tables[j].cell_count();
                if ci >= cj {
                    keep[j] = false;
                } else {
                    keep[i] = false;
                    break;
                }
            }
        }
    }

    let mut idx = 0;
    tables.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

/// 判断文本块是否在表格区域内（用于过滤）
pub fn is_block_inside_table(block_bbox: &BBox, table_bbox: &BBox) -> bool {
    // 计算 block 被 table 覆盖的面积比
    let inter_x_start = block_bbox.x.max(table_bbox.x);
    let inter_x_end = block_bbox.right().min(table_bbox.right());
    let inter_y_start = block_bbox.y.max(table_bbox.y);
    let inter_y_end = block_bbox.bottom().min(table_bbox.bottom());

    if inter_x_start >= inter_x_end || inter_y_start >= inter_y_end {
        return false;
    }

    let inter_area = (inter_x_end - inter_x_start) * (inter_y_end - inter_y_start);
    let block_area = block_bbox.area();

    if block_area <= 0.0 {
        return false;
    }

    // 如果 block 80% 以上面积在 table 内 → 认为在 table 内
    inter_area / block_area > 0.8
}

// ============================================================
// 辅助函数
// ============================================================

/// 按 Y 坐标聚类字符成行
fn cluster_rows_by_y(chars: &[RawChar], tolerance: f32) -> Vec<Vec<&RawChar>> {
    if chars.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<&RawChar> = chars.iter().collect();
    sorted.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rows: Vec<Vec<&RawChar>> = Vec::new();
    let mut current_row: Vec<&RawChar> = vec![sorted[0]];
    let mut current_y = sorted[0].bbox.y;

    for c in sorted.iter().skip(1) {
        if (c.bbox.y - current_y).abs() <= tolerance {
            current_row.push(c);
        } else {
            current_row.sort_by(|a, b| {
                a.bbox
                    .x
                    .partial_cmp(&b.bbox.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            rows.push(current_row);
            current_row = vec![c];
            current_y = c.bbox.y;
        }
    }
    if !current_row.is_empty() {
        current_row.sort_by(|a, b| {
            a.bbox
                .x
                .partial_cmp(&b.bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows.push(current_row);
    }

    rows
}

/// 行内文本段
struct TextSegment {
    x_start: f32,
    #[allow(dead_code)]
    x_end: f32,
    #[allow(dead_code)]
    text: String,
}

/// 按间距将行内字符拆分成段
fn segment_by_gap(chars: &[&RawChar], min_gap: f32) -> Vec<TextSegment> {
    if chars.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut seg_start = 0;

    for i in 1..chars.len() {
        let gap = chars[i].bbox.x - (chars[i - 1].bbox.x + chars[i - 1].bbox.width);
        if gap > min_gap {
            let text: String = chars[seg_start..i]
                .iter()
                .map(|c| c.unicode.to_string())
                .collect();
            segments.push(TextSegment {
                x_start: chars[seg_start].bbox.x,
                x_end: chars[i - 1].bbox.x + chars[i - 1].bbox.width,
                text,
            });
            seg_start = i;
        }
    }

    // 最后一段
    let text: String = chars[seg_start..]
        .iter()
        .map(|c| c.unicode.to_string())
        .collect();
    segments.push(TextSegment {
        x_start: chars[seg_start].bbox.x,
        x_end: chars.last().map(|c| c.bbox.x + c.bbox.width).unwrap_or(0.0),
        text,
    });

    segments
}

/// 对一维浮点数进行聚类，返回 (聚类中心, 频次)
fn cluster_values(values: &[f32], tolerance: f32) -> Vec<(f32, usize)> {
    if values.is_empty() {
        return Vec::new();
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut clusters: Vec<(f32, usize)> = Vec::new();
    let mut current_sum = sorted[0];
    let mut current_count = 1usize;

    for &v in sorted.iter().skip(1) {
        let current_center = current_sum / current_count as f32;
        if (v - current_center).abs() <= tolerance {
            current_sum += v;
            current_count += 1;
        } else {
            clusters.push((current_sum / current_count as f32, current_count));
            current_sum = v;
            current_count = 1;
        }
    }
    clusters.push((current_sum / current_count as f32, current_count));

    clusters
}

// ============================================================
// 表头检测增强
// ============================================================

/// 表头检测结果
#[derive(Debug, Clone)]
pub struct HeaderDetection {
    /// 是否检测到表头行
    pub has_header: bool,
    /// 表头行数（通常 1，偶尔 2）
    pub header_row_count: usize,
    /// 原因
    pub reason: String,
}

/// 分析表格首行/前两行的字体特征与后续行的差异，检测是否为表头
///
/// 策略：
/// 1. 首行字符的平均 font_size 比后续行大 1.2x+ → 表头
/// 2. 首行字符的 is_bold 比例 > 50% 且后续行 < 20% → 表头
/// 3. 首行字符全为大写字母/短文本 + 后续行为数字/混合 → 表头
pub fn detect_header_row(chars: &[RawChar]) -> HeaderDetection {
    let no_header = HeaderDetection {
        has_header: false,
        header_row_count: 0,
        reason: String::new(),
    };

    if chars.is_empty() {
        return no_header;
    }

    let rows = cluster_rows_by_y(chars, 3.0);
    if rows.len() < 2 {
        return no_header;
    }

    // 计算每行的字体特征
    let row_stats: Vec<RowFontStats> = rows
        .iter()
        .map(|row| {
            let total = row.len() as f32;
            if total == 0.0 {
                return RowFontStats {
                    avg_font_size: 0.0,
                    bold_ratio: 0.0,
                    char_count: 0,
                };
            }

            let sum_size: f32 = row.iter().map(|c| c.font_size).sum();
            let bold_count = row.iter().filter(|c| c.is_bold).count();

            RowFontStats {
                avg_font_size: sum_size / total,
                bold_ratio: bold_count as f32 / total,
                char_count: row.len(),
            }
        })
        .collect();

    let first = &row_stats[0];

    // 后续行的平均统计
    let body_rows = &row_stats[1..];
    let body_avg_size = if body_rows.is_empty() {
        first.avg_font_size
    } else {
        let total_chars: usize = body_rows.iter().map(|r| r.char_count).sum();
        if total_chars == 0 {
            first.avg_font_size
        } else {
            body_rows
                .iter()
                .map(|r| r.avg_font_size * r.char_count as f32)
                .sum::<f32>()
                / total_chars as f32
        }
    };

    let body_avg_bold = if body_rows.is_empty() {
        first.bold_ratio
    } else {
        body_rows.iter().map(|r| r.bold_ratio).sum::<f32>() / body_rows.len() as f32
    };

    // 策略 1: 字体大小差异
    if first.avg_font_size > 0.0 && body_avg_size > 0.0 {
        let size_ratio = first.avg_font_size / body_avg_size;
        if size_ratio >= 1.2 {
            return HeaderDetection {
                has_header: true,
                header_row_count: 1,
                reason: format!(
                    "首行字体({:.1}pt) 比后续行({:.1}pt) 大 {:.0}%",
                    first.avg_font_size,
                    body_avg_size,
                    (size_ratio - 1.0) * 100.0
                ),
            };
        }
    }

    // 策略 2: 加粗差异
    if first.bold_ratio > 0.5 && body_avg_bold < 0.2 {
        return HeaderDetection {
            has_header: true,
            header_row_count: 1,
            reason: format!(
                "首行加粗比例({:.0}%) 远高于后续行({:.0}%)",
                first.bold_ratio * 100.0,
                body_avg_bold * 100.0
            ),
        };
    }

    // 策略 3: 检查前两行是否都是表头（双行表头）
    if row_stats.len() >= 3 {
        let second = &row_stats[1];
        let body2_rows = &row_stats[2..];
        let body2_avg_bold = if body2_rows.is_empty() {
            0.0
        } else {
            body2_rows.iter().map(|r| r.bold_ratio).sum::<f32>() / body2_rows.len() as f32
        };

        if first.bold_ratio > 0.5 && second.bold_ratio > 0.5 && body2_avg_bold < 0.2 {
            return HeaderDetection {
                has_header: true,
                header_row_count: 2,
                reason: format!(
                    "前两行均加粗({}%/{}%), 后续行({}%)",
                    (first.bold_ratio * 100.0) as u32,
                    (second.bold_ratio * 100.0) as u32,
                    (body2_avg_bold * 100.0) as u32,
                ),
            };
        }
    }

    no_header
}

/// 行字体统计
struct RowFontStats {
    avg_font_size: f32,
    bold_ratio: f32,
    char_count: usize,
}

// ============================================================
// 低置信度告警
// ============================================================

/// 对低置信度表格添加 diagnostics 告警
///
/// 如果表格总置信度 < threshold → 在 fallback_text 中附加告警标记
pub fn flag_low_confidence_tables(tables: &mut [TableIR], threshold: f32) {
    for table in tables.iter_mut() {
        if let Some(ref conf) = table.confidence {
            if conf.score < threshold {
                let warning = format!(
                    "[knot-pdf warning] 低置信度表格 (score={:.2}): alignment={:.2}, spacing={:.2}, density={:.2}, consistency={:.2}",
                    conf.score, conf.alignment_score, conf.spacing_score,
                    conf.data_density_score, conf.consistency_score
                );
                log::warn!("{}", warning);
                // 在 fallback_text 末尾附加告警
                if !table.fallback_text.is_empty() {
                    table.fallback_text.push('\n');
                }
                table.fallback_text.push_str(&warning);
            }
        }
    }
}

/// 综合增强入口：对 TableIR 列表执行全部增强
///
/// 包含：
/// 1. 置信度评估
/// 2. 合并单元格检测
/// 3. IoU 重叠消歧
/// 4. 低置信度告警
pub fn enhance_tables(tables: &mut Vec<TableIR>, chars: &[RawChar], iou_threshold: f32) {
    // 1. 置信度评估
    for table in tables.iter_mut() {
        let conf = evaluate_table_confidence(table, chars);
        table.confidence = Some(conf);
    }

    // 2. 合并单元格检测
    for table in tables.iter_mut() {
        detect_merged_cells(table);
    }

    // 3. IoU 重叠消歧
    deduplicate_tables_by_iou(tables, iou_threshold);

    // 4. 低置信度告警 (阈值 0.3)
    flag_low_confidence_tables(tables, 0.3);
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CellType, ExtractionMode};

    fn make_char(x: f32, y: f32, text: &str) -> RawChar {
        RawChar {
            unicode: text.chars().next().unwrap_or(' '),
            bbox: BBox::new(x, y, 6.0, 12.0),
            font_size: 12.0,
            font_name: None,
            is_bold: false,
        }
    }

    fn make_table(rows: Vec<Vec<(&str, usize)>>) -> TableIR {
        let num_cols = rows.first().map(|r| r.len()).unwrap_or(0);
        let headers: Vec<String> = (0..num_cols).map(|i| format!("Col{}", i)).collect();

        let table_rows: Vec<TableRow> = rows
            .iter()
            .enumerate()
            .map(|(ri, row)| TableRow {
                row_index: ri,
                cells: row
                    .iter()
                    .enumerate()
                    .map(|(ci, (text, _))| TableCell {
                        row: ri,
                        col: ci,
                        text: text.to_string(),
                        cell_type: CellType::Text,
                        rowspan: 1,
                        colspan: 1,
                    })
                    .collect(),
            })
            .collect();

        TableIR {
            table_id: "test_t".to_string(),
            page_index: 0,
            bbox: BBox::new(0.0, 0.0, 500.0, 200.0),
            extraction_mode: ExtractionMode::Stream,
            headers,
            rows: table_rows,
            column_types: vec![CellType::Text; num_cols],
            fallback_text: String::new(),
            confidence: None,
        }
    }

    #[test]
    fn test_table_confidence_default() {
        let conf = TableConfidence::default();
        assert_eq!(conf.score, 0.0);
        assert_eq!(conf.alignment_score, 0.0);
    }

    #[test]
    fn test_table_confidence_serde() {
        let conf = TableConfidence {
            score: 0.85,
            alignment_score: 0.9,
            spacing_score: 0.8,
            data_density_score: 0.7,
            consistency_score: 0.95,
        };
        let json = serde_json::to_string(&conf).unwrap();
        let conf2: TableConfidence = serde_json::from_str(&json).unwrap();
        assert!((conf2.score - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_evaluate_row_spacing_uniform() {
        // 均匀间距的行
        let chars: Vec<RawChar> = (0..5)
            .flat_map(|row| {
                (0..3).map(move |col| {
                    make_char(50.0 + col as f32 * 100.0, 50.0 + row as f32 * 20.0, "A")
                })
            })
            .collect();

        let score = evaluate_row_spacing(&chars);
        assert!(score > 0.8, "均匀间距应得高分, got {}", score);
    }

    #[test]
    fn test_evaluate_row_spacing_irregular() {
        // 不均匀间距
        let chars = vec![
            make_char(50.0, 50.0, "A"),
            make_char(50.0, 70.0, "B"),
            make_char(50.0, 150.0, "C"), // 大间隙
            make_char(50.0, 160.0, "D"),
        ];

        let score = evaluate_row_spacing(&chars);
        assert!(score < 0.5, "不均匀间距应得低分, got {}", score);
    }

    #[test]
    fn test_evaluate_data_density_numeric() {
        let chars: Vec<RawChar> = "12345.67"
            .chars()
            .enumerate()
            .map(|(i, c)| make_char(i as f32 * 8.0, 50.0, &c.to_string()))
            .collect();

        let score = evaluate_data_density(&chars);
        assert!(score > 0.7, "高数字密度应得高分, got {}", score);
    }

    #[test]
    fn test_evaluate_data_density_text() {
        let chars: Vec<RawChar> = "Hello World"
            .chars()
            .enumerate()
            .map(|(i, c)| make_char(i as f32 * 8.0, 50.0, &c.to_string()))
            .collect();

        let score = evaluate_data_density(&chars);
        assert!(score < 0.3, "纯文本应得低分, got {}", score);
    }

    #[test]
    fn test_column_alignment_regular_grid() {
        // 3 列 × 5 行的规则网格
        let chars: Vec<RawChar> = (0..5)
            .flat_map(|row| {
                vec![
                    make_char(50.0, 50.0 + row as f32 * 20.0, "A"),
                    make_char(150.0, 50.0 + row as f32 * 20.0, "B"),
                    make_char(250.0, 50.0 + row as f32 * 20.0, "C"),
                ]
            })
            .collect();

        let (centers, score) = evaluate_column_alignment(&chars);
        assert!(centers.len() >= 2, "应检测到至少 2 个列中心");
        assert!(score > 0.5, "规则网格应得高分, got {}", score);
    }

    #[test]
    fn test_detect_merged_cells_colspan() {
        let mut table = make_table(vec![
            vec![("Header spanning all columns", 0)],
            vec![("A", 0), ("B", 1), ("C", 2)],
        ]);
        table.headers = vec!["Col0".into(), "Col1".into(), "Col2".into()];

        detect_merged_cells(&mut table);

        // 第一行只有 1 个 cell，应 colspan=3
        assert_eq!(table.rows[0].cells[0].colspan, 3);
    }

    #[test]
    fn test_detect_merged_cells_rowspan() {
        let mut table = make_table(vec![
            vec![("Category", 0), ("Value1", 1)],
            vec![("Category", 0), ("Value2", 1)],
            vec![("Other", 0), ("Value3", 1)],
        ]);
        table.headers = vec!["Label".into(), "Value".into()];

        detect_merged_cells(&mut table);

        // 第一列前两行是 "Category"，应 rowspan=2
        let first_cell = &table.rows[0].cells[0];
        assert_eq!(first_cell.rowspan, 2, "相同文本应合并");
        assert!(table.rows[1].cells[0].text.is_empty(), "被合并行应清空");
    }

    #[test]
    fn test_deduplicate_tables_by_iou() {
        let mut tables = vec![
            {
                let mut t = make_table(vec![vec![("A", 0), ("B", 1)]]);
                t.table_id = "t0".into();
                t.bbox = BBox::new(10.0, 10.0, 200.0, 100.0);
                t
            },
            {
                let mut t = make_table(vec![vec![("A", 0), ("B", 1)], vec![("C", 0), ("D", 1)]]);
                t.table_id = "t1".into();
                t.bbox = BBox::new(15.0, 15.0, 200.0, 100.0); // 高重叠
                t
            },
            {
                let mut t = make_table(vec![vec![("X", 0)]]);
                t.table_id = "t2".into();
                t.bbox = BBox::new(400.0, 400.0, 100.0, 50.0); // 不重叠
                t
            },
        ];

        deduplicate_tables_by_iou(&mut tables, 0.5);

        // t0 和 t1 高重叠，t1 有更多 cells，保留 t1
        assert_eq!(tables.len(), 2);
        assert!(tables.iter().any(|t| t.table_id == "t1"));
        assert!(tables.iter().any(|t| t.table_id == "t2"));
    }

    #[test]
    fn test_is_block_inside_table() {
        let table_bbox = BBox::new(50.0, 50.0, 400.0, 200.0);

        // 完全在内部
        let inside = BBox::new(60.0, 60.0, 100.0, 30.0);
        assert!(is_block_inside_table(&inside, &table_bbox));

        // 完全在外部
        let outside = BBox::new(500.0, 500.0, 100.0, 30.0);
        assert!(!is_block_inside_table(&outside, &table_bbox));

        // 部分重叠（不到 80%）
        let partial = BBox::new(400.0, 50.0, 200.0, 50.0);
        assert!(!is_block_inside_table(&partial, &table_bbox));
    }

    #[test]
    fn test_cluster_values() {
        let values = vec![10.0, 11.0, 12.0, 50.0, 51.0, 100.0];
        let clusters = cluster_values(&values, 5.0);
        assert_eq!(clusters.len(), 3, "应有 3 个聚类");
    }

    #[test]
    fn test_evaluate_table_confidence() {
        let table = make_table(vec![
            vec![("Name", 0), ("Age", 1), ("Score", 2)],
            vec![("Alice", 0), ("25", 1), ("95.5", 2)],
            vec![("Bob", 0), ("30", 1), ("88.0", 2)],
        ]);

        let chars: Vec<RawChar> = (0..3)
            .flat_map(|row| {
                (0..3).map(move |col| {
                    make_char(50.0 + col as f32 * 100.0, 50.0 + row as f32 * 20.0, "A")
                })
            })
            .collect();

        let conf = evaluate_table_confidence(&table, &chars);
        assert!(conf.score > 0.0, "非空表格应有正分");
        assert!(conf.consistency_score > 0.8, "一致的列数应得高分");
    }

    // ============================================================
    // 表头检测测试
    // ============================================================

    fn make_char_with_font(x: f32, y: f32, ch: char, font_size: f32, is_bold: bool) -> RawChar {
        RawChar {
            unicode: ch,
            bbox: BBox::new(x, y, 6.0, font_size),
            font_size,
            font_name: None,
            is_bold,
        }
    }

    #[test]
    fn test_header_detection_by_bold() {
        // 第一行加粗，后续行不加粗
        let mut chars = Vec::new();
        // 行1: 加粗
        for col in 0..5 {
            chars.push(make_char_with_font(
                col as f32 * 50.0,
                10.0,
                'H',
                12.0,
                true,
            ));
        }
        // 行2-4: 不加粗
        for row in 1..4 {
            for col in 0..5 {
                chars.push(make_char_with_font(
                    col as f32 * 50.0,
                    10.0 + row as f32 * 20.0,
                    '0',
                    12.0,
                    false,
                ));
            }
        }

        let result = detect_header_row(&chars);
        assert!(result.has_header, "加粗首行应被识别为表头");
        assert_eq!(result.header_row_count, 1);
        assert!(
            result.reason.contains("加粗"),
            "原因应提到加粗: {}",
            result.reason
        );
    }

    #[test]
    fn test_header_detection_by_font_size() {
        // 第一行字体 14pt，后续行 10pt
        let mut chars = Vec::new();
        for col in 0..4 {
            chars.push(make_char_with_font(
                col as f32 * 50.0,
                10.0,
                'A',
                14.0,
                false,
            ));
        }
        for row in 1..4 {
            for col in 0..4 {
                chars.push(make_char_with_font(
                    col as f32 * 50.0,
                    10.0 + row as f32 * 20.0,
                    '1',
                    10.0,
                    false,
                ));
            }
        }

        let result = detect_header_row(&chars);
        assert!(result.has_header, "更大字体的首行应被识别为表头");
        assert_eq!(result.header_row_count, 1);
        assert!(
            result.reason.contains("字体"),
            "原因应提到字体: {}",
            result.reason
        );
    }

    #[test]
    fn test_header_detection_no_header() {
        // 所有行字体和粗细相同
        let mut chars = Vec::new();
        for row in 0..4 {
            for col in 0..4 {
                chars.push(make_char_with_font(
                    col as f32 * 50.0,
                    row as f32 * 20.0,
                    'X',
                    12.0,
                    false,
                ));
            }
        }

        let result = detect_header_row(&chars);
        assert!(!result.has_header, "字体一致的行不应识别为表头");
    }

    #[test]
    fn test_header_detection_double_header() {
        // 前两行加粗，后续行不加粗
        let mut chars = Vec::new();
        for row in 0..2 {
            for col in 0..4 {
                chars.push(make_char_with_font(
                    col as f32 * 50.0,
                    row as f32 * 20.0,
                    'H',
                    12.0,
                    true,
                ));
            }
        }
        for row in 2..5 {
            for col in 0..4 {
                chars.push(make_char_with_font(
                    col as f32 * 50.0,
                    row as f32 * 20.0,
                    '0',
                    12.0,
                    false,
                ));
            }
        }

        let result = detect_header_row(&chars);
        assert!(result.has_header, "前两行加粗应被识别为双行表头");
        assert_eq!(result.header_row_count, 2);
    }

    // ============================================================
    // 低置信度告警测试
    // ============================================================

    #[test]
    fn test_flag_low_confidence_tables() {
        let mut tables = vec![make_table(vec![
            vec![("A", 0), ("B", 1)],
            vec![("1", 0), ("2", 1)],
        ])];
        // 设置一个低置信度
        tables[0].confidence = Some(TableConfidence {
            score: 0.2,
            alignment_score: 0.1,
            spacing_score: 0.3,
            data_density_score: 0.2,
            consistency_score: 0.1,
        });

        let _original_text = tables[0].fallback_text.clone();
        flag_low_confidence_tables(&mut tables, 0.3);

        assert!(
            tables[0].fallback_text.contains("低置信度"),
            "低置信度表格应在 fallback_text 中有告警"
        );
        assert!(tables[0].fallback_text.contains("0.20"), "应包含具体分数");
    }

    #[test]
    fn test_flag_high_confidence_no_warning() {
        let mut tables = vec![make_table(vec![
            vec![("A", 0), ("B", 1)],
            vec![("1", 0), ("2", 1)],
        ])];
        tables[0].confidence = Some(TableConfidence {
            score: 0.8,
            alignment_score: 0.9,
            spacing_score: 0.7,
            data_density_score: 0.8,
            consistency_score: 0.9,
        });

        let original_text = tables[0].fallback_text.clone();
        flag_low_confidence_tables(&mut tables, 0.3);

        assert_eq!(
            tables[0].fallback_text, original_text,
            "高置信度表格不应被修改"
        );
    }
}
