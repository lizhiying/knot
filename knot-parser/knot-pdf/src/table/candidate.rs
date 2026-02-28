//! 表格候选区域检测
//!
//! 基于文本对齐特征检测页面中可能包含表格的区域：
//! - 多行 x 坐标对齐分析
//! - 列间距规律性检测
//! - 数值密度分析

use super::{CandidateType, TableCandidate};
use crate::backend::RawChar;
use crate::ir::BBox;

/// 最少行数：至少需要这么多行才考虑是表格
const MIN_TABLE_ROWS: usize = 2;

/// 最少列数：至少需要这么多列才考虑是表格
const MIN_TABLE_COLS: usize = 2;

/// x 坐标对齐容差（pt）
const X_ALIGN_TOLERANCE: f32 = 5.0;

/// 行 y 坐标聚类容差（pt）
const ROW_Y_TOLERANCE: f32 = 3.0;

/// 列间隙最小宽度（pt）— 区分同一单元格内的空格和列间隙
const MIN_COLUMN_GAP: f32 = 15.0;

/// 对齐比例阈值：至少这个比例的行在某个 x 位置对齐才算一列
const ALIGN_RATIO_THRESHOLD: f32 = 0.4;

/// 数值字符（用于数值密度分析）
fn is_numeric_char(c: char) -> bool {
    c.is_ascii_digit()
        || c == '.'
        || c == ','
        || c == '%'
        || c == '$'
        || c == '¥'
        || c == '€'
        || c == '£'
        || c == '-'
        || c == '+'
}

/// 检测表格候选区域
pub fn detect_table_candidates(
    chars: &[RawChar],
    page_width: f32,
    _page_height: f32,
) -> Vec<TableCandidate> {
    if chars.is_empty() {
        return Vec::new();
    }

    // 1. 按 y 坐标聚类成行
    let rows = cluster_rows(chars);
    log::debug!(
        "Table candidate: {} chars -> {} rows",
        chars.len(),
        rows.len()
    );
    if rows.len() < MIN_TABLE_ROWS {
        return Vec::new();
    }

    // 2. 对每行进行 x 坐标分段（按列间隙拆分）
    let row_segments: Vec<Vec<Segment>> = rows
        .iter()
        .map(|row| segment_row(row, page_width))
        .collect();

    // debug: 打印每行的段数
    for (i, segs) in row_segments.iter().enumerate() {
        if i < 20 {
            let seg_info: Vec<String> = segs
                .iter()
                .map(|s| {
                    format!(
                        "x={:.0}..{:.0} '{}'",
                        s.x_start,
                        s.x_end,
                        &s.text[..s.text.len().min(15)]
                    )
                })
                .collect();
            log::trace!("  row[{}] {} segs: {:?}", i, segs.len(), seg_info);
        }
    }

    // 3. 寻找连续多行具有相似列结构的区域
    let regions = find_aligned_regions(&rows, &row_segments);
    log::debug!("Table candidate: {} aligned regions found", regions.len());
    for (i, r) in regions.iter().enumerate() {
        log::debug!(
            "  region[{}]: rows {}..{}, {} cols, col_x={:?}",
            i,
            r.start_row,
            r.end_row,
            r.col_count,
            r.col_x_positions
        );
    }

    // 4. 对每个区域评估置信度
    let mut candidates = Vec::new();
    for region in regions {
        let region_chars: Vec<RawChar> = rows[region.start_row..=region.end_row]
            .iter()
            .flat_map(|row| row.iter().cloned())
            .collect();

        if region_chars.is_empty() {
            continue;
        }

        let bbox = compute_bbox(&region_chars);
        let confidence =
            compute_confidence(&region, &row_segments[region.start_row..=region.end_row]);

        if confidence > 0.3 {
            candidates.push(TableCandidate {
                bbox,
                confidence,
                candidate_type: CandidateType::Stream,
                chars: region_chars,
            });
        }
    }

    candidates
}

/// 行中的文本段（一个段 ≈ 一个单元格的内容）
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Segment {
    /// 段的起始 x
    x_start: f32,
    /// 段的结束 x
    x_end: f32,
    /// 段的中心 x
    x_center: f32,
    /// 段中的字符
    chars: Vec<RawChar>,
    /// 文本内容
    text: String,
}

/// 对齐区域（连续多行具有相似列结构）
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AlignedRegion {
    start_row: usize,
    end_row: usize,
    /// 检测到的列数
    col_count: usize,
    /// 列的 x 坐标（左边界）
    col_x_positions: Vec<f32>,
}

/// 按 y 坐标将字符聚类成行
fn cluster_rows(chars: &[RawChar]) -> Vec<Vec<RawChar>> {
    let mut sorted: Vec<&RawChar> = chars.iter().collect();
    sorted.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rows: Vec<(f32, Vec<RawChar>)> = Vec::new();

    for ch in sorted {
        let y_center = ch.bbox.y + ch.bbox.height / 2.0;
        let mut found = false;
        for (row_y, row_chars) in rows.iter_mut() {
            if (y_center - *row_y).abs() < ROW_Y_TOLERANCE {
                row_chars.push(ch.clone());
                let n = row_chars.len() as f32;
                *row_y = *row_y * (n - 1.0) / n + y_center / n;
                found = true;
                break;
            }
        }
        if !found {
            rows.push((y_center, vec![ch.clone()]));
        }
    }

    // 每行内按 x 排序
    for (_, row_chars) in rows.iter_mut() {
        row_chars.sort_by(|a, b| {
            a.bbox
                .x
                .partial_cmp(&b.bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // 行按 y 排序
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    rows.into_iter().map(|(_, chars)| chars).collect()
}

/// 将一行字符按列间隙拆分成段
fn segment_row(row: &[RawChar], _page_width: f32) -> Vec<Segment> {
    if row.is_empty() {
        return Vec::new();
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut current_chars: Vec<RawChar> = vec![row[0].clone()];

    for i in 1..row.len() {
        let prev = &row[i - 1];
        let curr = &row[i];
        let gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);

        if gap > MIN_COLUMN_GAP {
            // 大间隙：拆分为新段
            let seg = build_segment(&current_chars);
            segments.push(seg);
            current_chars = vec![curr.clone()];
        } else {
            current_chars.push(curr.clone());
        }
    }

    // 收尾
    if !current_chars.is_empty() {
        let seg = build_segment(&current_chars);
        segments.push(seg);
    }

    segments
}

/// 从字符列表构建一个 Segment
fn build_segment(chars: &[RawChar]) -> Segment {
    let x_start = chars.iter().map(|c| c.bbox.x).fold(f32::MAX, f32::min);
    let x_end = chars
        .iter()
        .map(|c| c.bbox.x + c.bbox.width)
        .fold(f32::MIN, f32::max);
    let text: String = chars.iter().map(|c| c.unicode).collect();

    Segment {
        x_start,
        x_end,
        x_center: (x_start + x_end) / 2.0,
        chars: chars.to_vec(),
        text,
    }
}

/// 寻找连续多行具有相似列结构的区域
fn find_aligned_regions(
    rows: &[Vec<RawChar>],
    row_segments: &[Vec<Segment>],
) -> Vec<AlignedRegion> {
    if rows.is_empty() || row_segments.is_empty() {
        return Vec::new();
    }

    let mut regions = Vec::new();
    let mut i = 0;

    while i < row_segments.len() {
        // 跳过单段行（不可能是表格行）
        if row_segments[i].len() < MIN_TABLE_COLS {
            i += 1;
            continue;
        }

        // 以当前行为起点，尝试向下扩展
        let col_count = row_segments[i].len();
        let col_x: Vec<f32> = row_segments[i].iter().map(|s| s.x_start).collect();

        let mut end = i;
        let mut j = i + 1;
        let mut consecutive_noise = 0;
        while j < row_segments.len() {
            if is_column_compatible(&row_segments[j], &col_x) {
                end = j;
                consecutive_noise = 0;
            } else if is_noise_row(&rows[j]) {
                // 噪声行（只含标点/空白/逗号）：跳过但不中断
                consecutive_noise += 1;
                if consecutive_noise > 2 {
                    break; // 连续噪声太多，中断
                }
            } else {
                break;
            }
            j += 1;
        }

        let row_count = end - i + 1;
        if row_count >= MIN_TABLE_ROWS {
            regions.push(AlignedRegion {
                start_row: i,
                end_row: end,
                col_count,
                col_x_positions: col_x,
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }

    regions
}

/// 判断一行是否为"噪声行"（只含标点/空白/逗号等）
/// 典型场景：数字千位分隔符逗号被 PDF 渲染为独立的 y 行
fn is_noise_row(row: &[RawChar]) -> bool {
    if row.len() > 10 {
        return false; // 超过 10 个字符不太可能是噪声
    }
    row.iter().all(|c| {
        let ch = c.unicode;
        ch == ','
            || ch == '.'
            || ch == ' '
            || ch == '\u{00a0}'
            || ch == ';'
            || ch == ':'
            || ch == '-'
    })
}

/// 判断一行的段结构是否与参考列 x 坐标兼容
fn is_column_compatible(segments: &[Segment], ref_col_x: &[f32]) -> bool {
    if segments.len() < MIN_TABLE_COLS {
        return false;
    }

    // 允许段数在 ±1 范围内（有些行可能有合并单元格）
    let count_diff = (segments.len() as i32 - ref_col_x.len() as i32).abs();
    if count_diff > 1 {
        return false;
    }

    // 检查至少有多少个段的 x_start 与参考列对齐
    let mut aligned = 0;
    for seg in segments {
        for &ref_x in ref_col_x {
            if (seg.x_start - ref_x).abs() < X_ALIGN_TOLERANCE {
                aligned += 1;
                break;
            }
        }
    }

    let align_ratio = aligned as f32 / ref_col_x.len().max(1) as f32;
    align_ratio >= ALIGN_RATIO_THRESHOLD
}

/// 计算字符集合的边界框
fn compute_bbox(chars: &[RawChar]) -> BBox {
    let x_min = chars.iter().map(|c| c.bbox.x).fold(f32::MAX, f32::min);
    let y_min = chars.iter().map(|c| c.bbox.y).fold(f32::MAX, f32::min);
    let x_max = chars
        .iter()
        .map(|c| c.bbox.x + c.bbox.width)
        .fold(f32::MIN, f32::max);
    let y_max = chars
        .iter()
        .map(|c| c.bbox.y + c.bbox.height)
        .fold(f32::MIN, f32::max);

    BBox::new(x_min, y_min, x_max - x_min, y_max - y_min)
}

/// 计算候选区域的置信度
fn compute_confidence(region: &AlignedRegion, row_segments: &[Vec<Segment>]) -> f32 {
    let row_count = region.end_row - region.start_row + 1;
    let col_count = region.col_count;

    // 基础分：行数和列数
    let row_score = ((row_count as f32 - 1.0) / 5.0).min(1.0); // 6行以上满分
    let col_score = ((col_count as f32 - 1.0) / 3.0).min(1.0); // 4列以上满分

    // 对齐一致性：各行的列数变化程度
    let col_counts: Vec<usize> = row_segments.iter().map(|s| s.len()).collect();
    let avg_cols = col_counts.iter().sum::<usize>() as f32 / col_counts.len().max(1) as f32;
    let col_variance: f32 = col_counts
        .iter()
        .map(|&c| (c as f32 - avg_cols).powi(2))
        .sum::<f32>()
        / col_counts.len().max(1) as f32;
    let consistency_score = 1.0 / (1.0 + col_variance);

    // 数值密度
    let all_text: String = row_segments
        .iter()
        .flat_map(|segs| segs.iter().map(|s| s.text.as_str()))
        .collect::<Vec<_>>()
        .join("");
    let numeric_count = all_text.chars().filter(|c| is_numeric_char(*c)).count();
    let total_count = all_text.chars().count().max(1);
    let numeric_density = numeric_count as f32 / total_count as f32;
    let numeric_score = (numeric_density * 2.0).min(1.0); // 50%以上数字满分

    // 综合
    row_score * 0.25 + col_score * 0.25 + consistency_score * 0.3 + numeric_score * 0.2
}
