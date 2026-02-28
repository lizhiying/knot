//! 阅读顺序重建：行聚类、列检测、段落合并、隐式网格检测
//!
//! 将从 PDF 提取的原始字符转换为按阅读顺序排列的文本块。
//! 支持两种排序算法：
//! - Heuristic: 现有的启发式排序（基于 y 位置 + 列检测）
//! - XyCut: 递归分割算法，天然支持多栏布局

use crate::backend::RawChar;
use crate::config::ReadingOrderMethod;
use crate::ir::{BBox, BlockIR, BlockRole, TableIR, TextLine, TextSpan};
use crate::layout::xy_cut::{xy_cut_order, XyCutConfig};

/// 行聚类容差（y 坐标差异在此范围内视为同一行）
const LINE_Y_TOLERANCE: f32 = 3.0;

/// 列间隙阈值（x 方向间隙超过此比例视为分列）
const COLUMN_GAP_RATIO: f32 = 0.06;

/// 段落行距倍率（行距超过字号的此倍率视为新段落）
const PARAGRAPH_LINE_SPACING_RATIO: f32 = 1.8;

/// 词间距阈值（字符间距超过字号的此倍率视为空格）
/// PDFium 的字符 bbox 较紧凑，正常词间距的 ratio 约为 0.2~0.3，
/// 所以阈值取 0.15 以确保词间空格被正确检测。
const WORD_SPACING_RATIO: f32 = 0.15;

/// 最小行字符数（少于此数的行片段被视为噪声/侧边栏文字并过滤掉）
const MIN_LINE_CHAR_COUNT: usize = 2;

/// 横幅阈值：行宽度超过页面宽度的此比例视为横幅（不参与列分割）
const BANNER_WIDTH_RATIO: f32 = 0.7;

/// 最小列宽占页面宽度的比例
const MIN_COLUMN_WIDTH_RATIO: f32 = 0.15;

/// 网格检测：列 x 位置对齐容差（占页面宽度的比例）
const GRID_X_ALIGN_RATIO: f32 = 0.05;

/// 网格检测：最少需要的行数
const GRID_MIN_ROWS: usize = 2;

/// 网格检测：最少需要的列数
const GRID_MIN_COLS: usize = 2;

/// 常见列表项前缀模式
const BULLET_CHARS: &[char] = &['•', '·', '▪', '▸', '►', '◦', '‣', '⁃', '-', '–', '—'];

/// 从原始字符构建文本块（核心入口）
pub fn build_blocks(chars: &[RawChar], page_width: f32, page_height: f32) -> Vec<BlockIR> {
    let (blocks, _) = build_blocks_and_grids(chars, page_width, page_height, 0);
    blocks
}

/// 从原始字符构建文本块 + 支持配置阅读顺序算法
///
/// 根据 `reading_order_method` 配置选择使用 Heuristic 或 XY-Cut 算法排序。
pub fn build_blocks_with_config(
    chars: &[RawChar],
    page_width: f32,
    page_height: f32,
    page_index: usize,
    reading_order: ReadingOrderMethod,
    xy_cut_gap_ratio: f32,
) -> (Vec<BlockIR>, Vec<TableIR>) {
    // 先用现有方法构建块
    let (mut blocks, tables) = build_blocks_and_grids(chars, page_width, page_height, page_index);

    // 根据配置重新排序
    let use_xycut = match reading_order {
        ReadingOrderMethod::XyCut => true,
        ReadingOrderMethod::Heuristic => false,
        ReadingOrderMethod::Auto => {
            // Auto: 当检测到多个 block 的 x 范围有明显分离时，使用 XyCut
            has_multi_column_layout(&blocks, page_width)
        }
    };

    if use_xycut && blocks.len() > 1 {
        let bboxes: Vec<BBox> = blocks.iter().map(|b| b.bbox).collect();
        let config = XyCutConfig {
            gap_ratio: xy_cut_gap_ratio,
            ..XyCutConfig::default()
        };
        let order = xy_cut_order(&bboxes, page_width, page_height, &config);

        // 按 XY-Cut 顺序重新排列 blocks
        let old_blocks = blocks;
        blocks = order.into_iter().map(|i| old_blocks[i].clone()).collect();

        // 重新编号
        for (idx, blk) in blocks.iter_mut().enumerate() {
            blk.block_id = format!("blk_{}", idx);
        }
    }

    (blocks, tables)
}

/// 检测是否存在多栏布局
///
/// 通过分析 block 的 x 分布推断是否多栏：
/// - 如果有显著的 x 方向间隙将 blocks 分成了多组，则认为是多栏
fn has_multi_column_layout(blocks: &[BlockIR], page_width: f32) -> bool {
    if blocks.len() < 4 {
        return false;
    }

    // 过滤掉横幅块（宽度 > 70% 页宽，通常是标题/页眉）
    let narrow_blocks: Vec<&BlockIR> = blocks
        .iter()
        .filter(|b| b.bbox.width < page_width * 0.7)
        .collect();

    if narrow_blocks.len() < 4 {
        return false;
    }

    // 统计窄块的 x 中心分布
    let mut x_centers: Vec<f32> = narrow_blocks.iter().map(|b| b.bbox.center_x()).collect();
    x_centers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // 检查 x 中心是否有明显的双峰分布（双栏特征）
    let mid_x = page_width / 2.0;
    let gap_threshold = page_width * 0.1; // 10% 页宽的间隙
    let left_count = x_centers
        .iter()
        .filter(|&&x| x < mid_x - gap_threshold)
        .count();
    let right_count = x_centers
        .iter()
        .filter(|&&x| x > mid_x + gap_threshold)
        .count();

    // 左右两侧都有足够的块 → 多栏
    left_count >= 2 && right_count >= 2
}

/// 从原始字符构建文本块 + 隐式网格检测
///
/// 在行聚类后检测隐式网格布局（多行列对齐），按列优先阅读顺序输出。
pub fn build_blocks_and_grids(
    chars: &[RawChar],
    page_width: f32,
    _page_height: f32,
    page_index: usize,
) -> (Vec<BlockIR>, Vec<TableIR>) {
    if chars.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // 1. 行聚类（仅 y 分组，不拆分间隙）
    let raw_lines = cluster_lines_raw(chars);

    // 2. 隐式网格检测：在原始行上分析内部间隙模式
    //    返回列优先的 BlockIR（每列一个块）和剩余行
    let (grid_blocks, remaining_raw_lines) =
        detect_implicit_grids(&raw_lines, page_index, page_width);

    // 3. 对剩余行进行间隙拆分 + 短行过滤
    let lines = gap_split_and_filter(remaining_raw_lines, page_width);

    // 4. 分离横幅行（宽度跨越多列的行，如标题、页眉）
    let (banner_lines, narrow_lines) = separate_banners(&lines, page_width);

    // 5. 列检测（仅用窄行）
    let columns = detect_columns(&narrow_lines, page_width);

    // 6. 按列分组，每列内按 y 排序
    let column_lines = split_lines_by_columns(&narrow_lines, &columns);

    // 7. 构建块列表：按 y 位置交错横幅、网格块和列内容
    let mut blocks = Vec::new();
    let mut block_idx = 0;

    // 收集所有段落块及其 y 起始位置
    let mut positioned_blocks: Vec<(f32, BlockIR)> = Vec::new();

    // 网格块（列优先阅读顺序）
    for blk in grid_blocks {
        positioned_blocks.push((blk.bbox.y, blk));
    }

    // 横幅行先做段落合并（避免单栏正文每行一个独立 block）
    let banner_paragraphs = merge_paragraphs(&banner_lines);
    for para in banner_paragraphs {
        let bbox = compute_block_bbox(&para);
        let text_lines: Vec<TextLine> = para
            .iter()
            .map(|line| TextLine {
                spans: line_to_spans(line),
                bbox: Some(compute_line_bbox(line)),
            })
            .collect();

        let normalized_text = text_lines
            .iter()
            .map(|l| l.text())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        if normalized_text.is_empty() {
            continue;
        }

        let role = detect_block_role(&text_lines, &bbox, page_width);
        positioned_blocks.push((
            para[0].y_center,
            BlockIR {
                block_id: String::new(),
                bbox,
                role,
                lines: text_lines,
                normalized_text,
            },
        ));
    }

    // 列内段落
    for col_lines in &column_lines {
        let paragraphs = merge_paragraphs(col_lines);
        for para in paragraphs {
            let bbox = compute_block_bbox(&para);
            let text_lines: Vec<TextLine> = para
                .iter()
                .map(|line| TextLine {
                    spans: line_to_spans(line),
                    bbox: Some(compute_line_bbox(line)),
                })
                .collect();

            let normalized_text = text_lines
                .iter()
                .map(|l| l.text())
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();

            if normalized_text.is_empty() {
                continue;
            }

            let role = detect_block_role(&text_lines, &bbox, page_width);
            positioned_blocks.push((
                bbox.y,
                BlockIR {
                    block_id: String::new(),
                    bbox,
                    role,
                    lines: text_lines,
                    normalized_text,
                },
            ));
        }
    }

    // 按 y 位置排序（保持阅读顺序，更精确的排序由 build_blocks_with_config 层面的 XY-Cut 负责）
    positioned_blocks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // 重新编号
    for (_, mut blk) in positioned_blocks {
        blk.block_id = format!("blk_{}", block_idx);
        blocks.push(blk);
        block_idx += 1;
    }

    // 方案B不生成 TableIR，网格直接转为 BlockIR
    (blocks, Vec::new())
}

/// 分离横幅行（宽度跨越多列）和窄行
fn separate_banners(lines: &[CharLine], page_width: f32) -> (Vec<CharLine>, Vec<CharLine>) {
    let banner_threshold = page_width * BANNER_WIDTH_RATIO;
    let mut banners = Vec::new();
    let mut narrow = Vec::new();

    for line in lines {
        if line.bbox.width > banner_threshold {
            banners.push(line.clone());
        } else {
            narrow.push(line.clone());
        }
    }

    (banners, narrow)
}

/// 检测文本块角色（Title / List / Body）
fn detect_block_role(text_lines: &[TextLine], bbox: &BBox, page_width: f32) -> BlockRole {
    if text_lines.is_empty() {
        return BlockRole::Unknown;
    }

    let first_line_text = text_lines[0].text();
    let trimmed = first_line_text.trim();

    // 列表项检测
    if is_list_item(trimmed) {
        return BlockRole::List;
    }

    // 标题检测：单行 + 较大字体 + 宽度较窄（居中或左对齐）
    if text_lines.len() <= 2 {
        let avg_font = text_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter_map(|s| s.font_size)
            .sum::<f32>()
            / text_lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .filter_map(|s| s.font_size)
                .count()
                .max(1) as f32;

        let is_bold = text_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.is_bold);

        // 字体大于 14pt 或加粗，且行数少，视为标题
        if (avg_font > 14.0 || is_bold) && bbox.width < page_width * 0.8 {
            return BlockRole::Title;
        }
    }

    BlockRole::Body
}

/// 判断文本是否为列表项
fn is_list_item(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let first_char = trimmed.chars().next().unwrap();

    // bullet 字符开头
    if BULLET_CHARS.contains(&first_char) {
        return true;
    }

    // 数字编号开头：1. 2. 3. / 1) 2) 3) / (1) (2) (3)
    if first_char.is_ascii_digit() {
        let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
        if rest.starts_with(". ") || rest.starts_with(") ") || rest.starts_with("、") {
            return true;
        }
    }

    // (1) (a) 模式
    if trimmed.starts_with('(') {
        if let Some(close_pos) = trimmed.find(')') {
            let inner = &trimmed[1..close_pos];
            if inner.len() <= 3
                && (inner.chars().all(|c| c.is_ascii_digit())
                    || inner.chars().all(|c| c.is_ascii_alphabetic()))
            {
                return true;
            }
        }
    }

    false
}

/// 判断是否为序号后缀的首字符（st/nd/rd/th）
///
/// 例如 1st, 2nd, 3rd, 4th
fn is_ordinal_suffix(c: char, next: Option<char>) -> bool {
    match (c, next) {
        ('s', Some('t')) => true, // 1st, 21st, 31st
        ('n', Some('d')) => true, // 2nd, 22nd
        ('r', Some('d')) => true, // 3rd, 23rd
        ('t', Some('h')) => true, // 4th, 5th, ...
        _ => false,
    }
}

/// 聚类后的行
#[derive(Debug, Clone)]
pub struct CharLine {
    pub chars: Vec<RawChar>,
    pub y_center: f32,
    pub bbox: BBox,
}

/// 列边界
#[derive(Debug, Clone)]
struct ColumnBound {
    x_min: f32,
    x_max: f32,
}

/// 行聚类：按 y 坐标将字符分组到行（不做间隙拆分）
fn cluster_lines_raw(chars: &[RawChar]) -> Vec<CharLine> {
    let mut sorted_chars: Vec<&RawChar> = chars.iter().collect();
    sorted_chars.sort_by(|a, b| {
        a.bbox
            .y
            .partial_cmp(&b.bbox.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<CharLine> = Vec::new();

    for ch in sorted_chars {
        let ch_y_center = ch.bbox.y + ch.bbox.height / 2.0;

        // 查找是否有已有行可以归入
        let mut found = false;
        for line in lines.iter_mut() {
            if (line.y_center - ch_y_center).abs() < LINE_Y_TOLERANCE {
                line.chars.push(ch.clone());
                // 更新 y_center 为加权平均
                let n = line.chars.len() as f32;
                line.y_center = line.y_center * (n - 1.0) / n + ch_y_center / n;
                found = true;
                break;
            }
        }

        if !found {
            lines.push(CharLine {
                chars: vec![ch.clone()],
                y_center: ch_y_center,
                bbox: ch.bbox,
            });
        }
    }

    // 每行内按 x 排序
    for line in lines.iter_mut() {
        line.chars.sort_by(|a, b| {
            a.bbox
                .x
                .partial_cmp(&b.bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        line.bbox = compute_chars_bbox(&line.chars);
    }

    // 按 y 排序
    lines.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    lines
}

/// 对行执行间隙拆分 + 短行过滤
fn gap_split_and_filter(lines: Vec<CharLine>, page_width: f32) -> Vec<CharLine> {
    let gap_threshold = page_width * COLUMN_GAP_RATIO;
    let mut split_lines: Vec<CharLine> = Vec::new();
    for line in lines {
        let sub_lines = split_line_by_gap(&line, gap_threshold);
        split_lines.extend(sub_lines);
    }

    // 过滤掉字符数过少的行片段
    split_lines.retain(|line| line.chars.len() >= MIN_LINE_CHAR_COUNT);

    // 行按 y 排序
    split_lines.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    split_lines
}

/// 行聚类 + 间隙拆分（兼容旧接口）
fn cluster_lines(chars: &[RawChar], page_width: f32) -> Vec<CharLine> {
    let raw_lines = cluster_lines_raw(chars);
    gap_split_and_filter(raw_lines, page_width)
}

/// 按行内大间隙拆分行（用于分离同一 y 坐标的多列文本）
fn split_line_by_gap(line: &CharLine, gap_threshold: f32) -> Vec<CharLine> {
    if line.chars.len() < 2 {
        return vec![line.clone()];
    }

    let mut result = Vec::new();
    let mut current_chars: Vec<RawChar> = vec![line.chars[0].clone()];

    for i in 1..line.chars.len() {
        let prev = &line.chars[i - 1];
        let curr = &line.chars[i];
        let gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);

        if gap > gap_threshold {
            // 大间隙：拆分
            let bbox = compute_chars_bbox(&current_chars);
            let y_center = bbox.y + bbox.height / 2.0;
            result.push(CharLine {
                chars: current_chars,
                y_center,
                bbox,
            });
            current_chars = vec![curr.clone()];
        } else {
            current_chars.push(curr.clone());
        }
    }

    // 收尾
    if !current_chars.is_empty() {
        let bbox = compute_chars_bbox(&current_chars);
        let y_center = bbox.y + bbox.height / 2.0;
        result.push(CharLine {
            chars: current_chars,
            y_center,
            bbox,
        });
    }

    result
}

/// 列检测：分析行的 x 分布，检测列分隔
///
/// 改进算法：
/// - 使用间隙聚类分析，找到稳定的列间隙
/// - 支持 2 列、3 列常见布局
/// - 混合布局处理（已通过横幅分离预处理）
fn detect_columns(lines: &[CharLine], page_width: f32) -> Vec<ColumnBound> {
    if lines.is_empty() {
        return vec![ColumnBound {
            x_min: 0.0,
            x_max: page_width,
        }];
    }

    let gap_threshold = page_width * COLUMN_GAP_RATIO;
    let min_col_width = page_width * MIN_COLUMN_WIDTH_RATIO;

    // 收集所有行的 x 范围，按左边界排序
    let mut x_ranges: Vec<(f32, f32)> = lines.iter().map(|l| (l.bbox.x, l.bbox.right())).collect();
    x_ranges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // 合并重叠的 x 区间，找出连续覆盖区域
    let mut merged_ranges: Vec<(f32, f32)> = Vec::new();
    let mut cur_min = x_ranges[0].0;
    let mut cur_max = x_ranges[0].1;

    for &(x_min, x_max) in &x_ranges[1..] {
        if x_min - cur_max > gap_threshold {
            merged_ranges.push((cur_min, cur_max));
            cur_min = x_min;
            cur_max = x_max;
        } else {
            cur_max = cur_max.max(x_max);
        }
    }
    merged_ranges.push((cur_min, cur_max));

    // 过滤掉太窄的区域
    let columns: Vec<ColumnBound> = merged_ranges
        .iter()
        .filter(|(min, max)| max - min >= min_col_width)
        .map(|&(x_min, x_max)| ColumnBound { x_min, x_max })
        .collect();

    // 如果没有有效列或只有一列，返回单列
    if columns.len() <= 1 {
        return vec![ColumnBound {
            x_min: 0.0,
            x_max: page_width,
        }];
    }

    // 验证列间隙的一致性：各列宽度不应差异过大
    let widths: Vec<f32> = columns.iter().map(|c| c.x_max - c.x_min).collect();
    let max_width = widths.iter().cloned().fold(f32::MIN, f32::max);
    let min_width = widths.iter().cloned().fold(f32::MAX, f32::min);

    // 如果最宽列是最窄列的 3 倍以上，可能不是真正的多列
    if max_width > min_width * 3.0 {
        return vec![ColumnBound {
            x_min: 0.0,
            x_max: page_width,
        }];
    }

    columns
}

/// 按列分割行
fn split_lines_by_columns(lines: &[CharLine], columns: &[ColumnBound]) -> Vec<Vec<CharLine>> {
    let mut result: Vec<Vec<CharLine>> = columns.iter().map(|_| Vec::new()).collect();

    for line in lines {
        let line_center_x = line.bbox.center_x();
        let mut best_col = 0;
        let mut best_dist = f32::MAX;

        for (i, col) in columns.iter().enumerate() {
            let col_center = (col.x_min + col.x_max) / 2.0;
            let dist = (line_center_x - col_center).abs();
            if dist < best_dist {
                best_dist = dist;
                best_col = i;
            }
        }

        result[best_col].push(line.clone());
    }

    // 每列内按 y 排序
    for col_lines in result.iter_mut() {
        col_lines.sort_by(|a, b| {
            a.y_center
                .partial_cmp(&b.y_center)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    result
}

/// 段落合并增强：根据行距 + 缩进 + 列表项判断段落边界
///
/// 改进点：
/// - 行距自适应（使用局部行距统计）
/// - 缩进检测（区分段首/续行）
/// - 列表项检测（bullet / number prefix 自动断段）
fn merge_paragraphs(lines: &[CharLine]) -> Vec<Vec<&CharLine>> {
    if lines.is_empty() {
        return Vec::new();
    }

    // 先计算局部中位行距，用于自适应阈值
    let median_line_gap = compute_median_line_gap(lines);

    let mut paragraphs: Vec<Vec<&CharLine>> = Vec::new();
    let mut current_para: Vec<&CharLine> = vec![&lines[0]];

    for i in 1..lines.len() {
        let prev = &lines[i - 1];
        let curr = &lines[i];

        let should_break = should_break_paragraph(prev, curr, median_line_gap);

        if should_break {
            paragraphs.push(current_para);
            current_para = vec![&lines[i]];
        } else {
            current_para.push(&lines[i]);
        }
    }

    if !current_para.is_empty() {
        paragraphs.push(current_para);
    }

    paragraphs
}

/// 判断是否应该在两行之间断段
fn should_break_paragraph(prev: &CharLine, curr: &CharLine, median_gap: f32) -> bool {
    let line_gap = curr.y_center - prev.y_center;
    let avg_font_size = avg_char_font_size(&prev.chars)
        .max(avg_char_font_size(&curr.chars))
        .max(1.0);

    // 1. 行距过大 → 断段
    if line_gap > avg_font_size * PARAGRAPH_LINE_SPACING_RATIO {
        return true;
    }

    // 2. 行距显著大于中位行距（1.5 倍）→ 断段
    if median_gap > 0.0 && line_gap > median_gap * 1.5 {
        return true;
    }

    // 3. 当前行是列表项开头 → 断段
    let curr_text = line_to_text(curr);
    if is_list_item(curr_text.trim()) {
        return true;
    }

    // 4. 缩进检测：当前行明显缩进（段首缩进）
    let indent_diff = curr.bbox.x - prev.bbox.x;
    if indent_diff > avg_font_size * 1.5 {
        // 明显缩进，可能是新段落
        return true;
    }

    false
}

/// 计算行间距的中位数
fn compute_median_line_gap(lines: &[CharLine]) -> f32 {
    if lines.len() < 2 {
        return 0.0;
    }

    let mut gaps: Vec<f32> = Vec::new();
    for i in 1..lines.len() {
        let gap = lines[i].y_center - lines[i - 1].y_center;
        if gap > 0.0 {
            gaps.push(gap);
        }
    }

    if gaps.is_empty() {
        return 0.0;
    }

    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    gaps[gaps.len() / 2]
}

/// 将 CharLine 转为文本（快速版本，用于检测）
fn line_to_text(line: &CharLine) -> String {
    line.chars.iter().map(|c| c.unicode).collect()
}

/// 将一行字符转换为 TextSpan 列表（按词间距分割）
fn line_to_spans(line: &CharLine) -> Vec<TextSpan> {
    if line.chars.is_empty() {
        return Vec::new();
    }

    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_font_size: Option<f32> = None;
    let mut current_is_bold = false;

    for (i, ch) in line.chars.iter().enumerate() {
        // 检测词间距
        if i > 0 {
            let prev = &line.chars[i - 1];
            let gap = ch.bbox.x - prev.bbox.right();
            let font_size = prev.font_size.max(1.0);

            // 紧凑间距规则：
            // 1. `@` 两侧总是紧凑（邮箱）
            // 2. `.` 仅当两侧都是字母/数字时紧凑（域名如 google.com）
            //    句号后跟大写字母时应保持正常间距
            // 3. 数字-数字相邻总是紧凑（数字内部不应有空格）
            // 4. 数字后跟序号后缀（st/nd/rd/th）时紧凑
            // 5. 大写+小写(Aa)、大写+大写(AA) 使用紧凑阈值（PDF kerning 补偿）
            //    小写+小写(aa)、小写+大写(aA) 不紧凑（通常是词边界）
            // 6. 连字符/括号两侧紧凑
            let tight = if prev.unicode == '@' || ch.unicode == '@' {
                true
            } else if prev.unicode == '.' && ch.unicode.is_alphanumeric() {
                true
            } else if ch.unicode == '.' && prev.unicode.is_alphanumeric() {
                let next_is_lower_or_digit = line.chars.get(i + 1).map_or(false, |nc| {
                    nc.unicode.is_lowercase() || nc.unicode.is_ascii_digit()
                });
                next_is_lower_or_digit
            } else if prev.unicode.is_ascii_digit() && ch.unicode.is_ascii_digit() {
                true
            } else if prev.unicode.is_ascii_digit() && ch.unicode.is_lowercase() {
                is_ordinal_suffix(ch.unicode, line.chars.get(i + 1).map(|c| c.unicode))
            } else if prev.unicode.is_uppercase() && ch.unicode.is_lowercase() {
                // 大写+小写 (e.g. "In" in "Indicate"): 同词内 kerning 最常被误判
                true
            } else if prev.unicode.is_uppercase() && ch.unicode.is_uppercase() {
                // 大写+大写 (e.g. "AN" in "AND"): 全大写文本有 tracking 间距
                true
            } else if prev.unicode == '-' || ch.unicode == '-' {
                true
            } else if (ch.unicode == '(' || ch.unicode == ')') && prev.unicode.is_alphanumeric() {
                true
            } else if (prev.unicode == '(' || prev.unicode == ')') && ch.unicode.is_alphanumeric() {
                true
            } else {
                // 小写+小写(aa)、小写+大写(aA): 通常是真词边界，不使用紧凑阈值
                false
            };

            let ratio = if tight {
                WORD_SPACING_RATIO * 2.0
            } else {
                WORD_SPACING_RATIO
            };

            if gap > font_size * ratio {
                current_text.push(' ');
            }
        }

        current_text.push(ch.unicode);
        if current_font_size.is_none() {
            current_font_size = Some(ch.font_size);
            current_is_bold = ch.is_bold;
        }
    }

    if !current_text.is_empty() {
        spans.push(TextSpan {
            text: current_text,
            font_size: current_font_size,
            is_bold: current_is_bold,
            font_name: None,
        });
    }

    spans
}

/// 计算字符列表的 BBox
fn compute_chars_bbox(chars: &[RawChar]) -> BBox {
    if chars.is_empty() {
        return BBox::new(0.0, 0.0, 0.0, 0.0);
    }

    let mut x_min = f32::MAX;
    let mut y_min = f32::MAX;
    let mut x_max = f32::MIN;
    let mut y_max = f32::MIN;

    for ch in chars {
        x_min = x_min.min(ch.bbox.x);
        y_min = y_min.min(ch.bbox.y);
        x_max = x_max.max(ch.bbox.right());
        y_max = y_max.max(ch.bbox.bottom());
    }

    BBox::new(x_min, y_min, x_max - x_min, y_max - y_min)
}

/// 计算一行的 BBox
fn compute_line_bbox(line: &CharLine) -> BBox {
    compute_chars_bbox(&line.chars)
}

/// 计算段落（多行）的 BBox
fn compute_block_bbox(lines: &[&CharLine]) -> BBox {
    if lines.is_empty() {
        return BBox::new(0.0, 0.0, 0.0, 0.0);
    }

    let mut x_min = f32::MAX;
    let mut y_min = f32::MAX;
    let mut x_max = f32::MIN;
    let mut y_max = f32::MIN;

    for line in lines {
        x_min = x_min.min(line.bbox.x);
        y_min = y_min.min(line.bbox.y);
        x_max = x_max.max(line.bbox.right());
        y_max = y_max.max(line.bbox.bottom());
    }

    BBox::new(x_min, y_min, x_max - x_min, y_max - y_min)
}

/// 计算字符列表的平均字体大小
fn avg_char_font_size(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 12.0;
    }
    let sum: f32 = chars.iter().map(|c| c.font_size).sum();
    sum / chars.len() as f32
}

/// 行的间隙分析结果
#[derive(Clone)]
struct LineGapProfile {
    /// 原始行索引
    line_idx: usize,
    /// 间隙中心的 x 坐标列表
    gap_centers: Vec<f32>,
    /// 按间隙拆分后的字符段
    segments: Vec<Vec<RawChar>>,
    /// 原始行的 y 中心
    y_center: f32,
}

/// 检测隐式网格布局（基于行内间隙模式）
///
/// 对每行分析内部显著间隙位置。当连续行有相同数量的间隙且位置对齐时，
/// 识别为网格区域，按列优先顺序生成 BlockIR。
/// 附加：稀疏列检测 —— 当只有部分行在固定 x 位置有间隙时，也能识别为列布局。
fn detect_implicit_grids(
    lines: &[CharLine],
    _page_index: usize,
    page_width: f32,
) -> (Vec<BlockIR>, Vec<CharLine>) {
    if lines.len() < GRID_MIN_ROWS {
        return (Vec::new(), lines.to_vec());
    }

    // 间隙阈值：使用较小值以捕获更紧凑的网格
    let min_gap = page_width * 0.03;

    // Step 1: 分析每行的间隙模式
    let profiles: Vec<LineGapProfile> = lines
        .iter()
        .enumerate()
        .map(|(idx, line)| analyze_line_gaps(line, idx, min_gap))
        // 过滤：排除空段 profile 和包含单字符段的行（侧边栏噪声）
        .filter(|p| {
            !p.segments.is_empty()
                && p.segments
                    .iter()
                    .all(|seg| seg.len() >= MIN_LINE_CHAR_COUNT)
        })
        .collect();

    // 调试：输出 profiles 统计
    log::debug!(
        "detect_implicit_grids: {} lines → {} valid profiles",
        lines.len(),
        profiles.len()
    );
    for p in &profiles {
        let seg_lens: Vec<usize> = p.segments.iter().map(|s| s.len()).collect();
        let seg_texts: Vec<String> = p
            .segments
            .iter()
            .map(|s| s.iter().map(|c| c.unicode).collect::<String>())
            .collect();
        log::debug!(
            "  profile[{}]: y={:.1}, gaps={}, gap_centers={:?}, seg_lens={:?}, texts={:?}",
            p.line_idx,
            p.y_center,
            p.gap_centers.len(),
            p.gap_centers,
            seg_lens,
            seg_texts
                .iter()
                .map(|t| t.chars().take(15).collect::<String>())
                .collect::<Vec<_>>()
        );
    }

    // 对齐容差（基于页宽的百分比）
    let x_tolerance = page_width * GRID_X_ALIGN_RATIO;

    // Step 1.5: 先做 sparse 列检测（全量 profiles），识别局部双栏区域
    // 这样可以避免 Step 2 把双栏行错误地作为常规网格消费
    let mut consumed_indices: Vec<bool> = vec![false; lines.len()];
    let mut grid_blocks: Vec<BlockIR> = Vec::new();
    {
        let sparse_result =
            detect_sparse_column_grid(lines, &profiles, page_width, x_tolerance, min_gap);
        if let Some((sparse_blocks, sparse_remaining)) = sparse_result {
            // 将 sparse 生成的列优先 blocks 加入 grid_blocks
            grid_blocks.extend(sparse_blocks);
            // 将 sparse 消费的原始行标记为已消费
            for (idx, line) in lines.iter().enumerate() {
                let is_original_remaining = sparse_remaining.iter().any(|r| {
                    (r.y_center - line.y_center).abs() < 0.5
                        && (r.bbox.x - line.bbox.x).abs() < 1.0
                        && (r.bbox.width - line.bbox.width).abs() < 1.0
                });
                if !is_original_remaining {
                    consumed_indices[idx] = true;
                }
            }
            log::debug!(
                "Step 1.5 sparse: consumed {} lines, {} blocks generated",
                consumed_indices.iter().filter(|&&c| c).count(),
                grid_blocks.len()
            );
        }
    }

    // Step 2: 找到连续行中间隙模式匹配的区域（跳过 Step 1.5 已消费的行）

    let mut i = 0;
    while i < profiles.len() {
        // 跳过已被 Step 1.5 消费的行
        if consumed_indices[profiles[i].line_idx] {
            i += 1;
            continue;
        }

        let ref_gap_count = profiles[i].gap_centers.len();

        // 至少需要 GRID_MIN_COLS - 1 个间隙
        if ref_gap_count < GRID_MIN_COLS - 1 {
            i += 1;
            continue;
        }

        // 用平均间隙位置做参考（随新行加入不断更新）
        let mut avg_gap_centers: Vec<f32> = profiles[i].gap_centers.clone();
        let mut row_count: f32 = 1.0;

        // 向后扫描匹配行
        let mut end = i + 1;
        while end < profiles.len() {
            let profile = &profiles[end];

            if profile.gap_centers.len() != ref_gap_count {
                break;
            }

            // 与当前平均间隙位置比较
            let aligned = avg_gap_centers
                .iter()
                .zip(profile.gap_centers.iter())
                .all(|(avg, b)| (avg - b).abs() < x_tolerance);

            if !aligned {
                break;
            }

            // 更新平均间隙位置
            row_count += 1.0;
            for (j, gc) in profile.gap_centers.iter().enumerate() {
                avg_gap_centers[j] =
                    avg_gap_centers[j] * (row_count - 1.0) / row_count + gc / row_count;
            }

            end += 1;
        }

        let grid_rows = end - i;

        // 2 列（1 个间隙）需要更多行来确认——2 列对齐太容易偶然产生
        // （例如 SEC 10-K 中 "Delaware / 94-3177549" 这种左右对齐排版）
        let min_rows_for_grid = if ref_gap_count == 1 { 4 } else { GRID_MIN_ROWS };

        if grid_rows >= min_rows_for_grid {
            let mut all_grid_profiles: Vec<LineGapProfile> = profiles[i..end].to_vec();

            // 网格延伸：用已知间隙位置尝试拆分紧邻的原始行
            let grid_y_min = all_grid_profiles.first().map(|p| p.y_center).unwrap_or(0.0);
            let grid_y_max = all_grid_profiles.last().map(|p| p.y_center).unwrap_or(0.0);
            let avg_line_height = if all_grid_profiles.len() >= 2 {
                (grid_y_max - grid_y_min) / (all_grid_profiles.len() - 1) as f32
            } else {
                15.0
            };

            // 向上延伸：检查网格之前紧邻的原始行
            for line in lines.iter() {
                let gap_to_grid = grid_y_min - line.y_center;
                if gap_to_grid <= 0.0 || gap_to_grid > avg_line_height * 2.0 {
                    continue;
                }
                // 检查原始行是否有自然间隙，如果没有（连续文本如标题），不强制拆分
                let orig_profile = analyze_line_gaps(line, 0, min_gap);
                if orig_profile.gap_centers.is_empty() {
                    log::debug!(
                        "Grid extend-up skip: y={:.1} has no natural gaps (title?)",
                        line.y_center
                    );
                    continue;
                }
                let forced_profile =
                    force_split_by_gaps(line, &avg_gap_centers, MIN_LINE_CHAR_COUNT);
                if let Some(fp) = forced_profile {
                    if fp.segments.len() == ref_gap_count + 1 {
                        for (li, orig_line) in lines.iter().enumerate() {
                            if (orig_line.y_center - line.y_center).abs() < 0.5 {
                                consumed_indices[li] = true;
                                break;
                            }
                        }
                        all_grid_profiles.push(fp);
                    }
                }
            }

            // 向下延伸：检查网格之后紧邻的原始行
            for line in lines.iter() {
                let gap_to_grid = line.y_center - grid_y_max;
                if gap_to_grid <= 0.0 || gap_to_grid > avg_line_height * 2.0 {
                    continue;
                }
                // 检查原始行是否有自然间隙，如果没有（连续文本如标题），不强制拆分
                let orig_profile = analyze_line_gaps(line, 0, min_gap);
                if orig_profile.gap_centers.is_empty() {
                    log::debug!(
                        "Grid extend-down skip: y={:.1} has no natural gaps (title?)",
                        line.y_center
                    );
                    continue;
                }
                let forced_profile =
                    force_split_by_gaps(line, &avg_gap_centers, MIN_LINE_CHAR_COUNT);
                if let Some(fp) = forced_profile {
                    if fp.segments.len() == ref_gap_count + 1 {
                        for (li, orig_line) in lines.iter().enumerate() {
                            if (orig_line.y_center - line.y_center).abs() < 0.5 {
                                consumed_indices[li] = true;
                                break;
                            }
                        }
                        all_grid_profiles.push(fp);
                    }
                }
            }

            let mut col_blocks = build_column_blocks_from_profiles(&all_grid_profiles);
            grid_blocks.append(&mut col_blocks);

            for idx in i..end {
                consumed_indices[profiles[idx].line_idx] = true;
            }
            i = end;
        } else {
            i += 1;
        }
    }

    // Step 2b: 稀疏列检测
    let mut sparse_split_lines: Vec<CharLine> = Vec::new();
    // 对未被 Step 2 消费的行，尝试通过间隙位置聚类找到稳定的列分界
    // 典型场景：左窄标签+右宽内容布局、学术论文 ARTICLE INFO | ABSTRACT 区域
    let any_consumed = consumed_indices.iter().any(|&c| c);
    // 筛选未被消费行的 profiles
    let unconsumed_profiles: Vec<&LineGapProfile> = profiles
        .iter()
        .filter(|p| !consumed_indices[p.line_idx])
        .collect();
    log::debug!(
        "Step 2b check: any_consumed={}, unconsumed_profiles={}",
        any_consumed,
        unconsumed_profiles.len()
    );
    if !unconsumed_profiles.is_empty() {
        // 构造未消费行的 profiles（owned）
        let uc_profiles: Vec<LineGapProfile> =
            unconsumed_profiles.iter().map(|p| (*p).clone()).collect();
        let sparse_result =
            detect_sparse_column_grid(lines, &uc_profiles, page_width, x_tolerance, min_gap);
        log::debug!("Step 2b sparse_result: {:?}", sparse_result.is_some());
        if let Some((mut sparse_blocks, sparse_remaining)) = sparse_result {
            grid_blocks.append(&mut sparse_blocks);
            // 标记 sparse 消费的原始行
            for (idx, line) in lines.iter().enumerate() {
                if consumed_indices[idx] {
                    continue;
                }
                // sparse 消费的行不会出现在 sparse_remaining 中（按 y_center 精确匹配原始行）
                // 但 sparse_remaining 中可能有新创建的拆分行（与原始行同 y 但不同 x 范围）
                let is_original_remaining = sparse_remaining.iter().any(|r| {
                    (r.y_center - line.y_center).abs() < 0.5
                        && (r.bbox.x - line.bbox.x).abs() < 1.0
                        && (r.bbox.width - line.bbox.width).abs() < 1.0
                });
                if !is_original_remaining {
                    consumed_indices[idx] = true;
                }
            }
            // 收集 sparse 新创建的拆分行（非原始行的行）
            for sr in sparse_remaining {
                let is_original = lines.iter().any(|l| {
                    (l.y_center - sr.y_center).abs() < 0.5
                        && (l.bbox.x - sr.bbox.x).abs() < 1.0
                        && (l.bbox.width - sr.bbox.width).abs() < 1.0
                });
                if !is_original {
                    // 这是 sparse 拆分产生的新行，加入额外列表
                    sparse_split_lines.push(sr);
                }
            }
        }
    }

    // Step 3: 返回未消耗的行 + sparse 拆分产生的新行
    let mut remaining: Vec<CharLine> = lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !consumed_indices[*idx])
        .map(|(_, line)| line.clone())
        .collect();
    remaining.extend(sparse_split_lines);
    remaining.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    (grid_blocks, remaining)
}

/// 稀疏列检测：当只有部分行在固定位置有间隙时，识别列布局
///
/// 典型场景：左窄标签+右宽内容的分组列表布局
/// - 有些行同时有标签（左列）和内容（右列）→ 有间隙
/// - 有些行只有内容（右列）→ 无间隙，但左边界对齐
///
/// 检测逻辑：
/// 1. 聚类所有间隙位置，找到稳定的列分界 x 坐标
/// 2. 验证大多数无间隙行的左边界都在分界位置右侧附近（即它们是纯右列行）
/// 3. 用分界位置拆分所有行，左列允许为空
fn detect_sparse_column_grid(
    lines: &[CharLine],
    profiles: &[LineGapProfile],
    _page_width: f32,
    x_tolerance: f32,
    _min_gap: f32,
) -> Option<(Vec<BlockIR>, Vec<CharLine>)> {
    // 收集所有有间隙的 profiles 的 gap_centers
    let all_gaps: Vec<f32> = profiles
        .iter()
        .flat_map(|p| p.gap_centers.iter().cloned())
        .collect();

    log::debug!("sparse_column: all_gaps={:?}", all_gaps);

    if all_gaps.is_empty() {
        log::debug!("sparse_column: no gaps found, skipping");
        return None;
    }

    // 对间隙位置聚类
    let gap_clusters = cluster_gap_positions(&all_gaps, x_tolerance);

    log::debug!("sparse_column: gap_clusters={:?}", gap_clusters);

    if gap_clusters.is_empty() {
        return None;
    }

    // 选择最佳列分界线
    // 策略：当多个簇的支持行数接近时，优先选更靠近页面中心的间隙
    // （真正的列分界通常在页面中部，而序号/缩进间距通常在页面左侧）
    let page_center_x = _page_width / 2.0;
    let max_count = gap_clusters.iter().map(|(_, c)| *c).max().unwrap();

    // 候选簇：支持行数 >= max_count/2 且 >= 2
    let candidates: Vec<&(f32, usize)> = gap_clusters
        .iter()
        .filter(|(_, count)| *count >= max_count.max(2) / 2 && *count >= 1)
        .collect();

    let best_cluster = if candidates.len() > 1 {
        // 多个候选时，选最靠近页面中心的（距离加权）
        candidates
            .iter()
            .max_by(|(x1, c1), (x2, c2)| {
                // 综合评分：count 权重 + 距离中心的接近度
                let score1 = *c1 as f32 * 2.0 - (*x1 - page_center_x).abs() / page_center_x;
                let score2 = *c2 as f32 * 2.0 - (*x2 - page_center_x).abs() / page_center_x;
                score1
                    .partial_cmp(&score2)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap()
    } else {
        gap_clusters.iter().max_by_key(|(_, count)| *count).unwrap()
    };

    let boundary_x = best_cluster.0; // 聚类中心
    let boundary_count = best_cluster.1; // 支持行数

    // 需要至少 1 行支持这个分界位置（放宽条件配合中心优先策略）
    if boundary_count < 1 {
        log::debug!(
            "sparse_column: boundary_count={} < 1, skipping",
            boundary_count
        );
        return None;
    }

    // 过滤极端位置的 boundary：如果分界线在页面极左或极右（<15% 或 >85%），
    // 很可能是缩进列表的 label-content 结构（如 TOC："Item 1.  Business"）
    // 而不是真正的双栏布局。真正的列分界通常在页面的 15%-85% 区间。
    let boundary_ratio = boundary_x / _page_width;
    if boundary_ratio < 0.15 || boundary_ratio > 0.85 {
        log::debug!(
            "sparse_column: boundary_x={:.1} ({:.0}% of page) too close to edge, likely indented list, skipping",
            boundary_x,
            boundary_ratio * 100.0
        );
        return None;
    }

    // 确定网格 y 范围：从第一个有间隙行到最后一个有间隙行
    // 只有这个范围内的行参与网格拆分
    let gap_profile_y_values: Vec<f32> = profiles
        .iter()
        .filter(|p| {
            p.gap_centers
                .iter()
                .any(|gc| (gc - boundary_x).abs() < x_tolerance)
        })
        .map(|p| p.y_center)
        .collect();

    if gap_profile_y_values.is_empty() {
        return None;
    }

    let grid_y_min = gap_profile_y_values
        .iter()
        .cloned()
        .fold(f32::MAX, f32::min);
    let grid_y_max = gap_profile_y_values
        .iter()
        .cloned()
        .fold(f32::MIN, f32::max);

    // 扩展 y 范围：向上下各扩展一些（允许网格边缘的纯右列行）
    let avg_line_gap = if lines.len() >= 2 {
        let mut gaps: Vec<f32> = Vec::new();
        for i in 1..lines.len() {
            let g = lines[i].y_center - lines[i - 1].y_center;
            if g > 0.0 {
                gaps.push(g);
            }
        }
        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if gaps.is_empty() {
            30.0
        } else {
            gaps[gaps.len() / 2]
        }
    } else {
        30.0
    };

    // 大间隙（区域分隔线）的阈值：中位行距的 2 倍
    let section_gap_threshold = avg_line_gap * 2.0;

    // 向上扩展到第一个间隙行之前的连续行（不超过大间隙，使用固定参考点）
    let mut y_start = grid_y_min;
    for line in lines.iter().rev() {
        if line.y_center >= grid_y_min {
            continue;
        }
        if grid_y_min - line.y_center > section_gap_threshold {
            break;
        }
        y_start = line.y_center;
    }

    // 向下扩展到最后一个间隙行之后的连续行（不超过大间隙）
    // 使用滚动窗口，但只纳入窄行（纯左列或纯右列），全宽行终止扩展
    let max_extend = (grid_y_max - grid_y_min) * 0.5;
    let mut y_end = grid_y_max;
    for line in lines.iter() {
        if line.y_center <= y_end {
            continue;
        }
        if line.y_center - y_end > section_gap_threshold {
            break;
        }
        if line.y_center - grid_y_max > max_extend {
            break;
        }
        // 只扩展到纯左列或纯右列行，全宽行（横跨两列）终止扩展
        let line_right = line.bbox.x + line.bbox.width;
        let is_narrow = line_right < boundary_x + x_tolerance * 2.0
            || line.bbox.x > boundary_x - x_tolerance * 2.0;
        if !is_narrow {
            break;
        }
        y_end = line.y_center;
    }

    // 收集 y 范围内的行
    let grid_lines: Vec<(usize, &CharLine)> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.y_center >= y_start - 5.0 && line.y_center <= y_end + 5.0)
        .collect();

    if grid_lines.len() < 3 {
        return None;
    }

    // 验证：y 范围内的无间隙行中，大部分从 boundary_x 右侧开始
    let no_gap_in_range: Vec<&CharLine> = grid_lines
        .iter()
        .filter(|(_, line)| {
            !profiles.iter().any(|p| {
                p.line_idx < lines.len()
                    && (lines[p.line_idx].y_center - line.y_center).abs() < 0.5
                    && p.gap_centers
                        .iter()
                        .any(|gc| (gc - boundary_x).abs() < x_tolerance)
            })
        })
        .map(|(_, line)| *line)
        .collect();

    if no_gap_in_range.is_empty() {
        return None;
    }

    // 无间隙行应该完全在 boundary 的一侧（纯左列或纯右列），
    // 而不是横跨两列。这样才说明 boundary 是真正的列分界。
    let aligned_count = no_gap_in_range
        .iter()
        .filter(|line| {
            let line_right = line.bbox.x + line.bbox.width;
            // 纯右列：左边界在 boundary 右侧
            let is_right_col = line.bbox.x > boundary_x - x_tolerance * 2.0;
            // 纯左列：右边界在 boundary 左侧
            let is_left_col = line_right < boundary_x + x_tolerance * 2.0;
            is_right_col || is_left_col
        })
        .count();

    let alignment_ratio = aligned_count as f32 / no_gap_in_range.len() as f32;

    if alignment_ratio < 0.4 {
        log::debug!(
            "sparse_column: alignment_ratio={:.1}% < 40%, skipping",
            alignment_ratio * 100.0
        );
        return None;
    }

    log::debug!(
        "Sparse column grid detected: boundary_x={:.1}, y_range=[{:.1}, {:.1}], {} lines, alignment={:.1}%",
        boundary_x,
        y_start,
        y_end,
        grid_lines.len(),
        alignment_ratio * 100.0
    );

    // 用 boundary_x 拆分网格范围内的行
    let gap_centers = vec![boundary_x];
    let mut left_col_profiles: Vec<LineGapProfile> = Vec::new();
    let mut left_col_lines: Vec<CharLine> = Vec::new(); // 纯左列行（无间隙、完全在 boundary 左侧）
    let mut right_col_lines: Vec<CharLine> = Vec::new();
    let mut consumed: Vec<bool> = vec![false; lines.len()];

    for &(li, line) in &grid_lines {
        // 尝试强制拆分
        let forced = force_split_by_gaps(line, &gap_centers, 1); // min_chars=1 允许短标签

        if let Some(fp) = forced {
            if fp.segments.len() == 2 {
                // 左列有内容
                consumed[li] = true;
                left_col_profiles.push(fp);
                continue;
            }
        }

        // 无间隙行：检查是否是纯右列行或纯左列行
        let line_right = line.bbox.x + line.bbox.width;
        if line.bbox.x > boundary_x - x_tolerance * 2.0 {
            // 纯右列行 → 将整行作为右列内容
            consumed[li] = true;
            right_col_lines.push(line.clone());
        } else if line_right < boundary_x + x_tolerance * 2.0 {
            // 纯左列行 → 将整行作为左列内容
            consumed[li] = true;
            left_col_lines.push(line.clone());
        }
        // 否则这行不属于网格，交给后续处理
    }

    // 生成拆分后的行
    // 不生成 BlockIR，而是将拆分后的行加入 remaining，让下游段落合并逻辑处理
    let mut split_lines: Vec<CharLine> = Vec::new();

    // 左列行：从拆分后的第一个 segment 提取
    for profile in &left_col_profiles {
        if profile.segments.is_empty() {
            continue;
        }
        let seg = &profile.segments[0];
        if seg.is_empty() {
            continue;
        }
        split_lines.push(CharLine {
            chars: seg.clone(),
            y_center: profile.y_center,
            bbox: compute_chars_bbox(seg),
        });

        // 右列部分
        if profile.segments.len() >= 2 {
            let rseg = &profile.segments[1];
            if !rseg.is_empty() {
                split_lines.push(CharLine {
                    chars: rseg.clone(),
                    y_center: profile.y_center,
                    bbox: compute_chars_bbox(rseg),
                });
            }
        }
    }

    // 纯左列行直接添加
    for line in &left_col_lines {
        split_lines.push(line.clone());
    }

    // 纯右列行直接添加
    for line in &right_col_lines {
        split_lines.push(line.clone());
    }

    if split_lines.is_empty() {
        return None;
    }

    // 生成列优先的 BlockIR（先左列后右列）
    // 将 split_lines 按 x 位置分为左右两列
    let mut left_lines: Vec<CharLine> = Vec::new();
    let mut right_lines: Vec<CharLine> = Vec::new();
    for line in &split_lines {
        if line.bbox.x < boundary_x - x_tolerance {
            left_lines.push(line.clone());
        } else {
            right_lines.push(line.clone());
        }
    }
    // 每列按 y 排序
    left_lines.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    right_lines.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut blocks: Vec<BlockIR> = Vec::new();
    // 左列 block
    if !left_lines.is_empty() {
        let mut text_lines = Vec::new();
        let left_refs: Vec<&CharLine> = left_lines.iter().collect();
        for line in &left_lines {
            let spans = line_to_spans(line);
            if !spans.is_empty() {
                text_lines.push(TextLine {
                    spans,
                    bbox: Some(line.bbox),
                });
            }
        }
        if !text_lines.is_empty() {
            let bbox = compute_block_bbox(&left_refs);
            let normalized_text = text_lines
                .iter()
                .map(|l| l.text())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            if !normalized_text.is_empty() {
                blocks.push(BlockIR {
                    block_id: String::new(),
                    bbox,
                    role: BlockRole::Body,
                    lines: text_lines,
                    normalized_text,
                });
            }
        }
    }
    // 右列 block
    if !right_lines.is_empty() {
        let mut text_lines = Vec::new();
        let right_refs: Vec<&CharLine> = right_lines.iter().collect();
        for line in &right_lines {
            let spans = line_to_spans(line);
            if !spans.is_empty() {
                text_lines.push(TextLine {
                    spans,
                    bbox: Some(line.bbox),
                });
            }
        }
        if !text_lines.is_empty() {
            let bbox = compute_block_bbox(&right_refs);
            let normalized_text = text_lines
                .iter()
                .map(|l| l.text())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            if !normalized_text.is_empty() {
                blocks.push(BlockIR {
                    block_id: String::new(),
                    bbox,
                    role: BlockRole::Body,
                    lines: text_lines,
                    normalized_text,
                });
            }
        }
    }

    // 返回列优先 blocks + 未消耗的原始行
    let remaining: Vec<CharLine> = lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !consumed[*idx])
        .map(|(_, line)| line.clone())
        .collect();

    Some((blocks, remaining))
}

/// 对间隙位置聚类
///
/// 将接近的间隙位置归为同一簇，返回 (簇中心, 簇大小) 列表
fn cluster_gap_positions(gaps: &[f32], tolerance: f32) -> Vec<(f32, usize)> {
    if gaps.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<f32> = gaps.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut clusters: Vec<(f32, usize)> = Vec::new(); // (sum, count)
    let mut cur_sum = sorted[0];
    let mut cur_count: usize = 1;
    let mut cur_center = sorted[0];

    for &g in &sorted[1..] {
        if (g - cur_center).abs() < tolerance {
            cur_sum += g;
            cur_count += 1;
            cur_center = cur_sum / cur_count as f32;
        } else {
            clusters.push((cur_center, cur_count));
            cur_sum = g;
            cur_count = 1;
            cur_center = g;
        }
    }
    clusters.push((cur_center, cur_count));

    clusters
}

/// 用已知的间隙位置强制拆分一行
///
/// 当一行的自动间隙检测失败（例如邮箱行间距过小），但我们已经知道
/// 列分界位置（来自已检测的网格），用这些位置强制拆分字符。
fn force_split_by_gaps(
    line: &CharLine,
    gap_centers: &[f32],
    min_chars: usize,
) -> Option<LineGapProfile> {
    if line.chars.is_empty() || gap_centers.is_empty() {
        return None;
    }

    let num_cols = gap_centers.len() + 1;
    let mut segments: Vec<Vec<RawChar>> = vec![Vec::new(); num_cols];

    // 每个字符按其 x 中心分配到对应列
    for ch in &line.chars {
        let ch_center = ch.bbox.center_x();
        let mut col = 0;
        for (j, &gc) in gap_centers.iter().enumerate() {
            if ch_center > gc {
                col = j + 1;
            } else {
                break;
            }
        }
        segments[col].push(ch.clone());
    }

    // 每段内按 x 排序
    for seg in segments.iter_mut() {
        seg.sort_by(|a, b| {
            a.bbox
                .x
                .partial_cmp(&b.bbox.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // 过滤段首/尾的离群噪声字符
    for seg in segments.iter_mut() {
        // 如果段首字符与后续字符间隔过大，视为噪声
        while seg.len() > min_chars {
            let gap = seg[1].bbox.x - (seg[0].bbox.x + seg[0].bbox.width);
            let avg_width = seg.iter().map(|c| c.bbox.width).sum::<f32>() / seg.len() as f32;
            if gap > avg_width * 3.0 {
                seg.remove(0);
            } else {
                break;
            }
        }
        // 段尾同理
        while seg.len() > min_chars {
            let last = seg.len() - 1;
            let gap = seg[last].bbox.x - (seg[last - 1].bbox.x + seg[last - 1].bbox.width);
            let avg_width = seg.iter().map(|c| c.bbox.width).sum::<f32>() / seg.len() as f32;
            if gap > avg_width * 3.0 {
                seg.pop();
            } else {
                break;
            }
        }
    }

    // 检查每段是否有足够字符
    if segments.iter().any(|seg| seg.len() < min_chars) {
        return None;
    }

    Some(LineGapProfile {
        line_idx: usize::MAX, // 延伸行不在 profiles 索引中
        gap_centers: gap_centers.to_vec(),
        segments,
        y_center: line.y_center,
    })
}

/// 分析一行的内部间隙模式
fn analyze_line_gaps(line: &CharLine, line_idx: usize, min_gap: f32) -> LineGapProfile {
    let mut gap_centers = Vec::new();
    let mut segments: Vec<Vec<RawChar>> = Vec::new();
    let mut current_segment: Vec<RawChar> = Vec::new();

    if line.chars.is_empty() {
        return LineGapProfile {
            line_idx,
            gap_centers,
            segments,
            y_center: line.y_center,
        };
    }

    current_segment.push(line.chars[0].clone());

    for i in 1..line.chars.len() {
        let prev = &line.chars[i - 1];
        let curr = &line.chars[i];
        let gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);

        if gap > min_gap {
            let gap_center = (prev.bbox.x + prev.bbox.width + curr.bbox.x) / 2.0;
            gap_centers.push(gap_center);
            segments.push(current_segment);
            current_segment = vec![curr.clone()];
        } else {
            current_segment.push(curr.clone());
        }
    }

    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    // 修剪首尾的单字符段（通常是侧边栏噪声，如 arXiv 编号的竖排字符）
    while !segments.is_empty()
        && segments
            .first()
            .map_or(false, |s| s.len() < MIN_LINE_CHAR_COUNT)
    {
        segments.remove(0);
        if !gap_centers.is_empty() {
            gap_centers.remove(0);
        }
    }
    while !segments.is_empty()
        && segments
            .last()
            .map_or(false, |s| s.len() < MIN_LINE_CHAR_COUNT)
    {
        segments.pop();
        gap_centers.pop();
    }

    LineGapProfile {
        line_idx,
        gap_centers,
        segments,
        y_center: line.y_center,
    }
}

/// 方案B：从间隙分析结果构建列优先的 BlockIR
///
/// 每列生成一个独立的 BlockIR，列内各行逐行拼接。
/// 例如：作者名 → 机构 → 邮箱，纵向保持在同一个 Block 中。
fn build_column_blocks_from_profiles(profiles: &[LineGapProfile]) -> Vec<BlockIR> {
    if profiles.is_empty() {
        return Vec::new();
    }

    let num_cols = profiles[0].segments.len();
    if num_cols < GRID_MIN_COLS {
        return Vec::new();
    }

    let mut blocks: Vec<BlockIR> = Vec::new();

    // 逐列处理
    for col_idx in 0..num_cols {
        let mut text_lines: Vec<TextLine> = Vec::new();
        let mut col_chars: Vec<&RawChar> = Vec::new();

        for profile in profiles {
            if col_idx >= profile.segments.len() {
                continue;
            }

            let seg_chars = &profile.segments[col_idx];
            let temp_line = CharLine {
                chars: seg_chars.clone(),
                y_center: profile.y_center,
                bbox: compute_chars_bbox(seg_chars),
            };

            let spans = line_to_spans(&temp_line);
            if !spans.is_empty() {
                text_lines.push(TextLine {
                    spans,
                    bbox: Some(compute_chars_bbox(seg_chars)),
                });
            }

            // 收集字符用于计算 bbox
            for ch in seg_chars {
                col_chars.push(ch);
            }
        }

        if text_lines.is_empty() || col_chars.is_empty() {
            continue;
        }

        // 计算列的 bbox
        let mut x_min = f32::MAX;
        let mut y_min = f32::MAX;
        let mut x_max = f32::MIN;
        let mut y_max = f32::MIN;
        for ch in &col_chars {
            x_min = x_min.min(ch.bbox.x);
            y_min = y_min.min(ch.bbox.y);
            x_max = x_max.max(ch.bbox.right());
            y_max = y_max.max(ch.bbox.bottom());
        }
        let bbox = BBox::new(x_min, y_min, x_max - x_min, y_max - y_min);

        let normalized_text = text_lines
            .iter()
            .map(|l| l.text())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        if !normalized_text.is_empty() {
            blocks.push(BlockIR {
                block_id: String::new(),
                bbox,
                role: BlockRole::Body,
                lines: text_lines,
                normalized_text,
            });
        }
    }

    blocks
}
