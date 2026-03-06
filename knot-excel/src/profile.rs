//! TableProfile 生成
//!
//! 为每个 DataBlock 生成结构化摘要文本，用于向量化索引。
//! Profile 包含：元数据、Schema（列名+类型）、数据抽样。

use crate::reader::{ColumnType, DataBlock};
use serde::{Deserialize, Serialize};

/// 数据块的结构化摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableProfile {
    /// 来源标识
    pub source_id: String,
    /// 文件路径
    pub file_path: String,
    /// Sheet 名称
    pub sheet_name: String,
    /// 数据块索引
    pub block_index: usize,
    /// 列名列表
    pub column_names: Vec<String>,
    /// 列类型
    pub column_types: Vec<String>,
    /// 行数
    pub row_count: usize,
    /// 前 N 行数据抽样
    pub sample_rows: Vec<Vec<String>>,
    /// 额外描述（如"含合并单元格"、"多级表头"等）
    pub description: String,
}

impl TableProfile {
    /// 从 DataBlock 生成 TableProfile
    pub fn from_data_block(block: &DataBlock, file_path: &str, sample_count: usize) -> Self {
        let sample_rows: Vec<Vec<String>> = block.rows.iter().take(sample_count).cloned().collect();

        let column_types: Vec<String> = block.column_types.iter().map(|t| t.to_string()).collect();

        // 构建描述
        let mut desc_parts = Vec::new();
        desc_parts.push(format!(
            "Excel 表格 [{}] Sheet \"{}\"",
            file_path.rsplit('/').next().unwrap_or(file_path),
            block.sheet_name
        ));
        desc_parts.push(format!(
            "共 {} 行 {} 列",
            block.row_count,
            block.column_names.len()
        ));

        if block.header_levels > 1 {
            desc_parts.push(format!("{}级表头", block.header_levels));
        }
        if block.merged_region_count > 0 {
            desc_parts.push(format!("含{}个合并区域", block.merged_region_count));
        }

        // 列出数值型列的简要信息
        let numeric_cols: Vec<&str> = block
            .column_names
            .iter()
            .zip(block.column_types.iter())
            .filter(|(_, t)| matches!(t, ColumnType::Int | ColumnType::Float))
            .map(|(name, _)| name.as_str())
            .collect();
        if !numeric_cols.is_empty() {
            desc_parts.push(format!("数值列: {}", numeric_cols.join(", ")));
        }

        Self {
            source_id: block.source_id.clone(),
            file_path: file_path.to_string(),
            sheet_name: block.sheet_name.clone(),
            block_index: block.block_index,
            column_names: block.column_names.clone(),
            column_types,
            row_count: block.row_count,
            sample_rows,
            description: desc_parts.join("。"),
        }
    }

    /// 生成用于向量化索引的 Chunk 文本
    ///
    /// 输出格式：
    /// ```text
    /// [表格数据] report.xlsx / Sheet "销售数据"
    /// 共 150 行 4 列
    ///
    /// 列信息:
    /// - 日期 (DateTime)
    /// - 产品 (String)
    /// - 销量 (Int64)
    /// - 金额 (Float64)
    ///
    /// 数据示例（前 3 行）:
    /// | 日期       | 产品   | 销量 | 金额     |
    /// | 2024-01-15 | 产品A  | 100  | 15000.0  |
    /// | 2024-01-16 | 产品B  | 200  | 30000.0  |
    /// | 2024-01-17 | 产品A  | 150  | 22500.0  |
    /// ```
    pub fn to_chunk_text(&self) -> String {
        let mut parts = Vec::new();

        // 标题
        let file_name = self.file_path.rsplit('/').next().unwrap_or(&self.file_path);
        parts.push(format!(
            "[表格数据] {} / Sheet \"{}\"",
            file_name, self.sheet_name
        ));
        parts.push(format!(
            "共 {} 行 {} 列",
            self.row_count,
            self.column_names.len()
        ));
        parts.push(String::new());

        // 列信息
        parts.push("列信息:".to_string());
        for (name, dtype) in self.column_names.iter().zip(self.column_types.iter()) {
            parts.push(format!("- {} ({})", name, dtype));
        }
        parts.push(String::new());

        // 数据示例
        if !self.sample_rows.is_empty() {
            parts.push(format!("数据示例（前 {} 行）:", self.sample_rows.len()));

            // 构建 Markdown 表格
            // 表头
            parts.push(format!("| {} |", self.column_names.join(" | ")));
            // 分隔行
            let sep: Vec<&str> = self.column_names.iter().map(|_| "---").collect();
            parts.push(format!("| {} |", sep.join(" | ")));
            // 数据行
            for row in &self.sample_rows {
                parts.push(format!("| {} |", row.join(" | ")));
            }
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::DataBlock;

    #[test]
    fn test_table_profile_generation() {
        let block = DataBlock {
            source_id: "test.xlsx_Sheet1_0".to_string(),
            sheet_name: "Sheet1".to_string(),
            block_index: 0,
            column_names: vec!["Name".to_string(), "Age".to_string(), "Score".to_string()],
            column_types: vec![ColumnType::String, ColumnType::Int, ColumnType::Float],
            rows: vec![
                vec!["Alice".to_string(), "25".to_string(), "92.5".to_string()],
                vec!["Bob".to_string(), "30".to_string(), "88.0".to_string()],
                vec!["Charlie".to_string(), "28".to_string(), "95.3".to_string()],
            ],
            row_count: 3,
            header_levels: 1,
            merged_region_count: 0,
        };

        let profile = TableProfile::from_data_block(&block, "/data/test.xlsx", 3);

        assert_eq!(profile.column_names.len(), 3);
        assert_eq!(profile.row_count, 3);
        assert_eq!(profile.sample_rows.len(), 3);

        let chunk = profile.to_chunk_text();
        assert!(chunk.contains("[表格数据]"));
        assert!(chunk.contains("Sheet \"Sheet1\""));
        assert!(chunk.contains("Name (String)"));
        assert!(chunk.contains("Age (Int64)"));
        assert!(chunk.contains("Alice"));
    }
}
