//! 页眉页脚跨页重复检测与标记
//!
//! 算法：
//! 1. 提取每页顶部/底部区域的文本块
//! 2. 跨页对比重复内容（模糊匹配，容忍页码变化）
//! 3. 标记 BlockIR.role = Header / Footer

use crate::ir::{BlockRole, PageIR};

/// 页眉页脚区域占页面高度的比例
const HEADER_REGION_RATIO: f32 = 0.08;
const FOOTER_REGION_RATIO: f32 = 0.08;

/// 最少需要多少页出现重复才判定为页眉/页脚
const MIN_REPEAT_PAGES: usize = 2;

/// 模糊匹配相似度阈值（0~1）
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// 页眉页脚检测结果
#[derive(Debug, Clone)]
pub struct HfDetectResult {
    /// 检测到的页眉模式数
    pub header_patterns: usize,
    /// 检测到的页脚模式数
    pub footer_patterns: usize,
    /// 命中的页数
    pub affected_page_count: usize,
}

/// 在多页之间检测并标记页眉页脚
///
/// 该函数会修改 `pages` 中 BlockIR 的 role 字段。
/// 如果 `strip` 为 true，则从 blocks 中移除被标记的页眉页脚。
pub fn detect_and_mark_headers_footers(pages: &mut [PageIR], strip: bool) -> HfDetectResult {
    if pages.len() < MIN_REPEAT_PAGES {
        return HfDetectResult {
            header_patterns: 0,
            footer_patterns: 0,
            affected_page_count: 0,
        };
    }

    // 收集每页的候选文本
    let candidates: Vec<PageCandidates> = pages.iter().map(extract_candidates).collect();

    // 检测重复的页眉模式
    let header_patterns = find_repeated_patterns(
        &candidates
            .iter()
            .map(|c| c.header_texts.as_slice())
            .collect::<Vec<_>>(),
    );

    // 检测重复的页脚模式
    let footer_patterns = find_repeated_patterns(
        &candidates
            .iter()
            .map(|c| c.footer_texts.as_slice())
            .collect::<Vec<_>>(),
    );

    let mut affected_pages = std::collections::HashSet::new();

    // 标记页眉
    for (page_idx, page) in pages.iter_mut().enumerate() {
        let page_height = page.size.height;
        let header_limit = page_height * HEADER_REGION_RATIO;

        for block in page.blocks.iter_mut() {
            if block.bbox.y < header_limit {
                let norm = normalize_for_match(&block.normalized_text);
                if header_patterns.iter().any(|p| pattern_matches(&norm, p)) {
                    block.role = BlockRole::Header;
                    affected_pages.insert(page_idx);
                }
            }
        }
    }

    // 标记页脚
    for (page_idx, page) in pages.iter_mut().enumerate() {
        let page_height = page.size.height;
        let footer_start = page_height * (1.0 - FOOTER_REGION_RATIO);

        for block in page.blocks.iter_mut() {
            if block.bbox.y + block.bbox.height > footer_start {
                let norm = normalize_for_match(&block.normalized_text);
                if footer_patterns.iter().any(|p| pattern_matches(&norm, p)) {
                    block.role = BlockRole::Footer;
                    affected_pages.insert(page_idx);
                }
            }
        }
    }

    // 如果 strip=true，移除被标记的页眉页脚
    // 安全检查：如果移除后页面没有剩余文本块，则不移除（避免误删正文）
    if strip {
        for page in pages.iter_mut() {
            let remaining_count = page
                .blocks
                .iter()
                .filter(|b| b.role != BlockRole::Header && b.role != BlockRole::Footer)
                .count();
            // 仅在移除后仍有剩余文本块时才 strip
            if remaining_count > 0 {
                page.blocks
                    .retain(|b| b.role != BlockRole::Header && b.role != BlockRole::Footer);
            }
        }
    }

    HfDetectResult {
        header_patterns: header_patterns.len(),
        footer_patterns: footer_patterns.len(),
        affected_page_count: affected_pages.len(),
    }
}

/// 每页的候选文本
struct PageCandidates {
    header_texts: Vec<String>,
    footer_texts: Vec<String>,
}

/// 从页面中提取候选页眉/页脚文本
fn extract_candidates(page: &PageIR) -> PageCandidates {
    let page_height = page.size.height;
    let header_limit = page_height * HEADER_REGION_RATIO;
    let footer_start = page_height * (1.0 - FOOTER_REGION_RATIO);

    let mut header_texts = Vec::new();
    let mut footer_texts = Vec::new();

    for block in &page.blocks {
        if block.bbox.y < header_limit {
            header_texts.push(normalize_for_match(&block.normalized_text));
        }
        if block.bbox.y + block.bbox.height > footer_start {
            footer_texts.push(normalize_for_match(&block.normalized_text));
        }
    }

    PageCandidates {
        header_texts,
        footer_texts,
    }
}

/// 归一化文本用于匹配（去除页码、空白、大小写）
fn normalize_for_match(text: &str) -> String {
    // 移除页码模式
    let text = remove_page_numbers(text);
    // 去除多余空白，转小写
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .trim()
        .to_string()
}

/// 移除常见页码模式
fn remove_page_numbers(text: &str) -> String {
    let text = text.trim();

    // 纯数字（页码）
    if text
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace())
    {
        return String::new();
    }

    let mut result = text.to_string();

    // 移除 "Page X" / "page X of Y" 模式
    let page_patterns = [
        // "Page 1" / "Page 1 of 10"
        r"[Pp]age\s+\d+(\s+of\s+\d+)?",
        // "第X页" / "第X页 共Y页"
        r"第\s*\d+\s*页(\s*共\s*\d+\s*页)?",
        // "- X -" 居中页码
        r"-\s*\d+\s*-",
        // 末尾数字（通常是页码）
        r"\s+\d+\s*$",
        // 开头数字（通常是页码）
        r"^\s*\d+\s+",
    ];

    for pattern in &page_patterns {
        if let Ok(re) = regex_lite::Regex::new(pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }

    result
}

/// 在多页间查找重复出现的文本模式
fn find_repeated_patterns(pages_texts: &[&[String]]) -> Vec<String> {
    if pages_texts.len() < MIN_REPEAT_PAGES {
        return Vec::new();
    }

    let mut patterns = Vec::new();

    // 以第一页的候选文本为基准
    if let Some(first_page) = pages_texts.first() {
        for candidate in *first_page {
            if candidate.is_empty() {
                // 空串意味着归一化后被完全移除（纯页码模式）
                // 检查其他页面是否也有同样的纯页码文本
                let empty_count = pages_texts
                    .iter()
                    .filter(|page_texts| page_texts.iter().any(|t| t.is_empty()))
                    .count();
                if empty_count >= MIN_REPEAT_PAGES && !patterns.contains(&String::new()) {
                    // 使用特殊标记表示"纯页码"模式
                    patterns.push(String::new());
                }
                continue;
            }

            // 计算在其他页面中出现的次数
            let match_count = pages_texts[1..]
                .iter()
                .filter(|page_texts| page_texts.iter().any(|t| fuzzy_match(t, candidate)))
                .count();

            // 如果在足够多的页面中重复出现
            if match_count + 1 >= MIN_REPEAT_PAGES {
                patterns.push(candidate.clone());
            }
        }
    }

    patterns
}

/// 模式匹配：支持空串模式（纯页码）和模糊匹配
fn pattern_matches(text: &str, pattern: &str) -> bool {
    // 空串模式：纯页码模式，归一化后也是空串即匹配
    if pattern.is_empty() {
        return text.is_empty();
    }
    fuzzy_match(text, pattern)
}

/// 模糊匹配：容忍页码变化等微小差异
fn fuzzy_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.is_empty() || b.is_empty() {
        return false;
    }

    let similarity = compute_similarity(a, b);
    similarity >= SIMILARITY_THRESHOLD
}

/// 计算两个字符串的相似度（基于最长公共子序列 / Jaccard）
fn compute_similarity(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    if a_chars.is_empty() && b_chars.is_empty() {
        return 1.0;
    }
    if a_chars.is_empty() || b_chars.is_empty() {
        return 0.0;
    }

    // 使用 bigram 相似度（比 LCS 更高效）
    let a_bigrams = bigrams(&a_chars);
    let b_bigrams = bigrams(&b_chars);

    if a_bigrams.is_empty() && b_bigrams.is_empty() {
        // 单字符比较
        return if a_chars == b_chars { 1.0 } else { 0.0 };
    }

    let intersection = a_bigrams.iter().filter(|bg| b_bigrams.contains(bg)).count();
    let union = a_bigrams.len() + b_bigrams.len();

    if union == 0 {
        0.0
    } else {
        (2.0 * intersection as f64) / union as f64
    }
}

/// 生成 bigram 列表
fn bigrams(chars: &[char]) -> Vec<(char, char)> {
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}
