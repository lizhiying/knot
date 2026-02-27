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
                        output.push_str(&format!(
                            "![Figure {}](page_{}_fig_{})\n\n",
                            image.image_id,
                            page.page_index + 1,
                            image.image_id
                        ));
                        if let Some(ocr_text) = &image.ocr_text {
                            output.push_str(&format!("<!-- Figure text: {} -->\n\n", ocr_text));
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
