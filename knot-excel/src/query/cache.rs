//! DuckDB 持久化缓存
//!
//! 索引时将 Excel 数据写入持久化 DuckDB 文件，查询时直接打开使用。
//! 通过 mtime + size 快速检测缓存是否过期。

use crate::error::ExcelError;
use crate::reader::{ColumnType, DataBlock};
use duckdb::Connection;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 缓存文件的元数据
#[derive(Debug, Clone)]
pub struct CacheMeta {
    pub file_path: String,
    pub file_mtime: i64,
    pub file_size: i64,
    pub cached_at: i64,
    pub block_count: i32,
    pub total_rows: i32,
}

/// DuckDB 持久化缓存引擎
pub struct ExcelCache {
    db_path: PathBuf,
}

impl ExcelCache {
    /// 创建或打开持久化缓存
    pub fn new(db_path: &str) -> Result<Self, ExcelError> {
        let path = PathBuf::from(db_path);

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ExcelError::Query(format!("Failed to create cache directory: {}", e))
            })?;
        }

        // 初始化元数据表
        let conn = Connection::open(&path)
            .map_err(|e| ExcelError::Query(format!("DuckDB open failed: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS _cache_meta (
                file_path    VARCHAR PRIMARY KEY,
                file_mtime   BIGINT,
                file_size    BIGINT,
                cached_at    BIGINT,
                block_count  INTEGER,
                total_rows   INTEGER,
                table_names  VARCHAR
            )",
            [],
        )
        .map_err(|e| ExcelError::Query(format!("Create meta table failed: {}", e)))?;

        Ok(Self { db_path: path })
    }

    /// 获取数据库文件路径
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// 检查文件缓存是否有效（mtime + size 快速校验）
    pub fn is_cache_valid(&self, file_path: &str) -> bool {
        let fs_meta = match std::fs::metadata(file_path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        let file_mtime = fs_meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let file_size = fs_meta.len() as i64;

        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let result: Result<(i64, i64), _> = conn.query_row(
            "SELECT file_mtime, file_size FROM _cache_meta WHERE file_path = ?",
            [file_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok((cached_mtime, cached_size)) => {
                cached_mtime == file_mtime && cached_size == file_size
            }
            Err(_) => false,
        }
    }

    /// 将 Excel 解析结果写入缓存（upsert：先删旧数据再写新数据）
    pub fn upsert_file(&self, file_path: &str, blocks: &[DataBlock]) -> Result<(), ExcelError> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| ExcelError::Query(format!("DuckDB open failed: {}", e)))?;

        // 1. 删除旧表和元数据
        self.remove_file_tables(&conn, file_path)?;

        // 2. 获取文件元数据
        let fs_meta = std::fs::metadata(file_path)
            .map_err(|e| ExcelError::Query(format!("File metadata failed: {}", e)))?;
        let file_mtime = fs_meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let file_size = fs_meta.len() as i64;
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // 3. 生成文件前缀（避免不同文件的 sheet 名冲突）
        let file_prefix = make_file_prefix(file_path);

        // 4. 注册每个数据块
        let mut table_names = Vec::new();
        let mut total_rows = 0;

        for block in blocks {
            let table_name = format!(
                "{}_{}",
                file_prefix,
                make_safe_table_name(&block.sheet_name, block.block_index)
            );

            if let Err(e) = register_block_to_conn(&conn, &table_name, block) {
                log::warn!("Failed to cache block {}: {}", table_name, e);
                continue;
            }

            table_names.push(table_name);
            total_rows += block.row_count;
        }

        // 5. 写入元数据
        conn.execute(
            "INSERT OR REPLACE INTO _cache_meta
             (file_path, file_mtime, file_size, cached_at, block_count, total_rows, table_names)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            duckdb::params![
                file_path,
                file_mtime,
                file_size,
                now,
                blocks.len() as i32,
                total_rows as i32,
                table_names.join(","),
            ],
        )
        .map_err(|e| ExcelError::Query(format!("Insert meta failed: {}", e)))?;

        log::info!(
            "Cached {} blocks ({} rows) for {}",
            table_names.len(),
            total_rows,
            file_path
        );

        Ok(())
    }

    /// 删除文件的缓存数据
    pub fn remove_file(&self, file_path: &str) -> Result<(), ExcelError> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| ExcelError::Query(format!("DuckDB open failed: {}", e)))?;
        self.remove_file_tables(&conn, file_path)?;
        Ok(())
    }

    /// 获取文件对应的 QueryEngine（使用缓存的持久化数据库）
    /// 返回的 engine 可以直接执行 SQL
    pub fn get_query_engine(&self, file_path: &str) -> Result<CachedQueryEngine, ExcelError> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| ExcelError::Query(format!("DuckDB open failed: {}", e)))?;

        // 查找该文件的所有表名
        let table_names_str: String = conn
            .query_row(
                "SELECT table_names FROM _cache_meta WHERE file_path = ?",
                [file_path],
                |row| row.get(0),
            )
            .map_err(|e| ExcelError::Query(format!("Cache not found for {}: {}", file_path, e)))?;

        let table_names: Vec<String> = table_names_str
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if table_names.is_empty() {
            return Err(ExcelError::Query("No cached tables found".to_string()));
        }

        Ok(CachedQueryEngine { conn, table_names })
    }

    /// 获取所有已缓存文件的元数据
    pub fn list_cached_files(&self) -> Result<Vec<CacheMeta>, ExcelError> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| ExcelError::Query(format!("DuckDB open failed: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT file_path, file_mtime, file_size, cached_at, block_count, total_rows
                 FROM _cache_meta",
            )
            .map_err(|e| ExcelError::Query(format!("Prepare failed: {}", e)))?;

        let mut rows = stmt
            .query([])
            .map_err(|e| ExcelError::Query(format!("Query failed: {}", e)))?;

        let mut result = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| ExcelError::Query(format!("Row error: {}", e)))?
        {
            result.push(CacheMeta {
                file_path: row.get(0).unwrap_or_default(),
                file_mtime: row.get(1).unwrap_or(0),
                file_size: row.get(2).unwrap_or(0),
                cached_at: row.get(3).unwrap_or(0),
                block_count: row.get(4).unwrap_or(0),
                total_rows: row.get(5).unwrap_or(0),
            });
        }

        Ok(result)
    }

    /// 删除文件相关的所有表和元数据
    fn remove_file_tables(&self, conn: &Connection, file_path: &str) -> Result<(), ExcelError> {
        // 查找该文件的所有表名
        let table_names: Result<String, _> = conn.query_row(
            "SELECT table_names FROM _cache_meta WHERE file_path = ?",
            [file_path],
            |row| row.get(0),
        );

        if let Ok(names) = table_names {
            for table_name in names.split(',').filter(|s| !s.is_empty()) {
                let _ = conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", table_name), []);
            }
        }

        let _ = conn.execute("DELETE FROM _cache_meta WHERE file_path = ?", [file_path]);

        Ok(())
    }
}

/// 缓存的查询引擎（基于持久化 DuckDB 连接）
pub struct CachedQueryEngine {
    conn: Connection,
    table_names: Vec<String>,
}

impl CachedQueryEngine {
    /// 获取已注册的表名
    pub fn table_names(&self) -> &[String] {
        &self.table_names
    }

    /// 获取 Schema 信息
    pub fn get_schemas(&self) -> Vec<super::engine::TableSchema> {
        let mut schemas = Vec::new();
        for table_name in &self.table_names {
            if let Ok(result) = self.execute_sql(&format!(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{}'",
                table_name
            )) {
                let columns: Vec<(String, String)> = result
                    .rows
                    .iter()
                    .map(|row| (row[0].clone(), row[1].clone()))
                    .collect();
                schemas.push(super::engine::TableSchema {
                    table_name: table_name.clone(),
                    source_id: table_name.clone(),
                    columns,
                });
            }
        }
        schemas
    }

    /// 执行 SQL 查询
    pub fn execute_sql(&self, sql: &str) -> Result<super::engine::QueryResult, ExcelError> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| ExcelError::Query(format!("SQL prepare error: {}", e)))?;

        let mut result_rows = stmt
            .query([])
            .map_err(|e| ExcelError::Query(format!("SQL execution error: {}", e)))?;

        let inner_stmt = result_rows.as_ref().expect("Statement should be available");
        let col_count = inner_stmt.column_count();
        let columns: Vec<String> = (0..col_count)
            .map(|i| {
                inner_stmt
                    .column_name(i)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| format!("col_{}", i))
            })
            .collect();

        let mut rows = Vec::new();
        while let Some(row) = result_rows
            .next()
            .map_err(|e| ExcelError::Query(format!("Row error: {}", e)))?
        {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let val: String = match row.get::<_, String>(i) {
                    Ok(s) => s,
                    Err(_) => match row.get::<_, i64>(i) {
                        Ok(n) => n.to_string(),
                        Err(_) => match row.get::<_, f64>(i) {
                            Ok(f) => {
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
        Ok(super::engine::QueryResult {
            sql: sql.to_string(),
            columns,
            rows,
            row_count,
            retried: false,
            intermediate_steps: 0,
        })
    }

    /// 执行多步 SQL（分号分隔）
    pub fn execute_multi_step(
        &self,
        sql_text: &str,
    ) -> Result<super::engine::QueryResult, ExcelError> {
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

        let intermediate_count = statements.len() - 1;
        for (i, stmt) in statements[..intermediate_count].iter().enumerate() {
            let wrapped = format!("CREATE OR REPLACE TEMP TABLE step_{} AS ({})", i, stmt);
            self.conn
                .execute(&wrapped, [])
                .map_err(|e| ExcelError::Query(format!("Step {} failed: {}", i, e)))?;
        }

        let mut result = self.execute_sql(statements[intermediate_count])?;
        result.intermediate_steps = intermediate_count;
        Ok(result)
    }
}

/// 生成文件前缀（短 hash，避免不同文件的 sheet 名冲突）
fn make_file_prefix(file_path: &str) -> String {
    // 简单 hash：取路径的 FNV-1a 32-bit hash 的后 6 位 hex
    let mut hash: u32 = 0x811c9dc5;
    for byte in file_path.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("f{:06x}", hash & 0xFFFFFF)
}

/// 生成安全的表名
/// 保留中文等 Unicode 字母/数字，只替换 SQL 不安全的特殊字符
fn make_safe_table_name(sheet_name: &str, block_index: usize) -> String {
    let safe: String = sheet_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // 去除首尾和连续的下划线
    let trimmed = safe.trim_matches('_');
    let mut result = String::new();
    let mut prev_underscore = false;
    for c in trimmed.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }

    if result.is_empty() {
        result = format!("sheet_{}", block_index);
    }

    if block_index == 0 {
        result
    } else {
        format!("{}_b{}", result, block_index)
    }
}

/// 将 DataBlock 注册到指定的 Connection 和表名
fn register_block_to_conn(
    conn: &Connection,
    table_name: &str,
    block: &DataBlock,
) -> Result<(), ExcelError> {
    // CREATE TABLE
    let col_defs: Vec<String> = block
        .column_names
        .iter()
        .zip(block.column_types.iter())
        .map(|(name, dtype)| {
            let sql_type = match dtype {
                ColumnType::Int => "BIGINT",
                ColumnType::Float => "DOUBLE",
                ColumnType::Bool => "BOOLEAN",
                ColumnType::DateTime => "VARCHAR",
                _ => "VARCHAR",
            };
            format!("\"{}\" {}", name, sql_type)
        })
        .collect();

    conn.execute(
        &format!(
            "CREATE OR REPLACE TABLE \"{}\" ({})",
            table_name,
            col_defs.join(", ")
        ),
        [],
    )
    .map_err(|e| ExcelError::Query(format!("CREATE TABLE failed: {}", e)))?;

    // INSERT data
    if !block.rows.is_empty() {
        let placeholders: Vec<String> = (1..=block.column_names.len())
            .map(|i| format!("?{}", i))
            .collect();
        let insert_sql = format!(
            "INSERT INTO \"{}\" VALUES ({})",
            table_name,
            placeholders.join(", ")
        );

        let mut stmt = conn
            .prepare(&insert_sql)
            .map_err(|e| ExcelError::Query(format!("PREPARE failed: {}", e)))?;

        for row in &block.rows {
            let values: Vec<Box<dyn duckdb::ToSql>> = row
                .iter()
                .enumerate()
                .map(|(i, val)| {
                    let col_type = block.column_types.get(i).unwrap_or(&ColumnType::String);
                    convert_value(val, col_type)
                })
                .collect();

            let refs: Vec<&dyn duckdb::ToSql> = values.iter().map(|v| v.as_ref()).collect();
            stmt.execute(refs.as_slice())
                .map_err(|e| ExcelError::Query(format!("INSERT failed: {}", e)))?;
        }
    }

    Ok(())
}

/// 将字符串值转换为 DuckDB 类型（与 engine.rs 一致）
fn convert_value(val: &str, col_type: &ColumnType) -> Box<dyn duckdb::ToSql> {
    if val.trim().is_empty() {
        return Box::new(None::<String>);
    }

    match col_type {
        ColumnType::Int => {
            if let Ok(n) = val.parse::<i64>() {
                Box::new(n)
            } else {
                Box::new(None::<i64>)
            }
        }
        ColumnType::Float => {
            if let Ok(f) = val.parse::<f64>() {
                Box::new(f)
            } else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::ColumnType;

    fn make_test_blocks() -> Vec<DataBlock> {
        vec![DataBlock {
            source_id: "test_file_sheet1_0".to_string(),
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
            ],
            row_count: 2,
            header_levels: 1,
            merged_region_count: 0,
        }]
    }

    #[test]
    fn test_cache_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test_cache.duckdb");
        let cache = ExcelCache::new(db_path.to_str().unwrap()).unwrap();

        // 创建一个临时 Excel 文件路径（不需要真实文件，只用路径做 key）
        let fake_path = tmp.path().join("test.xlsx");
        std::fs::write(&fake_path, "fake").unwrap();
        let file_path = fake_path.to_str().unwrap();

        // 写入缓存
        let blocks = make_test_blocks();
        cache.upsert_file(file_path, &blocks).unwrap();

        // 验证缓存有效
        assert!(cache.is_cache_valid(file_path));

        // 获取查询引擎并查询
        let engine = cache.get_query_engine(file_path).unwrap();
        let schemas = engine.get_schemas();
        assert_eq!(schemas.len(), 1);

        let table_name = &schemas[0].table_name;
        let result = engine
            .execute_sql(&format!("SELECT * FROM \"{}\"", table_name))
            .unwrap();
        assert_eq!(result.row_count, 2);
        assert_eq!(result.columns.len(), 3);
    }

    #[test]
    fn test_cache_invalidation() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test_cache2.duckdb");
        let cache = ExcelCache::new(db_path.to_str().unwrap()).unwrap();

        let fake_path = tmp.path().join("test2.xlsx");
        std::fs::write(&fake_path, "v1").unwrap();
        let file_path = fake_path.to_str().unwrap();

        let blocks = make_test_blocks();
        cache.upsert_file(file_path, &blocks).unwrap();
        assert!(cache.is_cache_valid(file_path));

        // 修改文件（改变 size）
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&fake_path, "version2-longer").unwrap();

        // 缓存应该无效
        assert!(!cache.is_cache_valid(file_path));
    }

    #[test]
    fn test_remove_file() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test_cache3.duckdb");
        let cache = ExcelCache::new(db_path.to_str().unwrap()).unwrap();

        let fake_path = tmp.path().join("test3.xlsx");
        std::fs::write(&fake_path, "data").unwrap();
        let file_path = fake_path.to_str().unwrap();

        let blocks = make_test_blocks();
        cache.upsert_file(file_path, &blocks).unwrap();
        assert!(cache.is_cache_valid(file_path));

        cache.remove_file(file_path).unwrap();
        assert!(!cache.is_cache_valid(file_path));
    }
}
