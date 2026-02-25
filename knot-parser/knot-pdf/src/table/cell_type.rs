//! CellType 推断
//!
//! 根据单元格文本内容推断类型：
//! Number / Percent / Currency / Date / Text / Unknown

use crate::ir::CellType;

/// 检测单元格文本的类型
pub fn detect_cell_type(text: &str) -> CellType {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return CellType::Unknown;
    }

    // 百分比：以 % 结尾
    if trimmed.ends_with('%') {
        let num_part = trimmed.trim_end_matches('%').trim();
        if parse_number(num_part).is_some() {
            return CellType::Percent;
        }
    }

    // 货币：以货币符号开头
    if let Some(rest) = strip_currency_prefix(trimmed) {
        if parse_number(rest.trim()).is_some() {
            return CellType::Currency;
        }
    }

    // 货币：以货币符号结尾（如 "1,234元"）
    if let Some(rest) = strip_currency_suffix(trimmed) {
        if parse_number(rest.trim()).is_some() {
            return CellType::Currency;
        }
    }

    // 日期
    if looks_like_date(trimmed) {
        return CellType::Date;
    }

    // 纯数字（含千分位逗号、小数点、正负号）
    if parse_number(trimmed).is_some() {
        return CellType::Number;
    }

    // 带括号的负数：(1,234)
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if parse_number(inner).is_some() {
            return CellType::Number;
        }
    }

    CellType::Text
}

/// 尝试解析数字字符串（支持千分位逗号）
fn parse_number(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 去除千分位逗号和空格
    let cleaned: String = trimmed
        .chars()
        .filter(|c| *c != ',' && *c != ' ' && *c != '\u{00a0}')
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    cleaned.parse::<f64>().ok()
}

/// 去除货币前缀，返回剩余部分
fn strip_currency_prefix(s: &str) -> Option<&str> {
    for prefix in &["$", "¥", "€", "£", "USD", "CNY", "RMB", "JPY", "GBP", "EUR"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return Some(rest);
        }
    }
    None
}

/// 去除货币后缀，返回剩余部分
fn strip_currency_suffix(s: &str) -> Option<&str> {
    // 注意：长后缀必须排在短后缀前面，防止 "万元" 被 "元" 先匹配
    for suffix in &["万元", "亿元", "美元", "欧元", "英镑", "日元", "元"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            return Some(rest);
        }
    }
    None
}

/// 判断是否看起来像日期
fn looks_like_date(s: &str) -> bool {
    let trimmed = s.trim();

    // yyyy-mm-dd / yyyy/mm/dd / yyyy.mm.dd
    if trimmed.len() >= 8 && trimmed.len() <= 10 {
        let parts: Vec<&str> = trimmed.split(['-', '/', '.']).collect();
        if parts.len() == 3 {
            if parts[0].len() == 4 && parts[0].chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
            // mm/dd/yyyy
            if parts[2].len() == 4 && parts[2].chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }

    // yyyy年mm月dd日 / yyyy年mm月
    if trimmed.contains('年') && (trimmed.contains('月') || trimmed.contains("季度")) {
        return true;
    }

    // "2023Q1" / "FY2023" 等
    if trimmed.len() >= 5 && trimmed.len() <= 8 {
        let upper = trimmed.to_uppercase();
        if upper.starts_with("FY") || upper.contains('Q') {
            let digits: String = upper.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 4 {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_number() {
        assert_eq!(detect_cell_type("1234"), CellType::Number);
        assert_eq!(detect_cell_type("1,234"), CellType::Number);
        assert_eq!(detect_cell_type("-1,234.56"), CellType::Number);
        assert_eq!(detect_cell_type("0.5"), CellType::Number);
        assert_eq!(detect_cell_type("(1,234)"), CellType::Number);
    }

    #[test]
    fn test_detect_percent() {
        assert_eq!(detect_cell_type("12.3%"), CellType::Percent);
        assert_eq!(detect_cell_type("-5%"), CellType::Percent);
    }

    #[test]
    fn test_detect_currency() {
        assert_eq!(detect_cell_type("$1,234"), CellType::Currency);
        assert_eq!(detect_cell_type("¥5,678"), CellType::Currency);
        assert_eq!(detect_cell_type("€100"), CellType::Currency);
        assert_eq!(detect_cell_type("1,234万元"), CellType::Currency);
    }

    #[test]
    fn test_detect_date() {
        assert_eq!(detect_cell_type("2023-01-15"), CellType::Date);
        assert_eq!(detect_cell_type("2023/01/15"), CellType::Date);
        assert_eq!(detect_cell_type("2023年1月"), CellType::Date);
        assert_eq!(detect_cell_type("FY2023"), CellType::Date);
    }

    #[test]
    fn test_detect_text() {
        assert_eq!(detect_cell_type("Hello World"), CellType::Text);
        assert_eq!(detect_cell_type("收入"), CellType::Text);
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_cell_type(""), CellType::Unknown);
        assert_eq!(detect_cell_type("  "), CellType::Unknown);
    }
}
