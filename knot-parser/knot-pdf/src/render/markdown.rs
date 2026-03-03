use crate::ir::{BlockRole, DocumentIR, ImageSource, PageIR};

/// Markdown 渲染器
pub struct MarkdownRenderer {
    /// 是否包含页码标记
    pub include_page_markers: bool,
    /// 是否包含表格
    pub include_tables: bool,
    /// 是否包含图片引用
    pub include_images: bool,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self {
            include_page_markers: true,
            include_tables: true,
            include_images: true,
        }
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 渲染整个文档为 Markdown
    pub fn render_document(&self, doc: &DocumentIR) -> String {
        let mut output = String::new();

        // 文档标题
        if let Some(title) = &doc.metadata.title {
            if !title.is_empty() {
                output.push_str(&format!("# {}\n\n", title));
            }
        }

        for page in &doc.pages {
            output.push_str(&self.render_page(page));
        }

        output
    }

    /// 渲染单个页面为 Markdown
    ///
    /// 所有元素（文本块、图片、表格、公式）按 y 坐标统一排序后交错输出，
    /// 确保图表出现在原文流中正确的位置。
    pub fn render_page(&self, page: &PageIR) -> String {
        let mut output = String::new();

        if self.include_page_markers {
            output.push_str(&format!("<!-- Page {} -->\n\n", page.page_index + 1));
        }

        // 将所有元素统一收集并按 y 坐标排序
        enum PageElement<'a> {
            Block(&'a crate::ir::BlockIR),
            Image(&'a crate::ir::ImageIR),
            Table(&'a crate::ir::TableIR),
            Formula(&'a crate::ir::FormulaIR),
        }

        // 收集已被图片 caption_refs 引用的 block_id（避免重复输出）
        let caption_block_ids: std::collections::HashSet<&str> = page
            .images
            .iter()
            .flat_map(|img| img.caption_refs.iter().map(|s| s.as_str()))
            .collect();

        let mut elements: Vec<(f32, PageElement)> = Vec::new();

        for block in &page.blocks {
            // 跳过页眉页脚
            if matches!(block.role, BlockRole::Header | BlockRole::Footer) {
                continue;
            }
            // 跳过已被图片 caption 引用的 block（会在图片处以斜体输出）
            if caption_block_ids.contains(block.block_id.as_str()) {
                continue;
            }
            elements.push((block.bbox.y, PageElement::Block(block)));
        }

        if self.include_images {
            for image in &page.images {
                elements.push((image.bbox.y, PageElement::Image(image)));
            }
        }

        if self.include_tables {
            for table in &page.tables {
                elements.push((table.bbox.y, PageElement::Table(table)));
            }
        }

        for formula in &page.formulas {
            elements.push((formula.bbox.y, PageElement::Formula(formula)));
        }

        // 按 y 坐标排序（从上到下）
        elements.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // 按顺序渲染
        for (_y, elem) in &elements {
            match elem {
                PageElement::Block(block) => match block.role {
                    BlockRole::Title => {
                        output.push_str(&format!("## {}\n\n", block.normalized_text));
                    }
                    BlockRole::List => {
                        // 检测是否为误分类的编号标题（单行，以 "N." 或 "N.N" 开头）
                        if block.lines.len() == 1 {
                            let text = block.lines[0].text();
                            let trimmed = text.trim();
                            if is_numbered_heading(trimmed) {
                                output.push_str(&format!("## {}\n\n", trimmed));
                            } else {
                                output.push_str(&format!("- {}\n\n", trimmed));
                            }
                        } else {
                            for line in &block.lines {
                                output.push_str(&format!("- {}\n", line.text()));
                            }
                            output.push('\n');
                        }
                    }
                    BlockRole::Caption => {
                        output.push_str(&format!("*{}*\n\n", block.normalized_text));
                    }
                    _ => {
                        let text = &block.normalized_text;
                        output.push_str(&format_text_block(text));
                        output.push_str("\n\n");
                    }
                },
                PageElement::Image(image) => match image.source {
                    ImageSource::FigureRegion => {
                        output.push_str(&format!("**[图表：{}]**\n", image.image_id));
                        if let Some(ocr_text) = &image.ocr_text {
                            // 1. 插入结构性换行（拆分粘连的标题/KV/段落）
                            let with_breaks = insert_structural_breaks(ocr_text);
                            // 2. 检测表格模式并转为 markdown table
                            let with_tables = format_ocr_text_with_tables(&with_breaks);
                            // 3. 逐行检测 KV 对并格式化
                            let formatted = format_text_block(&with_tables);
                            output.push_str(&formatted);
                            output.push('\n');
                        }
                        for cap_id in &image.caption_refs {
                            for block in &page.blocks {
                                if &block.block_id == cap_id {
                                    output.push_str(&format!("*{}*\n\n", block.normalized_text));
                                    break;
                                }
                            }
                        }
                    }
                    ImageSource::Embedded => {
                        if image.is_qrcode {
                            output.push_str(&format!("[二维码/QR Code: {}]\n\n", image.image_id));
                        } else {
                            output.push_str(&format!(
                                "![image_{}](page_{}_img_{})\n\n",
                                image.image_id,
                                page.page_index + 1,
                                image.image_id
                            ));
                        }
                    }
                },
                PageElement::Table(table) => {
                    output.push_str(&table.to_markdown_or_html());
                    output.push_str("\n\n");
                }
                PageElement::Formula(formula) => {
                    output.push_str(&formula.to_markdown());
                    output.push_str("\n\n");
                }
            }
        }

        output
    }
}

/// 检测文本是否为编号章节标题
///
/// 匹配模式：以数字开头，后跟 `.` 和可选子编号（如 "1.", "4.1", "2.3.1"），
/// 后面跟空格和标题文字，总长度不超过 80 字符
fn is_numbered_heading(text: &str) -> bool {
    let trimmed = text.trim();

    // 长度检查：标题通常较短
    if trimmed.len() > 80 || trimmed.is_empty() {
        return false;
    }

    let chars: Vec<char> = trimmed.chars().collect();

    // 必须以数字开头
    if !chars[0].is_ascii_digit() {
        return false;
    }

    // 扫描编号部分：数字和点的组合（如 "1.", "4.1", "2.3.1"）
    let mut i = 0;
    let mut has_dot = false;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            i += 1;
        } else if chars[i] == '.' {
            has_dot = true;
            i += 1;
        } else {
            break;
        }
    }

    // 必须有至少一个点
    if !has_dot {
        return false;
    }

    // 编号后面必须有空格
    if i >= chars.len() || !chars[i].is_whitespace() {
        // 允许编号后直接跟字母（如 "1.Introduction"）
        if i >= chars.len() {
            return false;
        }
    }

    // 后面要有实际的标题文字（至少 1 个非空白字符）
    let remainder: String = chars[i..].iter().collect();
    let title_part = remainder.trim();
    !title_part.is_empty() && title_part.len() >= 1
}

/// 检测字符是否为 CJK（中日韩统一表意文字）
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4e00}'..='\u{9fff}' |
        '\u{3400}'..='\u{4dbf}' |
        '\u{f900}'..='\u{faff}'
    )
}

/// 常见中文标题/区段结尾词
const SECTION_SUFFIXES: &[&str] = &[
    "信息", "需求", "配置", "备注", "详情", "说明", "概要", "列表", "汇总", "合计", "小计", "总计",
    "明细",
];

/// 在 VLM/OCR 文本中插入结构性换行
///
/// 规则：
/// 1. CJK 文字紧跟 `key:` 模式时，在 key 前断行
/// 2. 标点符号（-、）、)等）后紧跟 CJK 文字时断行
/// 3. 常见标题后缀（信息/需求/配置等）后紧跟 CJK 时断行
fn insert_structural_breaks(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result = String::new();

    let mut i = 0;
    while i < len {
        // === 规则 1：在 key:value 模式的 key 前插入换行 ===
        // 当前字符是 CJK，且前一个字符也非空白、也非换行
        if i > 0 && is_cjk(chars[i]) && !chars[i - 1].is_whitespace() && chars[i - 1] != '\n' {
            // 向前看：是否有 `:` 或 `：` 在接下来 2~10 个字符内（key 长度限制）
            let search_end = (i + 10).min(len);
            let mut j = i + 1;
            let mut all_word = true;
            let mut found_colon = false;
            while j < search_end {
                if chars[j] == ':' || chars[j] == '：' {
                    let key_len = j - i;
                    if key_len >= 2 && key_len <= 8 && all_word {
                        found_colon = true;
                    }
                    break;
                }
                if chars[j].is_whitespace() {
                    break;
                }
                if !is_cjk(chars[j]) && !chars[j].is_alphanumeric() {
                    all_word = false;
                    break;
                }
                j += 1;
            }

            if found_colon && is_cjk(chars[i - 1]) {
                // 前面是 CJK 文字，后面紧跟 key:value → 在 key 前断行
                result.push('\n');
            }
        }

        // === 规则 2：标点后紧跟 CJK 文字 → 双换行（段落间距）===
        if i > 0
            && is_cjk(chars[i])
            && !chars[i - 1].is_whitespace()
            && matches!(chars[i - 1], '-' | '–' | '—' | ')' | '）' | ']' | '】')
        {
            result.push_str("\n\n");
        }

        // === 规则 3：常见标题后缀后紧跟 CJK → 断行 ===
        // 检查当前位置 i 是否是一个标题后缀的结束位置
        if i >= 2 && is_cjk(chars[i]) && i < len {
            for suffix in SECTION_SUFFIXES {
                let suffix_chars: Vec<char> = suffix.chars().collect();
                let slen = suffix_chars.len();
                if i >= slen {
                    let start = i - slen;
                    let candidate: String = chars[start..i].iter().collect();
                    if candidate == *suffix {
                        // 确保前面是 CJK（整体构成一个标题词）
                        // 并且后面紧跟 CJK 而不是冒号（冒号的情况已由规则 1 处理）
                        if (start == 0
                            || is_cjk(chars[start - 1])
                            || chars[start - 1].is_whitespace()
                            || chars[start - 1] == '\n')
                            && chars[i] != ':'
                            && chars[i] != '：'
                        {
                            // 用双换行创建段落间距
                            if !result.ends_with('\n') {
                                result.push_str("\n\n");
                            } else if !result.ends_with("\n\n") {
                                result.push('\n');
                            }
                        }
                        break;
                    }
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// 格式化文本块：检测 KV 表单数据并优化排版
///
/// 当一行文本包含 2 个以上的 `key: value` 或 `key：value` 模式时，
/// 将每个 KV 对拆分为独立行，以 `**key:** value` 格式输出。
fn format_text_block(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = String::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            result.push('\n');
            continue;
        }

        // 检测是否为区块标题（2-8 个 CJK 字符，无冒号，匹配常见标题后缀）
        if is_section_header(trimmed) {
            result.push_str(&format!("\n### {}\n\n", trimmed));
            continue;
        }

        // 检测 KV 对数量
        let kv_pairs = split_kv_pairs(trimmed);
        if kv_pairs.len() >= 2 {
            // 多个 KV 对，拆分为独立行
            for (key, value) in &kv_pairs {
                if value.is_empty() {
                    result.push_str(&format!("**{}**  \n", key));
                } else {
                    result.push_str(&format!("**{}:** {}  \n", key, value));
                }
            }
        } else {
            // 单个 KV 对或普通文本
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result
}

/// 检测一行文本是否为区块标题
///
/// 条件：2~8 个 CJK 字符，不含冒号，以常见中文标题后缀结尾
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    let char_count = trimmed.chars().count();

    // 长度检查：2~8 个字符
    if char_count < 2 || char_count > 8 {
        return false;
    }

    // 不能包含冒号
    if trimmed.contains(':') || trimmed.contains('：') {
        return false;
    }

    // 必须全部是 CJK 字符
    if !trimmed.chars().all(is_cjk) {
        return false;
    }

    // 以常见标题后缀结尾
    SECTION_SUFFIXES
        .iter()
        .any(|suffix| trimmed.ends_with(suffix))
}

/// 将一行文本拆分为 key:value 对
///
/// 匹配模式：`连续中文/字母（1~12字）` + `：` 或 `:` + `值`
/// 返回 (key, value) 列表
fn split_kv_pairs(line: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        // 跳过前导空格
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // 寻找冒号位置
        let key_start = i;
        let mut colon_pos = None;

        // key 最多 12 个字符
        let max_key_end = (i + 12).min(len);
        let mut j = i;
        while j < max_key_end {
            if chars[j] == ':' || chars[j] == '：' {
                // 确保前面有至少1个非空白字符作为key
                if j > key_start {
                    colon_pos = Some(j);
                }
                break;
            }
            j += 1;
        }

        if let Some(cp) = colon_pos {
            let key: String = chars[key_start..cp].iter().collect();
            let key = key.trim().to_string();

            // 跳过冒号和后面的空格
            let mut val_start = cp + 1;
            while val_start < len && chars[val_start].is_whitespace() {
                val_start += 1;
            }

            // value 到下一个 KV key 的开始（或行尾）
            let mut val_end = val_start;

            // 向前扫描找到下一个 key: 的开始
            let mut k = val_start;
            let mut last_non_space = val_start;
            while k < len {
                if (chars[k] == ':' || chars[k] == '：') && k > val_start {
                    // 回溯找到这个 key 的开始（连续非空白字符）
                    let mut kb = k - 1;
                    while kb > val_start && !chars[kb].is_whitespace() {
                        kb -= 1;
                    }
                    if chars[kb].is_whitespace() {
                        kb += 1;
                    }
                    // key 不能太长（最多 12 字）
                    if k - kb <= 12 && k - kb >= 1 {
                        val_end = kb;
                        // 去掉 value 尾部空格
                        while val_end > val_start && chars[val_end - 1].is_whitespace() {
                            val_end -= 1;
                        }
                        let value: String = chars[val_start..val_end].iter().collect();
                        pairs.push((key.clone(), value.trim().to_string()));
                        i = kb;
                        break;
                    }
                }
                if !chars[k].is_whitespace() {
                    last_non_space = k + 1;
                }
                k += 1;
            }

            if k >= len {
                // 到行尾了
                let value: String = chars[val_start..last_non_space].iter().collect();
                pairs.push((key, value.trim().to_string()));
                i = len;
            }
        } else {
            // 没有找到冒号，剩余文本作为普通内容
            if !pairs.is_empty() {
                // 已有一些 KV 对，把剩余文本追加到最后一个 value
                let remaining: String = chars[key_start..].iter().collect();
                if let Some(last) = pairs.last_mut() {
                    if !last.1.is_empty() {
                        last.1.push(' ');
                    }
                    last.1.push_str(remaining.trim());
                }
            }
            break;
        }
    }

    pairs
}

/// 将 OCR 文本中的表格部分转为 markdown table 格式
///
/// 检测逻辑：连续多行有相同数量的空格分隔字段（≥4列，≥3行含表头）→ markdown table
fn format_ocr_text_with_tables(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut output = String::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // 跳过 key:value 模式的行（表单数据，非表格）
        if looks_like_kv_line(line) {
            output.push_str(line);
            output.push('\n');
            i += 1;
            continue;
        }

        // 尝试检测表格：将当前行按空格分割
        let fields = split_table_fields(line);

        if fields.len() >= 4 {
            // 可能是表格行，向后扫描找到连续的同列数行
            let col_count = fields.len();
            let mut table_lines: Vec<Vec<String>> = vec![fields];
            let mut j = i + 1;

            while j < lines.len() {
                let next_line = lines[j].trim();
                if looks_like_kv_line(next_line) {
                    break;
                }
                let next_fields = split_table_fields(next_line);
                // 允许列数差 1 以内（某些行可能有空列）
                if next_fields.len() >= 4
                    && (next_fields.len() as isize - col_count as isize).abs() <= 1
                {
                    table_lines.push(next_fields);
                    j += 1;
                } else {
                    break;
                }
            }

            // 至少 3 行（1行表头 + 2行数据）才算表格
            if table_lines.len() >= 3 {
                // 统一列数（取最大）
                let max_cols = table_lines.iter().map(|r| r.len()).max().unwrap_or(0);

                // 第一行作为表头
                let header: Vec<String> = table_lines[0]
                    .iter()
                    .cloned()
                    .chain(std::iter::repeat(String::new()))
                    .take(max_cols)
                    .collect();
                output.push_str(&format!("\n| {} |\n", header.join(" | ")));
                output.push_str(&format!(
                    "| {} |\n",
                    header.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
                ));

                // 数据行
                for row in &table_lines[1..] {
                    let cells: Vec<String> = row
                        .iter()
                        .cloned()
                        .chain(std::iter::repeat(String::new()))
                        .take(max_cols)
                        .collect();
                    output.push_str(&format!("| {} |\n", cells.join(" | ")));
                }
                output.push('\n');
                i = j;
                continue;
            }
        }

        // 非表格行：直接输出
        output.push_str(line);
        output.push('\n');
        i += 1;
    }

    output
}

/// 检测行是否为 key:value 模式（表单数据，非表格行）
fn looks_like_kv_line(line: &str) -> bool {
    // 包含 ":" 或 "：" 的行，且 key:value 对数量 >= 2
    let colon_count = line.matches(':').count() + line.matches('：').count();
    colon_count >= 2
}

/// 按空格分割一行文本为字段
///
/// 使用连续 2+ 空格作为主分隔符，如果只有单空格也尝试分割
fn split_table_fields(line: &str) -> Vec<String> {
    if line.is_empty() {
        return Vec::new();
    }

    // 先尝试用 2+ 空格分割
    let parts: Vec<&str> = line.split("  ").filter(|s| !s.trim().is_empty()).collect();
    if parts.len() >= 4 {
        return parts.iter().map(|s| s.trim().to_string()).collect();
    }

    // 回退到单空格分割
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 4 {
        return parts.iter().map(|s| s.to_string()).collect();
    }

    Vec::new()
}
