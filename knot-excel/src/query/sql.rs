//! SQL Prompt 生成器
//!
//! 将用户 Query + 表 Schema 信息组装为 LLM Prompt，引导 LLM 生成 DuckDB SQL。

use super::engine::TableSchema;
use crate::reader::DataBlock;

/// SQL 生成器 — 构建 LLM Prompt
pub struct SqlGenerator;

impl SqlGenerator {
    /// 构建 Text-to-SQL 的 System Prompt
    pub fn build_system_prompt() -> String {
        r#"你是一个专业的 SQL 分析师。根据以下表结构和用户问题，生成一条兼容 DuckDB 语法的 SQL 查询语句。

## 要求
1. 仅返回 SQL 语句本身，不要任何解释、注释或 markdown 标记
2. 使用 DuckDB 兼容语法
3. 列名需用双引号包裹（尤其是中文列名）
4. **只 SELECT 用户问题涉及的列，禁止使用 SELECT ***
5. 优先使用 WITH (CTE) 或子查询实现多步逻辑，避免拆分为多条 SQL
6. DuckDB 完整支持 CTE、窗口函数（ROW_NUMBER, LAG, LEAD）、QUALIFY 子句
7. 如果需要聚合，使用恰当的 GROUP BY
8. 结果列名应有意义（使用 AS 别名）
9. 不要加 LIMIT 限制，除非用户明确要求"只看几条"
10. **重要：如果多个表都包含用户问题所需的列，必须使用 UNION ALL 合并所有表的数据，确保不遗漏**
11. 优先查询行数最多的表（行数已在表名后标注）"#
            .to_string()
    }

    /// 构建包含表 Schema 和用户问题的 User Prompt
    pub fn build_user_prompt(
        schemas: &[TableSchema],
        blocks: &[DataBlock],
        user_query: &str,
    ) -> String {
        let mut prompt = String::new();

        prompt.push_str("## 可用表\n\n");

        for schema in schemas {
            prompt.push_str(&format!("### 表名: `{}`\n", schema.table_name));

            // 列信息
            prompt.push_str("列信息:\n");
            for (col_name, col_type) in &schema.columns {
                prompt.push_str(&format!("  - \"{}\" ({})\n", col_name, col_type));
            }

            // 查找对应的 DataBlock 获取数据示例
            if let Some(block) = blocks.iter().find(|b| b.source_id == schema.source_id) {
                let sample_count = block.rows.len().min(3);
                if sample_count > 0 {
                    prompt.push_str(&format!(
                        "\n数据示例（前 {} 行，共 {} 行）:\n",
                        sample_count, block.row_count
                    ));

                    // Markdown 表头
                    prompt.push_str("| ");
                    prompt.push_str(&block.column_names.join(" | "));
                    prompt.push_str(" |\n| ");
                    prompt.push_str(
                        &block
                            .column_names
                            .iter()
                            .map(|_| "---")
                            .collect::<Vec<_>>()
                            .join(" | "),
                    );
                    prompt.push_str(" |\n");

                    for row in block.rows.iter().take(sample_count) {
                        prompt.push_str("| ");
                        prompt.push_str(&row.join(" | "));
                        prompt.push_str(" |\n");
                    }
                }
            }

            prompt.push('\n');
        }

        prompt.push_str(&format!("## 用户问题\n{}\n", user_query));

        prompt
    }

    /// 构建 SQL 修复 Prompt（当 SQL 执行失败时）
    pub fn build_fix_prompt(
        original_sql: &str,
        error_message: &str,
        schemas: &[TableSchema],
    ) -> String {
        let mut prompt = String::new();

        prompt.push_str("之前生成的 SQL 执行失败，请修复。\n\n");
        prompt.push_str(&format!("## 原始 SQL\n```sql\n{}\n```\n\n", original_sql));
        prompt.push_str(&format!("## 错误信息\n{}\n\n", error_message));

        prompt.push_str("## 表结构\n");
        for schema in schemas {
            prompt.push_str(&format!("表 `{}` 的列:\n", schema.table_name));
            for (col_name, col_type) in &schema.columns {
                prompt.push_str(&format!("  - \"{}\" ({})\n", col_name, col_type));
            }
            prompt.push('\n');
        }

        prompt.push_str("请仅返回修复后的 SQL 语句，不要任何解释。\n");

        prompt
    }

    /// 规则引擎：尝试自动生成 SQL（无需 LLM）
    ///
    /// 对于简单查询（如"列出所有的销售单号"），直接匹配列名生成 SQL。
    /// 返回 Some(sql) 表示成功，None 表示无法自动生成（需要 LLM）。
    pub fn try_auto_sql(
        query: &str,
        schemas: &[TableSchema],
        table_row_counts: &[(usize, usize)], // (schema_index, row_count)
    ) -> Option<String> {
        // 提取 query 中的关键词（去掉常见的动词/助词）
        let stop_words = [
            "列出", "显示", "查看", "查询", "所有", "全部", "的", "了", "吗", "把", "给", "我",
            "一下", "下", "有", "哪些", "什么", "多少", "是", "在", "请", "帮", "看看", "找",
            "搜索", "出来",
        ];

        let query_keywords: Vec<&str> = query
            .split(|c: char| c.is_whitespace() || c == '，' || c == '、' || c == '？')
            .filter(|w| !w.is_empty() && !stop_words.contains(w))
            .collect();

        if query_keywords.is_empty() {
            return None;
        }

        // 在所有非空表中查找匹配的列
        let mut matches: Vec<(usize, String, String)> = Vec::new(); // (schema_idx, table_name, col_name)

        for &(schema_idx, row_count) in table_row_counts {
            if row_count == 0 {
                continue;
            }
            let schema = &schemas[schema_idx];
            for (col_name, _col_type) in &schema.columns {
                // 检查 query 关键词是否匹配列名
                let col_lower = col_name.to_lowercase();
                for keyword in &query_keywords {
                    let kw_lower = keyword.to_lowercase();
                    if col_lower.contains(&kw_lower) || kw_lower.contains(&col_lower) {
                        matches.push((schema_idx, schema.table_name.clone(), col_name.clone()));
                        break;
                    }
                }
            }
        }

        if matches.is_empty() {
            return None;
        }

        // 去重：同表同列只保留一次
        matches.sort_by(|a, b| (&a.1, &a.2).cmp(&(&b.1, &b.2)));
        matches.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2);

        // 检测是否是"列出/显示"类型的查询（只需 SELECT DISTINCT）
        let is_list_query = query.contains("列出")
            || query.contains("显示")
            || query.contains("所有")
            || query.contains("全部")
            || query.contains("哪些")
            || query.contains("查看");

        if !is_list_query {
            // 非列表查询（如聚合、条件过滤等），交给 LLM
            return None;
        }

        // 按表分组
        let mut table_cols: Vec<(String, Vec<String>)> = Vec::new();
        for (_, table_name, col_name) in &matches {
            if let Some(entry) = table_cols.iter_mut().find(|(t, _)| t == table_name) {
                if !entry.1.contains(col_name) {
                    entry.1.push(col_name.clone());
                }
            } else {
                table_cols.push((table_name.clone(), vec![col_name.clone()]));
            }
        }

        if table_cols.len() == 1 {
            // 单表：简单 SELECT DISTINCT
            let (table_name, cols) = &table_cols[0];
            let col_list: Vec<String> = cols.iter().map(|c| format!("\"{}\"", c)).collect();
            Some(format!(
                "SELECT DISTINCT {} FROM \"{}\"",
                col_list.join(", "),
                table_name
            ))
        } else {
            // 多表：UNION ALL（只取第一个匹配列名，确保列数一致）
            // 找出所有表共有的列名
            let first_cols = &table_cols[0].1;
            let common_col = first_cols.first()?;

            let parts: Vec<String> = table_cols
                .iter()
                .filter(|(table_name, cols)| {
                    cols.contains(common_col) && {
                        // 确保这个表有足够的行数（> 0）
                        let _idx = schemas.iter().position(|s| &s.table_name == table_name);
                        true
                    }
                })
                .map(|(table_name, _)| {
                    format!("SELECT DISTINCT \"{}\" FROM \"{}\"", common_col, table_name)
                })
                .collect();

            if parts.len() == 1 {
                Some(parts[0].clone())
            } else {
                Some(parts.join(" UNION ALL "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt() {
        let prompt = SqlGenerator::build_system_prompt();
        assert!(prompt.contains("DuckDB"));
        assert!(prompt.contains("CTE"));
        assert!(prompt.contains("双引号"));
    }

    #[test]
    fn test_user_prompt() {
        let schemas = vec![TableSchema {
            table_name: "sales".to_string(),
            source_id: "test_0".to_string(),
            columns: vec![
                ("产品".to_string(), "VARCHAR".to_string()),
                ("销量".to_string(), "BIGINT".to_string()),
            ],
        }];

        let blocks = vec![DataBlock {
            source_id: "test_0".to_string(),
            sheet_name: "Sheet1".to_string(),
            block_index: 0,
            column_names: vec!["产品".to_string(), "销量".to_string()],
            column_types: vec![
                crate::reader::ColumnType::String,
                crate::reader::ColumnType::Int,
            ],
            rows: vec![
                vec!["产品A".to_string(), "100".to_string()],
                vec!["产品B".to_string(), "200".to_string()],
            ],
            row_count: 2,
            header_levels: 1,
            merged_region_count: 0,
        }];

        let prompt = SqlGenerator::build_user_prompt(&schemas, &blocks, "总销量是多少");
        assert!(prompt.contains("sales"));
        assert!(prompt.contains("产品A"));
        assert!(prompt.contains("总销量是多少"));
    }

    #[test]
    fn test_fix_prompt() {
        let schemas = vec![TableSchema {
            table_name: "sales".to_string(),
            source_id: "test".to_string(),
            columns: vec![("产品".to_string(), "VARCHAR".to_string())],
        }];

        let prompt = SqlGenerator::build_fix_prompt(
            "SELECT * FROM wrong_table",
            "Table 'wrong_table' not found",
            &schemas,
        );
        assert!(prompt.contains("wrong_table"));
        assert!(prompt.contains("not found"));
        assert!(prompt.contains("sales"));
    }
}
