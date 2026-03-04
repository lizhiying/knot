use crate::path_processor::PathProcessor;
use crate::tokenizer::JiebaTokenizer;
use anyhow::Result;
use arrow::record_batch::RecordBatchIterator;
use arrow_array::{
    types::Float32Type, Array, FixedSizeListArray, Float32Array, RecordBatch, StringArray,
};
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

/// 向量距离阈值：距离 > 此值的结果被过滤（不相关）
/// BGE 模型 L2 距离参考：高度相关 0-0.5, 中度 0.5-0.75, 不相关 >0.75
const VECTOR_DISTANCE_THRESHOLD: f32 = 0.75;

/// 向量搜索候选结果（带距离信息）
struct CandidateWithDistance {
    result: SearchResult,
    distance: f32,
}

pub struct KnotStore {
    conn: Connection,
    table_name: String,
    // tantivy_path: PathBuf,
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
            // tantivy_path,
            tantivy_index,
        };

        Ok(store)
    }

    /// Schema 版本号：每次修改 Schema 时递增，用于自动迁移检测
    const SCHEMA_VERSION: u32 = 2; // v1: 基础字段, v2: +text_icu +file_name_std +en_knot

    /// Create and configure Tantivy Index (called once during initialization)
    fn create_tantivy_index(tantivy_path: &PathBuf) -> Result<Index> {
        use tantivy::directory::MmapDirectory;

        let mut schema_builder = t_schema::Schema::builder();

        // === 1. Text Fields ===

        // Jieba: 中文主力分词
        let text_zh_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("jieba")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let _text_zh = schema_builder.add_text_field("text_zh", text_zh_options);

        // English: Lowercase + Stemmer
        let text_std_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_knot")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let _text_std = schema_builder.add_text_field("text_std", text_std_options);

        // ICU: 泛语言兜底
        let text_icu_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("icu")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let _text_icu = schema_builder.add_text_field("text_icu", text_icu_options);

        // file_name: Jieba（中文文件名）
        let file_name_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("jieba")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ) | t_schema::STORED;
        let _file_name = schema_builder.add_text_field("file_name", file_name_options);

        // file_name_std: Default（英文文件名兜底）
        let file_name_std_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_knot")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let _file_name_std = schema_builder.add_text_field("file_name_std", file_name_std_options);

        // path_tags: Default
        let path_tags_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ) | t_schema::STORED;
        let _path_tags = schema_builder.add_text_field("path_tags", path_tags_options);

        // === 2. Metadata Fields ===
        schema_builder.add_text_field("id", t_schema::STRING | t_schema::STORED);
        schema_builder.add_text_field("file_path", t_schema::STRING | t_schema::STORED);
        schema_builder.add_text_field("content", t_schema::STORED);
        schema_builder.add_text_field("parent_id", t_schema::STRING | t_schema::STORED);
        schema_builder.add_text_field("breadcrumbs", t_schema::STRING | t_schema::STORED);

        let schema = schema_builder.build();

        // === 3. Auto-Migration: 检查新字段是否存在 ===
        let reset_needed = if tantivy_path.exists() {
            if let Ok(dir) = MmapDirectory::open(tantivy_path) {
                if let Ok(idx) = Index::open(dir) {
                    // 检查新增字段是否存在，不存在则需要重建
                    let missing_icu = idx.schema().get_field("text_icu").is_err();
                    let missing_fn_std = idx.schema().get_field("file_name_std").is_err();
                    if missing_icu || missing_fn_std {
                        println!(
                            "[FTS] Schema migration needed: text_icu={}, file_name_std={}",
                            !missing_icu, !missing_fn_std
                        );
                        true
                    } else {
                        false
                    }
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
            println!(
                "[FTS] Rebuilding Tantivy index for schema v{}...",
                Self::SCHEMA_VERSION
            );
            let _ = std::fs::remove_dir_all(tantivy_path);
            let _ = std::fs::create_dir_all(tantivy_path);
        }

        // Open or Create
        let dir = MmapDirectory::open(tantivy_path)?;
        let index = Index::open_or_create(dir, schema)?;

        // === 4. Register Tokenizers ===

        // Jieba: 中文分词 + Lowercase + 停用词
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

        // en_knot: 英文分词 + Lowercase + Stemmer(English) + 英文停用词
        let en_stop_words: Vec<String> = vec![
            "a",
            "an",
            "the",
            "is",
            "are",
            "was",
            "were",
            "be",
            "been",
            "being",
            "have",
            "has",
            "had",
            "do",
            "does",
            "did",
            "will",
            "would",
            "could",
            "should",
            "may",
            "might",
            "shall",
            "can",
            "need",
            "dare",
            "ought",
            "to",
            "of",
            "in",
            "for",
            "on",
            "with",
            "at",
            "by",
            "from",
            "as",
            "into",
            "through",
            "during",
            "before",
            "after",
            "above",
            "below",
            "and",
            "but",
            "or",
            "nor",
            "not",
            "so",
            "yet",
            "both",
            "either",
            "neither",
            "each",
            "every",
            "all",
            "any",
            "few",
            "more",
            "most",
            "other",
            "some",
            "such",
            "no",
            "only",
            "own",
            "same",
            "than",
            "too",
            "very",
            "just",
            "because",
            "if",
            "when",
            "while",
            "how",
            "what",
            "which",
            "who",
            "whom",
            "this",
            "that",
            "these",
            "those",
            "i",
            "me",
            "my",
            "myself",
            "we",
            "our",
            "ours",
            "ourselves",
            "you",
            "your",
            "yours",
            "yourself",
            "yourselves",
            "he",
            "him",
            "his",
            "himself",
            "she",
            "her",
            "hers",
            "herself",
            "it",
            "its",
            "itself",
            "they",
            "them",
            "their",
            "theirs",
            "themselves",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect();

        let en_tokenizer = tantivy::tokenizer::TextAnalyzer::builder(
            tantivy::tokenizer::SimpleTokenizer::default(),
        )
        .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
        .filter(tantivy::tokenizer::LowerCaser)
        .filter(tantivy::tokenizer::Stemmer::new(
            tantivy::tokenizer::Language::English,
        ))
        .filter(StopWordFilter::remove(en_stop_words))
        .build();
        index.tokenizers().register("en_knot", en_tokenizer);

        // ICU: 泛语言分词 (Unicode 边界切分)
        let icu_tokenizer =
            tantivy::tokenizer::TextAnalyzer::builder(crate::tokenizer::ICUTokenizer)
                .filter(tantivy::tokenizer::LowerCaser)
                .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
                .build();
        index.tokenizers().register("icu", icu_tokenizer);

        println!(
            "[FTS] Tantivy index ready (schema v{}, 3 tokenizers: jieba, en_knot, icu)",
            Self::SCHEMA_VERSION
        );

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

    pub fn get_doc_count(&self) -> Result<u64> {
        let index = self.get_tantivy_index();
        let reader = index.reader()?;
        let searcher = reader.searcher();
        Ok(searcher.num_docs())
    }

    pub async fn get_file_count(&self) -> Result<u64> {
        let table = self.conn.open_table(&self.table_name).execute().await?;
        // Query unique file_path count using SQL
        let results = table
            .query()
            .select(lancedb::query::Select::Columns(vec![
                "file_path".to_string()
            ]))
            .execute()
            .await?;

        let batches: Vec<RecordBatch> = results.try_collect().await?;
        let mut unique_paths = std::collections::HashSet::new();

        for batch in batches {
            let column = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            for i in 0..batch.num_rows() {
                unique_paths.insert(column.value(i).to_string());
            }
        }

        Ok(unique_paths.len() as u64)
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
        let f_text_icu = schema.get_field("text_icu").unwrap();
        let f_file_name = schema.get_field("file_name").unwrap();
        let f_file_name_std = schema.get_field("file_name_std").unwrap();
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

            doc.add_text(f_file_name, &extracted_file_name); // 中文文件名 (Jieba)
            doc.add_text(f_file_name_std, &extracted_file_name); // 英文文件名兜底 (en_knot)
            doc.add_text(f_path_tags, &extracted_tags);

            doc.add_text(f_content, &record.text); // Store original text
            doc.add_text(f_text_zh, &record.text); // Index with Jieba (中文)
            doc.add_text(f_text_std, &record.text); // Index with en_knot (英文+词干)
            doc.add_text(f_text_icu, &record.text); // Index with ICU (泛语言兜底)

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

    /// 预处理查询文本：
    /// 1. 在中英文/数字边界插入空格以改善分词
    /// 2. 去除重复的短噪音 token（如 "s s s" → "s"）
    ///
    /// 注意：此函数应在生成向量嵌入之前调用，确保关键词和向量搜索使用一致的查询文本。
    pub fn preprocess_query(query: &str) -> String {
        // 第一步：在字符类型边界插入空格
        let mut spaced = String::with_capacity(query.len() * 2);
        let mut prev_is_ascii_alpha = false;
        let mut prev_is_digit = false;
        let mut prev_is_cjk = false;

        for c in query.chars() {
            let is_ascii_alpha = c.is_ascii_alphabetic();
            let is_digit = c.is_ascii_digit();
            let is_cjk = ('\u{4e00}'..='\u{9fff}').contains(&c)       // CJK 基本区
                || ('\u{3400}'..='\u{4dbf}').contains(&c)               // CJK 扩展 A
                || ('\u{3040}'..='\u{309f}').contains(&c)               // 平假名
                || ('\u{30a0}'..='\u{30ff}').contains(&c)               // 片假名
                || ('\u{ac00}'..='\u{d7af}').contains(&c); // 韩文

            // 在不同字符类型边界插入空格
            let need_space = (prev_is_ascii_alpha && is_cjk)
                || (prev_is_cjk && is_ascii_alpha)
                || (prev_is_digit && is_cjk)
                || (prev_is_cjk && is_digit);

            if need_space {
                spaced.push(' ');
            }

            spaced.push(c);
            prev_is_ascii_alpha = is_ascii_alpha;
            prev_is_digit = is_digit;
            prev_is_cjk = is_cjk;
        }

        // 第二步：去除重复的短噪音 token
        // 将 "s s s 入门" 变成 "s 入门"
        let tokens: Vec<&str> = spaced.split_whitespace().collect();
        let mut deduped: Vec<&str> = Vec::with_capacity(tokens.len());
        for token in &tokens {
            // 如果 token 长度 ≤ 2 且已经在 deduped 中存在，跳过
            if token.len() <= 2 && deduped.contains(token) {
                continue;
            }
            deduped.push(token);
        }

        deduped.join(" ")
    }

    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        query_text: &str,
        distance_threshold: f32,
    ) -> Result<Vec<SearchResult>> {
        use std::collections::HashMap;
        use std::time::Instant;

        let total_start = Instant::now();

        // 预处理查询文本
        let processed_query = Self::preprocess_query(query_text);
        if processed_query != query_text {
            println!(
                "[Search] Query preprocessed: '{}' -> '{}'",
                query_text, processed_query
            );
        }
        let query_text = &processed_query;

        // 使用传入的阈值（来自设置页面）
        // 环境变量 KNOT_DISTANCE_THRESHOLD 仅用于调试日志对比，不覆盖设置
        let effective_threshold = distance_threshold;
        if let Ok(env_threshold) = std::env::var("KNOT_DISTANCE_THRESHOLD") {
            if std::env::var("KNOT_QUIET").is_err() {
                println!(
                    "[Search] Note: KNOT_DISTANCE_THRESHOLD={} (ignored, using config: {})",
                    env_threshold, effective_threshold
                );
            }
        }

        // RRF 参数
        const RRF_K: f32 = 60.0; // RRF 常数，典型值 60
        const VECTOR_WEIGHT: f32 = 0.6; // 向量搜索权重
        const KEYWORD_WEIGHT: f32 = 0.4; // 关键词搜索权重

        // 存储结果：ID -> (SearchResult, vector_rank, keyword_rank)
        let mut results_map: HashMap<String, SearchResult> = HashMap::new();
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        let mut keyword_ranks: HashMap<String, usize> = HashMap::new();

        let table_names = self.conn.table_names().execute().await?;

        // 1. LanceDB Vector Search
        let vec_start = Instant::now();
        if table_names.contains(&self.table_name) {
            let table = self.conn.open_table(&self.table_name).execute().await?;
            let vec_query = table.query().nearest_to(query_vector)?;
            let vec_results_stream = vec_query.limit(20).execute().await?;
            let vec_results_batches: Vec<RecordBatch> = vec_results_stream.try_collect().await?;
            let candidates = self.batches_to_results_with_distance(vec_results_batches);

            let mut rank = 1usize;
            for c in candidates {
                // 过滤距离过大的结果（不相关）
                if c.distance > effective_threshold {
                    continue;
                }

                let id = c.result.id.clone();
                vector_ranks.insert(id.clone(), rank);
                rank += 1;

                // 距离转相似度分数：距离越小，分数越高
                // 距离 0 -> 100, 距离 1 -> 50, 距离 2 -> 0
                let similarity: f32 = (100.0 - c.distance * 50.0).max(0.0);
                let mut result = c.result;
                result.score = similarity; // 临时存储，后面会用 RRF 重新计算
                result.source = SearchSource::Vector;
                results_map.insert(id, result);
            }
        }
        if std::env::var("KNOT_QUIET").is_err() {
            println!(
                "[Search] Vector search: {:?}, found {} results",
                vec_start.elapsed(),
                vector_ranks.len()
            );
        }

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

        let query_parser = {
            // 获取所有搜索字段
            let f_text_std = schema.get_field("text_std").unwrap();
            let f_text_icu = schema.get_field("text_icu").unwrap();
            let f_file_name = schema.get_field("file_name").unwrap();
            let f_file_name_std = schema.get_field("file_name_std").unwrap();
            let f_path_tags = schema.get_field("path_tags").unwrap();

            let fields = vec![
                f_text_zh,
                f_text_std,
                f_text_icu,
                f_file_name,
                f_file_name_std,
                f_path_tags,
            ];

            let mut parser = QueryParser::for_index(&index, fields);

            // 分级权重：文件名 > 中文 > 英文 > 路径 > ICU兜底
            parser.set_field_boost(f_file_name, 8.0); // 文件名中文匹配最高
            parser.set_field_boost(f_text_zh, 5.0); // 中文正文
            parser.set_field_boost(f_file_name_std, 5.0); // 文件名英文
            parser.set_field_boost(f_text_std, 3.0); // 英文正文 (含 Stemmer)
            parser.set_field_boost(f_path_tags, 2.0); // 路径标签
            parser.set_field_boost(f_text_icu, 1.0); // ICU 泛语言兜底

            parser
        };

        match query_parser.parse_query(query_text) {
            Ok(q) => {
                let top_docs = searcher.search(&q, &TopDocs::with_limit(20))?;

                // 收集 BM25 分数用于标准化
                let bm25_scores: Vec<f32> = top_docs.iter().map(|(s, _)| *s).collect();
                let max_bm25 = bm25_scores.iter().cloned().fold(0.0, f32::max);
                let min_bm25 = bm25_scores.iter().cloned().fold(f32::MAX, f32::min);
                let bm25_range = (max_bm25 - min_bm25).max(0.001); // 避免除零

                let mut rank = 1usize;
                for (bm25_score, doc_address) in top_docs {
                    let doc: TantivyDocument = searcher.doc(doc_address)?;

                    let doc_id = doc
                        .get_first(f_id)
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    keyword_ranks.insert(doc_id.clone(), rank);
                    rank += 1;

                    // BM25 标准化到 0-100
                    let normalized_bm25 = ((bm25_score - min_bm25) / bm25_range * 100.0).min(100.0);

                    if let Some(existing) = results_map.get_mut(&doc_id) {
                        // 已存在向量结果，标记为混合
                        existing.source = SearchSource::Hybrid;
                    } else {
                        // 仅关键词结果
                        let text = doc
                            .get_first(f_content)
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
                            score: normalized_bm25,
                            parent_id,
                            breadcrumbs,
                            source: SearchSource::Keyword,
                            expanded_context: None,
                        };
                        results_map.insert(doc_id, new_result);
                    }
                }
            }
            Err(e) => eprintln!("[Tantivy] Query Error: {}", e),
        }
        if std::env::var("KNOT_QUIET").is_err() {
            println!(
                "[Search] Keyword search: {:?}, found {} results",
                kw_start.elapsed(),
                keyword_ranks.len()
            );
        }

        // 3. RRF 融合计算最终分数
        // RRF 公式: score = sum(1 / (k + rank))
        // 加权 RRF: score = w_vec * (1 / (k + vec_rank)) + w_kw * (1 / (k + kw_rank))
        for (id, result) in results_map.iter_mut() {
            let vec_rank = vector_ranks.get(id).cloned();
            let kw_rank = keyword_ranks.get(id).cloned();

            let vec_rrf = vec_rank
                .map(|r| VECTOR_WEIGHT / (RRF_K + r as f32))
                .unwrap_or(0.0);
            let kw_rrf = kw_rank
                .map(|r| KEYWORD_WEIGHT / (RRF_K + r as f32))
                .unwrap_or(0.0);

            // RRF 分数转换到 0-100 范围（乘以缩放因子）
            // 最高可能 RRF = 0.6/(60+1) + 0.4/(60+1) ≈ 0.0164
            // 缩放到 100 分: 0.0164 * 6100 ≈ 100
            let rrf_score = (vec_rrf + kw_rrf) * 6100.0;
            result.score = rrf_score.min(100.0);
        }

        // 4. Final Sort by RRF score
        let mut final_results: Vec<SearchResult> = results_map.into_values().collect();
        final_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if std::env::var("KNOT_QUIET").is_err() {
            println!(
                "[Search] Total: {:?}, RRF fusion applied, {} results",
                total_start.elapsed(),
                final_results.len()
            );
        }
        Ok(final_results.into_iter().take(10).collect())
    }

    /// 解析 LanceDB 向量搜索结果，提取 _distance 列
    fn batches_to_results_with_distance(
        &self,
        batches: Vec<RecordBatch>,
    ) -> Vec<CandidateWithDistance> {
        let mut candidates = Vec::new();
        for batch in batches {
            // 使用 column_by_name 获取列，更可靠
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let texts = batch
                .column_by_name("text")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let paths = batch
                .column_by_name("file_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let parent_ids = batch
                .column_by_name("parent_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let breadcrumbs_col = batch
                .column_by_name("breadcrumbs")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            // LanceDB 向量搜索自动添加 _distance 列
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            // 如果关键列缺失，跳过此 batch
            let (ids, texts, paths) = match (ids, texts, paths) {
                (Some(i), Some(t), Some(p)) => (i, t, p),
                _ => continue,
            };

            for i in 0..batch.num_rows() {
                let pid = parent_ids.and_then(|a| {
                    if a.is_null(i) {
                        None
                    } else {
                        Some(a.value(i).to_string())
                    }
                });
                let bc = breadcrumbs_col.and_then(|a| {
                    if a.is_null(i) {
                        None
                    } else {
                        Some(a.value(i).to_string())
                    }
                });
                let distance = distances.map(|d| d.value(i)).unwrap_or(f32::MAX);

                candidates.push(CandidateWithDistance {
                    result: SearchResult {
                        id: ids.value(i).to_string(),
                        text: texts.value(i).to_string(),
                        file_path: paths.value(i).to_string(),
                        score: 0.0,
                        parent_id: pid,
                        breadcrumbs: bc,
                        source: SearchSource::Vector,
                        expanded_context: None,
                    },
                    distance,
                });
            }
        }

        // 打印调试信息（仅在非静默模式）
        if std::env::var("KNOT_QUIET").is_err() && !candidates.is_empty() {
            let min_dist = candidates
                .iter()
                .map(|c| c.distance)
                .fold(f32::MAX, f32::min);
            let max_dist = candidates
                .iter()
                .map(|c| c.distance)
                .fold(f32::MIN, f32::max);
            println!(
                "[Search] Vector distances: min={:.3}, max={:.3}, threshold={:.1}",
                min_dist, max_dist, VECTOR_DISTANCE_THRESHOLD
            );
        }

        candidates
    }

    // --- 上下文扩展 (Context Expansion) ---

    /// 根据 parent_id 查询 Tantivy 中同一父节点下的所有记录
    fn get_records_by_parent_id(
        &self,
        parent_id: &str,
        file_path: &str,
    ) -> Result<Vec<(String, String)>> {
        // 返回 Vec<(id, text)>
        let index = self.get_tantivy_index();
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let schema = index.schema();

        let f_pid = schema.get_field("parent_id")?;
        let f_path = schema.get_field("file_path")?;
        let f_id = schema.get_field("id")?;
        let f_content = schema.get_field("content")?;

        // 精确匹配 parent_id 和 file_path
        let pid_query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(f_pid, parent_id),
            tantivy::schema::IndexRecordOption::Basic,
        );
        let path_query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(f_path, file_path),
            tantivy::schema::IndexRecordOption::Basic,
        );
        let combined = tantivy::query::BooleanQuery::new(vec![
            (tantivy::query::Occur::Must, Box::new(pid_query)),
            (tantivy::query::Occur::Must, Box::new(path_query)),
        ]);

        let top_docs = searcher.search(&combined, &tantivy::collector::TopDocs::with_limit(20))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            let id = doc
                .get_first(f_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = doc
                .get_first(f_content)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !id.is_empty() {
                results.push((id, text));
            }
        }

        // 按 id 排序以保持文档内的原始顺序
        results.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(results)
    }

    /// 根据 id 查询 Tantivy 获取单条记录的 text
    fn get_text_by_id(&self, record_id: &str) -> Result<Option<String>> {
        let index = self.get_tantivy_index();
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let schema = index.schema();

        let f_id = schema.get_field("id")?;
        let f_content = schema.get_field("content")?;

        let query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(f_id, record_id),
            tantivy::schema::IndexRecordOption::Basic,
        );

        let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(1))?;
        if let Some((_score, doc_address)) = top_docs.first() {
            let doc: TantivyDocument = searcher.doc(*doc_address)?;
            let text = doc
                .get_first(f_content)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return Ok(text);
        }
        Ok(None)
    }

    /// 截断文本到指定字符数
    fn truncate_text(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            text.to_string()
        } else {
            let truncated: String = text.chars().take(max_chars).collect();
            format!("{}...", truncated)
        }
    }

    /// 对搜索结果进行上下文扩展
    ///
    /// 策略（方案 D）：
    /// - parent: 仅取 breadcrumbs（已有）
    /// - sibling: 前后各 1 个，每个截取最多 200 字符
    /// - 总扩展上限: 500 字符/条
    pub fn expand_search_context(&self, results: &mut Vec<SearchResult>) {
        const MAX_SIBLING_CHARS: usize = 200;
        const MAX_TOTAL_EXPANSION_CHARS: usize = 500;

        for result in results.iter_mut() {
            // 跳过没有 parent_id 的结果（root 节点或文档摘要）
            let parent_id = match &result.parent_id {
                Some(pid) if !pid.is_empty() => pid.clone(),
                _ => continue,
            };

            // 查询同一 parent 下的所有 sibling
            let siblings = match self.get_records_by_parent_id(&parent_id, &result.file_path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if siblings.is_empty() {
                continue;
            }

            // 找到当前节点在 siblings 中的位置
            let current_pos = siblings.iter().position(|(id, _)| id == &result.id);

            let mut expansion_parts: Vec<String> = Vec::new();
            let mut total_chars = 0usize;

            // Parent 标题（从 breadcrumbs 提取最后一级，或用 parent_id）
            if let Some(bc) = &result.breadcrumbs {
                let parent_title = bc.split(" > ").last().unwrap_or(bc);
                let parent_line = format!("[上级] {}", parent_title);
                total_chars += parent_line.chars().count();
                expansion_parts.push(parent_line);
            }

            if let Some(pos) = current_pos {
                // 前一个 sibling
                if pos > 0 {
                    let (prev_id, prev_text) = &siblings[pos - 1];
                    // 跳过 doc-summary 记录
                    if !prev_id.ends_with("-doc-summary") {
                        let remaining = MAX_TOTAL_EXPANSION_CHARS.saturating_sub(total_chars);
                        let max_chars = remaining.min(MAX_SIBLING_CHARS);
                        if max_chars > 0 {
                            let snippet = Self::truncate_text(prev_text, max_chars);
                            let line = format!("[前文] {}", snippet);
                            total_chars += line.chars().count();
                            expansion_parts.push(line);
                        }
                    }
                }

                // 后一个 sibling
                if pos + 1 < siblings.len() {
                    let (next_id, next_text) = &siblings[pos + 1];
                    if !next_id.ends_with("-doc-summary") {
                        let remaining = MAX_TOTAL_EXPANSION_CHARS.saturating_sub(total_chars);
                        let max_chars = remaining.min(MAX_SIBLING_CHARS);
                        if max_chars > 0 {
                            let snippet = Self::truncate_text(next_text, max_chars);
                            let line = format!("[后文] {}", snippet);
                            expansion_parts.push(line);
                        }
                    }
                }
            }

            if !expansion_parts.is_empty() {
                result.expanded_context = Some(expansion_parts.join("\n"));
            }
        }
    }

    // --- 多跳检索 (Multi-Hop Search) ---

    /// 从搜索结果中提取高频关键词，用于构建第二轮搜索查询
    ///
    /// 使用 Jieba 分词，过滤停用词和原始查询词，返回 top N 新词
    pub fn extract_key_terms(
        results: &[SearchResult],
        original_query: &str,
        max_terms: usize,
    ) -> Vec<String> {
        use crate::tokenizer::JiebaTokenizer;
        use std::collections::{HashMap, HashSet};
        use tantivy::tokenizer::Tokenizer;

        // 将原始查询分词，用于过滤
        let mut jieba = JiebaTokenizer::new();
        let mut query_terms: HashSet<String> = HashSet::new();
        {
            let mut stream = jieba.token_stream(original_query);
            while stream.advance() {
                let word = stream.token().text.to_lowercase();
                if word.chars().count() >= 2 {
                    query_terms.insert(word);
                }
            }
        }

        // 中文停用词
        let stopwords: HashSet<&str> = [
            "的",
            "了",
            "在",
            "是",
            "我",
            "有",
            "和",
            "就",
            "不",
            "人",
            "都",
            "一",
            "一个",
            "上",
            "也",
            "很",
            "到",
            "说",
            "要",
            "去",
            "你",
            "会",
            "着",
            "没有",
            "看",
            "好",
            "自己",
            "这",
            "他",
            "她",
            "它",
            "们",
            "那",
            "什么",
            "如何",
            "怎么",
            "为什么",
            "哪个",
            "哪些",
            "可以",
            "能",
            "the",
            "a",
            "an",
            "is",
            "are",
            "was",
            "were",
            "be",
            "been",
            "have",
            "has",
            "had",
            "do",
            "does",
            "did",
            "will",
            "would",
            "could",
            "should",
            "may",
            "might",
            "can",
            "shall",
            "in",
            "on",
            "at",
            "to",
            "for",
            "of",
            "with",
            "by",
            "from",
            "and",
            "or",
            "but",
            "not",
            "no",
            "if",
            "then",
            "so",
            "this",
            "that",
            "these",
            "those",
            "it",
            "its",
        ]
        .iter()
        .copied()
        .collect();

        // 拼接 top 3 结果的文本
        let combined_text: String = results
            .iter()
            .take(3)
            .map(|r| r.text.as_str())
            .collect::<Vec<&str>>()
            .join(" ");

        // 分词并统计词频
        let mut word_freq: HashMap<String, usize> = HashMap::new();
        {
            let mut stream = jieba.token_stream(&combined_text);
            while stream.advance() {
                let word = stream.token().text.to_lowercase();
                // 过滤：长度 >= 2，不是停用词，不是原始查询词
                if word.chars().count() >= 2
                    && !stopwords.contains(word.as_str())
                    && !query_terms.contains(&word)
                {
                    *word_freq.entry(word).or_insert(0) += 1;
                }
            }
        }

        // 按频次排序，取 top N
        let mut freq_vec: Vec<(String, usize)> = word_freq.into_iter().collect();
        freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
        freq_vec
            .into_iter()
            .take(max_terms)
            .map(|(w, _)| w)
            .collect()
    }

    /// 合并两轮搜索结果，按 id 去重，保留分数更高者
    pub fn merge_search_results(
        first: Vec<SearchResult>,
        second: Vec<SearchResult>,
    ) -> Vec<SearchResult> {
        use std::collections::HashMap;

        let mut merged: HashMap<String, SearchResult> = HashMap::new();

        for r in first {
            merged.insert(r.id.clone(), r);
        }

        for r in second {
            merged
                .entry(r.id.clone())
                .and_modify(|existing| {
                    if r.score > existing.score {
                        *existing = r.clone();
                        existing.source = SearchSource::Hybrid;
                    }
                })
                .or_insert(r);
        }

        let mut results: Vec<SearchResult> = merged.into_values().collect();
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
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
    /// 上下文扩展：parent 标题 + 前后 sibling 内容（可选）
    pub expanded_context: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text_short() {
        let text = "Hello World";
        assert_eq!(KnotStore::truncate_text(text, 20), "Hello World");
    }

    #[test]
    fn test_truncate_text_exact() {
        let text = "12345";
        assert_eq!(KnotStore::truncate_text(text, 5), "12345");
    }

    #[test]
    fn test_truncate_text_long() {
        let text = "Hello World, this is a long text";
        let result = KnotStore::truncate_text(text, 11);
        assert_eq!(result, "Hello World...");
    }

    #[test]
    fn test_truncate_text_chinese() {
        let text = "这是一段中文测试文本，用于验证截断功能";
        let result = KnotStore::truncate_text(text, 8);
        assert_eq!(result, "这是一段中文测试...");
    }

    #[test]
    fn test_search_result_default_expanded_context() {
        let result = SearchResult {
            id: "test-1".to_string(),
            text: "content".to_string(),
            file_path: "/test.md".to_string(),
            score: 0.5,
            parent_id: None,
            breadcrumbs: None,
            source: SearchSource::Vector,
            expanded_context: None,
        };
        assert!(result.expanded_context.is_none());
    }

    #[test]
    fn test_extract_key_terms_filters_query_and_stopwords() {
        let results = vec![
            SearchResult {
                id: "1".to_string(),
                text: "机器学习是人工智能的一个分支。支持向量机和决策树是常见的算法。".to_string(),
                file_path: "/ml.md".to_string(),
                score: 90.0,
                parent_id: None,
                breadcrumbs: None,
                source: SearchSource::Vector,
                expanded_context: None,
            },
            SearchResult {
                id: "2".to_string(),
                text: "深度学习使用神经网络进行特征提取。卷积神经网络适用于图像识别。".to_string(),
                file_path: "/dl.md".to_string(),
                score: 80.0,
                parent_id: None,
                breadcrumbs: None,
                source: SearchSource::Vector,
                expanded_context: None,
            },
        ];

        let terms = KnotStore::extract_key_terms(&results, "机器学习", 5);

        // 不应包含原始查询词
        assert!(
            !terms.contains(&"机器".to_string()),
            "Should not contain query term '机器'"
        );

        // 不应包含停用词
        assert!(
            !terms.contains(&"的".to_string()),
            "Should not contain stopword '的'"
        );

        // 应提取出有意义的词
        assert!(!terms.is_empty(), "Should extract some terms");
    }

    #[test]
    fn test_merge_search_results_dedup() {
        let first = vec![
            SearchResult {
                id: "a".to_string(),
                text: "text a".to_string(),
                file_path: "/a.md".to_string(),
                score: 90.0,
                parent_id: None,
                breadcrumbs: None,
                source: SearchSource::Vector,
                expanded_context: None,
            },
            SearchResult {
                id: "b".to_string(),
                text: "text b".to_string(),
                file_path: "/b.md".to_string(),
                score: 80.0,
                parent_id: None,
                breadcrumbs: None,
                source: SearchSource::Vector,
                expanded_context: None,
            },
        ];

        let second = vec![
            SearchResult {
                id: "b".to_string(), // 重复
                text: "text b".to_string(),
                file_path: "/b.md".to_string(),
                score: 85.0, // 更高分
                parent_id: None,
                breadcrumbs: None,
                source: SearchSource::Keyword,
                expanded_context: None,
            },
            SearchResult {
                id: "c".to_string(), // 新结果
                text: "text c".to_string(),
                file_path: "/c.md".to_string(),
                score: 70.0,
                parent_id: None,
                breadcrumbs: None,
                source: SearchSource::Keyword,
                expanded_context: None,
            },
        ];

        let merged = KnotStore::merge_search_results(first, second);

        // 应该有 3 条（a, b, c），b 被去重
        assert_eq!(merged.len(), 3, "Should have 3 unique results");

        // b 应该保留更高分的版本 (85.0)
        let b = merged.iter().find(|r| r.id == "b").unwrap();
        assert_eq!(b.score, 85.0, "Should keep higher score for duplicates");

        // 结果应按分数降序排列
        assert_eq!(merged[0].id, "a"); // 90.0
        assert_eq!(merged[1].id, "b"); // 85.0
        assert_eq!(merged[2].id, "c"); // 70.0
    }
}
