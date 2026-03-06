//! Excel 数据读取与 DataFrame 构建
//!
//! 使用 calamine 读取 .xlsx / .xls 文件，自动推断列类型，
//! 清洗空行/空列，构建结构化 DataBlock。

use crate::config::ExcelConfig;
use crate::error::ExcelError;
use calamine::{open_workbook_auto, Data, Range, Reader};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 推断出的列类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnType {
    String,
    Int,
    Float,
    Bool,
    DateTime,
    Empty,
}

impl std::fmt::Display for ColumnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnType::String => write!(f, "String"),
            ColumnType::Int => write!(f, "Int64"),
            ColumnType::Float => write!(f, "Float64"),
            ColumnType::Bool => write!(f, "Bool"),
            ColumnType::DateTime => write!(f, "DateTime"),
            ColumnType::Empty => write!(f, "Empty"),
        }
    }
}

/// 解析后的单个数据块
#[derive(Debug, Clone)]
pub struct DataBlock {
    /// 来源标识：file_path + sheet_name + block_index
    pub source_id: String,
    /// Sheet 名称
    pub sheet_name: String,
    /// 数据块在 Sheet 中的索引（0-based）
    pub block_index: usize,
    /// 列名列表
    pub column_names: Vec<String>,
    /// 列类型列表
    pub column_types: Vec<ColumnType>,
    /// 数据行（每行是一个 Vec<String>，统一转为字符串表示）
    pub rows: Vec<Vec<String>>,
    /// 行数
    pub row_count: usize,
    /// 原始表头层级数（1 = 标准单行表头）
    pub header_levels: usize,
    /// 合并单元格区域数量
    pub merged_region_count: usize,
}

/// 读取 Excel 文件，返回所有数据块
pub fn read_excel<P: AsRef<Path>>(
    path: P,
    config: &ExcelConfig,
) -> Result<Vec<DataBlock>, ExcelError> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !matches!(ext.as_str(), "xlsx" | "xls" | "xlsm" | "xlsb" | "ods") {
        return Err(ExcelError::UnsupportedFormat(ext));
    }

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ExcelError::Calamine(format!("Failed to open {}: {}", path.display(), e)))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

    if sheet_names.is_empty() {
        return Err(ExcelError::EmptyFile);
    }

    let mut all_blocks = Vec::new();
    let file_path = path.to_string_lossy().to_string();

    for sheet_name in &sheet_names {
        let range: Range<Data> = match workbook.worksheet_range(sheet_name) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Skipping sheet '{}': {}", sheet_name, e);
                continue;
            }
        };

        if range.is_empty() {
            log::debug!("Sheet '{}' is empty, skipping", sheet_name);
            continue;
        }

        match parse_sheet_to_block(&range, &file_path, sheet_name, 0, config) {
            Ok(block) => {
                if !block.column_names.is_empty() && block.row_count > 0 {
                    all_blocks.push(block);
                }
            }
            Err(e) => {
                log::warn!("Failed to parse sheet '{}': {}", sheet_name, e);
            }
        }
    }

    if all_blocks.is_empty() {
        return Err(ExcelError::EmptyFile);
    }

    println!(
        "[ExcelReader] Parsed {} data blocks from {}",
        all_blocks.len(),
        path.display()
    );

    Ok(all_blocks)
}

/// 将一个 Sheet 的 Range 解析为 DataBlock
fn parse_sheet_to_block(
    range: &Range<Data>,
    file_path: &str,
    sheet_name: &str,
    block_index: usize,
    config: &ExcelConfig,
) -> Result<DataBlock, ExcelError> {
    let (height, width) = range.get_size();

    if height == 0 || width == 0 {
        return Err(ExcelError::Parse("Empty range".to_string()));
    }

    // 限制列数
    let width = width.min(config.max_columns);

    // 1. 提取表头（第一行）
    let mut column_names = Vec::with_capacity(width);
    for col in 0..width {
        let cell = range.get((0, col));
        let name = match cell {
            Some(data) => cell_to_string(data),
            None => String::new(),
        };
        let name = if name.trim().is_empty() {
            format!("Column_{}", col + 1)
        } else {
            name.trim().to_string()
        };
        column_names.push(name);
    }

    // 确保列名唯一
    deduplicate_column_names(&mut column_names);

    // 2. 提取数据行（从第二行开始）
    let max_data_rows = (height - 1).min(config.max_rows);
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(max_data_rows);
    let mut consecutive_empty = 0;

    for row_idx in 1..=max_data_rows {
        let mut row_data = Vec::with_capacity(width);
        let mut is_empty = true;

        for col in 0..width {
            let cell = range.get((row_idx, col));
            let value = match cell {
                Some(data) => {
                    let s = cell_to_string(data);
                    if !s.trim().is_empty() {
                        is_empty = false;
                    }
                    s
                }
                None => String::new(),
            };
            row_data.push(value);
        }

        if is_empty {
            consecutive_empty += 1;
            if consecutive_empty >= config.max_empty_rows {
                break;
            }
            // 跳过全空行
            continue;
        } else {
            consecutive_empty = 0;
        }

        rows.push(row_data);
    }

    // 3. Drop 全空列
    let non_empty_cols = find_non_empty_columns(&column_names, &rows);
    if non_empty_cols.len() < column_names.len() {
        let new_names: Vec<String> = non_empty_cols
            .iter()
            .map(|&i| column_names[i].clone())
            .collect();
        let new_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| non_empty_cols.iter().map(|&i| row[i].clone()).collect())
            .collect();
        let row_count = new_rows.len();

        // 4. 推断列类型
        let column_types = infer_column_types(&new_rows, new_names.len());

        let source_id = format!("{}_{}_{}", file_path, sheet_name, block_index);

        return Ok(DataBlock {
            source_id,
            sheet_name: sheet_name.to_string(),
            block_index,
            column_names: new_names,
            column_types,
            rows: new_rows,
            row_count,
            header_levels: 1,
            merged_region_count: 0,
        });
    }

    let row_count = rows.len();
    let column_types = infer_column_types(&rows, column_names.len());

    let source_id = format!("{}_{}_{}", file_path, sheet_name, block_index);

    Ok(DataBlock {
        source_id,
        sheet_name: sheet_name.to_string(),
        block_index,
        column_names,
        column_types,
        rows,
        row_count,
        header_levels: 1,
        merged_region_count: 0,
    })
}

/// 将 calamine Data 转换为字符串
fn cell_to_string(data: &Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            // 如果浮点数实际上是整数（如 2024.0），去掉小数部分
            if f.fract() == 0.0 && f.abs() < i64::MAX as f64 {
                (*f as i64).to_string()
            } else {
                format!("{:.6}", f)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            }
        }
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => {
            // calamine 的 DateTime 是 ExcelDateTime，通过 as_f64() 获取序列号
            let serial = dt.as_f64();
            if let Some(naive) = excel_serial_to_date(serial) {
                naive
            } else {
                format!("{}", serial)
            }
        }
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#ERR:{:?}", e),
    }
}

/// Excel 序列号转日期字符串
fn excel_serial_to_date(serial: f64) -> Option<String> {
    if serial < 1.0 {
        return None;
    }

    // Excel epoch: 1899-12-30 (day 0)
    // Day 1 = 1900-01-01
    // Excel has a bug: day 60 = 1900-02-29 (doesn't exist)
    // For serial > 60, subtract 1 to account for this

    let days = serial.trunc() as i64;
    let time_frac = serial.fract();

    // Convert days since 1899-12-31 (Excel day 1 = 1900-01-01)
    // Adjust for Excel leap year bug (serial 60 = Feb 29, 1900 which doesn't exist)
    // Unix epoch (1970-01-01) in Excel serial: 25569
    // So: unix_days = serial - 25569 (for serial > 60)
    let unix_days = if days > 59 {
        days - 25569
    } else {
        days - 25568
    };

    // Convert unix_days to y/m/d using civil_from_days algorithm
    // (from Howard Hinnant's date library)
    let (year, month, day) = civil_from_days(unix_days)?;

    if time_frac > 0.0001 {
        let total_seconds = (time_frac * 86400.0).round() as u32;
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        Some(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hours, minutes, seconds
        ))
    } else {
        Some(format!("{:04}-{:02}-{:02}", year, month, day))
    }
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day)
/// Based on Howard Hinnant's civil_from_days algorithm
fn civil_from_days(z: i64) -> Option<(i32, u32, u32)> {
    let z = z + 719468; // shift epoch from 1970-01-01 to 0000-03-01
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month period [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    if m >= 1 && m <= 12 && d >= 1 && d <= 31 {
        Some((y, m, d))
    } else {
        None
    }
}

/// 确保列名唯一（重复的加后缀 _2, _3, ...）
fn deduplicate_column_names(names: &mut Vec<String>) {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for i in 0..names.len() {
        let name = names[i].clone();
        let count = seen.entry(name.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            names[i] = format!("{}_{}", name, count);
        }
    }
}

/// 找出非全空列的索引
fn find_non_empty_columns(headers: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut non_empty = Vec::new();
    for col in 0..headers.len() {
        // 表头非空即保留
        if !headers[col].starts_with("Column_") {
            non_empty.push(col);
            continue;
        }
        // 检查数据列是否有值
        let has_data = rows
            .iter()
            .any(|row| col < row.len() && !row[col].trim().is_empty());
        if has_data {
            non_empty.push(col);
        }
    }
    non_empty
}

/// 推断列类型：遍历数据行，通过值的格式判断类型
fn infer_column_types(rows: &[Vec<String>], num_cols: usize) -> Vec<ColumnType> {
    let mut types = vec![ColumnType::Empty; num_cols];

    for col in 0..num_cols {
        let mut has_int = false;
        let mut has_float = false;
        let mut has_bool = false;
        let mut has_date = false;
        let mut has_string = false;
        let mut non_empty_count = 0;

        for row in rows {
            if col >= row.len() {
                continue;
            }
            let val = row[col].trim();
            if val.is_empty() {
                continue;
            }
            non_empty_count += 1;

            if val == "true" || val == "false" || val == "TRUE" || val == "FALSE" {
                has_bool = true;
            } else if val.parse::<i64>().is_ok() {
                has_int = true;
            } else if val.parse::<f64>().is_ok() {
                has_float = true;
            } else if looks_like_date(val) {
                has_date = true;
            } else {
                has_string = true;
            }
        }

        if non_empty_count == 0 {
            types[col] = ColumnType::Empty;
        } else if has_string {
            // 只要有一个非数值/日期/布尔的值，就视为 String
            types[col] = ColumnType::String;
        } else if has_date && !has_int && !has_float && !has_bool {
            types[col] = ColumnType::DateTime;
        } else if has_bool && !has_int && !has_float && !has_date {
            types[col] = ColumnType::Bool;
        } else if has_float || (has_int && has_float) {
            types[col] = ColumnType::Float;
        } else if has_int {
            types[col] = ColumnType::Int;
        } else {
            types[col] = ColumnType::String;
        }
    }

    types
}

/// 简单的日期格式检测
fn looks_like_date(s: &str) -> bool {
    // 匹配 YYYY-MM-DD, YYYY/MM/DD, YYYY-MM-DD HH:MM:SS 等
    let s = s.trim();
    if s.len() < 8 {
        return false;
    }

    // YYYY-MM-DD 或 YYYY/MM/DD
    if s.len() >= 10 {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() >= 10 && chars[4] == '-' || chars[4] == '/' {
            let year_part = &s[..4];
            let month_part = &s[5..7];
            let day_part = &s[8..10];
            return year_part.parse::<u16>().is_ok()
                && month_part.parse::<u8>().is_ok()
                && day_part.parse::<u8>().is_ok();
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_to_string() {
        assert_eq!(cell_to_string(&Data::Empty), "");
        assert_eq!(cell_to_string(&Data::String("hello".to_string())), "hello");
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::Float(3.14)), "3.14");
        assert_eq!(cell_to_string(&Data::Float(2024.0)), "2024");
        assert_eq!(cell_to_string(&Data::Bool(true)), "true");
    }

    #[test]
    fn test_deduplicate_column_names() {
        let mut names = vec![
            "Name".to_string(),
            "Value".to_string(),
            "Name".to_string(),
            "Value".to_string(),
            "Name".to_string(),
        ];
        deduplicate_column_names(&mut names);
        assert_eq!(names, vec!["Name", "Value", "Name_2", "Value_2", "Name_3"]);
    }

    #[test]
    fn test_infer_column_types() {
        let rows = vec![
            vec![
                "Alice".to_string(),
                "25".to_string(),
                "3.14".to_string(),
                "true".to_string(),
                "2024-01-15".to_string(),
            ],
            vec![
                "Bob".to_string(),
                "30".to_string(),
                "2.71".to_string(),
                "false".to_string(),
                "2024-02-20".to_string(),
            ],
        ];
        let types = infer_column_types(&rows, 5);
        assert_eq!(types[0], ColumnType::String);
        assert_eq!(types[1], ColumnType::Int);
        assert_eq!(types[2], ColumnType::Float);
        assert_eq!(types[3], ColumnType::Bool);
        assert_eq!(types[4], ColumnType::DateTime);
    }

    #[test]
    fn test_looks_like_date() {
        assert!(looks_like_date("2024-01-15"));
        assert!(looks_like_date("2024/01/15"));
        assert!(looks_like_date("2024-01-15 10:30:00"));
        assert!(!looks_like_date("hello"));
        assert!(!looks_like_date("123"));
        assert!(!looks_like_date(""));
    }

    #[test]
    fn test_excel_serial_to_date() {
        // 2024-01-01 = serial 45292
        let date = excel_serial_to_date(45292.0);
        assert!(date.is_some());
        assert_eq!(date.unwrap(), "2024-01-01");
    }
}
