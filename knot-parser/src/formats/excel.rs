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

        // 2. 将每个 DataBlock 的 TableProfile 转换为 PageNode
        let mut sheet_nodes: Vec<PageNode> = Vec::new();

        for (profile, block) in parsed.profiles.iter().zip(parsed.blocks.iter()) {
            let chunk_text = profile.to_chunk_text();
            let token_count = chunk_text.split_whitespace().count();

            let mut extra = HashMap::new();
            extra.insert("sheet_name".to_string(), block.sheet_name.clone());
            extra.insert("block_index".to_string(), block.block_index.to_string());
            extra.insert("row_count".to_string(), block.row_count.to_string());
            extra.insert(
                "col_count".to_string(),
                block.column_names.len().to_string(),
            );
            extra.insert("header_levels".to_string(), block.header_levels.to_string());
            extra.insert("doc_type".to_string(), "tabular".to_string());
            extra.insert("source_id".to_string(), block.source_id.clone());

            // 保存 schema 信息（JSON），供后续 Text-to-SQL 使用
            if let Ok(schema_json) = serde_json::to_string(&profile) {
                extra.insert("table_profile".to_string(), schema_json);
            }

            let node = PageNode {
                node_id: format!("excel-{}-{}", block.sheet_name, block.block_index),
                title: format!(
                    "Sheet \"{}\" ({}行×{}列)",
                    block.sheet_name,
                    block.row_count,
                    block.column_names.len()
                ),
                level: 1,
                content: chunk_text,
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

            sheet_nodes.push(node);
        }

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
