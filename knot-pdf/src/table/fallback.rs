//! fallback_text 生成
//!
//! 即使结构化成功也必须生成 fallback_text，确保信息不丢。
//! 支持两种格式：
//! - KV 行格式：`列名=值` 逐行输出
//! - KV lines 格式：每个单元格一行

use crate::ir::TableRow;

/// 生成 fallback_text（KV 行格式）
///
/// 格式示例：
/// ```text
/// [表t0_0 页0]
/// 行1: 年份=2023 收入=1234 支出=567
/// 行2: 年份=2022 收入=1100 支出=480
/// ```
pub fn generate_fallback_text(
    headers: &[String],
    rows: &[TableRow],
    table_id: &str,
    page_index: usize,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("[表{} 页{}]", table_id, page_index));

    for row in rows {
        let mut parts = vec![format!("行{}", row.row_index + 1)];
        for cell in &row.cells {
            let col_name = headers
                .get(cell.col)
                .cloned()
                .unwrap_or_else(|| format!("col_{}", cell.col));
            if !cell.text.is_empty() {
                parts.push(format!("{}={}", col_name, cell.text));
            }
        }
        if parts.len() > 1 {
            lines.push(format!("{}: {}", parts[0], parts[1..].join(" ")));
        }
    }

    lines.join("\n")
}

/// 生成 table_as_kv_lines（单元格粒度）
///
/// 格式：`表=T1 页=3 行=2 列=收入 值=1234`
pub fn generate_kv_lines(
    headers: &[String],
    rows: &[TableRow],
    table_id: &str,
    page_index: usize,
) -> Vec<String> {
    let mut result = Vec::new();
    for row in rows {
        for cell in &row.cells {
            let col_name = headers
                .get(cell.col)
                .cloned()
                .unwrap_or_else(|| format!("col_{}", cell.col));
            result.push(format!(
                "表={} 页={} 行={} 列={} 值={} 类型={:?}",
                table_id,
                page_index,
                cell.row + 1,
                col_name,
                cell.text,
                cell.cell_type
            ));
        }
    }
    result
}

/// 生成 table_row_lines（行级 KV 格式）
///
/// 格式：`表=T1 页=3 行key=2023年 列=收入 值=1234 列=支出 值=567`
pub fn generate_row_lines(
    headers: &[String],
    rows: &[TableRow],
    table_id: &str,
    page_index: usize,
) -> Vec<String> {
    let mut result = Vec::new();
    for row in rows {
        let mut parts = vec![format!("表={}", table_id), format!("页={}", page_index)];

        // 使用第一列的值作为 row key
        let row_key = row
            .cells
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_else(|| format!("{}", row.row_index + 1));
        parts.push(format!("行key={}", row_key));

        for cell in &row.cells {
            let col_name = headers
                .get(cell.col)
                .cloned()
                .unwrap_or_else(|| format!("col_{}", cell.col));
            parts.push(format!("列={} 值={}", col_name, cell.text));
        }

        result.push(parts.join(" "));
    }
    result
}
