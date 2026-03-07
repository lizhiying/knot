//! DuckDB 查询引擎模块
//!
//! 提供 Text-to-SQL 查询能力：
//! - `QueryEngine` — DuckDB 连接管理、DataBlock 注册、SQL 执行
//! - `SqlGenerator` — 将用户 Query + Schema 信息组装为 LLM Prompt
//! - `ResultSummarizer` — 查询结果膨胀控制（全量/摘要）

mod engine;
mod result;
mod sql;

pub use engine::{QueryEngine, QueryResult};
pub use result::{ResultContext, ResultSummarizer};
pub use sql::SqlGenerator;
