use crate::{DocumentParser, NodeMeta, PageIndexConfig, PageIndexError, PageNode};
use async_trait::async_trait;
use knot_excel::ExcelConfig;
use std::collections::HashMap;
use std::path::Path;

pub struct ExcelParser;

impl ExcelParser {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DocumentParser for ExcelParser {
    fn can_handle(&self, extension: &str) -> bool {
        matches!(extension, "xlsx" | "xls" | "xlsm" | "xlsb")
    }

    async fn parse(
        &self,
        path: &Path,
        _config: &PageIndexConfig,
    ) -> Result<PageNode, PageIndexError> {
        let start_time = std::time::Instant::now();
        let file_path = path.to_string_lossy().to_string();

        let excel_config = ExcelConfig::default();

        // 1. 使用 knot-excel 解析 -> DataBlock 列表
        let parsed = knot_excel::pipeline::parse_excel_full(path, &excel_config)
            .map_err(|e| PageIndexError::ParseError(format!("knot-excel error: {}", e)))?;

        if parsed.blocks.is_empty() {
            return Err(PageIndexError::ParseError(
                "knot-excel: no data extracted from Excel file".to_string(),
            ));
        }

        println!(
            "[ExcelParser] Parsed {} data blocks from {}",
            parsed.blocks.len(),
            path.display()
        );

        // 2. 将所有 DataBlock 的信息合并为单个摘要 chunk
        //    详细数据已存入 DuckDB 持久缓存，这里只需保留搜索发现所需的关键信息
        let mut summary_text = String::new();
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        for (profile, block) in parsed.profiles.iter().zip(parsed.blocks.iter()) {
            summary_text.push_str(&format!(
                "[表格数据] {} / Sheet \"{}\"\n共 {} 行 {} 列\n\n",
                file_name,
                block.sheet_name,
                block.row_count,
                block.column_names.len()
            ));

            // 列信息（用于关键词匹配）
            summary_text.push_str("列信息:\n");
            for (name, dtype) in block.column_names.iter().zip(profile.column_types.iter()) {
                summary_text.push_str(&format!("- {} ({})\n", name, dtype));
            }

            // 数据示例（前 3 行，用于关键词匹配）
            if !profile.sample_rows.is_empty() {
                summary_text.push_str(&format!(
                    "\n数据示例（前 {} 行）:\n",
                    profile.sample_rows.len().min(3)
                ));
                summary_text.push_str(&format!("| {} |\n", block.column_names.join(" | ")));
                let sep: Vec<&str> = block.column_names.iter().map(|_| "---").collect();
                summary_text.push_str(&format!("| {} |\n", sep.join(" | ")));
                for row in profile.sample_rows.iter().take(3) {
                    summary_text.push_str(&format!("| {} |\n", row.join(" | ")));
                }
            }
            summary_text.push('\n');
        }

        let token_count = summary_text.split_whitespace().count();

        let mut extra = HashMap::new();
        extra.insert("doc_type".to_string(), "tabular".to_string());
        extra.insert("total_blocks".to_string(), parsed.blocks.len().to_string());

        // 保存第一个 profile 的 schema（供 doc-summary 使用）
        if let Some(first_profile) = parsed.profiles.first() {
            if let Ok(schema_json) = serde_json::to_string(first_profile) {
                extra.insert("table_profile".to_string(), schema_json);
            }
        }

        let summary_node = PageNode {
            node_id: "excel-summary".to_string(),
            title: format!("{} ({}个表格)", file_name, parsed.blocks.len()),
            level: 1,
            content: summary_text,
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path: file_path.clone(),
                page_number: None,
                line_number: None,
                token_count,
                extra,
            },
            children: Vec::new(),
        };

        let sheet_nodes = vec![summary_node];

        // 3. 构建根节点
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let duration = start_time.elapsed();

        let mut root_extra = HashMap::new();
        root_extra.insert(
            "processing_time_ms".to_string(),
            duration.as_millis().to_string(),
        );
        root_extra.insert(
            "processing_time_display".to_string(),
            format!("{:.2}s", duration.as_secs_f64()),
        );
        root_extra.insert("parser".to_string(), "knot-excel".to_string());
        root_extra.insert("total_sheets".to_string(), parsed.blocks.len().to_string());
        root_extra.insert("doc_type".to_string(), "tabular".to_string());

        let root = PageNode {
            node_id: "excel-root".to_string(),
            title,
            level: 0,
            content: String::new(), // 根节点不存内容，内容在子节点中
            summary: None,
            embedding: None,
            metadata: NodeMeta {
                file_path,
                page_number: None,
                line_number: None,
                token_count: 0,
                extra: root_extra,
            },
            children: sheet_nodes,
        };

        println!(
            "[ExcelParser] Built PageNode tree with {} sheet nodes (elapsed: {:.1}s)",
            parsed.blocks.len(),
            duration.as_secs_f64()
        );

        Ok(root)
    }
}
