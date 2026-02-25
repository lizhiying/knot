//! 页码范围解析器
//!
//! 支持格式：
//! - 单页：`3`
//! - 范围：`1-5`
//! - 列表：`1,3,5`
//! - 混合：`1-3,5,8-10`
//! - 末尾省略：`5-`（第 5 页到最后）

use std::collections::BTreeSet;

/// 解析页码范围字符串，返回排序后的页码集合（0-indexed）
///
/// 输入的页码是 1-indexed（用户友好），返回的是 0-indexed（内部使用）。
/// `total_pages` 用于处理开放范围（如 `5-`）和边界检查。
pub fn parse_page_range(range_str: &str, total_pages: usize) -> Result<Vec<usize>, String> {
    let mut pages = BTreeSet::new();

    for part in range_str.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some(dash_pos) = part.find('-') {
            // 范围格式: "1-5" 或 "5-"
            let start_str = &part[..dash_pos];
            let end_str = &part[dash_pos + 1..];

            let start: usize = start_str
                .trim()
                .parse()
                .map_err(|_| format!("无效的页码: '{}'", start_str.trim()))?;

            if start == 0 {
                return Err("页码从 1 开始，不能为 0".to_string());
            }

            let end: usize = if end_str.trim().is_empty() {
                // 开放范围: "5-" → 5 到最后
                total_pages
            } else {
                end_str
                    .trim()
                    .parse()
                    .map_err(|_| format!("无效的页码: '{}'", end_str.trim()))?
            };

            if start > end {
                return Err(format!("无效的范围: {}-{}（起始页大于结束页）", start, end));
            }

            // 添加范围内的页码（转为 0-indexed，跳过超出范围的）
            for p in start..=end {
                if p <= total_pages {
                    pages.insert(p - 1);
                }
            }
        } else {
            // 单页格式: "3"
            let page: usize = part
                .parse()
                .map_err(|_| format!("无效的页码: '{}'", part))?;

            if page == 0 {
                return Err("页码从 1 开始，不能为 0".to_string());
            }

            if page <= total_pages {
                pages.insert(page - 1);
            }
        }
    }

    if pages.is_empty() {
        return Err("未选中任何有效页码".to_string());
    }

    Ok(pages.into_iter().collect())
}

/// 根据页码范围过滤 DocumentIR 的页面
pub fn filter_pages(doc: &mut knot_pdf::DocumentIR, page_indices: &[usize]) {
    let selected: BTreeSet<usize> = page_indices.iter().copied().collect();
    doc.pages.retain(|page| selected.contains(&page.page_index));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_page() {
        let result = parse_page_range("3", 10).unwrap();
        assert_eq!(result, vec![2]); // 0-indexed
    }

    #[test]
    fn test_range() {
        let result = parse_page_range("1-5", 10).unwrap();
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_list() {
        let result = parse_page_range("1,3,5", 10).unwrap();
        assert_eq!(result, vec![0, 2, 4]);
    }

    #[test]
    fn test_mixed() {
        let result = parse_page_range("1-3,5,8-10", 10).unwrap();
        assert_eq!(result, vec![0, 1, 2, 4, 7, 8, 9]);
    }

    #[test]
    fn test_open_end() {
        let result = parse_page_range("8-", 10).unwrap();
        assert_eq!(result, vec![7, 8, 9]);
    }

    #[test]
    fn test_out_of_range_skipped() {
        let result = parse_page_range("9-15", 10).unwrap();
        assert_eq!(result, vec![8, 9]); // 只保留 9 和 10
    }

    #[test]
    fn test_duplicates_removed() {
        let result = parse_page_range("1,1,2,2-3", 10).unwrap();
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn test_zero_page_error() {
        let result = parse_page_range("0", 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_range_error() {
        let result = parse_page_range("5-3", 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_format_error() {
        let result = parse_page_range("abc", 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_all_out_of_range() {
        let result = parse_page_range("20-30", 10);
        assert!(result.is_err()); // 所有页码超出范围
    }

    #[test]
    fn test_spaces_in_range() {
        let result = parse_page_range("1 - 3, 5 , 8 - 10", 10).unwrap();
        assert_eq!(result, vec![0, 1, 2, 4, 7, 8, 9]);
    }
}
