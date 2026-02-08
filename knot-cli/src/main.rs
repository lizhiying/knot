use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use knot_core::embedding::{EmbeddingEngine, ThreadSafeEmbeddingEngine};
use knot_core::index::KnotIndexer;
use knot_core::llm::{LlamaClient, LlamaSidecar};
use knot_core::store::KnotStore;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const EMBEDDING_DIM: usize = 512;
const LLM_PORT: u16 = 8081;

/// Knot 配置文件结构
#[derive(Debug, Deserialize, Default)]
struct KnotConfig {
    #[serde(default)]
    models: ModelsConfig,
    #[serde(default)]
    paths: PathsConfig,
}

#[derive(Debug, Deserialize, Default)]
struct ModelsConfig {
    embedding: Option<PathBuf>,
    llm: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
struct PathsConfig {
    models_dir: Option<PathBuf>,
    indexes_dir: Option<PathBuf>,
}

/// 加载配置文件
fn load_config() -> KnotConfig {
    let config_path = get_knot_root().join("config.toml");
    if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => return config,
                Err(e) => eprintln!("Warning: Failed to parse config.toml: {}", e),
            },
            Err(e) => eprintln!("Warning: Failed to read config.toml: {}", e),
        }
    }
    KnotConfig::default()
}

/// 获取 Knot 根目录 (~/.knot)
fn get_knot_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Path::new(&home).join(".knot")
}

/// 获取默认模型目录（可被配置覆盖）
fn get_models_dir() -> PathBuf {
    let config = load_config();
    config
        .paths
        .models_dir
        .unwrap_or_else(|| get_knot_root().join("models"))
}

/// 获取默认索引目录（可被配置覆盖）
fn get_indexes_dir() -> PathBuf {
    let config = load_config();
    config
        .paths
        .indexes_dir
        .unwrap_or_else(|| get_knot_root().join("indexes"))
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

/// 计算目录大小
fn get_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                size += get_dir_size(&path)?;
            } else {
                size += entry.metadata()?.len();
            }
        }
    }
    Ok(size)
}

/// 格式化文件大小
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Parser)]
#[command(name = "knot-cli")]
#[command(about = "CLI for Knot RAG Engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Show verbose debug output
    #[arg(long, short, global = true)]
    verbose: bool,
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

        /// Remove the index instead of creating it
        #[arg(long)]
        remove: bool,
    },

    /// List all indexed sources
    IndexList,

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

/// 初始化 Embedding 引擎（支持配置文件覆盖模型路径）
fn init_embedding_engine(model_path: Option<&Path>) -> Result<ThreadSafeEmbeddingEngine> {
    let config = load_config();
    let default_model = config
        .models
        .embedding
        .unwrap_or_else(|| get_models_dir().join("bge-small-zh-v1.5.onnx"));
    let model_path = model_path.unwrap_or(&default_model);

    if !model_path.exists() {
        eprintln!("❌ Embedding model not found: {:?}", model_path);
        eprintln!();
        eprintln!("To fix this, run:");
        eprintln!("  cargo run -q -p knot-cli -- download --model embedding");
        eprintln!();
        eprintln!("Or configure a custom path in ~/.knot/config.toml:");
        eprintln!("  [models]");
        eprintln!("  embedding = \"/path/to/your/model.onnx\"");
        std::process::exit(1);
    }

    let engine = EmbeddingEngine::init_onnx(model_path.to_str().unwrap())?;
    Ok(ThreadSafeEmbeddingEngine::new(engine))
}

/// 获取 LLM 模型路径（支持配置文件覆盖）
fn get_llm_model_path() -> PathBuf {
    let config = load_config();
    config
        .models
        .llm
        .unwrap_or_else(|| get_models_dir().join("Qwen3-1.7B-Q4_K_M.gguf"))
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

    // Set quiet mode unless verbose is enabled
    if !cli.verbose {
        std::env::set_var("KNOT_QUIET", "1");
    }

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
            remove,
        } => {
            let index_path = get_index_path_for_source(&input);

            if remove {
                // Remove the index
                if index_path.exists() {
                    println!("Removing index for: {:?}", input);
                    std::fs::remove_dir_all(&index_path)?;
                    println!("✓ Index removed successfully.");
                } else {
                    println!("No index found for: {:?}", input);
                }
                return Ok(());
            }

            println!("Indexing directory: {:?}", input);

            // Initialize embedding engine
            let embedding_engine = init_embedding_engine(embedding_model.as_deref())?;
            let provider = Arc::new(embedding_engine);

            // Calculate index path based on source directory
            let lance_path = index_path.join("knot_index.lance");

            println!("Index path: {:?}", lance_path);

            // Create index directory
            std::fs::create_dir_all(&index_path)?;

            // Save source path metadata for later display
            let metadata_path = index_path.join("source.txt");
            std::fs::write(&metadata_path, input.to_string_lossy().as_bytes())?;

            // Create registry path
            let registry_path = index_path.join("knot.db");

            let indexer = KnotIndexer::new(registry_path.to_str().unwrap(), Some(provider)).await;

            // Show spinner during file scanning
            let spinner = indicatif::ProgressBar::new_spinner();
            spinner.set_style(
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            spinner.set_message("Scanning files...");
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));

            let (records, deleted_files) = indexer.index_directory(&input).await?;
            spinner.finish_and_clear();

            println!(
                "Found {} vectors to add, {} files to delete.",
                records.len(),
                deleted_files.len()
            );

            let store = KnotStore::new(lance_path.to_str().unwrap()).await?;

            // Handle deletions with progress
            if !deleted_files.is_empty() {
                let del_bar = indicatif::ProgressBar::new(deleted_files.len() as u64);
                del_bar.set_style(
                    indicatif::ProgressStyle::default_bar()
                        .template("{spinner:.red} [{bar:40.red/dim}] {pos}/{len} Deleting...")
                        .unwrap()
                        .progress_chars("█▓░"),
                );
                for del_path in deleted_files {
                    store.delete_file(&del_path).await?;
                    del_bar.inc(1);
                }
                del_bar.finish_and_clear();
            }

            // Add records with progress
            if !records.is_empty() {
                let add_bar = indicatif::ProgressBar::new(records.len() as u64);
                add_bar.set_style(
                    indicatif::ProgressStyle::default_bar()
                        .template(
                            "{spinner:.green} [{bar:40.green/dim}] {pos}/{len} Adding vectors...",
                        )
                        .unwrap()
                        .progress_chars("█▓░"),
                );
                add_bar.set_position(0);

                // Add records in chunks for progress update
                let chunk_size = 50;
                for chunk in records.chunks(chunk_size) {
                    store.add_records(chunk.to_vec()).await?;
                    add_bar.inc(chunk.len() as u64);
                }
                add_bar.finish_and_clear();
            }

            // Create FTS index with spinner
            let fts_spinner = indicatif::ProgressBar::new_spinner();
            fts_spinner.set_style(
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.blue} {msg}")
                    .unwrap(),
            );
            fts_spinner.set_message("Creating full-text search index...");
            fts_spinner.enable_steady_tick(std::time::Duration::from_millis(100));
            store.create_fts_index().await?;
            fts_spinner.finish_and_clear();

            println!("✓ Indexing complete.");
        }

        Commands::IndexList => {
            println!("Indexed Sources");
            println!("===============\n");

            let indexes_dir = get_indexes_dir();
            if !indexes_dir.exists() {
                println!("No indexes found.");
                return Ok(());
            }

            let mut found = false;
            for entry in std::fs::read_dir(&indexes_dir)?.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("knot_index.lance").exists() {
                    found = true;
                    let index_id = path.file_name().unwrap().to_string_lossy();

                    // Try to read source metadata
                    let source_path = path.join("source.txt");
                    let source = if source_path.exists() {
                        std::fs::read_to_string(&source_path).unwrap_or_else(|_| "Unknown".into())
                    } else {
                        "Unknown".into()
                    };

                    // Get directory size
                    let size = get_dir_size(&path).unwrap_or(0);
                    let size_str = format_size(size);

                    println!("[{}]", index_id);
                    println!("  Source: {}", source);
                    println!("  Size:   {}", size_str);
                    println!();
                }
            }

            if !found {
                println!("No indexes found.");
            }
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
            let llm_model_path = get_llm_model_path();
            if !llm_model_path.exists() {
                eprintln!("❌ LLM model not found: {:?}", llm_model_path);
                eprintln!();
                eprintln!("To fix this, run:");
                eprintln!("  cargo run -q -p knot-cli -- download --model llm");
                eprintln!();
                eprintln!("Or configure a custom path in ~/.knot/config.toml:");
                eprintln!("  [models]");
                eprintln!("  llm = \"/path/to/your/model.gguf\"");
                std::process::exit(1);
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

            // 3. Start LLM sidecar (quiet mode - no debug output)
            let bin_dir = get_bin_dir();
            let _sidecar = LlamaSidecar::spawn_quiet(
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
                answer.push_str(&token);
            }

            // Filter out <think>...</think> from the answer
            let clean_answer = filter_think_tags(&answer);

            if json {
                // JSON output - use serde_json for proper escaping
                let json_output = serde_json::json!({
                    "query": query,
                    "sources_searched": indexes_to_search.len(),
                    "chunks_found": all_results.len(),
                    "answer": clean_answer,
                    "references": all_results.iter().enumerate().map(|(i, res)| {
                        serde_json::json!({
                            "rank": i + 1,
                            "file_path": res.file_path,
                            "score": res.score
                        })
                    }).collect::<Vec<_>>()
                });
                println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
            } else {
                // Print the clean answer for terminal output
                println!("{}", clean_answer);
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

/// Filter out <think>...</think> blocks from LLM output
fn filter_think_tags(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    let mut in_think = false;

    while let Some(c) = chars.next() {
        if c == '<' {
            // Check for <think> or </think>
            let mut tag = String::from("<");
            while let Some(&next) = chars.peek() {
                tag.push(chars.next().unwrap());
                if next == '>' {
                    break;
                }
            }

            if tag == "<think>" {
                in_think = true;
            } else if tag == "</think>" {
                in_think = false;
            } else if !in_think {
                result.push_str(&tag);
            }
        } else if !in_think {
            result.push(c);
        }
    }

    result.trim().to_string()
}
