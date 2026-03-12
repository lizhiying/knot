//! 查询结果膨胀控制
//!
//! 小结果（≤20 行）全量返回 Markdown 表格，
//! 大结果（>20 行）生成统计摘要（行数、各列统计、前 5 行样本）。

use super::engine::QueryResult;

/// 结果上下文（传递给 LLM 的内容）
#[derive(Debug, Clone)]
pub enum ResultContext {
    /// 结果不大（≤ MAX_FULL_ROWS 行），全量传递
    Full { markdown: String, row_count: usize },
    /// 结果过大，传递统计摘要
    Summary {
        summary_text: String,
        row_count: usize,
    },
}

impl ResultContext {
    /// 获取适合注入 LLM Prompt 的文本
    pub fn to_prompt_text(&self) -> String {
        match self {
            ResultContext::Full { markdown, .. } => markdown.clone(),
            ResultContext::Summary { summary_text, .. } => summary_text.clone(),
        }
    }

    /// 是否被摘要化了
    pub fn is_summarized(&self) -> bool {
        matches!(self, ResultContext::Summary { .. })
    }

    /// 结果行数
    pub fn row_count(&self) -> usize {
        match self {
            ResultContext::Full { row_count, .. } => *row_count,
            ResultContext::Summary { row_count, .. } => *row_count,
        }
    }
}

/// 结果摘要器
pub struct ResultSummarizer;

impl ResultSummarizer {
    /// 全量返回的最大行数阈值
    const MAX_FULL_ROWS: usize = 100;
    /// 全量返回时的最大列数（超过则截断）
    const MAX_FULL_COLS: usize = 8;
    /// 摘要中的样本行数
    const SAMPLE_ROWS: usize = 10;

    /// 处理查询结果，返回全量或摘要
    pub fn process(result: &QueryResult) -> ResultContext {
        if result.row_count <= Self::MAX_FULL_ROWS {
            // 列数过多时截断
            let markdown = if result.columns.len() > Self::MAX_FULL_COLS {
                Self::to_markdown_truncated_cols(result, Self::MAX_FULL_COLS)
            } else {
                result.to_markdown()
            };
            ResultContext::Full {
                markdown,
                row_count: result.row_count,
            }
        } else {
            let summary_text = Self::generate_summary(result);
            ResultContext::Summary {
                summary_text,
                row_count: result.row_count,
            }
        }
    }

    /// 仅根据总行数判断是否摘要化（用于分页场景，不需要完整数据）
    pub fn process_with_count(total_count: usize) -> ResultContext {
        if total_count <= Self::MAX_FULL_ROWS {
            ResultContext::Full {
                markdown: String::new(),
                row_count: total_count,
            }
        } else {
            ResultContext::Summary {
                summary_text: format!("查询结果共 {} 行，已分页展示。", total_count),
                row_count: total_count,
            }
        }
    }

    /// 生成统计摘要文本
    fn generate_summary(result: &QueryResult) -> String {
        let mut text = String::new();

        text.push_str(&format!(
            "查询结果（共 {} 行，展示前 {} 行样本）：\n\n",
            result.row_count,
            result.rows.len().min(Self::SAMPLE_ROWS)
        ));

        // 列数过多时限制显示列
        let max_summary_cols = Self::MAX_FULL_COLS;
        let display_cols = result.columns.len().min(max_summary_cols);
        let truncated_cols = result.columns.len() > max_summary_cols;

        // 各列统计
        for col_idx in 0..display_cols {
            let col_name = &result.columns[col_idx];
            let col_values: Vec<&str> = result.rows.iter().map(|r| r[col_idx].as_str()).collect();
            let stats = Self::compute_column_stats(&col_values);
            text.push_str(&format!("- 列 \"{}\"：{}\n", col_name, stats));
        }
        if truncated_cols {
            text.push_str(&format!(
                "  ... 还有 {} 列未显示\n",
                result.columns.len() - display_cols
            ));
        }

        // 样本行
        let sample_count = result.rows.len().min(Self::SAMPLE_ROWS);
        if sample_count > 0 {
            text.push_str(&format!("\n前 {} 行样本：\n", sample_count));
            text.push_str("| ");
            text.push_str(&result.columns[..display_cols].join(" | "));
            if truncated_cols {
                text.push_str(" | ...");
            }
            text.push_str(" |\n| ");
            text.push_str(
                &(0..display_cols)
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
            if truncated_cols {
                text.push_str(" | ---");
            }
            text.push_str(" |\n");

            for row in result.rows.iter().take(sample_count) {
                text.push_str("| ");
                text.push_str(&row[..display_cols].join(" | "));
                if truncated_cols {
                    text.push_str(" | ...");
                }
                text.push_str(" |\n");
            }

            if result.row_count > sample_count {
                text.push_str(&format!(
                    "\n... 还有 {} 行未显示\n",
                    result.row_count - sample_count
                ));
            }
        }

        text
    }

    /// 列数过多时截断列数的 Markdown 生成
    fn to_markdown_truncated_cols(result: &QueryResult, max_cols: usize) -> String {
        let display_cols = result.columns.len().min(max_cols);
        let truncated = result.columns.len() > max_cols;

        let mut md = String::new();
        // 表头
        md.push_str("| ");
        md.push_str(&result.columns[..display_cols].join(" | "));
        if truncated {
            md.push_str(&format!(
                " | ...({} 列省略)",
                result.columns.len() - display_cols
            ));
        }
        md.push_str(" |\n| ");
        md.push_str(
            &(0..display_cols)
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | "),
        );
        if truncated {
            md.push_str(" | ---");
        }
        md.push_str(" |\n");

        // 数据行
        for row in &result.rows {
            md.push_str("| ");
            md.push_str(&row[..display_cols.min(row.len())].join(" | "));
            if truncated {
                md.push_str(" | ...");
            }
            md.push_str(" |\n");
        }

        md
    }

    /// 计算单列的统计信息
    fn compute_column_stats(values: &[&str]) -> String {
        let non_null: Vec<&&str> = values
            .iter()
            .filter(|v| !v.is_empty() && **v != "NULL")
            .collect();
        let null_count = values.len() - non_null.len();

        // 尝试识别是否为数值列
        let numeric_values: Vec<f64> = non_null
            .iter()
            .filter_map(|v| v.parse::<f64>().ok())
            .collect();

        if numeric_values.len() as f64 > non_null.len() as f64 * 0.8 && !numeric_values.is_empty() {
            // 数值列
            let min = numeric_values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numeric_values
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            let sum: f64 = numeric_values.iter().sum();
            let avg = sum / numeric_values.len() as f64;

            let mut stats = format!("数值, min={:.2}, max={:.2}, avg={:.2}", min, max, avg);
            if null_count > 0 {
                stats.push_str(&format!(", {} 个空值", null_count));
            }
            stats
        } else {
            // 文本列
            let mut unique: Vec<&str> = non_null.iter().map(|v| **v).collect();
            unique.sort_unstable();
            unique.dedup();
            let distinct_count = unique.len();

            let mut stats = format!("文本, {} 个不同值", distinct_count);
            if null_count > 0 {
                stats.push_str(&format!(", {} 个空值", null_count));
            }
            stats
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_result_full() {
        let result = QueryResult {
            sql: "SELECT 1".to_string(),
            columns: vec!["a".to_string(), "b".to_string()],
            rows: vec![
                vec!["hello".to_string(), "100".to_string()],
                vec!["world".to_string(), "200".to_string()],
            ],
            row_count: 2,
            retried: false,
            intermediate_steps: 0,
        };

        let ctx = ResultSummarizer::process(&result);
        assert!(!ctx.is_summarized());
        assert_eq!(ctx.row_count(), 2);
        assert!(ctx.to_prompt_text().contains("hello"));
    }

    #[test]
    fn test_large_result_summarized() {
        let rows: Vec<Vec<String>> = (0..150)
            .map(|i| vec![format!("item_{}", i), format!("{}", i * 10)])
            .collect();

        let result = QueryResult {
            sql: "SELECT * FROM big_table".to_string(),
            columns: vec!["name".to_string(), "value".to_string()],
            rows,
            row_count: 150,
            retried: false,
            intermediate_steps: 0,
        };

        let ctx = ResultSummarizer::process(&result);
        assert!(ctx.is_summarized());
        assert_eq!(ctx.row_count(), 150);
        let text = ctx.to_prompt_text();
        assert!(text.contains("共 150 行"));
        assert!(text.contains("前 10 行样本"));
        println!("{}", text);
    }
}
