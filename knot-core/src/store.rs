use anyhow::Result;
use arrow::record_batch::RecordBatchIterator;
use arrow_array::{types::Float32Type, FixedSizeListArray, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection};
use std::sync::Arc;

pub struct KnotStore {
    conn: Connection,
    table_name: String,
}

impl KnotStore {
    pub async fn new(data_dir: &str) -> Result<Self> {
        let path = std::path::Path::new(data_dir).join("knot_index.lance");
        let path_str = path.to_string_lossy().to_string();

        let conn = connect(&path_str).execute().await?;
        Ok(Self {
            conn,
            table_name: "vectors".to_string(),
        })
    }

    pub async fn create_fts_index(&self) -> Result<()> {
        // Placeholder
        Ok(())
    }

    pub async fn add_records(&self, records: Vec<VectorRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let schema = self.get_schema();
        let batch = self.create_record_batch(records, schema.clone())?;
        let table_names = self.conn.table_names().execute().await?;
        let table_exists = table_names.contains(&self.table_name);
        let reader = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema.clone());

        if table_exists {
            let table = self.conn.open_table(&self.table_name).execute().await?;
            table.add(Box::new(reader)).execute().await?;
        } else {
            self.conn
                .create_table(&self.table_name, Box::new(reader))
                .execute()
                .await?;
        }
        Ok(())
    }

    pub async fn delete_file(&self, file_path: &str) -> Result<()> {
        let table_names = self.conn.table_names().execute().await?;
        if table_names.contains(&self.table_name) {
            let table = self.conn.open_table(&self.table_name).execute().await?;
            table
                .delete(&format!("file_path = '{}'", file_path))
                .await?;
        }
        Ok(())
    }

    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        query_text: &str,
    ) -> Result<Vec<SearchResult>> {
        let table_names = self.conn.table_names().execute().await?;
        if !table_names.contains(&self.table_name) {
            return Ok(vec![]);
        }

        let table = self.conn.open_table(&self.table_name).execute().await?;

        // 1. Retrieve Candidate Set (Vector Search)
        // Fetch more results (e.g. 50) to allow for reranking.
        let vec_query = table.query().nearest_to(query_vector)?;
        let vec_results_stream = vec_query.limit(50).execute().await?;
        let vec_results_batches: Vec<RecordBatch> = vec_results_stream.try_collect().await?;
        let mut candidates = self.batches_to_results(vec_results_batches);

        if query_text.is_empty() {
            return Ok(candidates.into_iter().take(10).collect());
        }

        // 2. Keyword Boosting (Hybrid / Reranking Logic)
        let keywords: Vec<&str> = query_text.split_whitespace().collect();

        for candidate in &mut candidates {
            let mut keyword_matches = 0;
            let content_lower = candidate.text.to_lowercase();

            for kw in &keywords {
                if content_lower.contains(&kw.to_lowercase()) {
                    keyword_matches += 1;
                }
            }

            // Boost score:
            let boost = (keyword_matches as f32) * 100.0; // Strong boost
            candidate.score += boost;
        }

        // 3. Assign base rank score
        for (i, candidate) in candidates.iter_mut().enumerate() {
            candidate.score += (50 - i) as f32; // Rank Score
        }

        // 4. Sort
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(candidates.into_iter().take(10).collect())
    }

    fn batches_to_results(&self, batches: Vec<RecordBatch>) -> Vec<SearchResult> {
        let mut search_results = Vec::new();
        for batch in batches {
            let ids = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let texts = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let paths = batch
                .column(3)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();

            let num_cols = batch.num_columns();
            let parent_ids = if num_cols > 4 {
                batch.column(4).as_any().downcast_ref::<StringArray>()
            } else {
                None
            };
            let breadcrumbs_col = if num_cols > 5 {
                batch.column(5).as_any().downcast_ref::<StringArray>()
            } else {
                None
            };

            for i in 0..batch.num_rows() {
                let pid = parent_ids.map(|a| a.value(i).to_string());
                let bc = breadcrumbs_col.map(|a| a.value(i).to_string());

                search_results.push(SearchResult {
                    id: ids.value(i).to_string(),
                    text: texts.value(i).to_string(),
                    file_path: paths.value(i).to_string(),
                    score: 0.0,
                    parent_id: pid,
                    breadcrumbs: bc,
                });
            }
        }
        search_results
    }

    fn get_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    384, // Dimension size
                ),
                false,
            ),
            Field::new("file_path", DataType::Utf8, false),
            Field::new("parent_id", DataType::Utf8, true),
            Field::new("breadcrumbs", DataType::Utf8, true),
        ]))
    }

    fn create_record_batch(
        &self,
        records: Vec<VectorRecord>,
        schema: Arc<Schema>,
    ) -> Result<RecordBatch> {
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let texts: Vec<String> = records.iter().map(|r| r.text.clone()).collect();
        let paths: Vec<String> = records.iter().map(|r| r.file_path.clone()).collect();
        let parent_ids: Vec<Option<String>> = records.iter().map(|r| r.parent_id.clone()).collect();
        let breadcrumbs: Vec<Option<String>> =
            records.iter().map(|r| r.breadcrumbs.clone()).collect();
        let vectors_flat: Vec<Option<Vec<Option<f32>>>> = records
            .iter()
            .map(|r| Some(r.vector.iter().map(|v| Some(*v)).collect()))
            .collect();

        let id_array = StringArray::from(ids);
        let text_array = StringArray::from(texts);
        let path_array = StringArray::from(paths);
        let parent_id_array = StringArray::from(parent_ids);
        let breadcrumbs_array = StringArray::from(breadcrumbs);
        let vector_array =
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(vectors_flat, 384);

        Ok(RecordBatch::try_new(
            schema,
            vec![
                Arc::new(id_array),
                Arc::new(text_array),
                Arc::new(vector_array),
                Arc::new(path_array),
                Arc::new(parent_id_array),
                Arc::new(breadcrumbs_array),
            ],
        )?)
    }
}

#[derive(Debug, Clone)]
pub struct VectorRecord {
    pub id: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub file_path: String,
    pub parent_id: Option<String>,
    pub breadcrumbs: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub text: String,
    pub file_path: String,
    pub score: f32, // Reranking score
    pub parent_id: Option<String>,
    pub breadcrumbs: Option<String>,
}
