//! # knot-excel
//!
//! Excel 结构化数据解析引擎，用于 Knot RAG 系统。
//!
//! ## 功能
//!
//! - 高速读取 `.xlsx` / `.xls` 文件（基于 calamine）
//! - 自动推断列类型（String / Float / Int / Date / Bool）
//! - 生成 TableProfile 结构化摘要（用于向量化索引）
//! - 支持标准二维表（单行表头，无合并单元格）
//!
//! ## Quick Start
//!
//! ```no_run
//! use knot_excel::{parse_excel, ExcelConfig};
//!
//! let blocks = parse_excel("data.xlsx", &ExcelConfig::default()).unwrap();
//! for block in &blocks {
//!     println!("Sheet: {}, rows: {}", block.sheet_name, block.row_count);
//! }
//! ```

pub mod config;
pub mod error;
pub mod pipeline;
pub mod profile;
pub mod reader;

pub use config::ExcelConfig;
pub use error::ExcelError;
pub use pipeline::parse_excel;
pub use profile::TableProfile;
pub use reader::{ColumnType, DataBlock};
