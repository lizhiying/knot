use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};

#[derive(Clone)]
pub struct FileRegistry {
    pool: Pool<Sqlite>,
}

impl FileRegistry {
    pub async fn new(db_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(db_url)
            .await?;

        // Create table if not exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS file_registry (
                file_path TEXT PRIMARY KEY,
                last_modified INTEGER,
                content_hash TEXT,
                index_version TEXT,
                indexed_at INTEGER
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn get_file_hash(&self, path: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT content_hash FROM file_registry WHERE file_path = ?")
            .bind(path)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.get("content_hash")))
    }

    pub async fn update_file(&self, path: &str, hash: &str, modified: i64) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"
            INSERT INTO file_registry (file_path, content_hash, last_modified, indexed_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(file_path) DO UPDATE SET
                content_hash = excluded.content_hash,
                last_modified = excluded.last_modified,
                indexed_at = excluded.indexed_at
            "#,
        )
        .bind(path)
        .bind(hash)
        .bind(modified)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_all_files(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT file_path FROM file_registry")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.get("file_path")).collect())
    }

    pub async fn remove_file(&self, path: &str) -> Result<()> {
        sqlx::query("DELETE FROM file_registry WHERE file_path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
