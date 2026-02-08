use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use knot_core::embedding::{EmbeddingEngine, ThreadSafeEmbeddingEngine};
use knot_core::index::KnotIndexer;
use knot_core::llm::{LlamaClient, LlamaSidecar};
use knot_core::store::KnotStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const EMBEDDING_DIM: usize = 512;
const LLM_PORT: u16 = 8081;

/// 获取 Knot 根目录 (~/.knot)
fn get_knot_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(".knot")
}

/// 获取默认模型目录
fn get_models_dir() -> PathBuf {
    get_knot_root().join("models")
}

/// 获取默认索引目录
fn get_indexes_dir() -> PathBuf {
    get_knot_root().join("indexes")
}

/// 获取 bin 目录 (llama-server)
fn get_bin_dir() -> PathBuf {
    // 优先从项目 knot-app/bin 目录加载
    let project_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("knot-app")
        .join("bin");
    if project_bin.join("llama").exists() {
        return project_bin;
    }

    // 尝试从 knot-workspaces/bin 加载
    let workspace_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bin");
    if workspace_bin.join("llama").exists() {
        return workspace_bin;
    }

    // 回退到 ~/.knot/bin
    get_knot_root().join("bin")
}

/// 根据输入目录计算索引路径
fn get_index_path_for_source(source_dir: &Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let abs_path = std::fs::canonicalize(source_dir).unwrap_or_else(|_| source_dir.to_path_buf());
    let path_str = abs_path.to_string_lossy();

    let mut hasher = DefaultHasher::new();
    path_str.hash(&mut hasher);
    let hash = hasher.finish();

    get_indexes_dir().join(format!("{:016x}", hash))
}

#[derive(Parser)]
#[command(name = "knot-cli")]
#[command(about = "CLI for Knot RAG Engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, ValueEnum)]
enum ModelType {
    Embedding,
    Llm,
    Ocr,
    All,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system status
    Status,

    /// Download required models
    Download {
        /// Which model to download
        #[arg(long, value_enum, default_value = "all")]
        model: ModelType,
    },

    /// Index a directory
    Index {
        /// Path to the directory to index
        #[arg(short, long)]
        input: PathBuf,

        /// Custom embedding model path
        #[arg(long)]
        embedding_model: Option<PathBuf>,
    },

    /// Query the index
    Query {
        /// The query text
        #[arg(short, long)]
        text: String,

        /// Limit search to a specific source directory
        #[arg(long)]
        source: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Ask a question using RAG
    Ask {
        /// The question
        #[arg(short, long)]
        query: String,

        /// Limit search to a specific source directory
        #[arg(long)]
        source: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Watch a directory for changes
    Watch {
        /// Path to watch
        #[arg(short, long)]
        input: PathBuf,
    },
}

/// 初始化 Embedding 引擎
fn init_embedding_engine(model_path: Option<&Path>) -> Result<ThreadSafeEmbeddingEngine> {
    let default_model = get_models_dir().join("bge-small-zh-v1.5.onnx");
    let model_path = model_path.unwrap_or(&default_model);

    if !model_path.exists() {
        anyhow::bail!(
            "Embedding model not found: {:?}\n\
             Run 'knot-cli download --model embedding' to install.",
            model_path
        );
    }

    let engine = EmbeddingEngine::init_onnx(model_path.to_str().unwrap())?;
    Ok(ThreadSafeEmbeddingEngine::new(engine))
}

/// 模型下载 URL 配置
fn get_model_url(filename: &str) -> String {
    let base = "https://huggingface.co";
    let path = match filename {
        "bge-small-zh-v1.5.onnx" => "nickmuchi/bge-small-zh-v1.5-onnx/resolve/main/model.onnx",
        "bge-small-zh-v1.5-tokenizer.json" => "BAAI/bge-small-zh-v1.5/resolve/main/tokenizer.json",
        "Qwen3-1.7B-Q4_K_M.gguf" => "unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf",
        "OCRFlux-3B.Q4_K_M.gguf" => {
            "mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.Q4_K_M.gguf"
        }
        "OCRFlux-3B.mmproj-f16.gguf" => {
            "mradermacher/OCRFlux-3B-GGUF/resolve/main/OCRFlux-3B.mmproj-f16.gguf"
        }
        _ => return format!("{}/unknown/{}", base, filename),
    };
    format!("{}/{}", base, path)
}

/// 下载文件到指定路径
async fn download_file(url: &str, path: &Path) -> Result<()> {
    use futures_util::StreamExt;
    use indicatif::{ProgressBar, ProgressStyle};
    use tokio::io::AsyncWriteExt;

    println!("Downloading: {}", url);
    println!("       To: {:?}", path);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);

    // 创建进度条
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // 确保父目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create file: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| anyhow::anyhow!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| anyhow::anyhow!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Done");
    println!("✓ Download complete!\n");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            println!("Knot CLI Status");
            println!("===============\n");

            // Models
            println!("Models:");
            let models_dir = get_models_dir();

            // Embedding
            let embedding_path = models_dir.join("bge-small-zh-v1.5.onnx");
            if embedding_path.exists() {
                println!(
                    "  ✓ Embedding:  bge-small-zh-v1.5.onnx ({} dim)",
                    EMBEDDING_DIM
                );
            } else {
                println!("  ✗ Embedding:  bge-small-zh-v1.5.onnx (missing)");
                println!("    → Run 'knot-cli download --model embedding' to install");
            }

            // LLM
            let llm_path = models_dir.join("Qwen3-1.7B-Q4_K_M.gguf");
            if llm_path.exists() {
                println!("  ✓ LLM:        Qwen3-1.7B-Q4_K_M.gguf");
            } else {
                println!("  ✗ LLM:        Qwen3-1.7B-Q4_K_M.gguf (missing)");
                println!("    → Run 'knot-cli download --model llm' to install");
            }

            // OCR
            let ocr_path = models_dir.join("OCRFlux-3B.Q4_K_M.gguf");
            if ocr_path.exists() {
                println!("  ✓ OCR:        OCRFlux-3B.Q4_K_M.gguf");
            } else {
                println!("  ✗ OCR:        OCRFlux-3B.Q4_K_M.gguf (missing)");
                println!("    → Run 'knot-cli download --model ocr' to install");
            }

            // Indexed Sources
            println!("\nIndexed Sources:");
            let indexes_dir = get_indexes_dir();
            if indexes_dir.exists() {
                let mut found_any = false;
                if let Ok(entries) = std::fs::read_dir(&indexes_dir) {
                    for (i, entry) in entries.flatten().enumerate() {
                        let lance_path = entry.path().join("knot_index.lance");
                        if lance_path.exists() {
                            found_any = true;
                            if let Ok(store) = KnotStore::new(lance_path.to_str().unwrap()).await {
                                let doc_count = store.get_doc_count().unwrap_or(0);
                                let file_count = store.get_file_count().await.unwrap_or(0);
                                println!("  {}. {:?}", i + 1, entry.path());
                                println!("     Files: {} | Chunks: {}", file_count, doc_count);
                            }
                        }
                    }
                }
                if !found_any {
                    println!("  (none)");
                }
            } else {
                println!("  (none)");
            }

            // Paths
            println!("\nPaths:");
            println!("  Knot Root:      {:?}", get_knot_root());
            println!(
                "  Config Home:    {:?}",
                get_knot_root().join("config.toml")
            );
            println!("  Index Storage:  {:?}", get_indexes_dir());
            println!("  Models:         {:?}", get_models_dir());
        }

        Commands::Download { model } => {
            println!("Knot Model Downloader");
            println!("=====================\n");

            let models_dir = get_models_dir();
            std::fs::create_dir_all(&models_dir)?;

            match model {
                ModelType::Embedding | ModelType::All => {
                    let onnx_path = models_dir.join("bge-small-zh-v1.5.onnx");
                    let tokenizer_path = models_dir.join("bge-small-zh-v1.5-tokenizer.json");

                    if !onnx_path.exists() {
                        download_file(&get_model_url("bge-small-zh-v1.5.onnx"), &onnx_path).await?;
                    } else {
                        println!("✓ Embedding model already exists: {:?}", onnx_path);
                    }

                    if !tokenizer_path.exists() {
                        download_file(
                            &get_model_url("bge-small-zh-v1.5-tokenizer.json"),
                            &tokenizer_path,
                        )
                        .await?;
                    } else {
                        println!("✓ Tokenizer already exists: {:?}", tokenizer_path);
                    }

                    if matches!(model, ModelType::Embedding) {
                        return Ok(());
                    }
                }
                _ => {}
            }

            match model {
                ModelType::Llm | ModelType::All => {
                    let llm_path = models_dir.join("Qwen3-1.7B-Q4_K_M.gguf");
                    if !llm_path.exists() {
                        download_file(&get_model_url("Qwen3-1.7B-Q4_K_M.gguf"), &llm_path).await?;
                    } else {
                        println!("✓ LLM model already exists: {:?}", llm_path);
                    }

                    if matches!(model, ModelType::Llm) {
                        return Ok(());
                    }
                }
                _ => {}
            }

            match model {
                ModelType::Ocr | ModelType::All => {
                    let ocr_path = models_dir.join("OCRFlux-3B.Q4_K_M.gguf");
                    let mmproj_path = models_dir.join("OCRFlux-3B.mmproj-f16.gguf");

                    if !ocr_path.exists() {
                        download_file(&get_model_url("OCRFlux-3B.Q4_K_M.gguf"), &ocr_path).await?;
                    } else {
                        println!("✓ OCR model already exists: {:?}", ocr_path);
                    }

                    if !mmproj_path.exists() {
                        download_file(&get_model_url("OCRFlux-3B.mmproj-f16.gguf"), &mmproj_path)
                            .await?;
                    } else {
                        println!("✓ OCR mmproj already exists: {:?}", mmproj_path);
                    }
                }
                _ => {}
            }

            println!("\n✓ All requested models are ready!");
        }

        Commands::Index {
            input,
            embedding_model,
        } => {
            println!("Indexing directory: {:?}", input);

            // Initialize embedding engine
            let embedding_engine = init_embedding_engine(embedding_model.as_deref())?;
            let provider = Arc::new(embedding_engine);

            // Calculate index path based on source directory
            let index_path = get_index_path_for_source(&input);
            let lance_path = index_path.join("knot_index.lance");

            println!("Index path: {:?}", lance_path);

            // Create index directory
            std::fs::create_dir_all(&index_path)?;

            // Create registry path
            let registry_path = index_path.join("knot.db");

            let indexer = KnotIndexer::new(registry_path.to_str().unwrap(), Some(provider)).await;
            let (records, deleted_files) = indexer.index_directory(&input).await?;

            println!("Found {} vectors to add.", records.len());
            println!("Found {} files to delete.", deleted_files.len());

            let store = KnotStore::new(lance_path.to_str().unwrap()).await?;

            // Handle deletions
            for del_path in deleted_files {
                println!("Deleting from store: {}", del_path);
                store.delete_file(&del_path).await?;
            }

            store.add_records(records).await?;
            store.create_fts_index().await?;

            println!("Indexing complete.");
        }

        Commands::Query { text, source, json } => {
            if !json {
                println!("Querying: {}", text);
            }

            // Initialize embedding engine for query
            let embedding_engine = init_embedding_engine(None)?;

            // Generate query embedding
            use pageindex_rs::EmbeddingProvider;
            let query_vec = embedding_engine
                .generate_embedding(&text)
                .await
                .map_err(|e| anyhow::anyhow!("Embedding error: {}", e))?;

            // Determine which indexes to search
            let indexes_to_search: Vec<PathBuf> = if let Some(source_dir) = source {
                vec![get_index_path_for_source(&source_dir)]
            } else {
                // Search all indexes
                let indexes_dir = get_indexes_dir();
                if indexes_dir.exists() {
                    std::fs::read_dir(&indexes_dir)?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.join("knot_index.lance").exists())
                        .collect()
                } else {
                    vec![]
                }
            };

            if indexes_to_search.is_empty() {
                if json {
                    println!(
                        r#"{{"query": "{}", "sources_searched": 0, "results": []}}"#,
                        text
                    );
                } else {
                    println!(
                        "No indexed sources found. Run 'knot-cli index -i <directory>' first."
                    );
                }
                return Ok(());
            }

            if !json {
                println!("Searching across {} sources...\n", indexes_to_search.len());
            }

            let mut all_results = Vec::new();

            for index_path in &indexes_to_search {
                let lance_path = index_path.join("knot_index.lance");
                if let Ok(store) = KnotStore::new(lance_path.to_str().unwrap()).await {
                    if let Ok(results) = store.search(query_vec.clone(), &text).await {
                        all_results.extend(results);
                    }
                }
            }

            // Sort by score
            all_results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            all_results.truncate(10);

            if json {
                // JSON output
                println!("{{");
                println!("  \"query\": \"{}\",", text);
                println!("  \"sources_searched\": {},", indexes_to_search.len());
                println!("  \"results\": [");
                for (i, res) in all_results.iter().enumerate() {
                    let comma = if i < all_results.len() - 1 { "," } else { "" };
                    println!("    {{");
                    println!("      \"rank\": {},", i + 1);
                    println!("      \"file_path\": \"{}\",", res.file_path);
                    println!("      \"score\": {:.4},", res.score);
                    println!("      \"source\": \"{}\",", res.source);
                    if let Some(bc) = &res.breadcrumbs {
                        println!("      \"context\": \"{}\",", bc);
                    }
                    let sample = res.text.lines().next().unwrap_or("").replace('"', "\\\"");
                    println!("      \"content\": \"{}\"", sample);
                    println!("    }}{}", comma);
                }
                println!("  ]");
                println!("}}");
            } else {
                // Human readable output
                println!("Results ({} matches):\n", all_results.len());
                for (i, res) in all_results.iter().enumerate() {
                    println!("[{}] {}", i + 1, res.file_path);
                    println!("    Score: {:.4} | Source: {}", res.score, res.source);
                    if let Some(bc) = &res.breadcrumbs {
                        println!("    Context: {}", bc);
                    }
                    let sample = res.text.lines().next().unwrap_or("");
                    println!("    \"{}...\"\n", &sample[..sample.len().min(80)]);
                }
            }
        }

        Commands::Ask {
            query,
            source,
            json,
        } => {
            // Check LLM model
            let llm_model_path = get_models_dir().join("Qwen3-1.7B-Q4_K_M.gguf");
            if !llm_model_path.exists() {
                anyhow::bail!(
                    "LLM model not found: {:?}\n\
                     Run 'knot-cli download --model llm' to install.",
                    llm_model_path
                );
            }

            if !json {
                println!("Question: {}\n", query);
            }

            // 1. Search for relevant chunks
            let embedding_engine = init_embedding_engine(None)?;
            use pageindex_rs::EmbeddingProvider;
            let query_vec = embedding_engine
                .generate_embedding(&query)
                .await
                .map_err(|e| anyhow::anyhow!("Embedding error: {}", e))?;

            let indexes_to_search: Vec<PathBuf> = if let Some(source_dir) = source {
                vec![get_index_path_for_source(&source_dir)]
            } else {
                let indexes_dir = get_indexes_dir();
                if indexes_dir.exists() {
                    std::fs::read_dir(&indexes_dir)?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.join("knot_index.lance").exists())
                        .collect()
                } else {
                    vec![]
                }
            };

            if indexes_to_search.is_empty() {
                if json {
                    println!(
                        r#"{{"query": "{}", "error": "No indexed sources found"}}"#,
                        query
                    );
                } else {
                    println!(
                        "No indexed sources found. Run 'knot-cli index -i <directory>' first."
                    );
                }
                return Ok(());
            }

            if !json {
                println!("Searching across {} sources...", indexes_to_search.len());
            }

            let mut all_results = Vec::new();
            for index_path in &indexes_to_search {
                let lance_path = index_path.join("knot_index.lance");
                if let Ok(store) = KnotStore::new(lance_path.to_str().unwrap()).await {
                    if let Ok(results) = store.search(query_vec.clone(), &query).await {
                        all_results.extend(results);
                    }
                }
            }

            all_results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            all_results.truncate(8);

            if !json {
                println!("Found {} relevant chunks.\n", all_results.len());
            }

            if all_results.is_empty() {
                if json {
                    println!(
                        r#"{{"query": "{}", "error": "No relevant documents found"}}"#,
                        query
                    );
                } else {
                    println!("No relevant documents found.");
                }
                return Ok(());
            }

            // 2. Build context
            let context: String = all_results
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    // Safely truncate UTF-8 text
                    let truncated: String = r.text.chars().take(500).collect();
                    format!(
                        "[{}] {}:\n{}\n",
                        i + 1,
                        r.breadcrumbs.as_deref().unwrap_or(&r.file_path),
                        truncated
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            let prompt = format!(
                "<|im_start|>system\n你是一个知识助手。根据以下文档内容回答用户问题。如果文档中没有相关信息，请如实说明。\n\n参考文档:\n{}\n<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                context, query
            );

            // 3. Start LLM sidecar
            if !json {
                println!("Generating answer...\n");
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
            }

            let bin_dir = get_bin_dir();
            let _sidecar = LlamaSidecar::spawn_with_mmap(
                llm_model_path.to_str().unwrap(),
                &bin_dir,
                None,
                LLM_PORT,
            )?;

            // Wait a bit for sidecar to start
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            // 4. Stream response
            let client = LlamaClient::new(LLM_PORT);
            let mut rx = client.generate_content_stream(&prompt).await?;

            let mut answer = String::new();
            while let Some(token) = rx.recv().await {
                if json {
                    answer.push_str(&token);
                } else {
                    print!("{}", token);
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            }

            if json {
                // JSON output
                let answer_escaped = answer.replace('"', "\\\"").replace('\n', "\\n");
                println!("{{");
                println!("  \"query\": \"{}\",", query);
                println!("  \"sources_searched\": {},", indexes_to_search.len());
                println!("  \"chunks_found\": {},", all_results.len());
                println!("  \"answer\": \"{}\",", answer_escaped);
                println!("  \"references\": [");
                for (i, res) in all_results.iter().enumerate() {
                    let comma = if i < all_results.len() - 1 { "," } else { "" };
                    println!("    {{");
                    println!("      \"rank\": {},", i + 1);
                    println!("      \"file_path\": \"{}\",", res.file_path);
                    println!("      \"score\": {:.4}", res.score);
                    println!("    }}{}", comma);
                }
                println!("  ]");
                println!("}}");
            } else {
                println!("\n\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
                println!("Referenced Chunks ({}):\n", all_results.len());
                for (i, res) in all_results.iter().enumerate() {
                    println!("[{}] {}", i + 1, res.file_path);
                    println!("    Score: {:.4} | Source: {}", res.score, res.source);
                    if let Some(bc) = &res.breadcrumbs {
                        println!("    Context: {}", bc);
                    }
                    let sample = res.text.lines().next().unwrap_or("");
                    println!("    \"{}...\"\n", &sample[..sample.len().min(60)]);
                }
            }
        }

        Commands::Watch { input } => {
            use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
            use std::sync::mpsc::channel;

            println!("Watching directory: {:?}", input);

            // Initialize embedding engine
            let embedding_engine = init_embedding_engine(None)?;
            let provider = Arc::new(embedding_engine);

            let (tx, rx) = channel();
            let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
            watcher.watch(&input, RecursiveMode::Recursive)?;

            // Calculate paths
            let index_path = get_index_path_for_source(&input);
            let lance_path = index_path.join("knot_index.lance");
            let registry_path = index_path.join("knot.db");

            std::fs::create_dir_all(&index_path)?;

            let indexer = KnotIndexer::new(registry_path.to_str().unwrap(), Some(provider)).await;

            for res in rx {
                match res {
                    Ok(event) => {
                        let store = KnotStore::new(lance_path.to_str().unwrap()).await?;
                        match event.kind {
                            notify::EventKind::Create(_) | notify::EventKind::Modify(_) => {
                                for path in event.paths {
                                    if should_process(&path) {
                                        println!("Change detected: {:?}", path);
                                        println!("Scanning...");
                                        match indexer.index_directory(&input).await {
                                            Ok((records, deleted)) => {
                                                if !records.is_empty() {
                                                    println!("Adding {} records", records.len());
                                                    store.add_records(records).await?;
                                                }
                                                for del in deleted {
                                                    println!("Deleting {}", del);
                                                    store.delete_file(&del).await?;
                                                }
                                            }
                                            Err(e) => eprintln!("Index error: {}", e),
                                        }
                                    }
                                }
                            }
                            notify::EventKind::Remove(_) => {
                                println!("File removed. Scanning...");
                                match indexer.index_directory(&input).await {
                                    Ok((records, deleted)) => {
                                        if !records.is_empty() {
                                            store.add_records(records).await?;
                                        }
                                        for del in deleted {
                                            println!("Deleting {}", del);
                                            store.delete_file(&del).await?;
                                        }
                                    }
                                    Err(e) => eprintln!("Index error: {}", e),
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(e) => println!("watch error: {:?}", e),
                }
            }
        }
    }

    Ok(())
}

fn should_process(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext == "md" || ext == "txt"
    } else {
        false
    }
}
