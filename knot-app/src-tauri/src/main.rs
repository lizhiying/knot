use knot_core::embedding::{EmbeddingEngine, ThreadSafeEmbeddingEngine};
use knot_core::llm::{LlamaClient, LlamaSidecar};
use knot_core::manager::EngineManager;
use pageindex_rs::{IndexDispatcher, PageIndexConfig, PageNode};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::RwLock;

mod models;
use models::config::{ModelSourceConfig, Region};
use models::downloader::Downloader;
use models::manager::ModelPathManager;

/// 获取 models 目录的绝对路径
/// 获取资源路径 (优先使用 Resource Path, 否则回退到 Dev Path)
fn resolve_resource(app: &tauri::AppHandle, name: &str, dev_fallback_depth: usize) -> PathBuf {
    // 1. Try bundled resource path
    if let Ok(res_dir) = app.path().resource_dir() {
        let p = res_dir.join(name);
        if p.exists() {
            println!("[Path] Found bundled resource: {:?}", p);
            return p;
        }

        // Try _up_ flattening (Tauri bundles ../ as _up_)
        let mut p_up = res_dir.clone();
        for _ in 0..dev_fallback_depth {
            p_up = p_up.join("_up_");
        }
        p_up = p_up.join(name);
        if p_up.exists() {
            println!("[Path] Found bundled resource (flattened): {:?}", p_up);
            return p_up;
        }
    }

    // 2. Fallback to Cargo Manifest (Dev mode)
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..dev_fallback_depth {
        p.pop();
    }
    p = p.join(name);
    println!("[Path] Using dev resource: {:?}", p);
    p
}

fn get_models_dir(app: &tauri::AppHandle) -> PathBuf {
    resolve_resource(app, "models", 2)
}

fn get_bin_dir(app: &tauri::AppHandle) -> PathBuf {
    resolve_resource(app, "bin", 1)
}

fn get_index_base_dir(app: &tauri::AppHandle, data_dir: &str) -> PathBuf {
    // 1. Get base ~/.knot/indexes
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let base = Path::new(&home).join(".knot").join("indexes");

    // 2. Hash absolute path of data_dir
    let abs_path = std::fs::canonicalize(data_dir).unwrap_or(PathBuf::from(data_dir));
    let path_str = abs_path.to_string_lossy();

    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();
    let hash_hex = format!("{:x}", hash);

    let index_base_dir = base.join(hash_hex);

    // Ensure directory exists
    if let Some(p) = index_base_dir.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    // Also ensure the base dir itself exists
    let _ = std::fs::create_dir_all(&index_base_dir);

    index_base_dir
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

async fn ensure_parsing_llm(app: &tauri::AppHandle, state: AppState) -> Result<(), String> {
    // Check if already running
    {
        let guard = state.parsing_llm.read().await;
        if guard.is_some() {
            return Ok(());
        }
    }

    println!("[LazyLoad] Starting Parsing LLM (OCRFlux)...");
    let _ = state.thread_safe_embedding.clone(); // Valid clone check

    // Prepare paths via ModelPathManager
    let manager = ModelPathManager::new(app);
    let bin_dir = get_bin_dir(app);

    // 使用 ModelPathManager 查找模型，不再硬编码 models_dir
    let parsing_model_path = manager.get_model_path("OCRFlux-3B.Q4_K_M.gguf");
    let parsing_mmproj_path = manager.get_model_path("OCRFlux-3B.mmproj-f16.gguf"); // 注意文件名根据 milestone 修正
                                                                                    // 兼容旧名字 fallback
    let parsing_mmproj_path = if parsing_mmproj_path.exists() {
        parsing_mmproj_path
    } else {
        manager.get_model_path("OCRFlux-3B.mmproj-Q8_0.gguf")
    };

    if !parsing_model_path.exists() {
        return Err(format!(
            "Parsing model not found at {:?}",
            parsing_model_path
        ));
    }

    let parsing_mmproj_arg = if parsing_mmproj_path.exists() {
        Some(parsing_mmproj_path.to_str().unwrap_or("").to_string())
    } else {
        // 如果 Projector 没找到，可能无法运行 Vision 任务，但先尝试
        None
    };

    let parsing_llm_store = state.parsing_llm.clone();
    let parsing_client_store = state.parsing_client.clone();

    // Use spawn_blocking for IO/Process operations
    let result = tokio::task::spawn_blocking(move || {
        LlamaSidecar::spawn_with_mmap(
            parsing_model_path.to_str().unwrap_or(""),
            &bin_dir,
            parsing_mmproj_arg.as_deref(),
            18080,
        )
    })
    .await
    .map_err(|e| format!("JoinError: {}", e))?;

    match result {
        Ok(sidecar) => {
            // Update State
            let mut guard = parsing_llm_store.write().await;
            *guard = Some(sidecar);

            let client = Arc::new(LlamaClient::new(18080));
            let mut client_guard = parsing_client_store.write().await;
            *client_guard = Some(client);

            println!("[LazyLoad] ✓ Parsing LLM Started on port 18080.");
            Ok(())
        }
        Err(e) => Err(format!("Failed to spawn Parsing LLM: {}", e)),
    }
}

/// Tauri 命令：解析文件（支持 Markdown 和 PDF）
#[tauri::command] // re-add annotation
async fn parse_file(
    app: tauri::AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<PageNode, String> {
    // Lazy Load Parsing LLM
    ensure_parsing_llm(&app, state.inner().clone()).await?;

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

    // 2. 获取 LLM Provider (Parsing Client)
    let llm_provider_guard = state.parsing_client.read().await;
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

#[tauri::command]
async fn stop_parsing_llm(state: State<'_, AppState>) -> Result<(), String> {
    let parsing_llm = state.parsing_llm.clone();
    let mut guard = parsing_llm.write().await;
    if guard.is_some() {
        println!("[App] Stopping Parsing LLM (OCRFlux)...");
        *guard = None; // Drop the Sidecar, which kills the process
    }
    Ok(())
}

/// 应用状态
use models::queue::QueueManager;

#[derive(Clone)]
pub struct AppState {
    pub embedding: Arc<RwLock<Option<EmbeddingEngine>>>,
    pub thread_safe_embedding: Arc<RwLock<Option<Arc<ThreadSafeEmbeddingEngine>>>>,
    pub parsing_llm: Arc<RwLock<Option<LlamaSidecar>>>,
    pub parsing_client: Arc<RwLock<Option<Arc<LlamaClient>>>>,
    pub chat_llm: Arc<RwLock<Option<LlamaSidecar>>>,
    pub chat_client: Arc<RwLock<Option<Arc<LlamaClient>>>>,
    pub queue_manager: Arc<QueueManager>,
}

fn main() {
    // Moved path resolution to setup

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            let embedding: Arc<RwLock<Option<EmbeddingEngine>>> = Arc::new(RwLock::new(None));
            let thread_safe_embedding: Arc<RwLock<Option<Arc<ThreadSafeEmbeddingEngine>>>> =
                Arc::new(RwLock::new(None));

            let parsing_llm: Arc<RwLock<Option<LlamaSidecar>>> = Arc::new(RwLock::new(None));
            let parsing_client: Arc<RwLock<Option<Arc<LlamaClient>>>> = Arc::new(RwLock::new(None));

            let chat_llm: Arc<RwLock<Option<LlamaSidecar>>> = Arc::new(RwLock::new(None));
            let chat_client: Arc<RwLock<Option<Arc<LlamaClient>>>> = Arc::new(RwLock::new(None));

            let app_handle = app.handle();
            let models_dir = get_models_dir(&app_handle);
            let bin_dir = get_bin_dir(&app_handle);

            // Configure ORT Library Path
            // ort 2.0+ needs to know where the dylib is if it's not in standard paths
            #[cfg(target_os = "macos")]
            {
                let arch = std::env::consts::ARCH; // "aarch64" or "x86_64"
                let ort_lib_path = if arch == "aarch64" {
                    bin_dir
                        .join("onnxruntime")
                        .join("macos-arm64")
                        .join("libonnxruntime.dylib")
                } else {
                    bin_dir
                        .join("onnxruntime")
                        .join("macos-x64")
                        .join("libonnxruntime.dylib")
                };

                if ort_lib_path.exists() {
                    println!("[ORT] Setting ORT_DYLIB_PATH to: {:?}", ort_lib_path);
                    std::env::set_var("ORT_DYLIB_PATH", ort_lib_path);
                } else {
                    println!(
                        "[ORT] Warning: bundled onnxruntime dylib not found at {:?}",
                        ort_lib_path
                    );
                }
            }

            let queue_manager = Arc::new(QueueManager::new());

            app.manage(AppState {
                embedding: embedding.clone(),
                thread_safe_embedding: thread_safe_embedding.clone(),
                parsing_llm: parsing_llm.clone(),
                parsing_client: parsing_client.clone(),
                chat_llm: chat_llm.clone(),
                chat_client: chat_client.clone(),
                queue_manager: queue_manager.clone(),
            });

            // 保留旧的 EngineManager
            app.manage(EngineManager {
                embedding: embedding.clone(),
                llm: parsing_llm.clone(),
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
                // Ensure ORT setup propagated to this thread context if needed (env vars are process global so OK)
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

            // 异步加载 Parsing LLM moved to lazy load (start on demand)
            // See ensure_parsing_llm helper

            // 异步加载 Chat LLM (Qwen2.5-3B) @ Port 8081

            // 异步加载 Chat LLM (Qwen2.5-3B) @ Port 8081
            let chat_llm_clone = chat_llm.clone();
            let chat_client_clone = chat_client.clone();

            // Chat Model Paths
            // Chat Model Paths
            let manager = ModelPathManager::new(&app_handle);
            let chat_model_path = manager.get_model_path("Qwen3-1.7B-Q4_K_M.gguf");

            let bin_dir_clone2 = bin_dir.clone();
            std::thread::spawn(move || {
                // HOT RELOAD LOGIC:
                // Check if model exists. If not, we do nothing and wait for "reload_models" command.
                if !chat_model_path.exists() {
                    println!(
                        "[Engine] Chat LLM model missing at {:?}. Waiting for download...",
                        chat_model_path
                    );
                    return;
                }

                println!(
                    "[Engine] Loading Chat LLM (Qwen3) from {:?}...",
                    chat_model_path
                );
                // Qwen3 usually doesn't need mmproj (Text only).
                match LlamaSidecar::spawn_with_mmap(
                    chat_model_path.to_str().unwrap_or(""),
                    &bin_dir_clone2,
                    None,
                    18081,
                ) {
                    Ok(sidecar) => {
                        tokio::runtime::Runtime::new().unwrap().block_on(async {
                            let mut guard = chat_llm_clone.write().await;
                            *guard = Some(sidecar);

                            let client = Arc::new(LlamaClient::new(18081));
                            let mut client_guard = chat_client_clone.write().await;
                            *client_guard = Some(client);
                        });
                        println!("[Engine] ✓ Chat LLM (Qwen3) started on port 18081");
                    }
                    Err(e) => {
                        eprintln!("[Engine] ✗ Failed to start Chat LLM: {}", e);
                    }
                }
            });

            // 监听 Ctrl+C 信号以清理子进程 (开发模式下常用)
            let parsing_signal = Arc::downgrade(&parsing_llm);
            let chat_signal = Arc::downgrade(&chat_llm);

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Ok(_) = tokio::signal::ctrl_c().await {
                        println!("[App] Received Ctrl+C, cleaning up...");
                        // Kill Parsing LLM
                        if let Some(llm) = parsing_signal.upgrade() {
                            let mut guard = llm.write().await;
                            if guard.is_some() {
                                println!("[App] Killing Parsing LLM...");
                                *guard = None;
                            }
                        }
                        // Kill Chat LLM
                        if let Some(llm) = chat_signal.upgrade() {
                            let mut guard = llm.write().await;
                            if guard.is_some() {
                                println!("[App] Killing Chat LLM...");
                                *guard = None;
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

            // 11. Auto-start indexing if configured
            // Moved here to ensure app is ready
            let app_handle_for_index = app.handle().clone();
            let state_for_index = app.state::<AppState>();
            let thread_safe_embedding_for_index = state_for_index.thread_safe_embedding.clone();

            // Spawn a task to check config and start indexing
            tauri::async_runtime::spawn(async move {
                // Give some time for other systems to init? Not strictly necessary but safe.
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                let config = load_config(&app_handle_for_index);
                if let Some(dir) = config.data_dir {
                    if !dir.is_empty() {
                        println!(
                            "[App] Found configured data_dir: {}. Starting background indexing...",
                            dir
                        );
                        let base_dir = get_index_base_dir(&app_handle_for_index, &dir);
                        let index_path = base_dir.join("knot_index.lance");
                        let db_path = base_dir.join("knot.db");

                        start_background_indexing(
                            app_handle_for_index,
                            thread_safe_embedding_for_index,
                            dir,
                            index_path.to_string_lossy().to_string(),
                            db_path.to_string_lossy().to_string(),
                        )
                        .await;
                    }
                }
            });

            println!("[App] Application started, engines loading in background...");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_file,
            open_doc_parser_window,
            get_app_config,
            set_data_dir,
            rag_query,
            check_model_status,
            download_model,
            get_detected_region,
            start_download_queue,
            reload_models,
            stop_parsing_llm,
            reset_index
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                    println!("[App] RunEvent::Exit/ExitRequested received.");
                    let state = app_handle.state::<AppState>();

                    // Kill Parsing LLM
                    let parsing_llm = state.parsing_llm.clone();
                    match parsing_llm.try_write() {
                        Ok(mut guard) => {
                            if guard.is_some() {
                                println!("[App] Killing Parsing LLM...");
                                *guard = None;
                            }
                        }
                        Err(e) => {
                            eprintln!("[App] Failed to acquire lock for Parsing cleanup: {}", e)
                        }
                    };

                    // Kill Chat LLM
                    let chat_llm = state.chat_llm.clone();
                    match chat_llm.try_write() {
                        Ok(mut guard) => {
                            if guard.is_some() {
                                println!("[App] Killing Chat LLM...");
                                *guard = None;
                            }
                        }
                        Err(e) => eprintln!("[App] Failed to acquire lock for Chat cleanup: {}", e),
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
async fn reset_index(app: tauri::AppHandle) -> Result<(), String> {
    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("Data directory not set")?;

    let base_dir = get_index_base_dir(&app, &data_dir);
    println!("[Command] Resetting index. Deleting: {:?}", base_dir);

    if base_dir.exists() {
        std::fs::remove_dir_all(&base_dir).map_err(|e| e.to_string())?;
    }

    Ok(())
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
    // state_clone removed (unused)

    // Cloning Arcs manually since AppState might not derive Clone
    let thread_safe_embedding = state.thread_safe_embedding.clone();

    // Get index path
    let base_dir = get_index_base_dir(&app_clone, &path);
    let index_path = base_dir.join("knot_index.lance");
    let db_path = base_dir.join("knot.db");

    let index_path_str = index_path.to_string_lossy().to_string();
    let db_path_str = db_path.to_string_lossy().to_string();

    tauri::async_runtime::spawn(async move {
        start_background_indexing(
            app_clone,
            thread_safe_embedding,
            path,
            index_path_str,
            db_path_str,
        )
        .await;
    });

    Ok(())
}

async fn start_background_indexing(
    app: tauri::AppHandle,
    embedding_store: Arc<RwLock<Option<Arc<ThreadSafeEmbeddingEngine>>>>,
    data_dir: String,
    index_path: String,
    db_path: String,
) {
    use knot_core::index::KnotIndexer;
    use knot_core::store::KnotStore;

    println!("[Indexer] Starting background indexing for: {}", data_dir);
    println!("[Indexer] Using external index path: {}", index_path);
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

    let indexer = KnotIndexer::new(&db_path, Some(provider_dyn)).await;
    let _ = app.emit("indexing-status", "scanning");

    // 3. Scan & Index (Initial Pass)
    let input_path = std::path::Path::new(&data_dir);

    // Initial Scan
    match indexer.index_directory(&input_path).await {
        Ok((records, deleted)) => {
            println!("[Indexer] Found {} new/modified records.", records.len());
            let _ = app.emit("indexing-status", "saving");

            if !records.is_empty() || !deleted.is_empty() {
                match KnotStore::new(&index_path).await {
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
            println!("[Indexer] Initial scan complete.");
            let _ = app.emit("indexing-status", "ready");
        }
        Err(e) => {
            eprintln!("[Indexer] Initial scan failed: {}", e);
            let _ = app.emit("indexing-status", format!("error: {}", e));
        }
    }

    // 4. Start Monitoring
    println!("[Indexer] Starting file monitor on: {:?}", input_path);
    use knot_core::monitor::{should_index_file, DirectoryWatcher};

    match DirectoryWatcher::new(input_path) {
        Ok(mut watcher) => {
            println!(
                "[Indexer] Watcher created successfully for: {:?}",
                input_path
            );
            let _ = app.emit("search-status", "monitoring");

            println!("[Indexer] Entering watch event loop with debounce...");
            // Need copies for async move
            let index_path_for_watch = index_path.clone();

            let mut pending_updates: std::collections::HashSet<PathBuf> =
                std::collections::HashSet::new();
            let mut pending_removals: std::collections::HashSet<PathBuf> =
                std::collections::HashSet::new();

            let debounce_duration = std::time::Duration::from_millis(500);
            let mut debounce_timer = Box::pin(tokio::time::sleep(tokio::time::Duration::MAX)); // Inactive initially
            let mut timer_active = false;

            loop {
                tokio::select! {
                    maybe_res = watcher.rx.recv() => {
                        match maybe_res {
                            Some(res) => {
                                match res {
                                    Ok(event) => {
                                        use knot_core::notify::EventKind;
                                        // Reset timer
                                        debounce_timer = Box::pin(tokio::time::sleep(debounce_duration));
                                        timer_active = true;

                                        match event.kind {
                                            EventKind::Create(_) | EventKind::Modify(_) => {
                                                for path in event.paths {
                                                     pending_updates.insert(path);
                                                }
                                            }
                                            EventKind::Remove(_) => {
                                                for path in event.paths {
                                                    // If a file is removed, remove it from updates if present
                                                    pending_updates.remove(&path);
                                                    pending_removals.insert(path);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(e) => eprintln!("[Monitor] Watch error: {}", e),
                                }
                            }
                            None => {
                                println!("[Indexer] Watcher channel closed.");
                                break;
                            }
                        }
                    }
                    _ = &mut debounce_timer, if timer_active => {
                        // Timer expired, process updates
                        timer_active = false;
                        // Reset timer to indefinite
                        debounce_timer = Box::pin(tokio::time::sleep(tokio::time::Duration::MAX));

                        if !pending_updates.is_empty() || !pending_removals.is_empty() {
                            println!("[Monitor] Debounce triggered. Processing {} updates, {} removals.", pending_updates.len(), pending_removals.len());
                            let _ = app.emit("indexing-status", "updating");

                            // Process Removals First
                            if !pending_removals.is_empty() {
                                if let Ok(store) = KnotStore::new(&index_path_for_watch).await {
                                    for path in pending_removals.drain() {
                                        println!("[Monitor] Removal detected: {:?}", path);
                                        let path_str = path.to_string_lossy();
                                        let _ = store.delete_file(&path_str).await;
                                        let _ = store.delete_folder(&path_str).await;
                                    }
                                }
                            }
                            pending_removals.clear(); // Ensure cleared if store init failed (drain clears it)

                            // Process Updates
                             if !pending_updates.is_empty() {
                                let paths: Vec<_> = pending_updates.drain().collect();
                                let mut total_records = 0;
                                let mut updated_cnt = 0;

                                // We need indexer reference.
                                // indexer is available in this scope? Yes.

                                // Optimization: If we have many files, maybe parallelize?
                                // For now, sequential is safer for DB locks.

                                // We need to re-open store once for efficiency?
                                // Or open per file?
                                // KnotStore::new is cheap (just struct init? no, it connects sqlite)
                                // Better to open once if possible.

                                let store_opt = KnotStore::new(&index_path_for_watch).await.ok();

                                for path in paths {
                                    if should_index_file(&path) {
                                         if path.exists() {
                                             // Index it
                                             match indexer.index_file(&path).await {
                                                 Ok(records) => {
                                                     if !records.is_empty() {
                                                         total_records += records.len();
                                                         updated_cnt += 1;
                                                         if let Some(store) = &store_opt {
                                                              let _ = store.delete_file(&path.to_string_lossy()).await;
                                                              let _ = store.add_records(records).await;
                                                         }
                                                     }
                                                 }
                                                 Err(e) => eprintln!("[Monitor] Failed to index {:?}: {}", path, e),
                                             }
                                         } else {
                                             // Just removed?
                                         }
                                    }
                                }

                                if let Some(store) = &store_opt {
                                    if total_records > 0 {
                                        let _ = store.create_fts_index().await;
                                    }
                                }

                                if updated_cnt > 0 {
                                    println!("[Monitor] Updated {} files with {} records.", updated_cnt, total_records);
                                }
                             }

                            let _ = app.emit("indexing-status", "ready");
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[Indexer] Failed to start watcher: {}", e);
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
    source: String,
}

#[tauri::command]
async fn rag_query(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: String,
) -> Result<RagResponse, String> {
    use knot_core::store::KnotStore;
    use pageindex_rs::LlmProvider;

    println!("[rag_query] Starting query: {}", query);

    // 1. Get Data Dir
    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("Data directory not set")?;

    // Resolve external index path
    let base_dir = get_index_base_dir(&app, &data_dir);
    let index_path = base_dir.join("knot_index.lance");
    let index_path_str = index_path.to_string_lossy().to_string();

    let store = knot_core::store::KnotStore::new(&index_path_str)
        .await
        .map_err(|e| format!("Store error: {}", e))?;
    println!("[rag_query] Store initialized");

    // Use Mock embedding for query vector for now OR implementation needed?
    // We NEED the embedding vector for the query to perform vector search!
    // KnotStore::search takes `query_vec: Vec<f32>` and `query_text: &str`.
    // So we MUST generate embedding for `query`.
    // We have `state.thread_safe_embedding`.

    println!("[rag_query] Acquiring embedding lock...");
    let embedding_provider = {
        let guard = state.thread_safe_embedding.read().await;
        println!("[rag_query] Embedding lock acquired");
        guard.clone()
    }
    .ok_or("Embedding Engine not ready")?;

    // Generate Query Embedding
    use pageindex_rs::EmbeddingProvider;
    println!("[rag_query] Generating embedding...");
    let query_vec = embedding_provider
        .generate_embedding(&query)
        .await
        .map_err(|e| e.to_string())?;
    println!("[rag_query] Embedding generated");

    let search_results = store
        .search(query_vec, &query)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "[rag_query] Search complete. Found {} results",
        search_results.len()
    );

    // 3. Format Context
    let mut context_str = String::new();
    let mut display_sources = Vec::new();

    for (i, res) in search_results.iter().take(5).enumerate() {
        let context_line = res.breadcrumbs.clone().unwrap_or_default();
        // 按照 milestone1.md 的格式构建参考文档片段
        // [1] (匹配度: 98%) 文件: {path} - 章节: {breadcrumbs}
        // 内容: {content}
        context_str.push_str(&format!(
            "[{}] (匹配度: {:.0}%) 文件: {} - 章节: {}\n内容: {}\n\n",
            i + 1,
            res.score, // Simple formatting of the score
            res.file_path,
            context_line,
            res.text
        ));

        display_sources.push(HybridSearchResultDisplay {
            file_path: res.file_path.clone(),
            score: res.score,
            context: res.breadcrumbs.clone(),
            text: res.text.clone(),
            source: res.source.to_string(),
        });
    }

    // 4. Call LLM (Chat Client)
    let llm_client = {
        let guard = state.chat_client.read().await;
        guard.clone()
    }
    .ok_or("Chat LLM (Qwen3) not ready")?;

    let prompt = format!(
        "<|im_start|>system\n你是一个智能助手。请根据参考文档回答用户问题。\n\n**回答原则**：\n1. **开门见山**：直接把文档中找到的关键信息（如日期、地点、结论）放在第一句。例如直接说：“行程安排在 2.17 (周二)。”\n2. **去除客套**：不要使用“根据参考文档…”、“综上所述…”等前缀。\n3. **详细展开**：在核心答案之后，引用文档细节进行说明。\n4. 只有当文档完全不包含相关信息时，才说“无法找到答案”。\n<|im_end|>\n<|im_start|>user\n参考文档：\n{}\n\n用户问题: {}<|im_end|>\n<|im_start|>assistant\n",
        context_str, query
    );

    println!(
        "[rag_query] Prompt Preview (first 500 chars):\n{:.500}...",
        prompt
    );

    let answer = llm_client
        .generate_content(&prompt)
        .await
        .map_err(|e| e.to_string())?;

    println!("[rag_query] LLM Raw Answer:\n{}", answer);

    Ok(RagResponse {
        answer,
        sources: display_sources,
    })
}

// --- Model Management Commands ---

#[tauri::command]
async fn check_model_status(app: tauri::AppHandle, filename: String) -> Result<bool, String> {
    let manager = ModelPathManager::new(&app);
    Ok(manager.get_model_path(&filename).exists())
}

#[tauri::command]
async fn download_model(
    app: tauri::AppHandle,
    filename: String,
    region: Option<Region>, // Optional override
) -> Result<(), String> {
    let manager = ModelPathManager::new(&app);
    let target_path = manager.get_download_target_path(&filename);

    let mut config = ModelSourceConfig::new();
    if let Some(r) = region {
        config.region = r;
    }

    let url = config.get_url(&filename);

    println!("[Command] Downloading {:?} from {}", filename, url);
    Downloader::download_file(&app, &url, &target_path, &filename).await?;

    Ok(())
}

#[tauri::command]
async fn get_detected_region() -> Result<Region, String> {
    // Return auto-detected region
    Ok(ModelSourceConfig::new().region)
}

#[tauri::command]
async fn start_download_queue(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    region: Option<Region>,
) -> Result<(), String> {
    let qm = &state.queue_manager;

    // Clear previous
    qm.clear_queue().await;

    if let Some(r) = region {
        qm.set_region(r).await;
    }

    // Add files in order: OCR Main -> OCR mmproj -> LLM
    qm.add_to_queue("OCRFlux-3B.Q4_K_M.gguf".to_string()).await;
    qm.add_to_queue("OCRFlux-3B.mmproj-f16.gguf".to_string())
        .await;
    qm.add_to_queue("Qwen3-1.7B-Q4_K_M.gguf".to_string()).await;

    // Trigger async processing
    let app_handle = app.clone();
    let qm_clone = qm.clone(); // Arc clone

    tauri::async_runtime::spawn(async move {
        match qm_clone.process_queue(app_handle.clone()).await {
            Ok(_) => println!("[Queue] Finished processing."),
            Err(e) => eprintln!("[Queue] processing failed: {}", e),
        }
    });

    Ok(())
}

#[tauri::command]
async fn reload_models(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    println!("[Command] Reloading models...");

    // 1. Try Reload Parsing LLM (OCR)
    let _ = ensure_parsing_llm(&app, state.inner().clone()).await;

    // 2. Try Reload Chat LLM (Qwen)
    // Check if running
    let need_reload_chat = {
        let guard = state.chat_llm.read().await;
        guard.is_none()
    };

    if need_reload_chat {
        println!("[Reload] Chat LLM not running, trying to start...");
        let manager = ModelPathManager::new(&app);
        let chat_model_path = manager.get_model_path("Qwen3-1.7B-Q4_K_M.gguf");

        if chat_model_path.exists() {
            let bin_dir = get_bin_dir(&app);
            let chat_llm_store = state.chat_llm.clone();
            let chat_client_store = state.chat_client.clone();

            let result = tokio::task::spawn_blocking(move || {
                LlamaSidecar::spawn_with_mmap(
                    chat_model_path.to_str().unwrap_or(""),
                    &bin_dir,
                    None,
                    8081,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            match result {
                Ok(sidecar) => {
                    let mut guard = chat_llm_store.write().await;
                    *guard = Some(sidecar);

                    let client = Arc::new(LlamaClient::new(8081));
                    let mut c_guard = chat_client_store.write().await;
                    *c_guard = Some(client);
                    println!("[Reload] ✓ Chat LLM Started.");
                }
                Err(e) => eprintln!("[Reload] Failed to start Chat LLM: {}", e),
            }
        } else {
            println!("[Reload] Chat model still missing.");
        }
    } else {
        println!("[Reload] Chat LLM already running.");
    }

    Ok(())
}
