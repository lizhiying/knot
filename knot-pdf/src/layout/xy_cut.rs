//! XY-Cut 递归分割算法
//!
//! 通过在 X 和 Y 方向上交替寻找最大空白间隙，递归地将页面分割为阅读块。
//! 天然支持多栏/混合布局（标题跨栏 + 正文双栏）的正确阅读顺序。
//!
//! 参考：MinerU 依赖的 xy-cut 算法 (https://github.com/Sanster/xy-cut)

use crate::ir::BBox;

/// XY-Cut 递归分割的结果节点
#[derive(Debug, Clone)]
pub struct XyCutNode {
    /// 包围框
    pub bbox: BBox,
    /// 切割方向
    pub cut_direction: CutDirection,
    /// 子节点（内部节点）
    pub children: Vec<XyCutNode>,
    /// 叶子节点中的元素索引
    pub element_indices: Vec<usize>,
}

/// 切割方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutDirection {
    /// 水平切割（Y 方向分割，上下分组）
    Horizontal,
    /// 垂直切割（X 方向分割，左右分组）
    Vertical,
    /// 叶子节点（不再分割）
    Leaf,
}

/// XY-Cut 算法配置
#[derive(Debug, Clone)]
pub struct XyCutConfig {
    /// 间隙阈值比例 (相对于页面宽度/高度)
    /// 默认 0.02 (页面宽度的 2%)
    pub gap_ratio: f32,
    /// 最小间隙像素（绝对值下限）
    pub min_gap_px: f32,
    /// 最大递归深度
    pub max_depth: usize,
    /// 叶子节点最少元素数（少于此数不再分割）
    pub min_elements: usize,
}

impl Default for XyCutConfig {
    fn default() -> Self {
        Self {
            gap_ratio: 0.02,
            min_gap_px: 5.0,
            max_depth: 20,
            min_elements: 1,
        }
    }
}

/// 对一组带 BBox 的元素执行 XY-Cut，返回按阅读顺序排列的元素索引
///
/// # Arguments
/// * `bboxes` - 每个元素的边界框
/// * `page_width` - 页面宽度
/// * `page_height` - 页面高度
/// * `config` - XY-Cut 配置
///
/// # Returns
/// 按阅读顺序排列的元素索引
pub fn xy_cut_sort(
    bboxes: &[BBox],
    page_width: f32,
    page_height: f32,
    config: &XyCutConfig,
) -> Vec<usize> {
    if bboxes.is_empty() {
        return Vec::new();
    }

    if bboxes.len() == 1 {
        return vec![0];
    }

    let indices: Vec<usize> = (0..bboxes.len()).collect();
    let page_bbox = BBox::new(0.0, 0.0, page_width, page_height);

    let root = xy_cut_recursive(bboxes, &indices, &page_bbox, config, 0);
    let mut result = Vec::with_capacity(bboxes.len());
    flatten_node(&root, &mut result);
    result
}

/// 递归执行 XY-Cut 分割
fn xy_cut_recursive(
    bboxes: &[BBox],
    indices: &[usize],
    region: &BBox,
    config: &XyCutConfig,
    depth: usize,
) -> XyCutNode {
    // 终止条件：达到最大深度、元素太少、或无法再分割
    if depth >= config.max_depth || indices.len() <= config.min_elements {
        return make_leaf(bboxes, indices);
    }

    let region_width = region.width;
    let region_height = region.height;

    // 动态间隙阈值：取 gap_ratio * 对应维度 和 min_gap_px 中的较大值
    let x_gap_threshold = (config.gap_ratio * region_width).max(config.min_gap_px);
    let y_gap_threshold = (config.gap_ratio * region_height).max(config.min_gap_px);

    // 尝试 Y 方向（水平）切割
    let y_cuts = find_projection_gaps_y(bboxes, indices, region, y_gap_threshold);
    // 尝试 X 方向（垂直）切割
    let x_cuts = find_projection_gaps_x(bboxes, indices, region, x_gap_threshold);

    // 选择最佳切割方向
    let best_cut = choose_best_cut(&y_cuts, &x_cuts);

    match best_cut {
        Some((CutDirection::Horizontal, splits)) => {
            // 水平切割：按 Y 分组（上→下）
            let mut children = Vec::new();
            for (sub_indices, sub_region) in splits {
                if sub_indices.is_empty() {
                    continue;
                }
                let child = xy_cut_recursive(bboxes, &sub_indices, &sub_region, config, depth + 1);
                children.push(child);
            }

            if children.len() == 1 {
                return children.into_iter().next().unwrap();
            }

            let bbox = compute_indices_bbox(bboxes, indices);
            XyCutNode {
                bbox,
                cut_direction: CutDirection::Horizontal,
                children,
                element_indices: Vec::new(),
            }
        }
        Some((CutDirection::Vertical, splits)) => {
            // 垂直切割：按 X 分组（左→右）
            let mut children = Vec::new();
            for (sub_indices, sub_region) in splits {
                if sub_indices.is_empty() {
                    continue;
                }
                let child = xy_cut_recursive(bboxes, &sub_indices, &sub_region, config, depth + 1);
                children.push(child);
            }

            if children.len() == 1 {
                return children.into_iter().next().unwrap();
            }

            let bbox = compute_indices_bbox(bboxes, indices);
            XyCutNode {
                bbox,
                cut_direction: CutDirection::Vertical,
                children,
                element_indices: Vec::new(),
            }
        }
        _ => {
            // 无法切割：叶子节点
            make_leaf(bboxes, indices)
        }
    }
}

/// 在 Y 方向找投影间隙（用于水平切割）
///
/// 将所有 bbox 投影到 Y 轴，找出投影覆盖中的空白带。
/// 返回按每个间隙分割后的子组 (indices, sub_region)。
fn find_projection_gaps_y(
    bboxes: &[BBox],
    indices: &[usize],
    region: &BBox,
    gap_threshold: f32,
) -> Vec<GapInfo> {
    if indices.is_empty() {
        return Vec::new();
    }

    // 收集所有元素在 Y 轴上的 (top, bottom) 区间
    let mut intervals: Vec<(f32, f32)> = indices
        .iter()
        .map(|&i| (bboxes[i].y, bboxes[i].bottom()))
        .collect();
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    find_gaps_in_intervals(&intervals, region.y, region.bottom(), gap_threshold)
}

/// 在 X 方向找投影间隙（用于垂直切割）
fn find_projection_gaps_x(
    bboxes: &[BBox],
    indices: &[usize],
    region: &BBox,
    gap_threshold: f32,
) -> Vec<GapInfo> {
    if indices.is_empty() {
        return Vec::new();
    }

    let mut intervals: Vec<(f32, f32)> = indices
        .iter()
        .map(|&i| (bboxes[i].x, bboxes[i].right()))
        .collect();
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    find_gaps_in_intervals(&intervals, region.x, region.right(), gap_threshold)
}

/// 间隙信息
#[derive(Debug, Clone)]
struct GapInfo {
    /// 间隙起始位置
    pub start: f32,
    /// 间隙结束位置
    pub end: f32,
    /// 间隙宽度
    pub width: f32,
}

/// 在一系列有序区间中查找大于阈值的间隙
///
/// 只检测元素之间的间隙，不检测元素与区域边缘的间隙。
/// 区域边缘的间隙仅仅是页面边距，不应当作为分割依据。
fn find_gaps_in_intervals(
    intervals: &[(f32, f32)],
    _region_start: f32,
    _region_end: f32,
    gap_threshold: f32,
) -> Vec<GapInfo> {
    if intervals.len() < 2 {
        return Vec::new();
    }

    // 合并重叠区间
    let mut merged: Vec<(f32, f32)> = Vec::new();
    let mut cur_start = intervals[0].0;
    let mut cur_end = intervals[0].1;

    for &(start, end) in &intervals[1..] {
        if start <= cur_end {
            cur_end = cur_end.max(end);
        } else {
            merged.push((cur_start, cur_end));
            cur_start = start;
            cur_end = end;
        }
    }
    merged.push((cur_start, cur_end));

    // 只查找元素之间的间隙（不包含边缘间隙）
    let mut gaps = Vec::new();
    for i in 1..merged.len() {
        let gap_start = merged[i - 1].1;
        let gap_end = merged[i].0;
        let gap_width = gap_end - gap_start;
        if gap_width > gap_threshold {
            gaps.push(GapInfo {
                start: gap_start,
                end: gap_end,
                width: gap_width,
            });
        }
    }

    gaps
}

/// 选择最佳切割方向
///
/// 策略：
/// 1. 优先选择 Y 方向（水平切割），因为文档通常自上而下阅读
/// 2. 如果 Y 方向没有间隙，尝试 X 方向（垂直切割，用于多栏）
/// 3. 如果两个方向都有间隙，选择最大间隙相对更大的方向
fn choose_best_cut(
    y_gaps: &[GapInfo],
    x_gaps: &[GapInfo],
) -> Option<(CutDirection, Vec<(Vec<usize>, BBox)>)> {
    let has_y = !y_gaps.is_empty();
    let has_x = !x_gaps.is_empty();

    if !has_y && !has_x {
        return None;
    }

    // 计算各方向最大间隙
    let max_y_gap = y_gaps.iter().map(|g| g.width).fold(0.0f32, f32::max);
    let max_x_gap = x_gaps.iter().map(|g| g.width).fold(0.0f32, f32::max);

    // 优先 Y 切割（水平分割），除非 X 方向间隙显著更大（1.5倍以上）
    if has_y && (!has_x || max_y_gap >= max_x_gap * 0.67) {
        Some((CutDirection::Horizontal, Vec::new())) // splits 在调用端计算
    } else if has_x {
        Some((CutDirection::Vertical, Vec::new()))
    } else {
        None
    }
}

/// 根据间隙将元素分组到各个子区域
fn split_by_gaps(
    bboxes: &[BBox],
    indices: &[usize],
    gaps: &[GapInfo],
    region: &BBox,
    direction: CutDirection,
) -> Vec<(Vec<usize>, BBox)> {
    if gaps.is_empty() || indices.is_empty() {
        return vec![(indices.to_vec(), *region)];
    }

    // 用间隙的中点作为分割线
    let cut_points: Vec<f32> = gaps.iter().map(|g| (g.start + g.end) / 2.0).collect();

    // 构建分区边界
    let mut boundaries = Vec::new();
    let (region_start, region_end) = match direction {
        CutDirection::Horizontal => (region.y, region.bottom()),
        CutDirection::Vertical => (region.x, region.right()),
        CutDirection::Leaf => return vec![(indices.to_vec(), *region)],
    };

    boundaries.push(region_start);
    for &cp in &cut_points {
        boundaries.push(cp);
    }
    boundaries.push(region_end);

    // 将元素分配到各分区
    let mut partitions: Vec<Vec<usize>> = vec![Vec::new(); boundaries.len() - 1];

    for &idx in indices {
        let center = match direction {
            CutDirection::Horizontal => bboxes[idx].center_y(),
            CutDirection::Vertical => bboxes[idx].center_x(),
            CutDirection::Leaf => continue,
        };

        // 找到 center 所在的分区
        let mut assigned = false;
        for p in 0..partitions.len() {
            if center >= boundaries[p] && center < boundaries[p + 1] {
                partitions[p].push(idx);
                assigned = true;
                break;
            }
        }
        // 如果 center 恰好等于最后边界，归入最后一个分区
        if !assigned && !partitions.is_empty() {
            partitions.last_mut().unwrap().push(idx);
        }
    }

    // 构建子区域
    let mut result = Vec::new();
    for (p, partition) in partitions.into_iter().enumerate() {
        if partition.is_empty() {
            continue;
        }

        let sub_region = match direction {
            CutDirection::Horizontal => BBox::new(
                region.x,
                boundaries[p],
                region.width,
                boundaries[p + 1] - boundaries[p],
            ),
            CutDirection::Vertical => BBox::new(
                boundaries[p],
                region.y,
                boundaries[p + 1] - boundaries[p],
                region.height,
            ),
            CutDirection::Leaf => *region,
        };

        result.push((partition, sub_region));
    }

    result
}

/// 创建叶子节点
///
/// 叶子节点内部的元素按 top→bottom, left→right 排序
fn make_leaf(bboxes: &[BBox], indices: &[usize]) -> XyCutNode {
    let mut sorted_indices = indices.to_vec();
    sorted_indices.sort_by(|&a, &b| {
        let ay = bboxes[a].y;
        let by = bboxes[b].y;
        let y_cmp = ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal);
        if y_cmp != std::cmp::Ordering::Equal {
            return y_cmp;
        }
        bboxes[a]
            .x
            .partial_cmp(&bboxes[b].x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let bbox = compute_indices_bbox(bboxes, indices);
    XyCutNode {
        bbox,
        cut_direction: CutDirection::Leaf,
        children: Vec::new(),
        element_indices: sorted_indices,
    }
}

/// 将递归树展平为线性顺序
fn flatten_node(node: &XyCutNode, result: &mut Vec<usize>) {
    if node.cut_direction == CutDirection::Leaf {
        result.extend_from_slice(&node.element_indices);
    } else {
        for child in &node.children {
            flatten_node(child, result);
        }
    }
}

/// 计算一组索引对应 bbox 的合并边界框
fn compute_indices_bbox(bboxes: &[BBox], indices: &[usize]) -> BBox {
    if indices.is_empty() {
        return BBox::new(0.0, 0.0, 0.0, 0.0);
    }

    let mut x_min = f32::MAX;
    let mut y_min = f32::MAX;
    let mut x_max = f32::MIN;
    let mut y_max = f32::MIN;

    for &i in indices {
        x_min = x_min.min(bboxes[i].x);
        y_min = y_min.min(bboxes[i].y);
        x_max = x_max.max(bboxes[i].right());
        y_max = y_max.max(bboxes[i].bottom());
    }

    BBox::new(x_min, y_min, x_max - x_min, y_max - y_min)
}

// ============================================================
// 完整版 xy_cut_recursive（使用 split_by_gaps）
// ============================================================

/// 完整实现的 XY-Cut 排序入口
///
/// 与 `xy_cut_sort` 相同的功能，但内部使用改进的递归实现。
pub fn xy_cut_order(
    bboxes: &[BBox],
    page_width: f32,
    page_height: f32,
    config: &XyCutConfig,
) -> Vec<usize> {
    if bboxes.is_empty() {
        return Vec::new();
    }
    if bboxes.len() == 1 {
        return vec![0];
    }

    let indices: Vec<usize> = (0..bboxes.len()).collect();
    let page_bbox = BBox::new(0.0, 0.0, page_width, page_height);

    let root = xy_cut_full(bboxes, &indices, &page_bbox, config, 0);
    let mut result = Vec::with_capacity(bboxes.len());
    flatten_node(&root, &mut result);
    result
}

/// 完整递归实现
fn xy_cut_full(
    bboxes: &[BBox],
    indices: &[usize],
    region: &BBox,
    config: &XyCutConfig,
    depth: usize,
) -> XyCutNode {
    if depth >= config.max_depth || indices.len() <= config.min_elements {
        return make_leaf(bboxes, indices);
    }

    let x_gap_threshold = (config.gap_ratio * region.width).max(config.min_gap_px);
    let y_gap_threshold = (config.gap_ratio * region.height).max(config.min_gap_px);

    let y_gaps = find_projection_gaps_y(bboxes, indices, region, y_gap_threshold);
    let x_gaps = find_projection_gaps_x(bboxes, indices, region, x_gap_threshold);

    // 计算两个方向的分组
    let y_splits = if !y_gaps.is_empty() {
        let s = split_by_gaps(bboxes, indices, &y_gaps, region, CutDirection::Horizontal);
        if s.len() > 1 {
            Some(s)
        } else {
            None
        }
    } else {
        None
    };

    let x_splits = if !x_gaps.is_empty() {
        let s = split_by_gaps(bboxes, indices, &x_gaps, region, CutDirection::Vertical);
        if s.len() > 1 {
            Some(s)
        } else {
            None
        }
    } else {
        None
    };

    // 评估哪个切割方向更好
    let best_direction = select_cut_direction(
        bboxes,
        indices,
        region,
        &y_gaps,
        &x_gaps,
        y_splits.as_deref(),
        x_splits.as_deref(),
        x_gap_threshold,
    );

    match best_direction {
        Some(CutDirection::Horizontal) => {
            if let Some(splits) = y_splits {
                // 多栏区域合并：检查相邻的 Y 子组是否有相同的 X 间隙模式
                // 如果有，将它们合并为一个子组，避免多栏正文被按行拆分
                let merged_splits =
                    merge_multicolumn_groups(bboxes, &splits, region, x_gap_threshold);

                let mut children = Vec::new();
                for (sub_indices, sub_region) in &merged_splits {
                    if sub_indices.is_empty() {
                        continue;
                    }
                    children.push(xy_cut_full(
                        bboxes,
                        sub_indices,
                        sub_region,
                        config,
                        depth + 1,
                    ));
                }
                if children.len() > 1 {
                    return XyCutNode {
                        bbox: compute_indices_bbox(bboxes, indices),
                        cut_direction: CutDirection::Horizontal,
                        children,
                        element_indices: Vec::new(),
                    };
                } else if children.len() == 1 {
                    return children.into_iter().next().unwrap();
                }
            }
        }
        Some(CutDirection::Vertical) => {
            if let Some(splits) = x_splits {
                let mut children = Vec::new();
                for (sub_indices, sub_region) in splits {
                    if sub_indices.is_empty() {
                        continue;
                    }
                    children.push(xy_cut_full(
                        bboxes,
                        &sub_indices,
                        &sub_region,
                        config,
                        depth + 1,
                    ));
                }
                if children.len() > 1 {
                    return XyCutNode {
                        bbox: compute_indices_bbox(bboxes, indices),
                        cut_direction: CutDirection::Vertical,
                        children,
                        element_indices: Vec::new(),
                    };
                } else if children.len() == 1 {
                    return children.into_iter().next().unwrap();
                }
            }
        }
        _ => {}
    }

    // 无法分割，作为叶子
    make_leaf(bboxes, indices)
}

/// 合并多栏区域：对 Y 切割后的子组，检查相邻的组是否有一致的 X 间隙模式
///
/// 典型场景：
/// - Y 切割把 [Title] / [A, C] / [B, D] 分成 3 组
/// - [A, C] 和 [B, D] 都有相同位置的 X 间隙 → 合并为 [A, B, C, D]
/// - 结果变成 [Title] / [A, B, C, D]，后续递归对 [A, B, C, D] 做 X 切割
fn merge_multicolumn_groups(
    bboxes: &[BBox],
    splits: &[(Vec<usize>, BBox)],
    _region: &BBox,
    x_gap_threshold: f32,
) -> Vec<(Vec<usize>, BBox)> {
    if splits.len() <= 1 {
        return splits.to_vec();
    }

    // 计算每个子组的 X 间隙模式
    let gap_patterns: Vec<Vec<GapInfo>> = splits
        .iter()
        .map(|(sub_indices, sub_region)| {
            if sub_indices.len() < 2 {
                Vec::new()
            } else {
                find_projection_gaps_x(bboxes, sub_indices, sub_region, x_gap_threshold)
            }
        })
        .collect();

    // 合并相邻的有一致 X 间隙模式的子组
    let mut result: Vec<(Vec<usize>, BBox)> = Vec::new();
    let mut current_indices = splits[0].0.clone();
    let mut current_region = splits[0].1;

    for i in 1..splits.len() {
        let should_merge =
            has_similar_x_gaps(&gap_patterns[i - 1], &gap_patterns[i], x_gap_threshold)
                && !gap_patterns[i - 1].is_empty()
                && !gap_patterns[i].is_empty();

        // 也合并前一组有 X 间隙、当前组加入后仍然有 X 间隙的情况
        // （处理连续的多栏行）
        if should_merge
            || should_merge_multicolumn(bboxes, &current_indices, &splits[i].0, x_gap_threshold)
        {
            // 合并
            current_indices.extend_from_slice(&splits[i].0);
            // 扩展 region
            let sr = &splits[i].1;
            let x_min = current_region.x.min(sr.x);
            let y_min = current_region.y.min(sr.y);
            let x_max = current_region.right().max(sr.right());
            let y_max = current_region.bottom().max(sr.bottom());
            current_region = BBox::new(x_min, y_min, x_max - x_min, y_max - y_min);
        } else {
            result.push((current_indices, current_region));
            current_indices = splits[i].0.clone();
            current_region = splits[i].1;
        }
    }
    result.push((current_indices, current_region));

    result
}

/// 检查两组 X 间隙是否有相似的模式（间隙中心位置接近）
fn has_similar_x_gaps(gaps_a: &[GapInfo], gaps_b: &[GapInfo], tolerance: f32) -> bool {
    if gaps_a.len() != gaps_b.len() || gaps_a.is_empty() {
        return false;
    }

    for (a, b) in gaps_a.iter().zip(gaps_b.iter()) {
        let center_a = (a.start + a.end) / 2.0;
        let center_b = (b.start + b.end) / 2.0;
        if (center_a - center_b).abs() > tolerance * 2.0 {
            return false;
        }
    }

    true
}

/// 检查是否应该合并两组为多栏区域
///
/// 当前一组已有多个元素且有 X 间隙，新组也有多个元素且 X 范围分布一致时合并
fn should_merge_multicolumn(
    bboxes: &[BBox],
    current: &[usize],
    next: &[usize],
    x_gap_threshold: f32,
) -> bool {
    if current.len() < 2 || next.len() < 2 {
        return false;
    }

    // 检查当前组是否有 X 间隙
    let current_bbox = compute_indices_bbox(bboxes, current);
    let current_x_gaps = find_projection_gaps_x(bboxes, current, &current_bbox, x_gap_threshold);
    if current_x_gaps.is_empty() {
        return false;
    }

    // 检查下一组是否在相似位置有 X 间隙
    let next_bbox = compute_indices_bbox(bboxes, next);
    let next_x_gaps = find_projection_gaps_x(bboxes, next, &next_bbox, x_gap_threshold);

    has_similar_x_gaps(&current_x_gaps, &next_x_gaps, x_gap_threshold)
}

/// 评估切割方向：选择 Y 切割（水平）还是 X 切割（垂直）
///
/// 策略：
/// 1. 如果只有一个方向有有效分组，选那个方向
/// 2. 如果两个方向都有有效分组：
///    - 检查 Y 切割后的子组内是否仍存在 X 间隙 →
///      如果是，说明 Y 切割把多栏的同行元素归到一组了，应优先 X 切割
///    - 否则优先 Y 切割（文档通常自上而下阅读）
#[allow(clippy::too_many_arguments)]
fn select_cut_direction(
    bboxes: &[BBox],
    _indices: &[usize],
    region: &BBox,
    y_gaps: &[GapInfo],
    x_gaps: &[GapInfo],
    y_splits: Option<&[(Vec<usize>, BBox)]>,
    x_splits: Option<&[(Vec<usize>, BBox)]>,
    x_gap_threshold: f32,
) -> Option<CutDirection> {
    let has_y = y_splits.is_some();
    let has_x = x_splits.is_some();

    if !has_y && !has_x {
        return None;
    }
    if has_y && !has_x {
        return Some(CutDirection::Horizontal);
    }
    if !has_y && has_x {
        return Some(CutDirection::Vertical);
    }

    // 两个方向都有有效分组 — 需要选择最佳方向
    let max_y_gap = y_gaps.iter().map(|g| g.width).fold(0.0f32, f32::max);
    let max_x_gap = x_gaps.iter().map(|g| g.width).fold(0.0f32, f32::max);

    // 检查：Y 切割后每个子组内是否仍存在 X 间隙？
    // 如果是，说明是多栏布局，应优先 X 切割
    if let Some(y_parts) = y_splits {
        let y_groups_with_x_gap = y_parts
            .iter()
            .filter(|(sub_indices, sub_region)| {
                if sub_indices.len() < 2 {
                    return false;
                }
                // 检查此子组内是否有 X 间隙
                let sub_x_gaps =
                    find_projection_gaps_x(bboxes, sub_indices, sub_region, x_gap_threshold);
                !sub_x_gaps.is_empty()
            })
            .count();

        // 如果所有/多数 Y 分组内都有 X 间隙 → 优先 X 切割（多栏布局）
        if y_groups_with_x_gap > 0 && y_groups_with_x_gap >= y_parts.len() / 2 {
            return Some(CutDirection::Vertical);
        }
    }

    // 默认策略：Y 间隙相对尺寸 vs X 间隙相对尺寸
    let y_ratio = if region.height > 0.0 {
        max_y_gap / region.height
    } else {
        0.0
    };
    let x_ratio = if region.width > 0.0 {
        max_x_gap / region.width
    } else {
        0.0
    };

    // 优先 Y 切割，除非 X 间隙比例显著更大
    if y_ratio >= x_ratio * 0.5 {
        Some(CutDirection::Horizontal)
    } else {
        Some(CutDirection::Vertical)
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> XyCutConfig {
        XyCutConfig::default()
    }

    #[test]
    fn test_empty_input() {
        let result = xy_cut_order(&[], 612.0, 792.0, &default_config());
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_element() {
        let bboxes = vec![BBox::new(100.0, 100.0, 200.0, 50.0)];
        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_single_column_top_to_bottom() {
        // 单栏文档：三个段落从上到下
        let bboxes = vec![
            BBox::new(50.0, 50.0, 500.0, 40.0),  // 段落 1（顶部）
            BBox::new(50.0, 150.0, 500.0, 40.0), // 段落 2（中部）
            BBox::new(50.0, 250.0, 500.0, 40.0), // 段落 3（底部）
        ];
        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_two_columns() {
        // 双栏文档：
        //  [A]  [C]
        //  [B]  [D]
        // 期望阅读顺序：A → B → C → D（先左栏从上到下，再右栏从上到下）
        let bboxes = vec![
            BBox::new(50.0, 100.0, 240.0, 40.0),  // A: 左栏上
            BBox::new(50.0, 200.0, 240.0, 40.0),  // B: 左栏下
            BBox::new(330.0, 100.0, 240.0, 40.0), // C: 右栏上
            BBox::new(330.0, 200.0, 240.0, 40.0), // D: 右栏下
        ];
        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());
        assert_eq!(result, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_title_spanning_two_columns() {
        // 标题跨栏 + 正文双栏：
        //  [  Title  ]            -- 跨栏标题
        //  [A]    [C]             -- 双栏正文
        //  [B]    [D]
        // 期望顺序：Title → A → B → C → D
        let bboxes = vec![
            BBox::new(50.0, 30.0, 500.0, 30.0),   // Title: 跨栏
            BBox::new(50.0, 120.0, 230.0, 40.0),  // A: 左栏上
            BBox::new(50.0, 220.0, 230.0, 40.0),  // B: 左栏下
            BBox::new(330.0, 120.0, 230.0, 40.0), // C: 右栏上
            BBox::new(330.0, 220.0, 230.0, 40.0), // D: 右栏下
        ];
        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());
        // Title 应该在最前面
        assert_eq!(result[0], 0, "Title should be first");
        // 左栏应在右栏之前
        let pos_a = result.iter().position(|&x| x == 1).unwrap();
        let pos_b = result.iter().position(|&x| x == 2).unwrap();
        let pos_c = result.iter().position(|&x| x == 3).unwrap();
        let pos_d = result.iter().position(|&x| x == 4).unwrap();
        assert!(pos_a < pos_b, "A should come before B");
        assert!(
            pos_b < pos_c,
            "B (end of left col) should come before C (start of right col)"
        );
        assert!(pos_c < pos_d, "C should come before D");
    }

    #[test]
    fn test_three_columns() {
        // 三栏文档
        let bboxes = vec![
            BBox::new(20.0, 100.0, 170.0, 40.0),  // 左栏
            BBox::new(220.0, 100.0, 170.0, 40.0), // 中栏
            BBox::new(420.0, 100.0, 170.0, 40.0), // 右栏
        ];
        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_figure_interrupting_text() {
        // 中间有图片中断文本流：
        //  [Text1]
        //  [  Figure  ]
        //  [Text2]
        let bboxes = vec![
            BBox::new(50.0, 50.0, 500.0, 40.0),   // Text1
            BBox::new(50.0, 150.0, 500.0, 200.0), // Figure
            BBox::new(50.0, 420.0, 500.0, 40.0),  // Text2
        ];
        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_find_gaps_basic() {
        // 测试间隙检测
        let intervals = vec![(10.0, 50.0), (100.0, 150.0), (200.0, 250.0)];
        let gaps = find_gaps_in_intervals(&intervals, 0.0, 300.0, 20.0);
        // 间隙: [50, 100] (宽度50) 和 [150, 200] (宽度50)
        assert_eq!(gaps.len(), 2);
        assert!((gaps[0].start - 50.0).abs() < 0.01);
        assert!((gaps[0].end - 100.0).abs() < 0.01);
        assert!((gaps[1].start - 150.0).abs() < 0.01);
        assert!((gaps[1].end - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_find_gaps_overlapping_intervals() {
        // 重叠区间应合并
        let intervals = vec![(10.0, 50.0), (30.0, 80.0), (200.0, 250.0)];
        let gaps = find_gaps_in_intervals(&intervals, 0.0, 300.0, 20.0);
        // 合并后: [10,80] 和 [200,250], 间隙: [80,200] (宽度120)
        assert_eq!(gaps.len(), 1);
        assert!((gaps[0].start - 80.0).abs() < 0.01);
        assert!((gaps[0].end - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_find_gaps_no_gap() {
        // 连续无间隙
        let intervals = vec![(10.0, 50.0), (48.0, 100.0)];
        let gaps = find_gaps_in_intervals(&intervals, 0.0, 120.0, 20.0);
        // 开头间隙 [0,10] 太小 (10 < 20)，中间无间隙
        assert!(gaps.is_empty() || gaps.iter().all(|g| g.width <= 20.0));
    }

    #[test]
    fn test_split_by_gaps_horizontal() {
        // 两个元素，上下排列，中间有大间隙
        let bboxes = vec![
            BBox::new(50.0, 50.0, 200.0, 40.0),
            BBox::new(50.0, 300.0, 200.0, 40.0),
        ];
        let indices = vec![0, 1];
        let region = BBox::new(0.0, 0.0, 612.0, 792.0);

        let gaps = vec![GapInfo {
            start: 90.0,
            end: 300.0,
            width: 210.0,
        }];

        let splits = split_by_gaps(&bboxes, &indices, &gaps, &region, CutDirection::Horizontal);
        assert_eq!(splits.len(), 2);
        assert_eq!(splits[0].0, vec![0]);
        assert_eq!(splits[1].0, vec![1]);
    }

    #[test]
    fn test_academic_paper_layout() {
        // 模拟学术论文布局:
        //  [      Paper Title       ]   y=30
        //  [Author1]  [Author2]         y=80
        //  [      Abstract          ]   y=130
        //  [Left col] [Right col]       y=200  (body)
        //  [Left col] [Right col]       y=300
        let bboxes = vec![
            BBox::new(50.0, 30.0, 512.0, 30.0),   // 0: Title
            BBox::new(80.0, 80.0, 180.0, 20.0),   // 1: Author1
            BBox::new(340.0, 80.0, 180.0, 20.0),  // 2: Author2
            BBox::new(50.0, 130.0, 512.0, 50.0),  // 3: Abstract
            BBox::new(50.0, 200.0, 240.0, 80.0),  // 4: Left body 1
            BBox::new(320.0, 200.0, 240.0, 80.0), // 5: Right body 1
            BBox::new(50.0, 300.0, 240.0, 80.0),  // 6: Left body 2
            BBox::new(320.0, 300.0, 240.0, 80.0), // 7: Right body 2
        ];

        let result = xy_cut_order(&bboxes, 612.0, 792.0, &default_config());

        // Title 应在最前
        assert_eq!(result[0], 0, "Title should be first");

        // Abstract 应在 authors 之后
        let pos_abstract = result.iter().position(|&x| x == 3).unwrap();
        let pos_author1 = result.iter().position(|&x| x == 1).unwrap();
        let pos_author2 = result.iter().position(|&x| x == 2).unwrap();
        assert!(pos_abstract > pos_author1, "Abstract after Author1");
        assert!(pos_abstract > pos_author2, "Abstract after Author2");

        // Left body 应在 Right body 之前
        let pos_lb1 = result.iter().position(|&x| x == 4).unwrap();
        let pos_lb2 = result.iter().position(|&x| x == 6).unwrap();
        let pos_rb1 = result.iter().position(|&x| x == 5).unwrap();
        assert!(pos_lb1 < pos_lb2, "Left body 1 before Left body 2");
        assert!(
            pos_lb2 < pos_rb1,
            "Left column completes before right column starts"
        );
    }
}
