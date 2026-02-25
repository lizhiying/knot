//! PageScore 评分逻辑
//!
//! 综合多个指标评估页面文本质量，用于判断是否需要 OCR 兜底。

use serde::{Deserialize, Serialize};

use crate::backend::RawChar;
use crate::config::Config;

/// 评分原因标记
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonFlag {
    /// 文本过少
    LowText,
    /// 乱码率高
    HighGarbled,
    /// 文本区域覆盖率低
    LowCoverage,
    /// 字符多样性过低（重复字符多）
    LowEntropy,
}

/// 页面质量评分结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageScore {
    /// 综合评分（0.0 ~ 1.0，越高越好）
    pub score: f32,
    /// 触发的原因标记
    pub reason_flags: Vec<ReasonFlag>,
    /// 详细指标
    pub metrics: ScoreMetrics,
}

/// 评分详细指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreMetrics {
    /// 总字符数
    pub total_char_count: usize,
    /// 可打印字符数
    pub printable_char_count: usize,
    /// 可打印字符占比
    pub printable_ratio: f32,
    /// 疑似乱码字符比率
    pub garbled_rate: f32,
    /// 文本 bbox 覆盖面积 / 页面面积
    pub text_area_coverage: f32,
    /// 中位字体大小
    pub median_font_size: f32,
    /// 字符多样性指标（unique chars / total chars）
    pub unique_ratio: f32,
    /// 简易熵代理指标
    pub entropy_proxy: f32,
}

impl Default for ScoreMetrics {
    fn default() -> Self {
        Self {
            total_char_count: 0,
            printable_char_count: 0,
            printable_ratio: 0.0,
            garbled_rate: 0.0,
            text_area_coverage: 0.0,
            median_font_size: 0.0,
            unique_ratio: 0.0,
            entropy_proxy: 0.0,
        }
    }
}

/// 计算页面评分
pub fn compute_page_score(
    chars: &[RawChar],
    page_width: f32,
    page_height: f32,
    config: &Config,
) -> PageScore {
    if chars.is_empty() {
        return PageScore {
            score: 0.0,
            reason_flags: vec![ReasonFlag::LowText],
            metrics: ScoreMetrics::default(),
        };
    }

    let metrics = compute_metrics(chars, page_width, page_height);
    let mut flags = Vec::new();

    // --- 各维度子分 ---

    // 1. 可打印字符比率（权重 0.2）
    let printable_score = metrics.printable_ratio;

    // 2. 乱码率（权重 0.35，反向 — 高权重以严格惩罚乱码）
    // 使用非线性映射：乱码率 > 50% 时分数急剧下降
    let garbled_score = if metrics.garbled_rate > 0.5 {
        // 严重乱码：分数极低
        (1.0 - metrics.garbled_rate) * 0.3
    } else {
        1.0 - metrics.garbled_rate * 1.5
    }
    .clamp(0.0, 1.0);
    if metrics.garbled_rate > config.garbled_threshold {
        flags.push(ReasonFlag::HighGarbled);
    }

    // 3. 文本覆盖率（权重 0.15）
    // 对覆盖率做 sigmoid 映射：覆盖 5% 以上即视为正常
    let coverage_score = sigmoid_map(metrics.text_area_coverage, 0.05, 10.0);
    if metrics.text_area_coverage < 0.01 {
        flags.push(ReasonFlag::LowCoverage);
    }

    // 4. 字符多样性 / 熵（权重 0.15）
    let entropy_score = metrics.entropy_proxy.min(1.0);
    if metrics.unique_ratio < 0.05 && metrics.total_char_count > 20 {
        flags.push(ReasonFlag::LowEntropy);
    }

    // 5. 文本量充足性（权重 0.15）
    // 少于 10 个可打印字符判低分，但标题页（少量文本+正常字体）不应误判
    let quantity_score = if metrics.printable_char_count >= 50 {
        1.0
    } else if metrics.printable_char_count >= 10 {
        // 如果字体大小合理（> 8pt），给更高分以避免标题页误判
        let font_bonus = if metrics.median_font_size > 8.0 {
            0.2
        } else {
            0.0
        };
        (metrics.printable_char_count as f32 / 50.0 + font_bonus).min(1.0)
    } else {
        flags.push(ReasonFlag::LowText);
        metrics.printable_char_count as f32 / 50.0
    };

    // 加权综合
    let score = (printable_score * 0.20
        + garbled_score * 0.35
        + coverage_score * 0.15
        + entropy_score * 0.15
        + quantity_score * 0.15)
        .clamp(0.0, 1.0);

    PageScore {
        score,
        reason_flags: flags,
        metrics,
    }
}

/// 计算详细指标
fn compute_metrics(chars: &[RawChar], page_width: f32, page_height: f32) -> ScoreMetrics {
    let total = chars.len();

    // 可打印字符（排除乱码字符）
    let printable_count = chars
        .iter()
        .filter(|c| {
            !c.unicode.is_control() && !c.unicode.is_whitespace() && !is_garbled_char(c.unicode)
        })
        .count();
    let printable_ratio = if total > 0 {
        printable_count as f32 / total as f32
    } else {
        0.0
    };

    // 乱码率：Unicode 替换字符、PUA 区域、不可见控制字符等
    let garbled_count = chars.iter().filter(|c| is_garbled_char(c.unicode)).count();
    let garbled_rate = if total > 0 {
        garbled_count as f32 / total as f32
    } else {
        0.0
    };

    // 文本覆盖率
    let page_area = page_width * page_height;
    let text_area = compute_text_area(chars);
    let text_area_coverage = if page_area > 0.0 {
        (text_area / page_area).min(1.0)
    } else {
        0.0
    };

    // 中位字体大小
    let median_font_size = compute_median_font_size(chars);

    // 字符多样性
    let (unique_ratio, entropy_proxy) = compute_diversity(chars);

    ScoreMetrics {
        total_char_count: total,
        printable_char_count: printable_count,
        printable_ratio,
        garbled_rate,
        text_area_coverage,
        median_font_size,
        unique_ratio,
        entropy_proxy,
    }
}

/// 判断是否为乱码字符
fn is_garbled_char(ch: char) -> bool {
    let cp = ch as u32;
    // Unicode 替换字符
    if cp == 0xFFFD {
        return true;
    }
    // 私用区 (PUA)
    if (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0xFFFFD).contains(&cp)
        || (0x100000..=0x10FFFD).contains(&cp)
    {
        return true;
    }
    // C0 控制字符（排除常见的 \t \n \r）
    if cp < 0x20 && cp != 0x09 && cp != 0x0A && cp != 0x0D {
        return true;
    }
    // C1 控制字符
    if (0x80..=0x9F).contains(&cp) {
        return true;
    }
    false
}

/// 计算文本区域面积（所有字符 bbox 的并集近似）
fn compute_text_area(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 0.0;
    }
    // 简单方法：用所有字符的外接矩形面积作为近似
    let mut x_min = f32::MAX;
    let mut y_min = f32::MAX;
    let mut x_max = f32::MIN;
    let mut y_max = f32::MIN;

    for ch in chars {
        x_min = x_min.min(ch.bbox.x);
        y_min = y_min.min(ch.bbox.y);
        x_max = x_max.max(ch.bbox.x + ch.bbox.width);
        y_max = y_max.max(ch.bbox.y + ch.bbox.height);
    }

    let w = (x_max - x_min).max(0.0);
    let h = (y_max - y_min).max(0.0);
    w * h
}

/// 计算中位字体大小
fn compute_median_font_size(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 0.0;
    }
    let mut sizes: Vec<f32> = chars.iter().map(|c| c.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sizes[sizes.len() / 2]
}

/// 计算字符多样性指标
fn compute_diversity(chars: &[RawChar]) -> (f32, f32) {
    if chars.is_empty() {
        return (0.0, 0.0);
    }

    use std::collections::HashMap;
    let mut freq: HashMap<char, usize> = HashMap::new();
    let mut printable_total = 0usize;

    for ch in chars {
        if !ch.unicode.is_control() && !ch.unicode.is_whitespace() {
            *freq.entry(ch.unicode).or_insert(0) += 1;
            printable_total += 1;
        }
    }

    if printable_total == 0 {
        return (0.0, 0.0);
    }

    let unique_count = freq.len();
    let unique_ratio = unique_count as f32 / printable_total as f32;

    // Shannon 熵近似（归一化到 0~1）
    let total_f = printable_total as f64;
    let entropy: f64 = freq
        .values()
        .map(|&count| {
            let p = count as f64 / total_f;
            if p > 0.0 {
                -p * p.ln()
            } else {
                0.0
            }
        })
        .sum();

    // 归一化：最大熵 = ln(unique_count)
    let max_entropy = (unique_count as f64).ln().max(1.0);
    let entropy_proxy = (entropy / max_entropy) as f32;

    (unique_ratio, entropy_proxy.clamp(0.0, 1.0))
}

/// Sigmoid 映射函数：将值映射到 0~1
fn sigmoid_map(value: f32, midpoint: f32, steepness: f32) -> f32 {
    1.0 / (1.0 + (-steepness * (value - midpoint)).exp())
}
