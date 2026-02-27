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
    pub fn render_page(&self, page: &PageIR) -> String {
        let mut output = String::new();

        if self.include_page_markers {
            output.push_str(&format!("<!-- Page {} -->\n\n", page.page_index + 1));
        }

        // 渲染文本块
        for block in &page.blocks {
            match block.role {
                BlockRole::Title => {
                    output.push_str(&format!("## {}\n\n", block.normalized_text));
                }
                BlockRole::Header | BlockRole::Footer => {
                    // 页眉页脚默认不输出，或以注释形式输出
                    continue;
                }
                BlockRole::List => {
                    for line in &block.lines {
                        output.push_str(&format!("- {}\n", line.text()));
                    }
                    output.push('\n');
                }
                BlockRole::Caption => {
                    output.push_str(&format!("*{}*\n\n", block.normalized_text));
                }
                _ => {
                    output.push_str(&block.normalized_text);
                    output.push_str("\n\n");
                }
            }
        }

        // 渲染公式（M12）
        if !page.formulas.is_empty() {
            // 按 y 坐标排序
            let mut formulas: Vec<&crate::ir::FormulaIR> = page.formulas.iter().collect();
            formulas.sort_by(|a, b| {
                a.bbox
                    .y
                    .partial_cmp(&b.bbox.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for formula in formulas {
                output.push_str(&formula.to_markdown());
                output.push_str("\n\n");
            }
        }

        // 渲染表格
        if self.include_tables {
            for table in &page.tables {
                output.push_str(&table.to_markdown());
                output.push_str("\n\n");
            }
        }

        // 渲染图片引用
        if self.include_images {
            for image in &page.images {
                match image.source {
                    ImageSource::FigureRegion => {
                        // 矢量图表区域：输出图片引用 + OCR 文字 + caption
                        output.push_str(&format!("**[图表：{}]**\n", image.image_id,));
                        if let Some(ocr_text) = &image.ocr_text {
                            // OCR/VLM 文本作为可见内容输出
                            // 检测并格式化表格部分为 markdown table
                            output.push_str(&format_ocr_text_with_tables(ocr_text));
                            output.push('\n');
                        }
                        // 输出关联 caption
                        for cap_id in &image.caption_refs {
                            // 从 blocks 中查找 caption 文本（但 blocks 可能已被剔除）
                            // 回退：使用 cap_id 作为提示
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
                            // 二维码输出为标签，方便检索
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
                }
            }
        }

        output
    }
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
