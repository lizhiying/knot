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
    const MAX_FULL_ROWS: usize = 20;
    /// 摘要中的样本行数
    const SAMPLE_ROWS: usize = 5;

    /// 处理查询结果，返回全量或摘要
    pub fn process(result: &QueryResult) -> ResultContext {
        if result.row_count <= Self::MAX_FULL_ROWS {
            ResultContext::Full {
                markdown: result.to_markdown(),
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

    /// 生成统计摘要文本
    fn generate_summary(result: &QueryResult) -> String {
        let mut text = String::new();

        text.push_str(&format!(
            "查询结果摘要（共 {} 行，因数据量大仅展示统计信息）：\n\n",
            result.row_count
        ));

        // 各列统计
        for (col_idx, col_name) in result.columns.iter().enumerate() {
            let col_values: Vec<&str> = result.rows.iter().map(|r| r[col_idx].as_str()).collect();
            let stats = Self::compute_column_stats(&col_values);
            text.push_str(&format!("- 列 \"{}\"：{}\n", col_name, stats));
        }

        // 前 N 行样本
        let sample_count = result.rows.len().min(Self::SAMPLE_ROWS);
        if sample_count > 0 {
            text.push_str(&format!("\n前 {} 行样本：\n", sample_count));
            text.push_str("| ");
            text.push_str(&result.columns.join(" | "));
            text.push_str(" |\n| ");
            text.push_str(
                &result
                    .columns
                    .iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
            text.push_str(" |\n");

            for row in result.rows.iter().take(sample_count) {
                text.push_str("| ");
                text.push_str(&row.join(" | "));
                text.push_str(" |\n");
            }
        }

        text
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
        let rows: Vec<Vec<String>> = (0..50)
            .map(|i| vec![format!("item_{}", i), format!("{}", i * 10)])
            .collect();

        let result = QueryResult {
            sql: "SELECT * FROM big_table".to_string(),
            columns: vec!["name".to_string(), "value".to_string()],
            rows,
            row_count: 50,
            retried: false,
            intermediate_steps: 0,
        };

        let ctx = ResultSummarizer::process(&result);
        assert!(ctx.is_summarized());
        assert_eq!(ctx.row_count(), 50);
        let text = ctx.to_prompt_text();
        assert!(text.contains("共 50 行"));
        assert!(text.contains("前 5 行样本"));
        println!("{}", text);
    }
}
