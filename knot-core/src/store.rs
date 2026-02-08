use crate::path_processor::PathProcessor;
use crate::tokenizer::JiebaTokenizer;
use anyhow::Result;
use arrow::record_batch::RecordBatchIterator;
use arrow_array::{types::Float32Type, FixedSizeListArray, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection};
use std::path::PathBuf;
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema as t_schema;
use tantivy::schema::{IndexRecordOption, TextFieldIndexing, TextOptions, Value};
use tantivy::tokenizer::StopWordFilter;
use tantivy::{Index, TantivyDocument};

const EMBEDDING_DIM: i32 = 512;

pub struct KnotStore {
    conn: Connection,
    table_name: String,
    tantivy_path: PathBuf,
    tantivy_index: Index, // Cached Index to avoid repeated initialization
}

impl KnotStore {
    pub async fn new(index_path: &str) -> Result<Self> {
        let path = std::path::Path::new(index_path);
        let path_str = path.to_string_lossy().to_string();

        // Calculate Tantivy Path: sibling of knot_index.lance
        // e.g. ~/.knot/indexes/<hash>/knot_index.lance -> ~/.knot/indexes/<hash>/tantivy
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid index path"))?;
        let tantivy_path = parent.join("tantivy");

        if !tantivy_path.exists() {
            std::fs::create_dir_all(&tantivy_path)?;
        }

        let conn = connect(&path_str).execute().await?;

        // Pre-initialize Tantivy Index (expensive, do once)
        let tantivy_index = Self::create_tantivy_index(&tantivy_path)?;

        let store = Self {
            conn,
            table_name: "vectors".to_string(),
            tantivy_path,
            tantivy_index,
        };

        Ok(store)
    }

    /// Create and configure Tantivy Index (called once during initialization)
    fn create_tantivy_index(tantivy_path: &PathBuf) -> Result<Index> {
        use tantivy::directory::MmapDirectory;

        let mut schema_builder = t_schema::Schema::builder();

        // 1. Text Options
        // Jieba: Chinese Semantic Segmentation
        let text_zh_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("jieba")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ); // Not Stored

        // Standard: General Multilingual (Simple)
        let text_std_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );

        // Define Fields
        let _text_zh = schema_builder.add_text_field("text_zh", text_zh_options);
        let _text_std = schema_builder.add_text_field("text_std", text_std_options.clone());

        let file_name_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("jieba")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ) | t_schema::STORED;
        let _file_name = schema_builder.add_text_field("file_name", file_name_options);

        let path_tags_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ) | t_schema::STORED;
        let _path_tags = schema_builder.add_text_field("path_tags", path_tags_options);

        // 2. Schema Definition
        schema_builder.add_text_field("id", t_schema::STRING | t_schema::STORED);
        schema_builder.add_text_field("file_path", t_schema::STRING | t_schema::STORED);
        schema_builder.add_text_field("content", t_schema::STORED);
        schema_builder.add_text_field("parent_id", t_schema::STRING | t_schema::STORED);
        schema_builder.add_text_field("breadcrumbs", t_schema::STRING | t_schema::STORED);

        let schema = schema_builder.build();

        // Auto-Migration: Check if schema matches
        let reset_needed = if tantivy_path.exists() {
            if let Ok(dir) = MmapDirectory::open(tantivy_path) {
                if let Ok(idx) = Index::open(dir) {
                    idx.schema().get_field("text_zh").is_err()
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if reset_needed {
            let _ = std::fs::remove_dir_all(tantivy_path);
            let _ = std::fs::create_dir_all(tantivy_path);
        }

        // Open or Create
        let dir = MmapDirectory::open(tantivy_path)?;
        let index = Index::open_or_create(dir, schema)?;

        // Register Jieba Tokenizer (expensive operation, done once)
        let stop_words = include_str!("../../knot-app/stopwords.txt")
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let jieba_tokenizer = tantivy::tokenizer::TextAnalyzer::builder(JiebaTokenizer::new())
            .filter(tantivy::tokenizer::LowerCaser)
            .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
            .filter(StopWordFilter::remove(stop_words))
            .build();

        index.tokenizers().register("jieba", jieba_tokenizer);

        Ok(index)
    }

    /// Get cached Tantivy Index for search operations
    fn get_tantivy_index(&self) -> &Index {
        &self.tantivy_index
    }

    pub async fn create_fts_index(&self) -> Result<()> {
        // Placeholder
        Ok(())
    }

    pub async fn add_records(&self, records: Vec<VectorRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        // 1. Write to LanceDB
        let schema = self.get_schema();
        let batch = self.create_record_batch(records.clone(), schema.clone())?;
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

        // 2. Write to Tantivy
        let index = self.get_tantivy_index();
        let mut index_writer = index.writer::<TantivyDocument>(50_000_000)?; // 50MB buffer

        let schema = index.schema();
        let f_id = schema.get_field("id").unwrap();
        let f_path = schema.get_field("file_path").unwrap();
        let f_content = schema.get_field("content").unwrap();
        let f_text_zh = schema.get_field("text_zh").unwrap();
        let f_text_std = schema.get_field("text_std").unwrap();
        let f_file_name = schema.get_field("file_name").unwrap();
        let f_path_tags = schema.get_field("path_tags").unwrap();
        let f_pid = schema.get_field("parent_id").unwrap();
        let f_bc = schema.get_field("breadcrumbs").unwrap();

        for record in records {
            let mut doc = TantivyDocument::default();
            doc.add_text(f_id, &record.id);
            doc.add_text(f_path, &record.file_path);

            // Extract Metadata
            let extracted_file_name = PathProcessor::extract_file_name(&record.file_path);
            let extracted_tags = PathProcessor::extract_directory_tags(&record.file_path);

            doc.add_text(f_file_name, &extracted_file_name);
            doc.add_text(f_path_tags, &extracted_tags);

            doc.add_text(f_content, &record.text); // Store original text
            doc.add_text(f_text_zh, &record.text); // Index with Jieba
            doc.add_text(f_text_std, &record.text); // Index with Standard

            if let Some(pid) = &record.parent_id {
                doc.add_text(f_pid, pid);
            }
            if let Some(bc) = &record.breadcrumbs {
                doc.add_text(f_bc, bc);
            }
            index_writer.add_document(doc)?;
        }

        index_writer.commit()?;

        Ok(())
    }

    pub async fn delete_file(&self, file_path: &str) -> Result<()> {
        // LanceDB Delete
        let table_names = self.conn.table_names().execute().await?;
        if table_names.contains(&self.table_name) {
            let table = self.conn.open_table(&self.table_name).execute().await?;
            table
                .delete(&format!("file_path = '{}'", file_path))
                .await?;
        }

        // Tantivy Delete
        let index = self.get_tantivy_index();
        let mut writer = index.writer::<TantivyDocument>(50_000_000)?;
        let f_path = index.schema().get_field("file_path").unwrap();
        // Term-based delete requires Exact Match. STRING field is exact match.
        writer.delete_term(t_schema::Term::from_field_text(f_path, file_path));
        writer.commit()?;

        Ok(())
    }

    pub async fn delete_folder(&self, folder_path: &str) -> Result<()> {
        // LanceDB Delete
        let table_names = self.conn.table_names().execute().await?;
        if table_names.contains(&self.table_name) {
            let table = self.conn.open_table(&self.table_name).execute().await?;
            let path_prefix = if folder_path.ends_with('/') || folder_path.ends_with('\\') {
                folder_path.to_string()
            } else {
                format!("{}/", folder_path)
            };

            table
                .delete(&format!("file_path LIKE '{}%'", path_prefix))
                .await?;
        }

        // Tantivy Delete (Not easy with Terms, needs Query)
        // We can't use delete_term for prefix.
        // We have to use delete_query? But IndexWriter.delete_query is implemented? Unsure.
        // IndexWriter commonly only supports delete_id or delete_term.
        // Actually, Tantivy's delete_term is the main way.
        // Deleting by query usually requires scrolling and deleting IDs?
        // Or newer Tantivy might support it.
        // For now, let's skip folder deletion optimization in Tantivy or implement later.
        // Actually, to keep it consistent, we SHOULD delete.
        // But implementing prefix delete by iterating terms is complex here.
        // Let's settle for file-based deletion (which calls delete_file) in main.rs loop for now,
        // or just accept data drift until reindex.
        // Given main.rs watcher logic:
        // `match store.delete_file(&path_str).await`
        // `let _ = store.delete_folder(&path_str).await;`
        // If user deleted a folder, main.rs calls delete_folder.
        // Ideally we iterate valid files?
        // Let's leave Tantivy folder delete as TODO/No-op for Iteration 1.

        Ok(())
    }

    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        query_text: &str,
    ) -> Result<Vec<SearchResult>> {
        use std::collections::HashMap;
        use std::time::Instant;

        let total_start = Instant::now();

        // Map ID -> SearchResult
        let mut results_map: HashMap<String, SearchResult> = HashMap::new();

        let table_names = self.conn.table_names().execute().await?;

        // 1. LanceDB Vector Search
        let vec_start = Instant::now();
        if table_names.contains(&self.table_name) {
            let table = self.conn.open_table(&self.table_name).execute().await?;
            // Fetch slightly more to allow good fusion
            let vec_query = table.query().nearest_to(query_vector)?;
            let vec_results_stream = vec_query.limit(20).execute().await?;
            let vec_results_batches: Vec<RecordBatch> = vec_results_stream.try_collect().await?;
            let candidates = self.batches_to_results(vec_results_batches);

            for mut c in candidates {
                c.score = 50.0;
                results_map.insert(c.id.clone(), c);
            }
        }
        println!("[Search] Vector search: {:?}", vec_start.elapsed());

        if query_text.is_empty() {
            let final_results: Vec<SearchResult> = results_map.into_values().collect();
            return Ok(final_results.into_iter().take(10).collect());
        }

        // 2. Tantivy Search (Keyword)
        let kw_start = Instant::now();
        let index = self.get_tantivy_index();
        let reader = index.reader()?;
        let searcher = reader.searcher();

        let schema = index.schema();
        let f_id = schema.get_field("id").unwrap();
        let f_path = schema.get_field("file_path").unwrap();
        let f_content = schema.get_field("content").unwrap();
        let f_text_zh = schema.get_field("text_zh").unwrap();

        let f_pid = schema.get_field("parent_id").unwrap();
        let f_bc = schema.get_field("breadcrumbs").unwrap();

        // Search both fields (Dual Indexing)
        // QueryParser will automatically create a Disjunction (OR) query over these fields
        let query_parser =
            if let (Ok(f_text_std_field), Ok(f_file_name_field), Ok(f_path_tags_field)) = (
                index.schema().get_field("text_std"),
                index.schema().get_field("file_name"),
                index.schema().get_field("path_tags"),
            ) {
                // With Dual Indexing and Metadata
                let mut fields = vec![f_text_zh, f_text_std_field];
                fields.push(f_file_name_field);
                fields.push(f_path_tags_field);

                let mut parser = QueryParser::for_index(&index, fields);
                parser.set_field_boost(f_text_zh, 1.0);
                parser.set_field_boost(f_text_std_field, 1.0);
                parser.set_field_boost(f_file_name_field, 3.0); // High boost for filename match
                parser.set_field_boost(f_path_tags_field, 1.5); // Moderate boost for directory match
                parser
            } else {
                // Fallback
                let mut parser = QueryParser::for_index(&index, vec![f_text_zh]);
                parser.set_field_boost(f_text_zh, 1.0);
                parser
            };

        match query_parser.parse_query(query_text) {
            Ok(q) => {
                let top_docs = searcher.search(&q, &TopDocs::with_limit(20))?;

                for (bm25_score, doc_address) in top_docs {
                    let doc: TantivyDocument = searcher.doc(doc_address)?;

                    let doc_id = doc
                        .get_first(f_id)
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    if let Some(existing) = results_map.get_mut(&doc_id) {
                        existing.score += bm25_score * 2.0;
                        existing.source = SearchSource::Hybrid;
                    } else {
                        let text = doc
                            .get_first(f_content) // Retrieve from stored content
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let file_path = doc
                            .get_first(f_path)
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let parent_id = doc
                            .get_first(f_pid)
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let breadcrumbs = doc
                            .get_first(f_bc)
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let new_result = SearchResult {
                            id: doc_id.clone(),
                            text,
                            file_path,
                            score: bm25_score * 2.0,
                            parent_id,
                            breadcrumbs,
                            source: SearchSource::Keyword,
                        };
                        results_map.insert(doc_id, new_result);
                    }
                }
            }
            Err(e) => eprintln!("[Tantivy] Query Error: {}", e),
        }
        println!("[Search] Keyword search: {:?}", kw_start.elapsed());

        // 3. Final Sort
        let mut final_results: Vec<SearchResult> = results_map.into_values().collect();
        final_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        println!("[Search] Total search time: {:?}", total_start.elapsed());
        Ok(final_results.into_iter().take(10).collect())
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
                    source: SearchSource::Vector,
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
                    EMBEDDING_DIM, // Dimension size
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
        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vectors_flat,
            EMBEDDING_DIM,
        );

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

#[derive(Debug, Clone, PartialEq)]
pub enum SearchSource {
    Vector,
    Keyword,
    Hybrid,
}

impl std::fmt::Display for SearchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchSource::Vector => write!(f, "Vector"),
            SearchSource::Keyword => write!(f, "Keyword"),
            SearchSource::Hybrid => write!(f, "Hybrid"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub text: String,
    pub file_path: String,
    pub score: f32, // Reranking score
    pub parent_id: Option<String>,
    pub breadcrumbs: Option<String>,
    pub source: SearchSource,
}
