//! DuckDB 查询引擎核心
//!
//! 封装 DuckDB 内存连接，将 DataBlock 注册为临时表，执行 SQL 查询。

use crate::error::ExcelError;
use crate::reader::{ColumnType, DataBlock};
use duckdb::Connection;
use std::collections::HashMap;

/// SQL 查询结果
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// 执行的 SQL 语句
    pub sql: String,
    /// 结果列名
    pub columns: Vec<String>,
    /// 结果数据行
    pub rows: Vec<Vec<String>>,
    /// 结果行数
    pub row_count: usize,
    /// 是否经过重试修复
    pub retried: bool,
    /// 多步执行时的中间步骤数
    pub intermediate_steps: usize,
}

impl QueryResult {
    /// 转换为 Markdown 表格
    pub fn to_markdown(&self) -> String {
        if self.columns.is_empty() || self.rows.is_empty() {
            return String::from("（空结果）");
        }

        let mut md = String::new();

        // 表头
        md.push_str("| ");
        md.push_str(&self.columns.join(" | "));
        md.push_str(" |\n");

        // 分隔线
        md.push_str("| ");
        md.push_str(
            &self
                .columns
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | "),
        );
        md.push_str(" |\n");

        // 数据行
        for row in &self.rows {
            md.push_str("| ");
            md.push_str(&row.join(" | "));
            md.push_str(" |\n");
        }

        md
    }
}

/// DuckDB 查询引擎
pub struct QueryEngine {
    conn: Connection,
    /// 已注册的表名 -> DataBlock source_id 映射
    registered_tables: HashMap<String, String>,
}

impl QueryEngine {
    /// 创建新的查询引擎（内存模式）
    pub fn new() -> Result<Self, ExcelError> {
        let conn = Connection::open_in_memory().map_err(|e| ExcelError::Query(format!("{}", e)))?;
        Ok(Self {
            conn,
            registered_tables: HashMap::new(),
        })
    }

    /// 将 DataBlock 注册为 DuckDB 临时表
    /// 返回注册的表名（格式：sheet_blockN）
    pub fn register_datablock(&mut self, block: &DataBlock) -> Result<String, ExcelError> {
        // 生成安全的表名
        let table_name = Self::make_table_name(&block.sheet_name, block.block_index);

        // 构建 CREATE TABLE 语句
        let col_defs: Vec<String> = block
            .column_names
            .iter()
            .zip(block.column_types.iter())
            .map(|(name, dtype)| {
                let sql_type = match dtype {
                    ColumnType::Int => "BIGINT",
                    ColumnType::Float => "DOUBLE",
                    ColumnType::Bool => "BOOLEAN",
                    ColumnType::DateTime => "VARCHAR", // 日期统一存为字符串
                    _ => "VARCHAR",
                };
                format!("\"{}\" {}", name, sql_type)
            })
            .collect();

        let create_sql = format!(
            "CREATE OR REPLACE TEMP TABLE \"{}\" ({})",
            table_name,
            col_defs.join(", ")
        );

        self.conn
            .execute(&create_sql, [])
            .map_err(|e| ExcelError::Query(format!("CREATE TABLE failed: {}", e)))?;

        // 插入数据 — 用参数化 INSERT
        if !block.rows.is_empty() {
            let placeholders: Vec<String> = (1..=block.column_names.len())
                .map(|i| format!("?{}", i))
                .collect();
            let insert_sql = format!(
                "INSERT INTO \"{}\" VALUES ({})",
                table_name,
                placeholders.join(", ")
            );

            let mut stmt = self
                .conn
                .prepare(&insert_sql)
                .map_err(|e| ExcelError::Query(format!("PREPARE failed: {}", e)))?;

            for row in &block.rows {
                // 将字符串值根据列类型转换
                let values: Vec<Box<dyn duckdb::ToSql>> = row
                    .iter()
                    .enumerate()
                    .map(|(i, val)| {
                        let col_type = block.column_types.get(i).unwrap_or(&ColumnType::String);
                        Self::convert_value(val, col_type)
                    })
                    .collect();

                let refs: Vec<&dyn duckdb::ToSql> = values.iter().map(|v| v.as_ref()).collect();
                stmt.execute(refs.as_slice())
                    .map_err(|e| ExcelError::Query(format!("INSERT failed: {}", e)))?;
            }
        }

        self.registered_tables
            .insert(table_name.clone(), block.source_id.clone());

        log::info!(
            "Registered table '{}' ({} rows x {} cols)",
            table_name,
            block.row_count,
            block.column_names.len()
        );

        Ok(table_name)
    }

    /// 执行单条 SQL 查询
    pub fn execute_sql(&self, sql: &str) -> Result<QueryResult, ExcelError> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| ExcelError::Query(format!("SQL prepare error: {}", e)))?;

        // duckdb-rs 需要先通过 query 执行语句，然后才能获取列名和数据
        let mut result_rows = stmt
            .query([])
            .map_err(|e| ExcelError::Query(format!("SQL execution error: {}", e)))?;

        // 获取列信息（通过 Rows 内部的 Statement 引用）
        let inner_stmt = result_rows
            .as_ref()
            .expect("Statement should be available after query");
        let col_count = inner_stmt.column_count();
        let columns: Vec<String> = (0..col_count)
            .map(|i| {
                inner_stmt
                    .column_name(i)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| format!("col_{}", i))
            })
            .collect();

        // 读取所有行
        let mut rows = Vec::new();
        while let Some(row) = result_rows
            .next()
            .map_err(|e| ExcelError::Query(format!("Row iteration error: {}", e)))?
        {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                // 尝试多种类型读取
                let val: String = match row.get::<_, String>(i) {
                    Ok(s) => s,
                    Err(_) => match row.get::<_, i64>(i) {
                        Ok(n) => n.to_string(),
                        Err(_) => match row.get::<_, f64>(i) {
                            Ok(f) => {
                                // 如果是整数值（如 250.0），去掉小数部分
                                if f == f.trunc() && f.abs() < i64::MAX as f64 {
                                    format!("{}", f as i64)
                                } else {
                                    format!("{:.2}", f)
                                }
                            }
                            Err(_) => match row.get::<_, bool>(i) {
                                Ok(b) => b.to_string(),
                                Err(_) => "NULL".to_string(),
                            },
                        },
                    },
                };
                vals.push(val);
            }
            rows.push(vals);
        }

        let row_count = rows.len();

        Ok(QueryResult {
            sql: sql.to_string(),
            columns,
            rows,
            row_count,
            retried: false,
            intermediate_steps: 0,
        })
    }

    /// 执行多步 SQL（分号分隔）
    /// 前 N-1 条自动包装为 CREATE TEMP TABLE，最后一条返回结果
    pub fn execute_multi_step(&self, sql_text: &str) -> Result<QueryResult, ExcelError> {
        let statements: Vec<&str> = sql_text
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if statements.is_empty() {
            return Err(ExcelError::Query("Empty SQL".to_string()));
        }

        if statements.len() == 1 {
            return self.execute_sql(statements[0]);
        }

        // 多步执行
        let intermediate_count = statements.len() - 1;
        for (i, stmt) in statements[..intermediate_count].iter().enumerate() {
            let wrapped = format!("CREATE OR REPLACE TEMP TABLE step_{} AS ({})", i, stmt);
            self.conn
                .execute(&wrapped, [])
                .map_err(|e| ExcelError::Query(format!("Step {} failed: {}", i, e)))?;
        }

        // 执行最后一条并返回结果
        let mut result = self.execute_sql(statements[intermediate_count])?;
        result.intermediate_steps = intermediate_count;
        Ok(result)
    }

    /// 获取已注册表的 Schema 信息（表名 -> 列信息列表）
    pub fn get_registered_schemas(&self) -> Vec<TableSchema> {
        let mut schemas = Vec::new();
        for (table_name, source_id) in &self.registered_tables {
            // 查询 DuckDB 获取列信息
            if let Ok(result) = self.execute_sql(&format!(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{}'",
                table_name
            )) {
                let columns: Vec<(String, String)> = result
                    .rows
                    .iter()
                    .map(|row| (row[0].clone(), row[1].clone()))
                    .collect();

                schemas.push(TableSchema {
                    table_name: table_name.clone(),
                    source_id: source_id.clone(),
                    columns,
                });
            }
        }
        schemas
    }

    /// 清理所有临时表
    pub fn unregister_all(&mut self) -> Result<(), ExcelError> {
        for table_name in self.registered_tables.keys() {
            let _ = self
                .conn
                .execute(&format!("DROP TABLE IF EXISTS \"{}\"", table_name), []);
        }
        self.registered_tables.clear();
        Ok(())
    }

    /// 生成安全的表名
    fn make_table_name(sheet_name: &str, block_index: usize) -> String {
        // 将中文和特殊字符替换为下划线
        let safe_name: String = sheet_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        if block_index == 0 {
            safe_name
        } else {
            format!("{}_block{}", safe_name, block_index)
        }
    }

    /// 将字符串值转换为适合 DuckDB 的类型
    fn convert_value(val: &str, col_type: &ColumnType) -> Box<dyn duckdb::ToSql> {
        if val.trim().is_empty() {
            return Box::new(None::<String>);
        }

        match col_type {
            ColumnType::Int => {
                if let Ok(n) = val.parse::<i64>() {
                    Box::new(n)
                } else {
                    // 无法解析为整数 → NULL（避免将字符串插入数值列）
                    Box::new(None::<i64>)
                }
            }
            ColumnType::Float => {
                if let Ok(f) = val.parse::<f64>() {
                    Box::new(f)
                } else {
                    // 无法解析为浮点数 → NULL
                    Box::new(None::<f64>)
                }
            }
            ColumnType::Bool => {
                let b = val == "true" || val == "TRUE" || val == "1";
                Box::new(b)
            }
            _ => Box::new(val.to_string()),
        }
    }
}

/// 表的 Schema 信息
#[derive(Debug, Clone)]
pub struct TableSchema {
    pub table_name: String,
    pub source_id: String,
    /// (列名, 数据类型)
    pub columns: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::ColumnType;

    fn make_test_block() -> DataBlock {
        DataBlock {
            source_id: "test_sales_0".to_string(),
            sheet_name: "销售数据".to_string(),
            block_index: 0,
            column_names: vec!["产品".to_string(), "销量".to_string(), "金额".to_string()],
            column_types: vec![ColumnType::String, ColumnType::Int, ColumnType::Float],
            rows: vec![
                vec![
                    "产品A".to_string(),
                    "100".to_string(),
                    "15000.5".to_string(),
                ],
                vec![
                    "产品B".to_string(),
                    "200".to_string(),
                    "30000.0".to_string(),
                ],
                vec![
                    "产品A".to_string(),
                    "150".to_string(),
                    "22500.0".to_string(),
                ],
                vec!["产品C".to_string(), "80".to_string(), "12000.0".to_string()],
            ],
            row_count: 4,
            header_levels: 1,
            merged_region_count: 0,
        }
    }

    #[test]
    fn test_register_and_query() {
        let mut engine = QueryEngine::new().unwrap();
        let block = make_test_block();

        let table_name = engine.register_datablock(&block).unwrap();
        assert!(!table_name.is_empty());

        // 简单 SELECT
        let result = engine
            .execute_sql(&format!("SELECT * FROM \"{}\"", table_name))
            .unwrap();
        assert_eq!(result.row_count, 4);
        assert_eq!(result.columns.len(), 3);

        // 聚合查询
        let result = engine
            .execute_sql(&format!(
                "SELECT \"产品\", SUM(\"销量\") as total FROM \"{}\" GROUP BY \"产品\" ORDER BY total DESC",
                table_name
            ))
            .unwrap();
        assert_eq!(result.row_count, 3); // 产品A, 产品B, 产品C
                                         // 产品A 的总销量应该是 250
        assert_eq!(result.rows[0][0], "产品A");
        assert_eq!(result.rows[0][1], "250");
    }

    #[test]
    fn test_markdown_output() {
        let mut engine = QueryEngine::new().unwrap();
        let block = make_test_block();

        let table_name = engine.register_datablock(&block).unwrap();
        let result = engine
            .execute_sql(&format!(
                "SELECT \"产品\", SUM(\"金额\") as 总金额 FROM \"{}\" GROUP BY \"产品\"",
                table_name
            ))
            .unwrap();

        let md = result.to_markdown();
        assert!(md.contains("产品"));
        assert!(md.contains("总金额"));
        println!("{}", md);
    }

    #[test]
    fn test_multi_step_sql() {
        let mut engine = QueryEngine::new().unwrap();
        let block = make_test_block();
        let table_name = engine.register_datablock(&block).unwrap();

        // 多步 SQL：先创建中间表，再查询
        let sql = format!(
            "SELECT \"产品\", SUM(\"销量\") as total FROM \"{}\" GROUP BY \"产品\"; SELECT * FROM step_0 WHERE total > 100",
            table_name
        );
        let result = engine.execute_multi_step(&sql).unwrap();
        assert_eq!(result.intermediate_steps, 1);
        // 产品A (250) 和 产品B (200) 的总销量 > 100
        assert_eq!(result.row_count, 2);
    }
}
