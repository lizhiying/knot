use knot_core::embedding::{EmbeddingEngine, ThreadSafeEmbeddingEngine};
use knot_core::llm::{LlamaClient, LlamaSidecar};
use knot_core::manager::EngineManager;
use pageindex_rs::{IndexDispatcher, PageIndexConfig, PageNode};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::RwLock;

/// 获取 models 目录的绝对路径
fn get_models_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("models"))
        .unwrap_or_else(|| PathBuf::from("models"))
}

/// 获取 bin 目录的绝对路径
fn get_bin_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .map(|p| p.join("bin"))
        .unwrap_or_else(|| PathBuf::from("bin"))
}

/// Tauri 命令：打开 Doc Parser 窗口
#[tauri::command]
async fn open_doc_parser_window(app: tauri::AppHandle) -> Result<(), String> {
    // 检查窗口是否已存在
    if let Some(window) = app.get_webview_window("doc-parser") {
        // 如果窗口已存在，激活它
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // 创建新窗口
    WebviewWindowBuilder::new(&app, "doc-parser", WebviewUrl::App("/doc-parser".into()))
        .title("Knot - Doc Parser Demo")
        .inner_size(896.0, 700.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .center()
        .build()
        .map_err(|e| format!("Failed to create window: {}", e))?;

    Ok(())
}

/// Tauri 命令：解析文件（支持 Markdown 和 PDF）
#[tauri::command]
async fn parse_file(
    app: tauri::AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<PageNode, String> {
    let file_path = Path::new(&path);

    if !file_path.exists() {
        return Err(format!("文件不存在: {}", path));
    }

    let dispatcher = IndexDispatcher::new();

    // Progress callback closure
    let app_handle = app.clone();
    let progress_callback = move |current: usize, total: usize| {
        let _ = app_handle.emit(
            "parse-progress",
            serde_json::json!({
                "current": current,
                "total": total
            }),
        );
    };

    // 基础配置
    let mut config = PageIndexConfig {
        vision_provider: None,
        llm_provider: None,       // 稍后注入
        embedding_provider: None, // 稍后注入
        min_token_threshold: 20,
        summary_token_threshold: 50,
        enable_auto_summary: false, // 稍后启用
        default_language: "zh".to_string(),
        progress_callback: Some(&progress_callback),
    };

    // 1. 获取 Embedding Provider
    let embedding_provider_guard = state.thread_safe_embedding.read().await;
    let embedding_provider = embedding_provider_guard.as_ref().map(|p| p.as_ref());

    // 2. 获取 LLM Provider
    let llm_provider_guard = state.llm_client.read().await;
    let llm_provider = llm_provider_guard.as_ref().map(|p| p.as_ref());

    // 注入 LLM Provider (PDF 解析可能需要)
    if let Some(provider) = llm_provider {
        config.llm_provider = Some(provider);
    }

    let mut root = dispatcher
        .index_file(file_path, &config)
        .await
        .map_err(|e| format!("解析失败: {}", e))?;

    // 4. 注入向量
    if let Some(provider) = embedding_provider {
        config.embedding_provider = Some(provider);
        println!("Starting embedding generation...");
        dispatcher.inject_embeddings(&mut root, &config).await;
    } else {
        println!("Warning: Embedding provider not available (model likely still loading). Embeddings will be null.");
    }

    // 5. 注入摘要（暂时禁用，避免解析过慢）
    // 5. 注入摘要
    if let Some(provider) = llm_provider {
        config.llm_provider = Some(provider);
        config.enable_auto_summary = true;
        println!("Starting summary generation...");
        dispatcher.inject_summaries(&mut root, &config).await;
    }

    Ok(root)
}

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub embedding: Arc<RwLock<Option<EmbeddingEngine>>>,
    pub thread_safe_embedding: Arc<RwLock<Option<Arc<ThreadSafeEmbeddingEngine>>>>,
    pub llm: Arc<RwLock<Option<LlamaSidecar>>>,
    pub llm_client: Arc<RwLock<Option<Arc<LlamaClient>>>>,
}

fn main() {
    let models_dir = get_models_dir();
    let bin_dir = get_bin_dir();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            let embedding: Arc<RwLock<Option<EmbeddingEngine>>> = Arc::new(RwLock::new(None));
            let thread_safe_embedding: Arc<RwLock<Option<Arc<ThreadSafeEmbeddingEngine>>>> =
                Arc::new(RwLock::new(None));
            let llm: Arc<RwLock<Option<LlamaSidecar>>> = Arc::new(RwLock::new(None));
            let llm_client: Arc<RwLock<Option<Arc<LlamaClient>>>> = Arc::new(RwLock::new(None));

            app.manage(AppState {
                embedding: embedding.clone(),
                thread_safe_embedding: thread_safe_embedding.clone(),
                llm: llm.clone(),
                llm_client: llm_client.clone(),
            });

            // 保留旧的 EngineManager
            app.manage(EngineManager {
                embedding: embedding.clone(),
                llm: llm.clone(),
            });

            // 异步加载 Embedding Engine
            let embedding_clone = embedding.clone();
            let thread_safe_clone = thread_safe_embedding.clone();
            let embedding_model_path = models_dir.join("bge-small-zh-v1.5.onnx");
            std::thread::spawn(move || {
                println!(
                    "[Engine] Loading embedding model from {:?}...",
                    embedding_model_path
                );
                match EmbeddingEngine::init_onnx(embedding_model_path.to_str().unwrap_or("")) {
                    Ok(engine) => {
                        tokio::runtime::Runtime::new().unwrap().block_on(async {
                            // 创建 ThreadSafeEmbeddingEngine
                            let ts_engine = Arc::new(ThreadSafeEmbeddingEngine::new(
                                EmbeddingEngine::init_onnx(
                                    embedding_model_path.to_str().unwrap_or(""),
                                )
                                .unwrap(),
                            ));

                            let mut guard = embedding_clone.write().await;
                            *guard = Some(engine);

                            let mut ts_guard = thread_safe_clone.write().await;
                            *ts_guard = Some(ts_engine);
                        });
                        println!(
                            "[Engine] ✓ Embedding model loaded successfully (bge-small-zh-v1.5)"
                        );
                    }
                    Err(e) => {
                        eprintln!("[Engine] ✗ Failed to load embedding model: {}", e);
                    }
                }
            });

            // 异步加载 LLM Sidecar
            let llm_clone = llm.clone();
            let llm_client_clone = llm_client.clone();
            let llm_model_path = models_dir.join("OCRFlux-3B.Q4_K_M.gguf");
            // Define separate mmproj path for OCRFlux (split GGUF)
            let mmproj_path_check = models_dir.join("OCRFlux-3B.mmproj-Q8_0.gguf");

            let final_mmproj_arg = if mmproj_path_check.exists() {
                Some(mmproj_path_check.to_str().unwrap_or("").to_string())
            } else {
                // Fallback: Assume unified GGUF, pass model path as mmproj
                Some(llm_model_path.to_str().unwrap_or("").to_string())
            };

            let llm_bin_dir = bin_dir.clone();
            std::thread::spawn(move || {
                println!("[Engine] Loading LLM model from {:?}...", llm_model_path);
                match LlamaSidecar::spawn_with_mmap(
                    llm_model_path.to_str().unwrap_or(""),
                    &llm_bin_dir,
                    final_mmproj_arg.as_deref(),
                ) {
                    Ok(sidecar) => {
                        tokio::runtime::Runtime::new().unwrap().block_on(async {
                            let mut guard = llm_clone.write().await;
                            *guard = Some(sidecar);

                            // 初始化 LlamaClient
                            let client = Arc::new(LlamaClient::new(8080));
                            let mut client_guard = llm_client_clone.write().await;
                            *client_guard = Some(client);
                        });
                        println!("[Engine] ✓ LLM sidecar started successfully (Qwen3-0.6B)");
                    }
                    Err(e) => {
                        eprintln!("[Engine] ✗ Failed to start LLM sidecar: {}", e);
                    }
                }
            });

            // 监听 Ctrl+C 信号以清理子进程 (开发模式下常用)
            let llm_signal = Arc::downgrade(&llm);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Ok(_) = tokio::signal::ctrl_c().await {
                        println!("[App] Received Ctrl+C, cleaning up...");
                        if let Some(llm) = llm_signal.upgrade() {
                            let mut guard = llm.write().await;
                            if guard.is_some() {
                                println!("[App] Killing LLM sidecar (Signal)...");
                                *guard = None; // 触发 Drop
                            }
                        }
                        std::process::exit(0);
                    }
                });
            });

            // Custom window positioning
            let app_handle = app.handle();
            let window = app_handle.get_webview_window("spotlight");
            if let Some(window) = window {
                // Use a standard thread to handle window positioning asynchronously to ensure it happens
                // but doesn't block the setup if something is weird (though setup is ideally fast condition)
                // Actually, doing it directly here is fine as the window should be created.
                // We'll calculate position based on monitor size.

                if let Ok(Some(monitor)) = window.primary_monitor() {
                    let screen_size = monitor.size();
                    let window_size = window.outer_size().unwrap_or(tauri::PhysicalSize {
                        width: 896,
                        height: 153,
                    });

                    // Calculate center X
                    let x = (screen_size.width as i32 - window_size.width as i32) / 2;

                    // Calculate Y (20% from top)
                    let y = (screen_size.height as f64 * 0.26) as i32;

                    let _ = window
                        .set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                    let _ = window.show();
                    let _ = window.set_focus();
                } else {
                    // Fallback if monitor can't be found (unlikely)
                    let _ = window.show();
                }
            }

            println!("[App] Application started, engines loading in background...");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_file,
            open_doc_parser_window,
            get_app_config,
            set_data_dir,
            rag_query
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { .. } => {
                    println!("[App] RunEvent::ExitRequested received.");
                    let state = app_handle.state::<AppState>();
                    let llm = state.llm.clone();

                    match llm.try_write() {
                        Ok(mut guard) => {
                            if guard.is_some() {
                                println!("[App] Killing LLM sidecar (ExitRequested)...");
                                *guard = None; // 这会触发 LlamaSidecar 的 drop，杀死子进程
                            }
                        }
                        Err(e) => {
                            eprintln!("[App] Failed to acquire lock for cleanup: {}", e);
                        }
                    };
                }
                tauri::RunEvent::Exit => {
                    println!("[App] RunEvent::Exit received. App is terminating.");
                    let state = app_handle.state::<AppState>();
                    let llm = state.llm.clone();

                    match llm.try_write() {
                        Ok(mut guard) => {
                            if guard.is_some() {
                                println!("[App] Killing LLM sidecar (Exit)...");
                                *guard = None; // 这会触发 LlamaSidecar 的 drop，杀死子进程
                            }
                        }
                        Err(e) => {
                            eprintln!("[App] Failed to acquire lock for cleanup at Exit: {}", e);
                        }
                    };
                }
                tauri::RunEvent::Reopen { .. } => {
                    println!("[App] RunEvent::Reopen received.");
                    if let Some(window) = app_handle.get_webview_window("spotlight") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        });
}

// --- Configuration ---

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
struct AppConfig {
    data_dir: Option<String>,
}

fn get_config_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or(PathBuf::from("."))
        .join("config.json")
}

fn load_config(app: &tauri::AppHandle) -> AppConfig {
    let path = get_config_path(app);
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    AppConfig::default()
}

fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}

// --- Commands ---

#[tauri::command]
async fn get_app_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    Ok(load_config(&app))
}

#[tauri::command]
async fn set_data_dir(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let mut config = load_config(&app);
    config.data_dir = Some(path.clone());
    save_config(&app, &config)?;

    // Trigger indexing in background
    let app_clone = app.clone();
    let state_clone = state.inner().clone(); // AppState needs to be Clone

    // Cloning Arcs manually since AppState might not derive Clone
    let thread_safe_embedding = state.thread_safe_embedding.clone();

    tauri::async_runtime::spawn(async move {
        start_background_indexing(app_clone, thread_safe_embedding, path).await;
    });

    Ok(())
}

async fn start_background_indexing(
    app: tauri::AppHandle,
    embedding_store: Arc<RwLock<Option<Arc<ThreadSafeEmbeddingEngine>>>>,
    data_dir: String,
) {
    use knot_core::index::KnotIndexer;
    use knot_core::store::KnotStore;

    println!("[Indexer] Starting background indexing for: {}", data_dir);
    let _ = app.emit("indexing-status", "starting");

    // 1. Get Embedding Engine (Wait loop logic could be better, here just check)
    let embedding_provider = {
        let guard = embedding_store.read().await;
        guard.clone()
    };

    if embedding_provider.is_none() {
        println!("[Indexer] Error: Embedding Engine not loaded yet.");
        let _ = app.emit("indexing-status", "error: engine not loaded");
        return;
    }
    let embedding_provider = embedding_provider.unwrap();

    // 2. Init Indexer with REAL provider
    // Cast Arc<ThreadSafeEmbeddingEngine> to Arc<dyn EmbeddingProvider>
    // ThreadSafeEmbeddingEngine implements EmbeddingProvider.
    // However, Arc<Struct> does not automatically CoerceUnsized to Arc<dyn Trait> in all contexts easily without explicit cast.
    let provider_dyn: Arc<dyn pageindex_rs::EmbeddingProvider + Send + Sync> = embedding_provider;

    let indexer = KnotIndexer::new(&data_dir, Some(provider_dyn)).await;
    let _ = app.emit("indexing-status", "scanning");

    // 3. Scan & Index
    let input_path = std::path::Path::new(&data_dir); // Wait, data_dir is where we STORE index.
                                                      // We need SOURCE directory.
                                                      // The user request said: "Select Datadir in Settings... Select after save, start parsing and saving index."
                                                      // Usually "Datadir" means where DB is. "Source" means where docs are.
                                                      // For "Personal Wiki" apps, usually we open a "Vault" (Source Dir) and store index inside `.knot` folder IN it.
                                                      // OR we select a Source Dir, and store index in AppData.
                                                      // The user said: "Select Datadir... start parsing". This implies Datadir IS the Source Dir.
                                                      // And we should probably store index in `.knot` inside it, or in global AppData?
                                                      // "knot_index.lance" in data_dir.
                                                      // If I pick my notes dir as data_dir, putting `knot_index.lance` there pollutes it?
                                                      // User request: "Select Datadir... start parsing".
                                                      // I will assume input path = data_dir. And I will store index in `${data_dir}/.knot_rag`.
                                                      // Or just make `KnotIndexer` usage clearer.
                                                      // `KnotIndexer::new(data_dir)`: expects data_dir to be where valid DB goes.
                                                      // If user selects `~/Documents/Notes`, and I pass that as `data_dir`:
                                                      // DB goes to `~/Documents/Notes/knot.db`.
                                                      // Index goes to `~/Documents/Notes/knot_index.lance`.
                                                      // This is "acceptable" for a workspace-based app (VSCode style `.vscode` or similar).
                                                      // I will use that approach for now as it makes "portable vaults" possible.

    match indexer.index_directory(&input_path).await {
        Ok((records, deleted)) => {
            println!("[Indexer] Found {} new/modified records.", records.len());
            let _ = app.emit("indexing-status", "saving");

            if !records.is_empty() || !deleted.is_empty() {
                match KnotStore::new(&data_dir).await {
                    Ok(store) => {
                        for del in deleted {
                            let _ = store.delete_file(&del).await;
                        }
                        if !records.is_empty() {
                            if let Err(e) = store.add_records(records).await {
                                eprintln!("[Indexer] Store Add Error: {}", e);
                            } else {
                                let _ = store.create_fts_index().await;
                            }
                        }
                    }
                    Err(e) => eprintln!("[Indexer] Store Init Error: {}", e),
                }
            }
            println!("[Indexer] Complete.");
            let _ = app.emit("indexing-status", "ready");
        }
        Err(e) => {
            eprintln!("[Indexer] Failed: {}", e);
            let _ = app.emit("indexing-status", format!("error: {}", e));
        }
    }
}

// --- RAG Commands ---

#[derive(serde::Serialize)]
struct RagResponse {
    answer: String,
    sources: Vec<HybridSearchResultDisplay>,
}

#[derive(serde::Serialize)]
struct HybridSearchResultDisplay {
    file_path: String,
    score: f32,
    context: Option<String>,
    text: String,
}

#[tauri::command]
async fn rag_query(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: String,
) -> Result<RagResponse, String> {
    use knot_core::store::KnotStore;
    use pageindex_rs::LlmProvider;

    // 1. Get Data Dir
    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("请先在设置中选择文档目录")?;

    // 2. Search Context
    let store = KnotStore::new(&data_dir).await.map_err(|e| e.to_string())?;

    // Use Mock embedding for query vector for now OR implementation needed?
    // We NEED the embedding vector for the query to perform vector search!
    // KnotStore::search takes `query_vec: Vec<f32>` and `query_text: &str`.
    // So we MUST generate embedding for `query`.
    // We have `state.thread_safe_embedding`.

    let embedding_provider = {
        let guard = state.thread_safe_embedding.read().await;
        guard.clone()
    }
    .ok_or("Embedding Engine not ready")?;

    // Generate Query Embedding
    use pageindex_rs::EmbeddingProvider;
    let query_vec = embedding_provider
        .generate_embedding(&query)
        .await
        .map_err(|e| e.to_string())?;

    let search_results = store
        .search(query_vec, &query)
        .await
        .map_err(|e| e.to_string())?;

    // 3. Format Context
    let mut context_str = String::new();
    let mut display_sources = Vec::new();

    for (i, res) in search_results.iter().take(5).enumerate() {
        let context_line = res.breadcrumbs.clone().unwrap_or_default();
        context_str.push_str(&format!(
            "[{}] File: {}\nContext: {}\nContent: {}\n\n",
            i + 1,
            res.file_path,
            context_line,
            res.text
        ));

        display_sources.push(HybridSearchResultDisplay {
            file_path: res.file_path.clone(),
            score: res.score,
            context: res.breadcrumbs.clone(),
            text: res.text.clone(),
        });
    }

    // 4. Call LLM
    let llm_client = {
        let guard = state.llm_client.read().await;
        guard.clone()
    }
    .ok_or("LLM Engine not ready")?;

    let prompt = format!(
        "Based on the following context, answer the user's question. If the answer is not in the context, say so.\n\nContext:\n{}\n\nQuestion: {}",
        context_str, query
    );

    let answer = llm_client
        .generate_content(&prompt)
        .await
        .map_err(|e| e.to_string())?;

    Ok(RagResponse {
        answer,
        sources: display_sources,
    })
}
