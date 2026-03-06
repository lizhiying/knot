//! 配置结构体

use serde::{Deserialize, Serialize};

/// knot-excel 解析配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcelConfig {
    /// 数据抽样行数（用于 TableProfile），默认 3
    #[serde(default = "default_sample_rows")]
    pub sample_rows: usize,

    /// 最大空行容忍数（连续空行超过此值则认为数据结束），默认 3
    #[serde(default = "default_max_empty_rows")]
    pub max_empty_rows: usize,

    /// 是否跳过隐藏的 Sheet，默认 true
    #[serde(default = "default_true")]
    pub skip_hidden_sheets: bool,

    /// 最大列数限制（超过则截断），默认 100
    #[serde(default = "default_max_columns")]
    pub max_columns: usize,

    /// 最大行数限制（超过则截断），默认 100000
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,

    /// 最大表头探测行数（多级表头场景），默认 5
    #[serde(default = "default_max_header_rows")]
    pub max_header_rows: usize,

    /// 是否启用 forward_fill（垂直合并空值填充），默认 true
    #[serde(default = "default_true")]
    pub enable_forward_fill: bool,

    /// 是否启用脏数据行过滤（表头前说明、表尾备注等），默认 true
    #[serde(default = "default_true")]
    pub enable_dirty_row_filter: bool,
}

fn default_sample_rows() -> usize {
    3
}

fn default_max_empty_rows() -> usize {
    3
}

fn default_true() -> bool {
    true
}

fn default_max_columns() -> usize {
    100
}

fn default_max_rows() -> usize {
    100_000
}

fn default_max_header_rows() -> usize {
    5
}

impl Default for ExcelConfig {
    fn default() -> Self {
        Self {
            sample_rows: default_sample_rows(),
            max_empty_rows: default_max_empty_rows(),
            skip_hidden_sheets: true,
            max_columns: default_max_columns(),
            max_rows: default_max_rows(),
            max_header_rows: default_max_header_rows(),
            enable_forward_fill: true,
            enable_dirty_row_filter: true,
        }
    }
}
