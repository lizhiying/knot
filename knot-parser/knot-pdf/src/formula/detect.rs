//! 公式区域检测模块
//!
//! M12 Phase A：基于规则的公式区域检测
//!
//! 检测策略：
//! 1. 特殊字符密度 — 数学符号比例 > 30%
//! 2. 字体特征 — CMMI/CMSY/Symbol 等数学字体
//! 3. 几何特征 — 上下标嵌套、垂直堆叠
//! 4. 孤立短块 — 独占一行的短公式
//! 5. 公式编号 — 行末 "(1)"、"(2.3)" 等

use crate::backend::RawChar;
use crate::ir::{FormulaIR, FormulaType};

// ============================================================
// 数学字符集定义
// ============================================================

/// 数学特殊字符（Unicode 范围）
fn is_math_symbol(ch: char) -> bool {
    matches!(ch,
        // 希腊字母
        '\u{0391}'..='\u{03C9}' |
        // 数学运算符
        '±' | '×' | '÷' | '≤' | '≥' | '≠' | '≈' | '∝' |
        '∞' | '∂' | '∇' | '∀' | '∃' | '∈' | '∉' | '∋' |
        '⊂' | '⊃' | '⊆' | '⊇' | '∪' | '∩' | '⊕' | '⊗' |
        // 求和/积分/乘积
        '∑' | '∫' | '∬' | '∭' | '∮' | '∏' | '√' |
        // 关系/箭头
        '→' | '←' | '↔' | '⇒' | '⇐' | '⇔' | '↦' |
        '⟶' | '⟵' | '⟷' | '⟹' | '⟸' | '⟺' |
        // 其他数学符号
        '∧' | '∨' | '¬' | '⊤' | '⊥' |
        '⊢' | '⊣' | '⊨' | '⊩' |
        '∘' | '∙' | '⋅' | '⋆' | '⋮' | '⋯' | '⋰' | '⋱' |
        // 上下标常见
        '′' | '″' | '‴' |
        // 界符
        '⟨' | '⟩' | '⌈' | '⌉' | '⌊' | '⌋' |
        // 数学字母（Math italic 等）
        '\u{1D400}'..='\u{1D7FF}' |
        // 常用数学重音/修饰
        '̂' | '̃' | '̄' | '̇' | '̈' | '⃗'
    )
}

/// 判断字体名是否为数学字体
fn is_math_font(font_name: &str) -> bool {
    let lower = font_name.to_lowercase();
    lower.contains("cmmi")      // Computer Modern Math Italic
        || lower.contains("cmsy")   // Computer Modern Symbol
        || lower.contains("cmex")   // Computer Modern Extra (大括号等)
        || lower.contains("cmbx")   // Computer Modern Bold Extended (加粗变量)
        || lower.contains("msbm")   // AMS blackboard bold
        || lower.contains("msam")   // AMS symbol-A
        || lower.contains("symbol")
        || lower.contains("mathit")
        || lower.contains("mathsy")
        || lower.contains("mathex")
        || lower.contains("mathrm")
        || lower.contains("mathbb")
        || lower.contains("mathcal")
        || lower.contains("mathfrak")
        || lower.contains("stix")
        || lower.contains("cambria math")
        || (lower.contains("math") && !lower.contains("matho"))
}

// ============================================================
// 检测评分函数
// ============================================================

/// 特殊字符密度评分
///
/// 返回 0.0~1.0，数学符号比例越高分数越高
pub fn score_math_char_density(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 0.0;
    }

    let total = chars.len() as f32;
    let math_count = chars.iter().filter(|c| is_math_symbol(c.unicode)).count() as f32;
    let ratio = math_count / total;

    // 阈值映射：比例 > 0.3 分数高
    if ratio > 0.5 {
        1.0
    } else if ratio > 0.3 {
        0.8
    } else if ratio > 0.15 {
        0.5
    } else if ratio > 0.05 {
        0.2
    } else {
        0.0
    }
}

/// 数学字体评分
///
/// 返回 0.0~1.0，使用数学字体的字符比例越高分数越高
pub fn score_math_font(chars: &[RawChar]) -> f32 {
    if chars.is_empty() {
        return 0.0;
    }

    let total = chars.len() as f32;
    let math_font_count = chars
        .iter()
        .filter(|c| {
            c.font_name
                .as_ref()
                .map(|f| is_math_font(f))
                .unwrap_or(false)
        })
        .count() as f32;

    let ratio = math_font_count / total;

    if ratio > 0.5 {
        1.0
    } else if ratio > 0.3 {
        0.7
    } else if ratio > 0.1 {
        0.4
    } else {
        0.0
    }
}

/// 上下标检测评分
///
/// 检测字符 y 坐标与主 baseline 的偏移
/// 返回 0.0~1.0
pub fn score_supersubscript(chars: &[RawChar]) -> f32 {
    if chars.len() < 3 {
        return 0.0;
    }

    // 计算中位 y 坐标作为 baseline
    let mut ys: Vec<f32> = chars.iter().map(|c| c.bbox.y).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let baseline = ys[ys.len() / 2];

    // 计算主字体大小
    let mut sizes: Vec<f32> = chars.iter().map(|c| c.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let main_size = sizes[sizes.len() / 2];

    if main_size <= 0.0 {
        return 0.0;
    }

    // 统计明显偏移的字符（偏移超过主字体大小的 20%）
    let threshold = main_size * 0.2;
    let offset_count = chars
        .iter()
        .filter(|c| (c.bbox.y - baseline).abs() > threshold)
        .count();

    // 统计明显更小的字符（上下标通常字体更小）
    let small_count = chars
        .iter()
        .filter(|c| c.font_size < main_size * 0.85)
        .count();

    let offset_ratio = offset_count as f32 / chars.len() as f32;
    let small_ratio = small_count as f32 / chars.len() as f32;

    // 组合评分
    let combined = offset_ratio * 0.6 + small_ratio * 0.4;
    (combined * 3.0).min(1.0) // 放大并限制到 1.0
}

/// 检测公式编号
///
/// 在行末查找 "(1)"、"(2.3)"、"(A.1)" 等模式
pub fn detect_equation_number(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 正则：行末的 (数字) 或 (数字.数字) 或 (字母.数字)
    // 手动匹配避免额外依赖
    if let Some(open) = trimmed.rfind('(') {
        if trimmed.ends_with(')') {
            let inner = &trimmed[open + 1..trimmed.len() - 1];
            // 验证内容是合法的公式编号格式
            if is_equation_number_content(inner) {
                return Some(format!("({})", inner));
            }
        }
    }

    None
}

/// 验证字符串是否是合法的公式编号内容
///
/// 合法格式：数字, 数字.数字, 字母.数字, 星号变体
fn is_equation_number_content(s: &str) -> bool {
    if s.is_empty() || s.len() > 10 {
        return false;
    }

    let chars: Vec<char> = s.chars().collect();
    // 至少有一个数字
    if !chars.iter().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // 只能包含数字、字母、点、星号
    chars
        .iter()
        .all(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '*')
}

// ============================================================
// 公式区域检测入口
// ============================================================

/// 公式检测配置
pub struct FormulaDetectConfig {
    /// 综合置信度阈值
    pub min_confidence: f32,
    /// 行间公式最小行间距（相对于前后行距的倍数）
    pub display_min_gap_ratio: f32,
}

impl Default for FormulaDetectConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.3,
            display_min_gap_ratio: 1.5,
        }
    }
}

/// 从 RawChar 列表检测公式区域
///
/// 返回检测到的 FormulaIR 列表
pub fn detect_formulas(
    chars: &[RawChar],
    blocks: &[crate::ir::BlockIR],
    page_index: usize,
) -> Vec<FormulaIR> {
    let config = FormulaDetectConfig::default();
    detect_formulas_with_config(chars, blocks, page_index, &config)
}

/// 带配置的公式检测
pub fn detect_formulas_with_config(
    chars: &[RawChar],
    blocks: &[crate::ir::BlockIR],
    page_index: usize,
    config: &FormulaDetectConfig,
) -> Vec<FormulaIR> {
    let mut formulas = Vec::new();
    let mut formula_idx = 0;

    // 按文本块检测
    for block in blocks {
        // 收集块 bbox 范围内的字符
        let block_chars: Vec<&RawChar> = chars
            .iter()
            .filter(|c| block.bbox.overlaps(&c.bbox))
            .collect();

        if block_chars.is_empty() {
            continue;
        }

        // 计算各维度评分
        let owned_chars: Vec<RawChar> = block_chars.iter().map(|c| (*c).clone()).collect();
        let char_score = score_math_char_density(&owned_chars);
        let font_score = score_math_font(&owned_chars);
        let supsub_score = score_supersubscript(&owned_chars);

        // 综合评分
        let confidence = char_score * 0.35 + font_score * 0.40 + supsub_score * 0.25;

        if confidence < config.min_confidence {
            continue;
        }

        // 判断行内 vs 行间
        let formula_type = classify_formula_type(block, blocks, config);

        // 检测公式编号
        let eq_number = detect_equation_number(&block.normalized_text);

        let formula_id = format!("f{}_{}", page_index, formula_idx);
        formula_idx += 1;

        formulas.push(FormulaIR {
            formula_id,
            page_index,
            bbox: block.bbox,
            formula_type,
            confidence,
            raw_text: block.normalized_text.clone(),
            latex: None,
            equation_number: eq_number,
            contained_block_ids: vec![block.block_id.clone()],
        });
    }

    formulas
}

/// 判断公式类型：行内 vs 行间
fn classify_formula_type(
    block: &crate::ir::BlockIR,
    all_blocks: &[crate::ir::BlockIR],
    config: &FormulaDetectConfig,
) -> FormulaType {
    // 策略 1: 如果块的文本很短（< 60 字符）且独占一行（前后间距大）→ 行间
    let text_len = block.normalized_text.len();
    if text_len > 100 {
        return FormulaType::Inline; // 太长不太可能是行间公式
    }

    // 策略 2: 检查该块上下是否有大间距
    let block_center_y = block.bbox.center_y();
    let block_height = block.bbox.height;

    // 查找最近的上方和下方块
    let mut gap_above = f32::MAX;
    let mut gap_below = f32::MAX;

    for other in all_blocks {
        if std::ptr::eq(other, block) {
            continue;
        }
        let other_center_y = other.bbox.center_y();

        if other_center_y < block_center_y {
            // 上方块
            let gap = block.bbox.y - other.bbox.bottom();
            if gap > 0.0 && gap < gap_above {
                gap_above = gap;
            }
        } else {
            // 下方块
            let gap = other.bbox.y - block.bbox.bottom();
            if gap > 0.0 && gap < gap_below {
                gap_below = gap;
            }
        }
    }

    // 如果上下间距都大于块高度的 display_min_gap_ratio 倍 → 行间公式
    if gap_above > block_height * config.display_min_gap_ratio
        && gap_below > block_height * config.display_min_gap_ratio
    {
        return FormulaType::Display;
    }

    // 策略 3: 如果文本很短且包含公式编号 → 行间公式
    if text_len < 60 && detect_equation_number(&block.normalized_text).is_some() {
        return FormulaType::Display;
    }

    FormulaType::Inline
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::BBox;

    fn make_char(ch: char, x: f32, y: f32) -> RawChar {
        RawChar {
            unicode: ch,
            bbox: BBox::new(x, y, 6.0, 12.0),
            font_size: 12.0,
            font_name: None,
            is_bold: false,
        }
    }

    fn make_math_char(ch: char, x: f32, y: f32) -> RawChar {
        RawChar {
            unicode: ch,
            bbox: BBox::new(x, y, 6.0, 12.0),
            font_size: 12.0,
            font_name: Some("CMMI10".to_string()),
            is_bold: false,
        }
    }

    // --- is_math_symbol ---

    #[test]
    fn test_math_symbol_detection() {
        assert!(is_math_symbol('∑'));
        assert!(is_math_symbol('∫'));
        assert!(is_math_symbol('α'));
        assert!(is_math_symbol('→'));
        assert!(is_math_symbol('≤'));
        assert!(!is_math_symbol('a'));
        assert!(!is_math_symbol('1'));
        assert!(!is_math_symbol('+'));
    }

    // --- is_math_font ---

    #[test]
    fn test_math_font_detection() {
        assert!(is_math_font("CMMI10"));
        assert!(is_math_font("CMSY8"));
        assert!(is_math_font("CMEX10"));
        assert!(is_math_font("TimesNewRoman-MathItalic"));
        assert!(is_math_font("Cambria Math"));
        assert!(is_math_font("STIX-Regular"));
        assert!(!is_math_font("TimesNewRoman"));
        assert!(!is_math_font("Arial"));
        assert!(!is_math_font("Helvetica"));
    }

    // --- score_math_char_density ---

    #[test]
    fn test_score_math_char_density_high() {
        // 全是数学符号
        let chars = vec![
            make_char('∑', 0.0, 0.0),
            make_char('α', 10.0, 0.0),
            make_char('β', 20.0, 0.0),
            make_char('∫', 30.0, 0.0),
        ];
        let score = score_math_char_density(&chars);
        assert!(score >= 0.8, "全数学符号应得高分: {}", score);
    }

    #[test]
    fn test_score_math_char_density_low() {
        // 全是普通文本
        let chars = vec![
            make_char('H', 0.0, 0.0),
            make_char('e', 10.0, 0.0),
            make_char('l', 20.0, 0.0),
            make_char('l', 30.0, 0.0),
            make_char('o', 40.0, 0.0),
        ];
        let score = score_math_char_density(&chars);
        assert!(score < 0.1, "纯文本应得低分: {}", score);
    }

    // --- score_math_font ---

    #[test]
    fn test_score_math_font_high() {
        let chars = vec![
            make_math_char('x', 0.0, 0.0),
            make_math_char('y', 10.0, 0.0),
            make_math_char('+', 20.0, 0.0),
        ];
        let score = score_math_font(&chars);
        assert!(score >= 0.7, "全数学字体应得高分: {}", score);
    }

    #[test]
    fn test_score_math_font_low() {
        let chars = vec![make_char('a', 0.0, 0.0), make_char('b', 10.0, 0.0)];
        let score = score_math_font(&chars);
        assert!(score < 0.1, "无数学字体应得低分: {}", score);
    }

    // --- score_supersubscript ---

    #[test]
    fn test_score_supersubscript_with_sub() {
        // 模拟 x₂ — 第二个字符 y 偏移大、字体小
        let chars = vec![
            RawChar {
                unicode: 'x',
                bbox: BBox::new(0.0, 10.0, 8.0, 12.0),
                font_size: 12.0,
                font_name: None,
                is_bold: false,
            },
            RawChar {
                unicode: '2',
                bbox: BBox::new(8.0, 14.0, 5.0, 8.0), // y 偏移 + 字体小
                font_size: 8.0,
                font_name: None,
                is_bold: false,
            },
            RawChar {
                unicode: '+',
                bbox: BBox::new(16.0, 10.0, 8.0, 12.0),
                font_size: 12.0,
                font_name: None,
                is_bold: false,
            },
            RawChar {
                unicode: 'y',
                bbox: BBox::new(24.0, 10.0, 8.0, 12.0),
                font_size: 12.0,
                font_name: None,
                is_bold: false,
            },
        ];
        let score = score_supersubscript(&chars);
        assert!(score > 0.1, "有下标应得正分: {}", score);
    }

    #[test]
    fn test_score_supersubscript_normal() {
        // 所有字符在同一 baseline，同一字体大小
        let chars = vec![
            make_char('H', 0.0, 10.0),
            make_char('e', 10.0, 10.0),
            make_char('l', 20.0, 10.0),
            make_char('l', 30.0, 10.0),
        ];
        let score = score_supersubscript(&chars);
        assert!(score < 0.1, "无下标应得接近 0: {}", score);
    }

    // --- detect_equation_number ---

    #[test]
    fn test_detect_equation_number() {
        assert_eq!(
            detect_equation_number("E = mc² (1)"),
            Some("(1)".to_string())
        );
        assert_eq!(
            detect_equation_number("f(x) = ax + b (2.3)"),
            Some("(2.3)".to_string())
        );
        assert_eq!(
            detect_equation_number("∑xᵢ (A.1)"),
            Some("(A.1)".to_string())
        );
        assert_eq!(detect_equation_number("Hello world"), None);
        assert_eq!(detect_equation_number("f(x)"), None); // "x" 没有数字
    }

    // --- classify_formula_type ---

    #[test]
    fn test_classify_display_formula() {
        use crate::ir::{BlockIR, BlockRole, TextLine, TextSpan};

        let formula_block = BlockIR {
            block_id: "b0".to_string(),
            bbox: BBox::new(100.0, 200.0, 300.0, 20.0),
            role: BlockRole::Body,
            lines: vec![TextLine {
                spans: vec![TextSpan {
                    text: "E = mc²".to_string(),
                    font_size: Some(12.0),
                    is_bold: false,
                    font_name: None,
                }],
                bbox: None,
            }],
            normalized_text: "E = mc²".to_string(),
        };

        let blocks = vec![
            BlockIR {
                block_id: "b_above".to_string(),
                bbox: BBox::new(50.0, 100.0, 400.0, 15.0), // 上方，gap=85
                role: BlockRole::Body,
                lines: vec![],
                normalized_text: "Some text above".to_string(),
            },
            formula_block.clone(),
            BlockIR {
                block_id: "b_below".to_string(),
                bbox: BBox::new(50.0, 300.0, 400.0, 15.0), // 下方，gap=80
                role: BlockRole::Body,
                lines: vec![],
                normalized_text: "Some text below".to_string(),
            },
        ];

        let config = FormulaDetectConfig::default();
        let ft = classify_formula_type(&formula_block, &blocks, &config);
        assert_eq!(ft, FormulaType::Display, "上下大间距应识别为行间公式");
    }

    #[test]
    fn test_classify_inline_formula() {
        use crate::ir::{BlockIR, BlockRole};

        let formula_block = BlockIR {
            block_id: "b0".to_string(),
            bbox: BBox::new(100.0, 200.0, 300.0, 15.0),
            role: BlockRole::Body,
            lines: vec![],
            normalized_text: "where α is a constant and β → 0".to_string(),
        };

        let blocks = vec![
            BlockIR {
                block_id: "b_above".to_string(),
                bbox: BBox::new(50.0, 180.0, 400.0, 15.0), // 上方，gap=5
                role: BlockRole::Body,
                lines: vec![],
                normalized_text: "Previous text".to_string(),
            },
            formula_block.clone(),
            BlockIR {
                block_id: "b_below".to_string(),
                bbox: BBox::new(50.0, 220.0, 400.0, 15.0), // 下方，gap=5
                role: BlockRole::Body,
                lines: vec![],
                normalized_text: "Next text".to_string(),
            },
        ];

        let config = FormulaDetectConfig::default();
        let ft = classify_formula_type(&formula_block, &blocks, &config);
        assert_eq!(ft, FormulaType::Inline, "上下间距小应识别为行内公式");
    }
}
