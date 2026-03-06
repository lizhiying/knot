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

        // 4.1 多数据块切割：检测空白楚河汉界
        let block_ranges = split_sheet_into_ranges(&range, config);

        if block_ranges.len() > 1 {
            println!(
                "[ExcelReader] Sheet '{}' contains {} data blocks",
                sheet_name,
                block_ranges.len()
            );
        }

        for (block_idx, (start_row, end_row)) in block_ranges.iter().enumerate() {
            // 创建子范围的虚拟 Range：通过偏移传递给 parse_sheet_to_block
            match parse_sheet_to_block_with_offset(
                &range, &file_path, sheet_name, block_idx, *start_row, *end_row, config,
            ) {
                Ok(block) => {
                    if !block.column_names.is_empty() && block.row_count > 0 {
                        all_blocks.push(block);
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to parse sheet '{}' block {}: {}",
                        sheet_name,
                        block_idx,
                        e
                    );
                }
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

/// 检测 Sheet 中的数据块边界（空白楚河汉界模式）
/// 返回 Vec<(start_row, end_row)>，每个元素代表一个数据块的行范围（包含 end_row）
fn split_sheet_into_ranges(range: &Range<Data>, config: &ExcelConfig) -> Vec<(usize, usize)> {
    let (height, width) = range.get_size();
    let width = width.min(config.max_columns);

    if height == 0 {
        return vec![];
    }

    let mut blocks = Vec::new();
    let mut block_start: Option<usize> = None;
    let mut consecutive_empty = 0;
    let empty_threshold = 2; // 连续 2 行以上空行才认为是分隔

    for row_idx in 0..height {
        let is_empty = (0..width).all(|col| match range.get((row_idx, col)) {
            Some(data) => cell_to_string(data).trim().is_empty(),
            None => true,
        });

        if is_empty {
            consecutive_empty += 1;
            if consecutive_empty >= empty_threshold {
                // 结束当前数据块
                if let Some(start) = block_start {
                    let end = row_idx - consecutive_empty; // 回退到最后一个非空行
                    if end >= start {
                        blocks.push((start, end));
                    }
                    block_start = None;
                }
            }
        } else {
            if block_start.is_none() {
                block_start = Some(row_idx);
            }
            consecutive_empty = 0;
        }
    }

    // 处理最后一个数据块
    if let Some(start) = block_start {
        blocks.push((start, height - 1));
    }

    // 如果没有检测到分隔（只有一个数据块），返回整个 Sheet
    if blocks.is_empty() {
        blocks.push((0, height - 1));
    }

    blocks
}

/// 带行偏移的 Sheet 解析（用于多数据块场景）
fn parse_sheet_to_block_with_offset(
    range: &Range<Data>,
    file_path: &str,
    sheet_name: &str,
    block_index: usize,
    offset_start: usize,
    offset_end: usize,
    config: &ExcelConfig,
) -> Result<DataBlock, ExcelError> {
    let (_, total_width) = range.get_size();
    let width = total_width.min(config.max_columns);
    let sub_height = offset_end - offset_start + 1;

    if sub_height < 2 || width == 0 {
        return Err(ExcelError::Parse("Block too small".to_string()));
    }

    // 在子范围内检测数据起始行（跳过说明行）
    let local_data_start = if config.enable_dirty_row_filter {
        detect_data_start_in_range(
            range,
            offset_start,
            offset_end,
            width,
            config.max_header_rows,
        )
    } else {
        offset_start
    };

    let remaining = offset_end - local_data_start + 1;
    if remaining < 2 {
        return Err(ExcelError::Parse(
            "Not enough rows after data start".to_string(),
        ));
    }

    // 多级表头检测
    let header_levels =
        detect_header_rows_in_range(range, local_data_start, width, remaining, config);

    // 多级表头降维拼接
    let column_names = if header_levels > 1 {
        merge_multi_level_headers(range, local_data_start, header_levels, width)
    } else {
        let mut names = Vec::with_capacity(width);
        for col in 0..width {
            let cell = range.get((local_data_start, col));
            let name = match cell {
                Some(data) => cell_to_string(data),
                None => String::new(),
            };
            let name = if name.trim().is_empty() {
                format!("Column_{}", col + 1)
            } else {
                name.trim().to_string()
            };
            names.push(name);
        }
        names
    };

    let mut column_names = column_names;
    deduplicate_column_names(&mut column_names);

    // 提取数据行
    let data_row_start = local_data_start + header_levels;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut consecutive_empty = 0;

    for row_idx in data_row_start..=offset_end {
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
            continue;
        } else {
            consecutive_empty = 0;
        }

        if config.enable_dirty_row_filter && is_dirty_row(&row_data, width) {
            continue;
        }

        rows.push(row_data);

        if rows.len() >= config.max_rows {
            break;
        }
    }

    // Drop 全空列
    let non_empty_cols = find_non_empty_columns(&column_names, &rows);
    if non_empty_cols.len() < column_names.len() {
        column_names = non_empty_cols
            .iter()
            .map(|&i| column_names[i].clone())
            .collect();
        rows = rows
            .iter()
            .map(|row| non_empty_cols.iter().map(|&i| row[i].clone()).collect())
            .collect();
    }

    // forward_fill
    if config.enable_forward_fill && !rows.is_empty() {
        forward_fill(&mut rows, &column_names);
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
        header_levels,
        merged_region_count: 0,
    })
}

/// 带范围限制的数据起始行检测（用于多数据块场景）
fn detect_data_start_in_range(
    range: &Range<Data>,
    range_start: usize,
    range_end: usize,
    width: usize,
    max_scan: usize,
) -> usize {
    let scan_limit = (range_end + 1).min(range_start + max_scan + 3);

    for row_idx in range_start..scan_limit {
        let mut non_empty_count = 0;
        let mut total_checked = 0;

        for col in 0..width.min(20) {
            total_checked += 1;
            if let Some(data) = range.get((row_idx, col)) {
                let val = cell_to_string(data);
                if !val.trim().is_empty() {
                    non_empty_count += 1;
                }
            }
        }

        if total_checked > 0 && non_empty_count * 100 / total_checked >= 40 {
            return row_idx;
        }
    }

    range_start
}

/// 带范围限制的表头行数检测
fn detect_header_rows_in_range(
    range: &Range<Data>,
    data_start: usize,
    width: usize,
    remaining: usize,
    config: &ExcelConfig,
) -> usize {
    // 直接复用现有函数，因为它已经使用绝对行号
    detect_header_rows(range, data_start, width, remaining, config)
}

/// 检测表头行数（多级表头）
/// 启发式：从 data_start 开始，判断连续行是否为表头：
/// - 全部是文本（非数值）
/// - 且与下一行的类型模式不同（表头→数据的转换点）
fn detect_header_rows(
    range: &Range<Data>,
    data_start: usize,
    width: usize,
    remaining: usize,
    config: &ExcelConfig,
) -> usize {
    let max_check = config.max_header_rows.min(remaining - 1);

    if max_check <= 1 {
        return 1;
    }

    // 先收集前 max_check+1 行的类型统计
    let mut row_stats: Vec<(usize, usize, usize)> = Vec::new(); // (text_count, numeric_count, empty_count)

    for offset in 0..=max_check {
        let row_idx = data_start + offset;
        let mut text_count = 0;
        let mut numeric_count = 0;
        let mut empty_count = 0;

        for col in 0..width {
            if let Some(data) = range.get((row_idx, col)) {
                match data {
                    Data::Empty => empty_count += 1,
                    Data::Int(_) | Data::Float(_) => numeric_count += 1,
                    Data::DateTime(_) => numeric_count += 1,
                    Data::Bool(_) => numeric_count += 1,
                    Data::String(s) => {
                        if s.parse::<f64>().is_ok() {
                            numeric_count += 1;
                        } else {
                            text_count += 1;
                        }
                    }
                    _ => text_count += 1,
                }
            } else {
                empty_count += 1;
            }
        }

        row_stats.push((text_count, numeric_count, empty_count));
    }

    // 找表头→数据的转换点：
    // 表头行特征：text 比例高，numeric 少
    // 数据行特征：numeric 比例增加
    for level in 1..=max_check {
        let (_, prev_numeric, _) = row_stats[level - 1]; // 上一行（候选表头）
        let (_, curr_numeric, _) = row_stats[level]; // 当前行（候选数据）

        // 如果当前行数值列明显增多（>= 2 个），且上一行数值很少，
        // 说明在这里发生了 表头→数据 转换
        if curr_numeric >= 2 && prev_numeric < curr_numeric {
            return level;
        }
    }

    1 // 默认单行表头
}

/// 多级表头降维拼接
/// 将 N 行同一列的表头文本自上而下拼接（如 ["2025", "上半年", "收入"] -> "2025_上半年_收入"）
fn merge_multi_level_headers(
    range: &Range<Data>,
    start_row: usize,
    levels: usize,
    width: usize,
) -> Vec<String> {
    let mut result = Vec::with_capacity(width);

    for col in 0..width {
        let mut parts: Vec<String> = Vec::new();
        let mut last_nonempty = String::new();

        for level in 0..levels {
            let row_idx = start_row + level;
            let val = range
                .get((row_idx, col))
                .map(|d| cell_to_string(d))
                .unwrap_or_default()
                .trim()
                .to_string();

            if val.is_empty() {
                // 合并单元格场景：空值继承左侧或上方值
                // 这里使用上一级的值（对于水平合并的表头）
                // 不重复添加
            } else if val != last_nonempty {
                parts.push(val.clone());
                last_nonempty = val;
            }
        }

        let name = if parts.is_empty() {
            format!("Column_{}", col + 1)
        } else {
            parts.join("_")
        };

        result.push(name);
    }

    result
}

/// 数据体 forward_fill：对维度列（String 类型）进行空值前向填充
/// 只填充 String 类型的列（通常是类别/维度列，如"部门"、"地区"）
fn forward_fill(rows: &mut [Vec<String>], column_names: &[String]) {
    if rows.is_empty() {
        return;
    }

    let num_cols = column_names.len();

    // 只对前几列做 forward_fill（通常维度列在左侧）
    // 启发式：只填充前 3 列或 30% 的列（取较小者）
    let fill_limit = (num_cols * 30 / 100).max(1).min(3);

    for col in 0..fill_limit.min(num_cols) {
        let mut last_value = String::new();

        for row in rows.iter_mut() {
            if col >= row.len() {
                continue;
            }

            if row[col].trim().is_empty() {
                if !last_value.is_empty() {
                    row[col] = last_value.clone();
                }
            } else {
                last_value = row[col].clone();
            }
        }
    }
}

/// 判断是否为脏数据行（表尾备注/说明行）
/// 启发式规则：
/// - 只有 1-2 列有值，且大部分列为空（说明是备注行）
/// - 第一列是典型的脏数据标识词
fn is_dirty_row(row: &[String], expected_width: usize) -> bool {
    let non_empty: Vec<&String> = row.iter().filter(|v| !v.trim().is_empty()).collect();

    // 如果非空列数 <= 1 且总列宽 >= 5，可能是备注行
    if non_empty.len() <= 1 && expected_width >= 5 {
        if let Some(first) = non_empty.first() {
            let s = first.trim();
            // 典型的表尾标识
            let dirty_patterns = [
                "备注",
                "注：",
                "注:",
                "说明",
                "数据来源",
                "制表人",
                "审核人",
                "合计",
                "总计",
                "小计",
                "※",
                "*注",
            ];
            return dirty_patterns
                .iter()
                .any(|p| s.starts_with(p) || s.contains(p));
        }
    }

    false
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
