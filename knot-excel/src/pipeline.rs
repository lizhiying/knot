//! Pipeline：串联 Reader -> Profile 完整流程

use crate::config::ExcelConfig;
use crate::error::ExcelError;
use crate::profile::TableProfile;
use crate::reader::DataBlock;
use std::path::Path;

/// 解析结果
#[derive(Debug, Clone)]
pub struct ParsedExcel {
    /// 原始数据块（包含列名、类型、数据）
    pub blocks: Vec<DataBlock>,
    /// 对应的 TableProfile（用于索引）
    pub profiles: Vec<TableProfile>,
}

/// 解析 Excel 文件的主入口
///
/// 返回所有 DataBlock 和对应的 TableProfile。
pub fn parse_excel<P: AsRef<Path>>(
    path: P,
    config: &ExcelConfig,
) -> Result<Vec<DataBlock>, ExcelError> {
    crate::reader::read_excel(path, config)
}

/// 完整 Pipeline：读取 + Profile 生成
pub fn parse_excel_full<P: AsRef<Path>>(
    path: P,
    config: &ExcelConfig,
) -> Result<ParsedExcel, ExcelError> {
    let path = path.as_ref();
    let file_path = path.to_string_lossy().to_string();

    let blocks = crate::reader::read_excel(path, config)?;

    let profiles: Vec<TableProfile> = blocks
        .iter()
        .map(|block| TableProfile::from_data_block(block, &file_path, config.sample_rows))
        .collect();

    println!(
        "[ExcelPipeline] {} -> {} blocks, {} profiles",
        path.display(),
        blocks.len(),
        profiles.len()
    );

    Ok(ParsedExcel { blocks, profiles })
}
