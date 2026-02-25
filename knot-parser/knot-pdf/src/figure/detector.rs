//! 图表区域检测算法
//!
//! 基于 Path object 密度分析 + 连通区域合并，检测页面中的矢量图表区域。

use crate::backend::RawPathObject;
use crate::ir::{BBox, BlockIR};

use super::types::FigureRegion;

/// 检测参数
pub struct DetectParams {
    /// 图表区域最小面积（占页面面积比例）
    pub min_area_ratio: f32,
    /// 图表区域最小 Path objects 数量
    pub min_path_count: usize,
    /// 网格行数
    pub grid_rows: usize,
    /// 网格列数
    pub grid_cols: usize,
    /// 密度阈值（每个网格单元中的 path 数量超过此值视为高密度）
    pub density_threshold: usize,
}

impl Default for DetectParams {
    fn default() -> Self {
        Self {
            min_area_ratio: 0.05,
            min_path_count: 10,
            grid_rows: 30,
            grid_cols: 20,
            density_threshold: 2,
        }
    }
}

/// 检测页面中的图表区域
///
/// # 参数
/// - `path_objects`: 页面中提取的所有 Path objects
/// - `blocks`: 页面中已识别的文字块（用于判断图区域内包含哪些文字块）
/// - `table_bboxes`: 已识别的表格 bbox 列表（用于排除与表格重叠的区域）
/// - `page_width` / `page_height`: 页面尺寸
/// - `page_index`: 页码
/// - `params`: 检测参数
pub fn detect_figure_regions(
    path_objects: &[RawPathObject],
    blocks: &[BlockIR],
    table_bboxes: &[BBox],
    page_width: f32,
    page_height: f32,
    page_index: usize,
    params: &DetectParams,
) -> Vec<FigureRegion> {
    if path_objects.len() < params.min_path_count {
        return Vec::new();
    }

    let page_area = page_width * page_height;
    if page_area <= 0.0 {
        return Vec::new();
    }

    // 1. 构建密度网格
    let grid = build_density_grid(
        path_objects,
        page_width,
        page_height,
        params.grid_rows,
        params.grid_cols,
    );

    // 2. 提取高密度连通区域
    let regions = extract_connected_regions(
        &grid,
        params.grid_rows,
        params.grid_cols,
        params.density_threshold,
    );

    // 3. 将网格坐标转换为页面坐标 BBox
    let cell_w = page_width / params.grid_cols as f32;
    let cell_h = page_height / params.grid_rows as f32;

    let mut figure_regions = Vec::new();
    let mut fig_idx = 0;

    for region in &regions {
        if region.is_empty() {
            continue;
        }

        // 计算区域 BBox
        let mut min_row = params.grid_rows;
        let mut max_row = 0;
        let mut min_col = params.grid_cols;
        let mut max_col = 0;

        for &(row, col) in region {
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            min_col = min_col.min(col);
            max_col = max_col.max(col);
        }

        let bbox = BBox::new(
            min_col as f32 * cell_w,
            min_row as f32 * cell_h,
            (max_col - min_col + 1) as f32 * cell_w,
            (max_row - min_row + 1) as f32 * cell_h,
        );

        // 过滤：面积太小
        let area_ratio = bbox.area() / page_area;
        if area_ratio < params.min_area_ratio {
            continue;
        }

        // 过滤：区域内 path objects 数量不足
        let paths_in_region: usize = path_objects
            .iter()
            .filter(|p| bbox_contains(&bbox, &p.bbox))
            .count();
        if paths_in_region < params.min_path_count {
            continue;
        }

        // 过滤：与已识别表格高度重叠（> 50%）
        let table_overlap = table_bboxes.iter().any(|t| {
            let overlap = bbox.overlap_area(t);
            let ratio = overlap / bbox.area().max(1.0);
            ratio > 0.5
        });
        if table_overlap {
            continue;
        }

        // 收集区域内的文字块 ID
        let contained_block_ids: Vec<String> = blocks
            .iter()
            .filter(|blk| {
                // 文字块的中心点在图区域内
                let cx = blk.bbox.center_x();
                let cy = blk.bbox.center_y();
                cx >= bbox.x && cx <= bbox.right() && cy >= bbox.y && cy <= bbox.bottom()
            })
            .map(|blk| blk.block_id.clone())
            .collect();

        // 计算置信度：基于 path 密度 + 覆盖文字块数
        let density_score = (paths_in_region as f32 / 20.0).min(1.0);
        let text_score = if contained_block_ids.len() >= 3 {
            0.8
        } else if contained_block_ids.len() >= 1 {
            0.5
        } else {
            0.3
        };
        let confidence = (density_score * 0.6 + text_score * 0.4).min(1.0);

        // Caption 检测：在图区域正下方寻找 "Figure" / "Fig." / "图" 开头的文字块
        let caption = detect_caption(blocks, &bbox, &contained_block_ids);

        let figure_id = format!("fig_p{}_{}", page_index, fig_idx);
        fig_idx += 1;

        figure_regions.push(FigureRegion {
            figure_id,
            bbox,
            path_count: paths_in_region,
            contained_block_ids,
            confidence,
            caption,
        });
    }

    log::debug!(
        "Figure detection: page {} -> {} path objects -> {} figure regions",
        page_index,
        path_objects.len(),
        figure_regions.len()
    );

    figure_regions
}

/// 构建密度网格
fn build_density_grid(
    path_objects: &[RawPathObject],
    page_width: f32,
    page_height: f32,
    rows: usize,
    cols: usize,
) -> Vec<Vec<usize>> {
    let mut grid = vec![vec![0usize; cols]; rows];
    let cell_w = page_width / cols as f32;
    let cell_h = page_height / rows as f32;

    for path in path_objects {
        // Path 中心点所在的网格
        let cx = path.bbox.center_x();
        let cy = path.bbox.center_y();

        let col = ((cx / cell_w) as usize).min(cols - 1);
        let row = ((cy / cell_h) as usize).min(rows - 1);

        grid[row][col] += 1;
    }

    grid
}

/// 从密度网格中提取连通的高密度区域（BFS flood fill）
fn extract_connected_regions(
    grid: &[Vec<usize>],
    rows: usize,
    cols: usize,
    threshold: usize,
) -> Vec<Vec<(usize, usize)>> {
    let mut visited = vec![vec![false; cols]; rows];
    let mut regions = Vec::new();

    for r in 0..rows {
        for c in 0..cols {
            if !visited[r][c] && grid[r][c] >= threshold {
                // BFS
                let mut region = Vec::new();
                let mut queue = std::collections::VecDeque::new();
                queue.push_back((r, c));
                visited[r][c] = true;

                while let Some((cr, cc)) = queue.pop_front() {
                    region.push((cr, cc));

                    // 8-邻域
                    for dr in [-1i32, 0, 1] {
                        for dc in [-1i32, 0, 1] {
                            if dr == 0 && dc == 0 {
                                continue;
                            }
                            let nr = cr as i32 + dr;
                            let nc = cc as i32 + dc;
                            if nr >= 0 && nr < rows as i32 && nc >= 0 && nc < cols as i32 {
                                let nr = nr as usize;
                                let nc = nc as usize;
                                if !visited[nr][nc] && grid[nr][nc] >= threshold {
                                    visited[nr][nc] = true;
                                    queue.push_back((nr, nc));
                                }
                            }
                        }
                    }
                }

                if region.len() >= 3 {
                    // 至少 3 个网格单元才算一个区域
                    regions.push(region);
                }
            }
        }
    }

    regions
}

/// 判断 inner bbox 的中心是否在 outer bbox 内
fn bbox_contains(outer: &BBox, inner: &BBox) -> bool {
    let cx = inner.center_x();
    let cy = inner.center_y();
    cx >= outer.x && cx <= outer.right() && cy >= outer.y && cy <= outer.bottom()
}

/// 检测图区域下方的 Caption 文字块
fn detect_caption(
    blocks: &[BlockIR],
    figure_bbox: &BBox,
    contained_ids: &[String],
) -> Option<String> {
    let figure_bottom = figure_bbox.bottom();
    let figure_left = figure_bbox.x;
    let figure_right = figure_bbox.right();

    // 搜索图区域正下方一定距离内的文字块
    let max_gap = 30.0; // 图与 caption 之间最大间距（pt）

    for block in blocks {
        // 跳过已在图区域内的块
        if contained_ids.contains(&block.block_id) {
            continue;
        }

        let block_top = block.bbox.y;
        let block_cx = block.bbox.center_x();

        // 必须在图区域下方、水平范围内
        if block_top >= figure_bottom
            && block_top <= figure_bottom + max_gap
            && block_cx >= figure_left
            && block_cx <= figure_right
        {
            let text = block.normalized_text.trim();
            // 检查是否以 "Figure" / "Fig." / "图" 开头
            if text.starts_with("Figure")
                || text.starts_with("Fig.")
                || text.starts_with("fig.")
                || text.starts_with("图")
                || text.starts_with("FIGURE")
            {
                return Some(text.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::PathObjectKind;

    #[test]
    fn test_empty_path_objects() {
        let result =
            detect_figure_regions(&[], &[], &[], 612.0, 792.0, 0, &DetectParams::default());
        assert!(result.is_empty());
    }

    #[test]
    fn test_density_grid_basic() {
        let paths = vec![
            RawPathObject {
                bbox: BBox::new(100.0, 100.0, 10.0, 10.0),
                kind: PathObjectKind::Rect,
            },
            RawPathObject {
                bbox: BBox::new(105.0, 105.0, 10.0, 10.0),
                kind: PathObjectKind::Rect,
            },
            RawPathObject {
                bbox: BBox::new(110.0, 110.0, 10.0, 10.0),
                kind: PathObjectKind::Line,
            },
        ];

        let grid = build_density_grid(&paths, 612.0, 792.0, 30, 20);
        // 所有 3 个 path 中心点在 x ≈ 100-120，y ≈ 100-120 相同的网格区域
        let total: usize = grid.iter().flat_map(|r| r.iter()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_connected_regions() {
        // 构造一个 5x5 网格，中间有一个 3x3 高密度区域
        let mut grid = vec![vec![0usize; 5]; 5];
        grid[1][1] = 3;
        grid[1][2] = 2;
        grid[1][3] = 2;
        grid[2][1] = 2;
        grid[2][2] = 5;
        grid[2][3] = 2;
        grid[3][1] = 2;
        grid[3][2] = 2;
        grid[3][3] = 3;

        let regions = extract_connected_regions(&grid, 5, 5, 2);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].len(), 9); // 3x3
    }

    #[test]
    fn test_caption_detection() {
        let blocks = vec![BlockIR {
            block_id: "b0".to_string(),
            bbox: BBox::new(100.0, 510.0, 400.0, 15.0),
            role: crate::ir::BlockRole::Caption,
            lines: vec![],
            normalized_text: "Figure 1: The Transformer - model architecture.".to_string(),
        }];

        let figure_bbox = BBox::new(100.0, 50.0, 400.0, 450.0);
        let caption = detect_caption(&blocks, &figure_bbox, &[]);
        assert!(caption.is_some());
        assert!(caption.unwrap().contains("Transformer"));
    }
}
