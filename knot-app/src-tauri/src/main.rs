use knot_core::embedding::{EmbeddingEngine, ThreadSafeEmbeddingEngine};
use knot_core::llm::{LlamaClient, LlamaSidecar};
use knot_core::manager::EngineManager;
use knot_parser::{IndexDispatcher, PageIndexConfig, PageNode};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::RwLock;

mod eval_api;
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

fn get_index_base_dir(_app: &tauri::AppHandle, data_dir: &str) -> PathBuf {
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

    println!("[LazyLoad] Starting Parsing LLM (GLM-OCR)...");
    let _ = state.thread_safe_embedding.clone(); // Valid clone check

    // Prepare paths via ModelPathManager
    let manager = ModelPathManager::new(app);
    let bin_dir = get_bin_dir(app);

    // 使用 ModelPathManager 查找模型，不再硬编码 models_dir
    let parsing_model_path = manager.get_model_path("GLM-OCR-Q8_0.gguf");
    let parsing_mmproj_path = manager.get_model_path("mmproj-GLM-OCR-Q8_0.gguf");

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
        // VLM 图片解析需要大 context（图片 token 可能超过 4096）
        // 使用 16384 上下文，parallel=2 时每个 slot 有 8192
        LlamaSidecar::spawn_with_context(
            parsing_model_path.to_str().unwrap_or(""),
            &bin_dir,
            parsing_mmproj_arg.as_deref(),
            18080,
            16384,
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
    let file_path = Path::new(&path);

    if !file_path.exists() {
        return Err(format!("文件不存在: {}", path));
    }

    let dispatcher = IndexDispatcher::new();

    // Progress callback closure（PDF 逐页进度）
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

    // Page content callback（逐页 markdown 实时推送）
    let app_handle2 = app.clone();
    let page_callback = move |page_index: usize, total_pages: usize, markdown: String| {
        let _ = app_handle2.emit(
            "parse-page-ready",
            serde_json::json!({
                "pageIndex": page_index,
                "totalPages": total_pages,
                "markdown": markdown
            }),
        );
    };

    // 基础配置
    let manager = ModelPathManager::new(&app);
    let ocr_model_dir = manager.get_download_target_path("ppocrv5");
    let ocr_model_dir_str = ocr_model_dir.to_string_lossy().to_string();

    let mut config = PageIndexConfig {
        vision_provider: None,
        llm_provider: None,       // 稍后注入
        embedding_provider: None, // 稍后注入
        min_token_threshold: 20,
        summary_token_threshold: 50,
        enable_auto_summary: false, // 稍后启用
        default_language: "zh".to_string(),
        progress_callback: Some(std::sync::Arc::new(progress_callback)),
        page_content_callback: Some(std::sync::Arc::new(page_callback)),
        // PDF OCR 配置（PaddleOCR）
        pdf_ocr_enabled: ocr_model_dir.join("det.onnx").exists(),
        pdf_ocr_model_dir: Some(ocr_model_dir_str),
        // PDF Vision 配置（使用 Ollama 的 glm-ocr）
        pdf_vision_api_url: Some("http://localhost:11434/v1/chat/completions".to_string()),
        pdf_vision_model: Some("glm-ocr:latest".to_string()),
        pdf_page_indices: None,
    };

    // 1. 获取 Embedding Provider
    let embedding_provider_guard = state.thread_safe_embedding.read().await;
    let embedding_provider: Option<&dyn knot_parser::EmbeddingProvider> = embedding_provider_guard
        .as_ref()
        .map(|p| &**p as &dyn knot_parser::EmbeddingProvider);

    // 2. 获取 LLM Provider (Chat Client / Qwen3)
    let llm_provider_guard = state.chat_client.read().await;
    let llm_provider = llm_provider_guard.as_ref().map(|p| p.as_ref());

    // 注入 LLM Provider (摘要生成)
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
        println!("[App] Stopping Parsing LLM (GLM-OCR)...");
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
    pub knot_store: Arc<RwLock<Option<Arc<knot_core::store::KnotStore>>>>,
    pub model_status: Arc<RwLock<String>>,
    /// LLM 生成版本号。每次 rag_generate 启动时递增。
    /// rag_generate 在发射 token 前检查此值是否与启动时一致，
    /// 不一致则说明有新生成取代了当前生成，立即停止发射。
    pub generation_id: Arc<std::sync::atomic::AtomicU64>,
    /// DuckDB 查询引擎缓存（供分页翻页复用）
    /// Key: file_path, Value: (QueryEngine, last_sql, last_access_time)
    pub excel_engine_cache: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<
                String,
                (knot_excel::QueryEngine, String, std::time::Instant),
            >,
        >,
    >,
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
            let knot_store: Arc<RwLock<Option<Arc<knot_core::store::KnotStore>>>> =
                Arc::new(RwLock::new(None));
            let model_status = Arc::new(RwLock::new("loading".to_string()));

            app.manage(AppState {
                embedding: embedding.clone(),
                thread_safe_embedding: thread_safe_embedding.clone(),
                parsing_llm: parsing_llm.clone(),
                parsing_client: parsing_client.clone(),
                chat_llm: chat_llm.clone(),
                chat_client: chat_client.clone(),
                queue_manager: queue_manager.clone(),
                knot_store: knot_store.clone(),
                model_status: model_status.clone(),
                generation_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                excel_engine_cache: Arc::new(tokio::sync::Mutex::new(
                    std::collections::HashMap::new(),
                )),
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
            let chat_model_path = manager.get_model_path("Qwen3.5-4B-Q4_K_M.gguf");

            let bin_dir_clone2 = bin_dir.clone();
            let app_handle_clone = app_handle.clone();
            let model_status_clone = model_status.clone();

            // 读取配置的 context_size
            let config = load_config(&app_handle);
            let llm_context_size = config.llm_context_size;

            std::thread::spawn(move || {
                // HOT RELOAD LOGIC:
                // Check if model exists. If not, we do nothing and wait for "reload_models" command.
                if !chat_model_path.exists() {
                    println!(
                        "[Engine] Chat LLM model missing at {:?}. Waiting for download...",
                        chat_model_path
                    );
                    // 模型不存在时也标记为 ready（避免 UI 永远等待）
                    tokio::runtime::Runtime::new().unwrap().block_on(async {
                        {
                            let mut status = model_status_clone.write().await;
                            *status = "ready".to_string();
                        }
                        let _ = app_handle_clone.emit("model-status", "ready");
                    });
                    return;
                }

                println!(
                    "[Engine] Loading Chat LLM (Qwen3) from {:?}...",
                    chat_model_path
                );
                // Qwen3 usually doesn't need mmproj (Text only).
                match LlamaSidecar::spawn_with_context(
                    chat_model_path.to_str().unwrap_or(""),
                    &bin_dir_clone2,
                    None,
                    18081,
                    llm_context_size,
                ) {
                    Ok(sidecar) => {
                        let app_handle_g = app_handle_clone.clone();
                        let model_status_g = model_status_clone.clone();

                        tokio::runtime::Runtime::new().unwrap().block_on(async {
                            let mut guard = chat_llm_clone.write().await;
                            *guard = Some(sidecar);

                            let client = Arc::new(LlamaClient::new(18081));
                            let mut client_guard = chat_client_clone.write().await;
                            *client_guard = Some(client.clone()); // Cloning ARC

                            // Warmup on startup
                            println!("[Engine] Performing startup warmup...");
                            if let Err(e) = client.warmup().await {
                                eprintln!("[Engine] Startup warmup failed: {}", e);
                                // Don't fail the whole startup, just log
                            } else {
                                println!("[Engine] Startup warmup successful.");
                            }

                            // 无论 warmup 是否成功都设为 ready（模型进程已启动）
                            {
                                let mut status = model_status_g.write().await;
                                *status = "ready".to_string();
                            }
                            let _ = app_handle_g.emit("model-status", "ready");
                        });
                        println!("[Engine] ✓ Chat LLM (Qwen3) started on port 18081");
                    }
                    Err(e) => {
                        eprintln!("[Engine] ✗ Failed to start Chat LLM: {}", e);
                        // 即使 Chat LLM 失败也标记为 ready（搜索等核心功能不依赖 Chat）
                        tokio::runtime::Runtime::new().unwrap().block_on(async {
                            {
                                let mut status = model_status_clone.write().await;
                                *status = "ready".to_string();
                            }
                            let _ = app_handle_clone.emit("model-status", "ready");
                        });
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
            let knot_store_for_prewarm = state_for_index.knot_store.clone();

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

                        // Pre-warm KnotStore
                        let index_path_str = index_path.to_string_lossy().to_string();
                        println!("[Store] Pre-warming KnotStore at startup...");
                        match knot_core::store::KnotStore::new(&index_path_str).await {
                            Ok(store) => {
                                let mut guard = knot_store_for_prewarm.write().await;
                                *guard = Some(Arc::new(store));
                                println!("[Store] KnotStore pre-warmed at startup");
                            }
                            Err(e) => {
                                eprintln!("[Store] Failed to pre-warm KnotStore: {}", e);
                            }
                        }

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

            // 12. Start Eval HTTP API Server (for Python evaluation scripts)
            let eval_thread_safe_embedding = thread_safe_embedding.clone();
            let eval_chat_client = chat_client.clone();
            let eval_app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                // Wait for engines to possibly load
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                // Get index path from config
                let config = load_config(&eval_app_handle);
                let index_path = if let Some(dir) = config.data_dir {
                    let base_dir = get_index_base_dir(&eval_app_handle, &dir);
                    Some(
                        base_dir
                            .join("knot_index.lance")
                            .to_string_lossy()
                            .to_string(),
                    )
                } else {
                    None
                };

                // Cast ThreadSafeEmbeddingEngine to dyn EmbeddingProvider
                let embedding_for_api: std::sync::Arc<
                    tokio::sync::RwLock<
                        Option<std::sync::Arc<dyn knot_parser::EmbeddingProvider + Send + Sync>>,
                    >,
                > = std::sync::Arc::new(tokio::sync::RwLock::new(None));

                // Clone to update
                let embedding_for_api_clone = embedding_for_api.clone();
                let eval_thread_safe_embedding_clone = eval_thread_safe_embedding.clone();

                // Spawn a task to sync embedding provider
                tokio::spawn(async move {
                    loop {
                        let provider_opt = {
                            let guard = eval_thread_safe_embedding_clone.read().await;
                            guard.clone()
                        };
                        if let Some(provider) = provider_opt {
                            let mut guard = embedding_for_api_clone.write().await;
                            let dyn_provider: std::sync::Arc<
                                dyn knot_parser::EmbeddingProvider + Send + Sync,
                            > = provider as _;
                            *guard = Some(dyn_provider);
                            break;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                });

                let eval_state = eval_api::EvalApiState {
                    thread_safe_embedding: embedding_for_api,
                    chat_client: eval_chat_client,
                    index_path: std::sync::Arc::new(tokio::sync::RwLock::new(index_path)),
                };

                eval_api::start_eval_server(eval_state, 18765).await;
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
            rag_search,
            rag_generate,
            cancel_generation,
            check_model_status,
            check_all_models,
            get_model_status,
            download_model,
            get_detected_region,
            start_download_queue,
            reload_models,
            stop_parsing_llm,
            stop_parsing_llm,
            reset_index,
            set_streaming_enabled,
            set_vector_distance_threshold,
            set_llm_context_size,
            set_llm_max_tokens,
            set_llm_think_enabled,
            set_context_expansion_enabled,
            set_multi_hop_enabled,
            set_graph_rag_enabled,
            get_graph_data,
            get_index_status,
            list_knowledge_files,
            get_file_index_detail,
            reindex_file,
            ignore_file,
            unignore_file,
            query_excel_table,
            query_excel_page
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

#[derive(serde::Serialize)]
struct IndexStatus {
    file_count: u64,
    doc_count: u64,
}

#[tauri::command]
async fn get_index_status(state: State<'_, AppState>) -> Result<IndexStatus, String> {
    let store_guard = state.knot_store.read().await;
    if let Some(store) = store_guard.as_ref() {
        let doc_count = store.get_doc_count().map_err(|e| e.to_string())?;
        let file_count = store.get_file_count().await.map_err(|e| e.to_string())?;
        return Ok(IndexStatus {
            file_count,
            doc_count,
        });
    }
    Ok(IndexStatus {
        file_count: 0,
        doc_count: 0,
    })
}

// --- Configuration ---

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
struct AppConfig {
    data_dir: Option<String>,
    #[serde(default = "default_streaming_enabled")]
    streaming_enabled: bool,
    /// 向量距离阈值：距离 > 此值的结果被过滤
    #[serde(default = "default_vector_distance_threshold")]
    vector_distance_threshold: f32,
    /// LLM 上下文窗口大小（需要重启生效）
    #[serde(default = "default_llm_context_size")]
    llm_context_size: u32,
    /// LLM 最大生成 token 数
    #[serde(default = "default_llm_max_tokens")]
    llm_max_tokens: u32,
    /// 是否启用 LLM Think 模式
    #[serde(default = "default_llm_think_enabled")]
    llm_think_enabled: bool,
    /// 搜索时是否自动扩展上下文（拉取 parent/sibling 节点）
    #[serde(default = "default_context_expansion_enabled")]
    context_expansion_enabled: bool,
    /// 是否启用多跳检索（两轮搜索，关键词扩展）
    #[serde(default = "default_multi_hop_enabled")]
    multi_hop_enabled: bool,
    /// 是否启用知识图谱（实验性功能）
    #[serde(default = "default_graph_rag_enabled")]
    graph_rag_enabled: bool,
    /// 忽略文件列表：这些文件不会被索引和监控
    #[serde(default)]
    ignored_files: Vec<String>,
}

fn default_streaming_enabled() -> bool {
    true
}

fn default_vector_distance_threshold() -> f32 {
    0.9 // 默认值：过滤距离>0.9的结果（放宽阈值以支持自然语言问句）
}

fn default_llm_context_size() -> u32 {
    16384 // 默认上下文窗口大小 (parallel=2, 每 slot 8192)
}

fn default_llm_max_tokens() -> u32 {
    1024 // 默认最大生成 token 数
}

fn default_llm_think_enabled() -> bool {
    false // 默认关闭 Think 模式，因为会增加延迟
}

fn default_context_expansion_enabled() -> bool {
    true // 默认开启上下文扩展
}

fn default_multi_hop_enabled() -> bool {
    true // 默认开启多跳检索
}

fn default_graph_rag_enabled() -> bool {
    false // 默认关闭，实验性功能
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

/// 计算上下文字符预算
/// 公式：(context_size / parallel - max_tokens - prompt_overhead) * chars_per_token
/// parallel=2（2个slot），prompt_overhead=300（system prompt等），chars_per_token≈2（中文）
fn compute_context_budget(config: &AppConfig) -> usize {
    let tokens_for_context = (config.llm_context_size / 2)
        .saturating_sub(config.llm_max_tokens)
        .saturating_sub(300);
    (tokens_for_context as usize) * 2
}

/// 预估 DataBlock 注入为 Markdown 表格后的字符数
/// 包含：表头行 + 分隔行 + 数据行 + 元数据开销
fn estimate_markdown_chars(block: &knot_excel::DataBlock) -> usize {
    let n_cols = block.column_names.len();
    let n_rows = block.row_count;

    // 估算平均 cell 宽度（采样前10行中最长的 cell）
    let avg_cell_width = if !block.rows.is_empty() {
        let sample_rows = block.rows.iter().take(10);
        let total_chars: usize = sample_rows
            .flat_map(|row| row.iter())
            .map(|cell| cell.len())
            .sum();
        let sample_cells = block.rows.len().min(10) * n_cols;
        if sample_cells > 0 {
            (total_chars / sample_cells).max(4) // 最少4字符
        } else {
            8
        }
    } else {
        8 // 默认 cell 宽度
    };

    // 每行字符数：| cell | cell | ... | + 换行
    let row_chars = n_cols * (avg_cell_width + 3) + 2; // +3 = " | ", +2 = "| " + " |"

    // 元数据开销（文件名、sheet名、列信息等）
    let metadata_overhead = 200 + n_cols * 30; // 列名+类型信息

    metadata_overhead + row_chars * (2 + n_rows) // +2 = header + separator
}

// --- Knowledge Page ---

#[derive(serde::Serialize, Clone, Debug)]
enum KnowledgeFileType {
    Markdown,
    Text,
    Pdf,
    Html,
    Word,
    PowerPoint,
    Excel,
    Csv,
    Image,
    Other,
}

#[derive(serde::Serialize, Clone, Debug)]
enum KnowledgeIndexState {
    Unindexed,
    Indexed,
    Outdated,
    Ignored,
    Unsupported,
}

#[derive(serde::Serialize, Clone, Debug)]
struct KnowledgeFile {
    path: String,
    name: String,
    relative_path: String,
    size: u64,
    modified: i64,
    file_type: KnowledgeFileType,
    index_status: KnowledgeIndexState,
}

fn classify_file_type(ext: &str) -> KnowledgeFileType {
    match ext.to_lowercase().as_str() {
        "md" => KnowledgeFileType::Markdown,
        "txt" => KnowledgeFileType::Text,
        "pdf" => KnowledgeFileType::Pdf,
        "html" | "htm" => KnowledgeFileType::Html,
        "docx" => KnowledgeFileType::Word,
        "pptx" => KnowledgeFileType::PowerPoint,
        "xlsx" | "xls" | "xlsm" | "xlsb" => KnowledgeFileType::Excel,
        "csv" => KnowledgeFileType::Csv,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" => KnowledgeFileType::Image,
        _ => KnowledgeFileType::Other,
    }
}

fn is_indexable_type(file_type: &KnowledgeFileType) -> bool {
    matches!(
        file_type,
        KnowledgeFileType::Markdown
            | KnowledgeFileType::Text
            | KnowledgeFileType::Pdf
            | KnowledgeFileType::Html
            | KnowledgeFileType::Excel
    )
}

#[tauri::command]
async fn list_knowledge_files(app: tauri::AppHandle) -> Result<Vec<KnowledgeFile>, String> {
    use walkdir::WalkDir;

    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("Data directory not set")?;
    let ignored_files: std::collections::HashSet<String> =
        config.ignored_files.into_iter().collect();

    // Get index base dir for FileRegistry
    let base_dir = get_index_base_dir(&app, &data_dir);
    let db_path = base_dir.join("knot.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

    let registry = knot_core::registry::FileRegistry::new(&db_url).await.ok();

    // Get all registered file hashes for batch lookup
    let mut registered_hashes: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some(reg) = &registry {
        if let Ok(files) = reg.get_all_file_hashes().await {
            registered_hashes = files;
        }
    }

    let data_path = std::path::Path::new(&data_dir);
    let mut files = Vec::new();

    for entry in WalkDir::new(&data_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path();

        // Skip hidden files
        let file_name_str = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name_str.starts_with('.') {
            continue;
        }

        // Skip temp/lock files (Office ~$, editor backups, etc.)
        if file_name_str.starts_with("~$")
            || file_name_str.starts_with('~')
            || file_name_str.ends_with('~')
            || file_name_str == "Thumbs.db"
            || file_name_str == "desktop.ini"
        {
            continue;
        }

        // Skip hidden directories in path
        let is_in_hidden_dir = file_path.ancestors().any(|ancestor| {
            ancestor
                .file_name()
                .map(|s| s.to_string_lossy().starts_with('.') && s != ".")
                .unwrap_or(false)
        });
        if is_in_hidden_dir {
            continue;
        }

        let ext = file_path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        let file_type = classify_file_type(&ext);

        // Skip Other type (unknown extensions)
        if matches!(file_type, KnowledgeFileType::Other) {
            continue;
        }

        let abs_path = std::fs::canonicalize(file_path)
            .unwrap_or_else(|_| file_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        let name = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let relative_path = file_path
            .strip_prefix(data_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| abs_path.clone());

        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Determine index status
        let index_status = if ignored_files.contains(&abs_path) {
            KnowledgeIndexState::Ignored
        } else if !is_indexable_type(&file_type) {
            KnowledgeIndexState::Unsupported
        } else if let Some(stored_hash) = registered_hashes.get(&abs_path) {
            // File is in registry, check if hash matches
            if let Ok(content) = std::fs::read(file_path) {
                let current_hash = hex::encode(blake3::hash(&content).as_bytes());
                if &current_hash == stored_hash {
                    KnowledgeIndexState::Indexed
                } else {
                    KnowledgeIndexState::Outdated
                }
            } else {
                KnowledgeIndexState::Indexed // Can't read file, assume indexed
            }
        } else {
            // Debug: print unindexed files that should have been indexed
            if ext == "pdf" || ext == "md" {
                println!("[Knowledge] Unindexed file: '{}'", abs_path);
                // Print similar keys in registry for debugging
                for key in registered_hashes.keys() {
                    if key.contains(&name) {
                        println!("[Knowledge]   Registry has similar: '{}'", key);
                    }
                }
            }
            KnowledgeIndexState::Unindexed
        };

        files.push(KnowledgeFile {
            path: abs_path,
            name,
            relative_path,
            size,
            modified,
            file_type,
            index_status,
        });
    }

    // Sort by name
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(files)
}

// --- Knowledge Page Detail & Actions ---

#[derive(serde::Serialize, Clone, Debug)]
struct ChunkSummary {
    id: String,
    preview: String,
    breadcrumbs: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
struct FileIndexDetail {
    file_path: String,
    chunk_count: usize,
    chunks: Vec<ChunkSummary>,
    indexed_at: Option<i64>,
    content_hash: Option<String>,
}

#[tauri::command]
async fn get_file_index_detail(
    app: tauri::AppHandle,
    file_path: String,
) -> Result<FileIndexDetail, String> {
    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("Data directory not set")?;
    let base_dir = get_index_base_dir(&app, &data_dir);

    let index_path = base_dir
        .join("knot_index.lance")
        .to_string_lossy()
        .to_string();
    let db_path = base_dir.join("knot.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

    // Get chunks from Tantivy
    let store = knot_core::store::KnotStore::new(&index_path)
        .await
        .map_err(|e| format!("Store error: {}", e))?;
    let raw_chunks = store
        .get_file_chunks(&file_path)
        .map_err(|e| format!("Chunk query error: {}", e))?;

    let chunks: Vec<ChunkSummary> = raw_chunks
        .into_iter()
        .map(|(id, preview, breadcrumbs)| ChunkSummary {
            id,
            preview,
            breadcrumbs,
        })
        .collect();
    let chunk_count = chunks.len();

    // Get registry info
    let registry = knot_core::registry::FileRegistry::new(&db_url).await.ok();
    let (indexed_at, content_hash) = if let Some(reg) = &registry {
        let hash = reg.get_file_hash(&file_path).await.ok().flatten();
        let at = reg.get_indexed_at(&file_path).await.ok().flatten();
        (at, hash)
    } else {
        (None, None)
    };

    Ok(FileIndexDetail {
        file_path,
        chunk_count,
        chunks,
        indexed_at,
        content_hash,
    })
}

#[tauri::command]
async fn reindex_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_path: String,
) -> Result<(), String> {
    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("Data directory not set")?;
    let base_dir = get_index_base_dir(&app, &data_dir);

    let index_path = base_dir
        .join("knot_index.lance")
        .to_string_lossy()
        .to_string();
    let db_path = base_dir.join("knot.db").to_string_lossy().to_string();
    let db_url = format!("sqlite://{}?mode=rwc", db_path);

    let _ = app.emit("indexing-status", format!("reindexing: {}", file_path));

    // 1. Delete old index data
    let store = knot_core::store::KnotStore::new(&index_path)
        .await
        .map_err(|e| format!("Store error: {}", e))?;
    store
        .delete_file(&file_path)
        .await
        .map_err(|e| format!("Delete error: {}", e))?;

    // 2. Clear registry for this file
    let registry = knot_core::registry::FileRegistry::new(&db_url)
        .await
        .map_err(|e| format!("Registry error: {}", e))?;
    registry
        .remove_file(&file_path)
        .await
        .map_err(|e| format!("Registry remove error: {}", e))?;

    // 3. Get embedding provider
    let embedding_provider = {
        let guard = state.thread_safe_embedding.read().await;
        guard.clone()
    };
    let embedding_provider = embedding_provider.ok_or("Embedding engine not loaded")?;
    let provider_dyn: Arc<dyn knot_parser::EmbeddingProvider + Send + Sync> = embedding_provider;

    // 4. Re-index the file
    let mut indexer = knot_core::index::KnotIndexer::new(&db_path, Some(provider_dyn)).await;
    // Configure PDF OCR/VLM
    let manager = ModelPathManager::new(&app);
    let ocr_model_dir = manager.get_download_target_path("ppocrv5");
    indexer.pdf_ocr_enabled = ocr_model_dir.join("det.onnx").exists();
    indexer.pdf_ocr_model_dir = Some(ocr_model_dir.to_string_lossy().to_string());
    indexer.pdf_vision_api_url = Some("http://localhost:11434/v1/chat/completions".to_string());
    indexer.pdf_vision_model = Some("glm-ocr:latest".to_string());
    let path = std::path::Path::new(&file_path);
    let records = indexer
        .index_file(path)
        .await
        .map_err(|e| format!("Index error: {}", e))?;

    // 5. Save new records
    println!("[Knowledge] Reindex produced {} records", records.len());
    if !records.is_empty() {
        // GraphRAG extraction
        let graph_config = load_config(&app);
        if graph_config.graph_rag_enabled {
            let (entities, relations) = knot_core::entity::extract_from_records(&records);
            let deduped = knot_core::entity::dedup_entities(entities);
            let graph_db_path = base_dir.join("knot_graph.db").to_string_lossy().to_string();
            // Delete old entities for this file first
            if let Ok(graph) = knot_core::entity::EntityGraph::new(&graph_db_path).await {
                let _ = graph.delete_by_file(&file_path).await;
                let _ = graph.add_entities(&deduped).await;
                let _ = graph.add_relations(&relations).await;
            }
        }

        store
            .add_records(records)
            .await
            .map_err(|e| format!("Store add error: {}", e))?;
        store
            .create_fts_index()
            .await
            .map_err(|e| format!("FTS error: {}", e))?;
    }

    let _ = app.emit("indexing-status", "ready");
    println!("[Knowledge] Reindexed file: {}", file_path);

    // 6. Update FileRegistry with new hash
    if let Ok(content) = std::fs::read(&file_path) {
        let hash = hex::encode(blake3::hash(&content).as_bytes());
        let modified = std::fs::metadata(&file_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let registry = knot_core::registry::FileRegistry::new(&db_url).await.ok();
        if let Some(reg) = registry {
            let _ = reg
                .update_file(
                    &file_path,
                    &hash,
                    modified,
                    knot_core::index::KnotIndexer::PARSER_VERSION,
                )
                .await;
            println!("[Knowledge] Registry updated for: {}", file_path);
        }
    }

    // 7. Invalidate cached KnotStore so searches see updated Tantivy data
    {
        let mut guard = state.knot_store.write().await;
        *guard = None;
        println!("[Knowledge] Invalidated cached KnotStore after reindex");
    }

    Ok(())
}

#[tauri::command]
async fn ignore_file(app: tauri::AppHandle, file_path: String) -> Result<(), String> {
    // 1. Add to ignored list in config
    let mut config = load_config(&app);
    if !config.ignored_files.contains(&file_path) {
        config.ignored_files.push(file_path.clone());
        let _ = save_config(&app, &config);
    }

    let data_dir = config.data_dir.ok_or("Data directory not set")?;
    let base_dir = get_index_base_dir(&app, &data_dir);
    let index_path = base_dir
        .join("knot_index.lance")
        .to_string_lossy()
        .to_string();
    let db_path = base_dir.join("knot.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.to_string_lossy());

    // 2. Delete from KnotStore (LanceDB + Tantivy)
    if let Ok(store) = knot_core::store::KnotStore::new(&index_path).await {
        let _ = store.delete_file(&file_path).await;
    }

    // 3. Delete from FileRegistry
    if let Ok(registry) = knot_core::registry::FileRegistry::new(&db_url).await {
        let _ = registry.remove_file(&file_path).await;
    }

    // 4. Delete from EntityGraph if GraphRAG enabled
    if config.graph_rag_enabled {
        let graph_db_path = base_dir.join("knot_graph.db").to_string_lossy().to_string();
        if let Ok(graph) = knot_core::entity::EntityGraph::new(&graph_db_path).await {
            let _ = graph.delete_by_file(&file_path).await;
        }
    }

    println!("[Knowledge] Ignored file: {}", file_path);
    Ok(())
}

#[tauri::command]
async fn unignore_file(app: tauri::AppHandle, file_path: String) -> Result<(), String> {
    let mut config = load_config(&app);
    config.ignored_files.retain(|f| f != &file_path);
    let _ = save_config(&app, &config);
    println!("[Knowledge] Unignored file: {}", file_path);
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

    // Pre-warm KnotStore in background
    let knot_store_clone = state.knot_store.clone();
    let app_clone_for_store = app.clone();
    let path_for_store = path.clone();
    tauri::async_runtime::spawn(async move {
        println!("[Store] Pre-warming KnotStore...");
        let base_dir = get_index_base_dir(&app_clone_for_store, &path_for_store);
        let index_path = base_dir.join("knot_index.lance");
        let index_path_str = index_path.to_string_lossy().to_string();

        match knot_core::store::KnotStore::new(&index_path_str).await {
            Ok(store) => {
                let mut guard = knot_store_clone.write().await;
                *guard = Some(Arc::new(store));
                println!("[Store] KnotStore pre-warmed and cached");
            }
            Err(e) => {
                eprintln!("[Store] Failed to pre-warm KnotStore: {}", e);
            }
        }
    });

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
    let provider_dyn: Arc<dyn knot_parser::EmbeddingProvider + Send + Sync> = embedding_provider;

    let mut indexer = KnotIndexer::new(&db_path, Some(provider_dyn)).await;
    // Configure PDF OCR/VLM for background indexing
    let manager = ModelPathManager::new(&app);
    let ocr_model_dir = manager.get_download_target_path("ppocrv5");
    indexer.pdf_ocr_enabled = ocr_model_dir.join("det.onnx").exists();
    indexer.pdf_ocr_model_dir = Some(ocr_model_dir.to_string_lossy().to_string());
    indexer.pdf_vision_api_url = Some("http://localhost:11434/v1/chat/completions".to_string());
    indexer.pdf_vision_model = Some("glm-ocr:latest".to_string());
    let _ = app.emit("indexing-status", "scanning");

    // 3. Tantivy 一致性检查：
    // - 如果 Tantivy schema 刚被升级重建 → 清空 file_registry 强制全量重建
    // - 如果 Tantivy 为空（可能之前升级没清 registry 遗留）→ 同样强制重建
    {
        match KnotStore::new(&index_path).await {
            Ok(store) => {
                if store.schema_was_rebuilt() {
                    println!("[Indexer] Tantivy schema was upgraded. Clearing file_registry for full rebuild...");
                    indexer.clear_registry().await;
                } else if store.get_doc_count().unwrap_or(0) == 0 {
                    println!("[Indexer] Tantivy is empty. Forcing full rebuild...");
                    indexer.clear_registry().await;
                }
            }
            Err(e) => {
                eprintln!("[Indexer] Warning: could not check Tantivy state: {}", e);
            }
        }
    }

    // 4. Scan & Index (Initial Pass)
    let input_path = std::path::Path::new(&data_dir);

    // Initial Scan
    match indexer.index_directory(&input_path).await {
        Ok((records, deleted, pending_files)) => {
            println!("[Indexer] Found {} new/modified records.", records.len());
            let _ = app.emit("indexing-status", "saving");

            // GraphRAG: 提取实体（在 records 被 move 前）
            let config = load_config(&app);
            let records_empty_for_graph = records.is_empty();
            let entity_data = if config.graph_rag_enabled && !records.is_empty() {
                Some(knot_core::entity::extract_from_records(&records))
            } else {
                None
            };

            if !records.is_empty() || !deleted.is_empty() {
                match KnotStore::new(&index_path).await {
                    Ok(store) => {
                        for del in &deleted {
                            let _ = store.delete_file(del).await;
                        }
                        if !records.is_empty() {
                            if let Err(e) = store.add_records(records).await {
                                eprintln!("[Indexer] Store Add Error: {}", e);
                            } else {
                                let _ = store.create_fts_index().await;
                                // LanceDB 写入成功，确认 file_registry
                                indexer.confirm_indexed(&pending_files).await;
                                println!(
                                    "[Indexer] Confirmed {} files in registry",
                                    pending_files.len()
                                );
                            }
                        }

                        // GraphRAG: 写入实体图（去重后写入）
                        if let Some((entities, relations)) = entity_data {
                            let deduped = knot_core::entity::dedup_entities(entities);
                            let graph_db = index_path.replace("knot_index.lance", "");
                            let graph_db_path = format!("{}knot_graph.db", graph_db);
                            match knot_core::entity::EntityGraph::new(&graph_db_path).await {
                                Ok(graph) => {
                                    let _ = graph.add_entities(&deduped).await;
                                    let _ = graph.add_relations(&relations).await;
                                    println!(
                                        "[GraphRAG] Indexed {} entities ({} before dedup), {} relations",
                                        deduped.len(),
                                        deduped.len(),
                                        relations.len()
                                    );
                                }
                                Err(e) => eprintln!("[GraphRAG] Init error: {}", e),
                            }
                        }
                    }
                    Err(e) => eprintln!("[Indexer] Store Init Error: {}", e),
                }
            } else if !pending_files.is_empty() {
                // records 为空但有 pending files（不应该发生，但保险起见）
                indexer.confirm_indexed(&pending_files).await;
            }

            // GraphRAG: 当 records 为空但图谱为空时，尝试回填
            let config_recheck = load_config(&app);
            if config_recheck.graph_rag_enabled && records_empty_for_graph {
                let graph_db_path = index_path.replace("knot_index.lance", "knot_graph.db");
                let graph_db_exists = std::path::Path::new(&graph_db_path).exists();
                let graph_empty = if graph_db_exists {
                    match knot_core::entity::EntityGraph::new(&graph_db_path).await {
                        Ok(g) => {
                            let count = g.entity_count().await.unwrap_or(0);
                            if count > 0 {
                                println!(
                                    "[GraphRAG] Graph already has {} entities, no backfill needed",
                                    count
                                );
                            }
                            count == 0
                        }
                        Err(_) => true,
                    }
                } else {
                    true
                };

                if graph_empty {
                    println!("[GraphRAG] Graph is empty, attempting backfill...");

                    // 尝试从 Tantivy 读取已有数据
                    let mut backfill_records = Vec::new();
                    if let Ok(store) = KnotStore::new(&index_path).await {
                        if let Ok(texts) = store.get_all_texts() {
                            backfill_records = texts;
                        }
                    }

                    if backfill_records.is_empty() {
                        // Tantivy 也是空的（可能 Clear Index 后 registry 残留），
                        // 清除 file registry 强制重新扫描
                        println!(
                            "[GraphRAG] Index is empty, clearing file registry for full re-scan..."
                        );
                        let db_path_for_reg = index_path.replace("knot_index.lance", "knot.db");
                        let db_url = format!("sqlite:{}", db_path_for_reg);
                        if let Ok(registry) = knot_core::registry::FileRegistry::new(&db_url).await
                        {
                            let _ = registry.clear_all().await;
                            println!("[GraphRAG] File registry cleared, re-scanning...");
                        }
                        // 用 indexer 重新扫描
                        match indexer.index_directory(&input_path).await {
                            Ok((new_records, _, pending_files)) if !new_records.is_empty() => {
                                println!("[GraphRAG] Re-scan found {} records", new_records.len());
                                let (entities, relations) =
                                    knot_core::entity::extract_from_records(&new_records);
                                let deduped = knot_core::entity::dedup_entities(entities);
                                // 保存 records 到 store
                                if let Ok(store) = KnotStore::new(&index_path).await {
                                    if let Ok(()) = store.add_records(new_records).await {
                                        let _ = store.create_fts_index().await;
                                        indexer.confirm_indexed(&pending_files).await;
                                    }
                                }
                                // 写入图谱
                                if let Ok(graph) =
                                    knot_core::entity::EntityGraph::new(&graph_db_path).await
                                {
                                    let _ = graph.add_entities(&deduped).await;
                                    let _ = graph.add_relations(&relations).await;
                                    println!(
                                        "[GraphRAG] Backfilled {} entities, {} relations (via re-scan)",
                                        deduped.len(), relations.len()
                                    );
                                }
                            }
                            _ => println!("[GraphRAG] Re-scan found no records"),
                        }
                    } else {
                        // Tantivy 有数据，直接从已有数据提取
                        println!(
                            "[GraphRAG] Found {} existing records for backfill",
                            backfill_records.len()
                        );
                        let (entities, relations) =
                            knot_core::entity::extract_from_records(&backfill_records);
                        let deduped = knot_core::entity::dedup_entities(entities);
                        if let Ok(graph) = knot_core::entity::EntityGraph::new(&graph_db_path).await
                        {
                            let _ = graph.add_entities(&deduped).await;
                            let _ = graph.add_relations(&relations).await;
                            println!(
                                "[GraphRAG] Backfilled {} entities, {} relations",
                                deduped.len(),
                                relations.len()
                            );
                        }
                    }
                }
            }

            println!("[Indexer] Initial scan complete.");
            let _ = app.emit("indexing-status", "ready");

            // 重新创建 KnotStore 并缓存（而不是仅 invalidate）
            // 这样第一次搜索就能命中缓存，避免冷启动 Jieba 字典加载（约 800ms）
            let app_state = app.state::<AppState>();
            match KnotStore::new(&index_path).await {
                Ok(new_store) => {
                    let mut guard = app_state.knot_store.write().await;
                    *guard = Some(Arc::new(new_store));
                    println!("[Indexer] KnotStore re-warmed after indexing");
                }
                Err(e) => {
                    eprintln!(
                        "[Indexer] Failed to re-warm store: {}, invalidating cache",
                        e
                    );
                    let mut guard = app_state.knot_store.write().await;
                    *guard = None;
                }
            }

            // Excel DuckDB 持久化缓存预热
            // 遍历数据目录中所有 Excel 文件，解析后写入 DuckDB 缓存文件
            {
                let cache_db_path =
                    index_path.replace("knot_index.lance", "knot_excel_cache.duckdb");
                match knot_excel::ExcelCache::new(&cache_db_path) {
                    Ok(cache) => {
                        let excel_config = knot_excel::ExcelConfig::default();
                        let mut cached_count = 0;
                        let mut skipped_count = 0;

                        // 递归收集 Excel 文件
                        fn collect_excel_files(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
                            if let Ok(entries) = std::fs::read_dir(dir) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if path.is_dir() {
                                        collect_excel_files(&path, files);
                                    } else if let Some(name) = path.file_name() {
                                        let name_str = name.to_string_lossy();
                                        // 跳过 ~$ 临时锁文件和隐藏文件
                                        if name_str.starts_with("~$") || name_str.starts_with('.') {
                                            continue;
                                        }
                                        if let Some(ext) = path.extension() {
                                            let ext = ext.to_string_lossy().to_lowercase();
                                            if matches!(
                                                ext.as_str(),
                                                "xlsx" | "xls" | "xlsm" | "xlsb"
                                            ) {
                                                files.push(path);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let mut excel_files = Vec::new();
                        collect_excel_files(input_path, &mut excel_files);

                        for file_path in &excel_files {
                            let path_str = file_path.to_string_lossy().to_string();

                            // 跳过已缓存且未变更的文件
                            if cache.is_cache_valid(&path_str) {
                                skipped_count += 1;
                                continue;
                            }

                            match knot_excel::pipeline::parse_excel_full(file_path, &excel_config) {
                                Ok(parsed) => {
                                    if let Err(e) = cache.upsert_file(&path_str, &parsed.blocks) {
                                        eprintln!(
                                            "[ExcelCache] Failed to cache {}: {}",
                                            path_str, e
                                        );
                                    } else {
                                        cached_count += 1;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[ExcelCache] Failed to parse {}: {}", path_str, e);
                                }
                            }
                        }

                        if cached_count > 0 || skipped_count > 0 {
                            println!(
                                "[ExcelCache] Warm-up complete: {} cached, {} skipped (unchanged), {} total Excel files",
                                cached_count, skipped_count, excel_files.len()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("[ExcelCache] Failed to init cache: {}", e);
                    }
                }
            }
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
                                    // Excel 缓存清理
                                    let cache_db_path = index_path_for_watch.replace("knot_index.lance", "knot_excel_cache.duckdb");
                                    let excel_cache = knot_excel::ExcelCache::new(&cache_db_path).ok();

                                    for path in pending_removals.drain() {
                                        println!("[Monitor] Removal detected: {:?}", path);
                                        let path_str = path.to_string_lossy();
                                        let _ = store.delete_file(&path_str).await;
                                        let _ = store.delete_folder(&path_str).await;

                                        // 清除 Excel 缓存
                                        if let Some(ref cache) = excel_cache {
                                            if let Some(ext) = path.extension() {
                                                let ext = ext.to_string_lossy().to_lowercase();
                                                if matches!(ext.as_str(), "xlsx" | "xls" | "xlsm" | "xlsb") {
                                                    let _ = cache.remove_file(&path_str);
                                                    println!("[ExcelCache] Removed cache for {}", path_str);
                                                }
                                            }
                                        }
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

                                let mut updated_excel_paths: Vec<PathBuf> = Vec::new();

                                for path in paths {
                                    if should_index_file(&path) {
                                         if path.exists() {
                                             // 记录 Excel 文件用于缓存更新
                                             if let Some(ext) = path.extension() {
                                                 let ext = ext.to_string_lossy().to_lowercase();
                                                 if matches!(ext.as_str(), "xlsx" | "xls" | "xlsm" | "xlsb") {
                                                     updated_excel_paths.push(path.clone());
                                                 }
                                             }

                                             // Index it
                                                             match indexer.index_file(&path).await {
                                                 Ok(records) => {
                                                      if !records.is_empty() {
                                                          total_records += records.len();
                                                          updated_cnt += 1;

                                                          // GraphRAG: 混合提取（LLM + 规则降级）
                                                          let watch_config = load_config(&app);
                                                          let entity_data = if watch_config.graph_rag_enabled {
                                                              // 尝试获取 parsing LLM client
                                                              let parsing_client = {
                                                                  let state = app.state::<AppState>();
                                                                  let guard = state.parsing_client.read().await;
                                                                  guard.clone()
                                                              };

                                                              let (entities, relations) = if let Some(client) = parsing_client {
                                                                  let client_clone = client.clone();
                                                                  let llm_fn = |prompt: String| {
                                                                      let c = client_clone.clone();
                                                                      async move {
                                                                          use knot_parser::LlmProvider;
                                                                          c.generate_content(&prompt).await.ok()
                                                                      }
                                                                  };
                                                                  knot_core::entity::extract_from_records_with_llm(&records, Some(llm_fn)).await
                                                              } else {
                                                                  knot_core::entity::extract_from_records(&records)
                                                              };
                                                              Some((entities, relations))
                                                          } else {
                                                              None
                                                          };

                                                      if let Some(store) = &store_opt {
                                                           let _ = store.delete_file(&path.to_string_lossy()).await;
                                                           let _ = store.add_records(records).await;
                                                      }

                                                      // GraphRAG: 写入实体图（去重后写入）
                                                      if let Some((entities, relations)) = entity_data {
                                                          let deduped = knot_core::entity::dedup_entities(entities);
                                                          let graph_path = index_path_for_watch.replace("knot_index.lance", "knot_graph.db");
                                                          if let Ok(graph) = knot_core::entity::EntityGraph::new(&graph_path).await {
                                                              let _ = graph.delete_by_file(&path.to_string_lossy()).await;
                                                              let _ = graph.add_entities(&deduped).await;
                                                              let _ = graph.add_relations(&relations).await;
                                                          }
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

                                // Excel DuckDB 缓存更新
                                if !updated_excel_paths.is_empty() {
                                    let cache_db_path = index_path_for_watch.replace("knot_index.lance", "knot_excel_cache.duckdb");
                                    if let Ok(cache) = knot_excel::ExcelCache::new(&cache_db_path) {
                                        let excel_config = knot_excel::ExcelConfig::default();
                                        for excel_path in &updated_excel_paths {
                                            let path_str = excel_path.to_string_lossy().to_string();
                                            match knot_excel::pipeline::parse_excel_full(excel_path, &excel_config) {
                                                Ok(parsed) => {
                                                    if let Err(e) = cache.upsert_file(&path_str, &parsed.blocks) {
                                                        eprintln!("[ExcelCache] Monitor update failed for {}: {}", path_str, e);
                                                    } else {
                                                        println!("[ExcelCache] Updated cache for {}", path_str);
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("[ExcelCache] Monitor parse failed for {}: {}", path_str, e);
                                                }
                                            }
                                        }
                                    }
                                }
                             }

                            let _ = app.emit("indexing-status", "ready");

                            // 文件监控索引后重新预热 KnotStore
                            {
                                let app_state = app.state::<AppState>();
                                match KnotStore::new(&index_path_for_watch).await {
                                    Ok(new_store) => {
                                        let mut guard = app_state.knot_store.write().await;
                                        *guard = Some(Arc::new(new_store));
                                    }
                                    Err(_) => {
                                        let mut guard = app_state.knot_store.write().await;
                                        *guard = None;
                                    }
                                }
                            }
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

#[derive(serde::Serialize, Clone)]
struct HybridSearchResultDisplay {
    file_path: String,
    score: f32,
    context: Option<String>,
    text: String,
    source: String,
}

/// 搜索响应（不含 LLM 回答）
#[derive(serde::Serialize)]
struct RagSearchResponse {
    sources: Vec<HybridSearchResultDisplay>,
    context: String, // 预构建的上下文，供 rag_generate 使用
}

/// 将 DataBlock 注入为 Markdown 表格到 context（最多 50 行）
/// 将 DataBlock 作为 Markdown 表格注入上下文
/// remaining_budget: 剩余可用字符数，注入后会更新
/// 返回实际写入的字符数
fn inject_block_as_markdown(
    block: &knot_excel::DataBlock,
    profiles: &[knot_excel::TableProfile],
    file_path: &std::path::Path,
    tabular_context: &mut String,
    remaining_budget: &mut usize,
) -> usize {
    if *remaining_budget == 0 {
        return 0;
    }

    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy();
    // 根据 budget 动态计算最大行数（每行约 100-300 字符）
    let estimated_row_size: usize = block.column_names.len() * 15 + 20;
    let max_rows_by_budget = if estimated_row_size > 0 {
        (*remaining_budget / estimated_row_size).max(3).min(30)
    } else {
        30
    };
    let max_rows = max_rows_by_budget;
    let truncated = block.row_count > max_rows;
    let display_rows = block.row_count.min(max_rows);

    let mut chunk = String::new();
    chunk.push_str(&format!(
        "[表格数据] {} / Sheet \"{}\"\n共 {} 行 {} 列\n\n",
        file_name,
        block.sheet_name,
        block.row_count,
        block.column_names.len()
    ));

    // 列信息
    if let Some(profile) = profiles.iter().find(|p| p.source_id == block.source_id) {
        chunk.push_str("列信息:\n");
        for (name, dtype) in block.column_names.iter().zip(profile.column_types.iter()) {
            chunk.push_str(&format!("- {} ({})\n", name, dtype));
        }
        chunk.push('\n');
    }

    // Markdown 表格
    chunk.push_str(&format!(
        "数据（共 {} 行{}）:\n",
        display_rows,
        if truncated {
            format!("，已截断，原始共 {} 行", block.row_count)
        } else {
            String::new()
        }
    ));
    chunk.push_str(&format!("| {} |\n", block.column_names.join(" | ")));
    let sep: Vec<&str> = block.column_names.iter().map(|_| "---").collect();
    chunk.push_str(&format!("| {} |\n", sep.join(" | ")));
    for row in block.rows.iter().take(max_rows) {
        chunk.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    if truncated {
        chunk.push_str(&format!("... (省略了 {} 行)\n", block.row_count - max_rows));
    }
    chunk.push('\n');

    // 检查 budget
    let written = chunk.len();
    if written > *remaining_budget {
        // 即使超 budget 也至少注入表头和列信息（截断数据行）
        let header_end = chunk.find("数据（").unwrap_or(chunk.len());
        let header_part = &chunk[..header_end.min(*remaining_budget)];
        tabular_context.push_str(header_part);
        tabular_context.push_str("... (预算不足，数据已省略)\n\n");
        *remaining_budget = 0;
        return header_part.len();
    }

    tabular_context.push_str(&chunk);
    *remaining_budget = remaining_budget.saturating_sub(written);
    written
}

/// 仅执行搜索，快速返回结果
#[tauri::command]
async fn rag_search(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: String,
    file_path: Option<String>,
) -> Result<RagSearchResponse, String> {
    use std::time::Instant;

    // 边缘情况：空查询直接返回空结果
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(RagSearchResponse {
            sources: vec![],
            context: String::new(),
        });
    }

    // 边缘情况：超长查询截断到 500 字符
    const MAX_QUERY_LENGTH: usize = 500;
    let query = if query.chars().count() > MAX_QUERY_LENGTH {
        println!(
            "[rag_search] Query truncated from {} to {} chars",
            query.chars().count(),
            MAX_QUERY_LENGTH
        );
        query.chars().take(MAX_QUERY_LENGTH).collect::<String>()
    } else {
        query
    };

    // 预处理查询文本（中英文边界、噪音去重）
    let query = knot_core::store::KnotStore::preprocess_query(&query);

    let total_start = Instant::now();
    println!("[rag_search] Starting query: {}", query);

    // 1. Get or initialize cached store
    let store_start = Instant::now();
    let store = {
        // Try to get cached store
        let guard = state.knot_store.read().await;
        if let Some(s) = guard.as_ref() {
            println!("[rag_search] Using cached store");
            s.clone()
        } else {
            drop(guard); // Release read lock

            // Initialize store
            let config = load_config(&app);
            let data_dir = config.data_dir.ok_or("Data directory not set")?;
            let base_dir = get_index_base_dir(&app, &data_dir);
            let index_path = base_dir.join("knot_index.lance");
            let index_path_str = index_path.to_string_lossy().to_string();

            let new_store = Arc::new(
                knot_core::store::KnotStore::new(&index_path_str)
                    .await
                    .map_err(|e| format!("Store error: {}", e))?,
            );

            // Cache the store
            let mut write_guard = state.knot_store.write().await;
            *write_guard = Some(new_store.clone());
            println!("[rag_search] Store initialized and cached");
            new_store
        }
    };
    println!("[rag_search] Store ready: {:?}", store_start.elapsed());

    // 2. Generate Query Embedding
    use knot_parser::EmbeddingProvider;
    let embedding_provider = {
        let guard = state.thread_safe_embedding.read().await;
        guard.clone()
    }
    .ok_or("Embedding Engine not ready")?;

    let embed_start = Instant::now();
    let query_vec = embedding_provider
        .generate_embedding(&query)
        .await
        .map_err(|e| e.to_string())?;
    println!("[rag_search] Embedding: {:?}", embed_start.elapsed());

    // 3. Execute Search
    let config = load_config(&app);
    let distance_threshold = config.vector_distance_threshold;

    let search_start = Instant::now();
    let mut search_results = store
        .search_with_filter(query_vec, &query, distance_threshold, file_path.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "[rag_search] Search: {:?}, found {} results",
        search_start.elapsed(),
        search_results.len()
    );

    // 3.5 Multi-Hop Search (if enabled)
    if config.multi_hop_enabled && !search_results.is_empty() {
        let hop_start = Instant::now();
        let key_terms = knot_core::store::KnotStore::extract_key_terms(&search_results, &query, 5);
        if !key_terms.is_empty() {
            let expanded_query = format!("{} {}", query, key_terms.join(" "));
            println!(
                "[rag_search] Multi-hop expanded query: '{}'",
                expanded_query
            );
            let hop_vec = embedding_provider
                .generate_embedding(&expanded_query)
                .await
                .map_err(|e| e.to_string())?;
            let hop_results = store
                .search_with_filter(
                    hop_vec,
                    &expanded_query,
                    distance_threshold,
                    file_path.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?;
            let first_count = search_results.len();
            search_results =
                knot_core::store::KnotStore::merge_search_results(search_results, hop_results);
            println!(
                "[rag_search] Multi-hop: {:?}, {} -> {} results (terms: {:?})",
                hop_start.elapsed(),
                first_count,
                search_results.len(),
                key_terms
            );
        }
    }

    // 3.6 Context Expansion (if enabled)
    if config.context_expansion_enabled {
        let expand_start = Instant::now();
        store.expand_search_context(&mut search_results);
        println!(
            "[rag_search] Context expansion: {:?}",
            expand_start.elapsed()
        );
    }

    // 3.7 GraphRAG: 图查询增强 (if enabled)
    if config.graph_rag_enabled {
        let graph_start = Instant::now();
        let data_dir = config.data_dir.as_deref().unwrap_or("");
        let graph_base = get_index_base_dir(&app, data_dir);
        let graph_db_path = graph_base.join("knot_graph.db");
        if graph_db_path.exists() {
            if let Ok(graph) =
                knot_core::entity::EntityGraph::new(&graph_db_path.to_string_lossy()).await
            {
                // 策略 1: 从查询中提取实体
                let query_entities = knot_core::entity::extract_entities_rule_based(&query, "", "");
                let mut entity_names: Vec<String> =
                    query_entities.iter().map(|e| e.name.clone()).collect();

                // 策略 2: 直接用查询关键词查图谱（大小写不敏感）
                // 这样 "rust"、"react" 等小写关键词也能匹配图谱中的 "Rust"、"React"
                for word in query.split_whitespace() {
                    let word_clean = word.trim_matches(|c: char| !c.is_alphanumeric());
                    if word_clean.len() >= 2
                        && !entity_names
                            .iter()
                            .any(|e| e.to_lowercase() == word_clean.to_lowercase())
                    {
                        entity_names.push(word_clean.to_string());
                    }
                }

                let mut graph_context = String::new();
                for entity_name in entity_names.iter().take(5) {
                    if let Ok(related) = graph.get_related_entities(entity_name).await {
                        if !related.is_empty() {
                            graph_context.push_str(&format!(
                                "[知识图谱] {} 关联: {}\n",
                                entity_name,
                                related
                                    .iter()
                                    .take(5)
                                    .map(|r| format!("{}({})", r.name, r.relation_type))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                    }
                }
                // 将图谱上下文附加到第一个搜索结果
                if !graph_context.is_empty() {
                    if let Some(first) = search_results.first_mut() {
                        let existing = first.expanded_context.take().unwrap_or_default();
                        first.expanded_context = Some(
                            format!("{}\n{}", existing, graph_context)
                                .trim()
                                .to_string(),
                        );
                    }
                    println!(
                        "[rag_search] GraphRAG: {:?}, found relations for {} entities",
                        graph_start.elapsed(),
                        query_entities.len()
                    );
                }
            }
        }
    }
    // 4. Format Context and Display Sources
    // HybridQueryRouter: 按 doc_type 分流，tabular 数据注入完整表格
    let mut context_str = String::new();
    let mut display_sources = Vec::new();

    // 4.1 分流: 分离 text 和 tabular 结果
    let text_results: Vec<_> = search_results
        .iter()
        .filter(|r| r.doc_type != "tabular")
        .collect();
    let tabular_results: Vec<_> = search_results
        .iter()
        .filter(|r| r.doc_type == "tabular")
        .collect();

    let has_tabular = !tabular_results.is_empty();
    let has_text = !text_results.is_empty();

    if has_tabular {
        println!(
            "[rag_search] HybridRouter: {} text + {} tabular results",
            text_results.len(),
            tabular_results.len()
        );
    }

    // 4.2 处理 tabular 结果: 根据上下文预算动态选择策略
    let mut tabular_context = String::new();
    // tabular markdown fallback 的预算上限（占总 budget 的 70%，留 30% 给 text）
    let mut tabular_budget: usize = {
        let config = load_config(&app);
        let budget = compute_context_budget(&config);
        (budget as f64 * 0.7) as usize
    };
    if has_tabular {
        // === 计算上下文预算 ===
        let config = load_config(&app);
        let budget = compute_context_budget(&config);

        // 估算 text 切片总字符数
        let text_chars_estimate: usize = text_results
            .iter()
            .take(5)
            .map(|r| {
                let base = r.text.len();
                let expanded = r.expanded_context.as_ref().map_or(0, |e| e.len());
                base + expanded + 120 // 格式化开销: [序号] 文件: ... 内容: ...
            })
            .sum();

        let mut seen_files: std::collections::HashSet<String> = std::collections::HashSet::new();
        for res in tabular_results.iter().take(3) {
            // 避免重复加载同一文件
            if !seen_files.insert(res.file_path.clone()) {
                continue;
            }

            let file_path = std::path::Path::new(&res.file_path);
            if file_path.exists() {
                let excel_config = knot_excel::ExcelConfig::default();
                match knot_excel::pipeline::parse_excel_full(file_path, &excel_config) {
                    Ok(parsed) => {
                        // === 动态预算判断（替代硬编码 row_count > 50）===
                        let tabular_chars_estimate: usize = parsed
                            .blocks
                            .iter()
                            .map(|b| estimate_markdown_chars(b))
                            .sum();
                        let total_estimate = tabular_chars_estimate + text_chars_estimate;
                        // 预留 10% headroom
                        let budget_with_margin = (budget as f64 * 0.9) as usize;
                        let use_sql = total_estimate > budget_with_margin;

                        println!(
                            "[rag_search] Budget: {} chars | Text: ~{} chars | Tabular: ~{} chars | Total: ~{} | Strategy: {}",
                            budget, text_chars_estimate, tabular_chars_estimate, total_estimate,
                            if use_sql { "SQL (exceeds budget)" } else { "Direct inject (fits)" }
                        );

                        if use_sql {
                            // === 大表格策略：DuckDB Text-to-SQL（使用持久化缓存）===
                            println!(
                                "[rag_search] Large table detected in {}, using DuckDB Text-to-SQL",
                                res.file_path
                            );

                            // 获取或更新 DuckDB 缓存
                            let config = load_config(&app);
                            let data_dir = config.data_dir.ok_or("Data directory not set")?;
                            let base_dir = get_index_base_dir(&app, &data_dir);
                            let index_path_str = base_dir
                                .join("knot_index.lance")
                                .to_string_lossy()
                                .to_string();
                            let cache_db_path = index_path_str
                                .replace("knot_index.lance", "knot_excel_cache.duckdb");

                            match knot_excel::ExcelCache::new(&cache_db_path) {
                                Ok(cache) => {
                                    // 查询时校验：如果缓存过期则懒更新
                                    if !cache.is_cache_valid(&res.file_path) {
                                        println!(
                                            "[rag_search] Cache stale for {}, updating...",
                                            res.file_path
                                        );
                                        if let Err(e) =
                                            cache.upsert_file(&res.file_path, &parsed.blocks)
                                        {
                                            println!("[rag_search] Cache update failed: {}, falling back to in-memory", e);
                                        }
                                    } else {
                                        println!(
                                            "[rag_search] Using cached DuckDB data for {}",
                                            res.file_path
                                        );
                                    }

                                    // 从缓存获取查询引擎
                                    match cache.get_query_engine(&res.file_path) {
                                        Ok(engine) => {
                                            let schemas = engine.get_schemas();
                                            if !schemas.is_empty() {
                                                // 用 SqlGenerator 构建 Prompt
                                                let sql_system =
                                                    knot_excel::SqlGenerator::build_system_prompt();
                                                let sql_user =
                                                    knot_excel::SqlGenerator::build_user_prompt(
                                                        &schemas,
                                                        &parsed.blocks,
                                                        &query,
                                                    );
                                                let sql_prompt = format!(
                                            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                                            sql_system, sql_user
                                        );

                                                // 调用 LLM 生成 SQL（带超时保护，避免 GPU 竞争时无限等待）
                                                let llm_for_sql = {
                                                    let guard = state.chat_client.read().await;
                                                    guard.clone()
                                                };

                                                if let Some(llm_client) = llm_for_sql {
                                                    use knot_parser::LlmProvider;
                                                    let sql_gen_result = tokio::time::timeout(
                                                        std::time::Duration::from_secs(10),
                                                        llm_client.generate_content(&sql_prompt),
                                                    )
                                                    .await;

                                                    match sql_gen_result {
                                                        Ok(Ok(generated_sql)) => {
                                                            let sql = generated_sql
                                                                .trim()
                                                                .trim_start_matches("```sql")
                                                                .trim_start_matches("```")
                                                                .trim_end_matches("```")
                                                                .trim()
                                                                .to_string();

                                                            println!(
                                                                "[rag_search] Generated SQL: {}",
                                                                sql
                                                            );

                                                            // 执行 SQL（带重试）
                                                            let mut final_result =
                                                                engine.execute_multi_step(&sql);
                                                            let mut retried = false;

                                                            // 5.6: SQL 容错重试（最多 2 次）
                                                            if final_result.is_err() {
                                                                for retry in 0..2 {
                                                                    let err_msg = final_result
                                                                        .as_ref()
                                                                        .unwrap_err()
                                                                        .to_string();
                                                                    println!(
                                                                "[rag_search] SQL failed (retry {}): {}",
                                                                retry + 1,
                                                                err_msg
                                                            );

                                                                    let fix_prompt_text = knot_excel::SqlGenerator::build_fix_prompt(
                                                                &sql,
                                                                &err_msg,
                                                                &schemas,
                                                            );
                                                                    let fix_prompt = format!(
                                                                "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                                                                sql_system, fix_prompt_text
                                                            );

                                                                    match llm_client
                                                                        .generate_content(
                                                                            &fix_prompt,
                                                                        )
                                                                        .await
                                                                    {
                                                                        Ok(fixed_sql) => {
                                                                            let fixed = fixed_sql
                                                                                .trim()
                                                                                .trim_start_matches(
                                                                                    "```sql",
                                                                                )
                                                                                .trim_start_matches(
                                                                                    "```",
                                                                                )
                                                                                .trim_end_matches(
                                                                                    "```",
                                                                                )
                                                                                .trim()
                                                                                .to_string();
                                                                            println!("[rag_search] Fixed SQL: {}", fixed);
                                                                            final_result = engine
                                                                                .execute_multi_step(
                                                                                    &fixed,
                                                                                );
                                                                            if final_result.is_ok()
                                                                            {
                                                                                retried = true;
                                                                                break;
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            println!(
                                                                        "[rag_search] LLM fix failed: {}",
                                                                        e
                                                                    );
                                                                            break;
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            // 处理 SQL 结果
                                                            match final_result {
                                                                Ok(mut result) => {
                                                                    result.retried = retried;
                                                                    let ctx = knot_excel::ResultSummarizer::process(&result);
                                                                    let file_name = file_path
                                                                        .file_name()
                                                                        .unwrap_or_default()
                                                                        .to_string_lossy();

                                                                    tabular_context.push_str(
                                                                        &format!(
                                                                            "[表格数据查询] {}\n",
                                                                            file_name
                                                                        ),
                                                                    );
                                                                    tabular_context.push_str(
                                                                        &format!(
                                                                            "执行的 SQL: {}\n\n",
                                                                            result.sql
                                                                        ),
                                                                    );
                                                                    tabular_context.push_str(
                                                                        &ctx.to_prompt_text(),
                                                                    );
                                                                    tabular_context.push('\n');
                                                                    println!(
                                                                "[rag_search] SQL query success: {} rows{}",
                                                                result.row_count,
                                                                if retried { " (retried)" } else { "" }
                                                            );
                                                                }
                                                                Err(e) => {
                                                                    println!(
                                                                "[rag_search] SQL execution failed after retries: {}",
                                                                e
                                                            );
                                                                    // Fallback: 使用 Markdown 注入前 50 行
                                                                    for block in &parsed.blocks {
                                                                        inject_block_as_markdown(
                                                                            block,
                                                                            &parsed.profiles,
                                                                            file_path,
                                                                            &mut tabular_context,
                                                                            &mut tabular_budget,
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        Err(_) => {
                                                            println!(
                                                        "[rag_search] LLM SQL generation timed out (10s), falling back to markdown"
                                                    );
                                                            for block in &parsed.blocks {
                                                                inject_block_as_markdown(
                                                                    block,
                                                                    &parsed.profiles,
                                                                    file_path,
                                                                    &mut tabular_context,
                                                                    &mut tabular_budget,
                                                                );
                                                            }
                                                        }
                                                        Ok(Err(e)) => {
                                                            println!(
                                                        "[rag_search] LLM SQL generation error: {}, falling back to markdown",
                                                        e
                                                    );
                                                            for block in &parsed.blocks {
                                                                inject_block_as_markdown(
                                                                    block,
                                                                    &parsed.profiles,
                                                                    file_path,
                                                                    &mut tabular_context,
                                                                    &mut tabular_budget,
                                                                );
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    // LLM 不可用，fallback 到 Markdown
                                                    for block in &parsed.blocks {
                                                        inject_block_as_markdown(
                                                            block,
                                                            &parsed.profiles,
                                                            file_path,
                                                            &mut tabular_context,
                                                            &mut tabular_budget,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            println!(
                                                "[rag_search] Cache query engine failed: {}, falling back to markdown",
                                                e
                                            );
                                            for block in &parsed.blocks {
                                                inject_block_as_markdown(
                                                    block,
                                                    &parsed.profiles,
                                                    file_path,
                                                    &mut tabular_context,
                                                    &mut tabular_budget,
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!(
                                        "[rag_search] ExcelCache init failed: {}, falling back to markdown",
                                        e
                                    );
                                    for block in &parsed.blocks {
                                        inject_block_as_markdown(
                                            block,
                                            &parsed.profiles,
                                            file_path,
                                            &mut tabular_context,
                                            &mut tabular_budget,
                                        );
                                    }
                                }
                            }
                        } else {
                            // === 小表格策略：直接注入 Markdown 表格 ===
                            for block in &parsed.blocks {
                                inject_block_as_markdown(
                                    block,
                                    &parsed.profiles,
                                    file_path,
                                    &mut tabular_context,
                                    &mut tabular_budget,
                                );
                            }
                        }
                        println!("[rag_search] Loaded full Excel data from {}", res.file_path);
                    }
                    Err(e) => {
                        println!(
                            "[rag_search] Failed to reload Excel {}: {}",
                            res.file_path, e
                        );
                        // Fallback: 使用搜索结果中的 profile chunk
                        tabular_context.push_str(&res.text);
                        tabular_context.push('\n');
                    }
                }
            } else {
                // 文件不存在，使用搜索结果中的 profile chunk
                tabular_context.push_str(&res.text);
                tabular_context.push('\n');
            }
        }
    }

    // 4.3 组装最终 context
    // 先放 tabular 数据（精确数据优先），再放 text 数据
    if !tabular_context.is_empty() {
        context_str.push_str("=== 表格数据 ===\n");
        context_str.push_str("以下是与查询相关的结构化表格数据，可直接用于数据分析和计算：\n\n");
        context_str.push_str(&tabular_context);
        context_str.push_str("=== 表格数据结束 ===\n\n");
    }

    // 4.4 Text 结果正常处理
    let mut all_results_for_display: Vec<_> = if has_tabular && has_text {
        // 混合场景: 合并 text 和 tabular 结果，按分数排序
        text_results
            .iter()
            .take(4)
            .chain(tabular_results.iter().take(1))
            .copied()
            .collect()
    } else {
        search_results.iter().take(5).collect()
    };
    // 按分数降序排列（高分靠前）
    all_results_for_display.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 4.4 Text 结果正常处理（预算感知截取）
    let text_budget = if has_tabular {
        let config = load_config(&app);
        let total_budget = compute_context_budget(&config);
        // text 预算 = 总预算 - 已用的 tabular 上下文字符数
        total_budget.saturating_sub(context_str.len())
    } else {
        usize::MAX // 无 tabular 时不限制
    };
    let mut text_chars_used: usize = 0;

    for (i, res) in all_results_for_display.iter().enumerate() {
        if res.doc_type == "tabular" && has_tabular {
            // tabular 结果已经在上面处理过，这里只添加 source
            display_sources.push(HybridSearchResultDisplay {
                file_path: res.file_path.clone(),
                score: res.score,
                context: res.breadcrumbs.clone(),
                text: format!(
                    "[表格数据] {}",
                    res.file_path.rsplit('/').next().unwrap_or(&res.file_path)
                ),
                source: "Tabular".to_string(),
            });
            continue;
        }

        let context_line = res.breadcrumbs.clone().unwrap_or_default();
        // 拼入扩展上下文
        let expanded = res.expanded_context.as_deref().unwrap_or("");
        let content_with_context = if expanded.is_empty() {
            res.text.clone()
        } else {
            format!("{}\n---\n{}", res.text, expanded)
        };
        let entry = format!(
            "[{}] (匹配度: {:.0}%) 文件: {} - 章节: {}\n内容: {}\n\n",
            i + 1,
            res.score,
            res.file_path,
            context_line,
            content_with_context
        );

        // 预算检查：超出预算的低分切片不注入
        if text_chars_used + entry.len() > text_budget {
            println!(
                "[rag_search] Text budget exhausted ({}/{} chars), skipping remaining {} slices",
                text_chars_used,
                text_budget,
                all_results_for_display.len() - i
            );
            // 但仍添加到 display_sources 以便前端展示
            display_sources.push(HybridSearchResultDisplay {
                file_path: res.file_path.clone(),
                score: res.score,
                context: res.breadcrumbs.clone(),
                text: res.text.clone(),
                source: res.source.to_string(),
            });
            continue;
        }

        text_chars_used += entry.len();
        context_str.push_str(&entry);

        display_sources.push(HybridSearchResultDisplay {
            file_path: res.file_path.clone(),
            score: res.score,
            context: res.breadcrumbs.clone(),
            text: res.text.clone(),
            source: res.source.to_string(),
        });
    }

    println!("[rag_search] Total: {:?}", total_start.elapsed());

    Ok(RagSearchResponse {
        sources: display_sources,
        context: context_str,
    })
}

/// 单文件 Excel Text-to-SQL 查询
#[tauri::command]
async fn query_excel_table(
    state: State<'_, AppState>,
    file_path: String,
    query: String,
    page: Option<usize>,
    page_size: Option<usize>,
) -> Result<ExcelQueryResponse, String> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).min(200);

    println!(
        "[query_excel_table] file={}, query={}, page={}, page_size={}",
        file_path, query, page, page_size
    );

    // 1. 加载 Excel 文件
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let excel_config = knot_excel::ExcelConfig::default();
    let parsed = knot_excel::pipeline::parse_excel_full(path, &excel_config)
        .map_err(|e| format!("Failed to parse Excel: {}", e))?;

    if parsed.blocks.is_empty() {
        return Err("No data blocks found in file".to_string());
    }

    // 2. 注册到 DuckDB
    let mut engine =
        knot_excel::QueryEngine::new().map_err(|e| format!("DuckDB init failed: {}", e))?;

    for block in &parsed.blocks {
        engine
            .register_datablock(block)
            .map_err(|e| format!("Register block failed: {}", e))?;
    }

    let schemas = engine.get_registered_schemas();
    if schemas.is_empty() {
        return Err("No tables registered".to_string());
    }

    // 3. 生成 SQL Prompt
    let sql_system = knot_excel::SqlGenerator::build_system_prompt();
    let sql_user = knot_excel::SqlGenerator::build_user_prompt(&schemas, &parsed.blocks, &query);
    let sql_prompt = format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        sql_system, sql_user
    );

    // 4. LLM 生成 SQL
    let llm_client = {
        let guard = state.chat_client.read().await;
        guard.clone()
    }
    .ok_or("Chat LLM not ready")?;

    // SQL 只需短输出（~200 tokens），使用流式 API 收集结果，避免阻塞过久
    let mut rx = llm_client
        .generate_content_stream(&sql_prompt, 256)
        .await
        .map_err(|e| format!("LLM generation failed: {}", e))?;

    let mut generated_sql = String::new();
    while let Some(token) = rx.recv().await {
        generated_sql.push_str(&token);
    }
    // 过滤掉 <think>...</think> 内容
    let generated_sql = if let Some(end_idx) = generated_sql.find("</think>") {
        generated_sql[end_idx + 8..].trim().to_string()
    } else {
        generated_sql.trim().to_string()
    };

    let sql = generated_sql
        .trim()
        .trim_start_matches("```sql")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    println!("[query_excel_table] Generated SQL: {}", sql);

    // 5. 执行 SQL（带重试）
    let mut final_result = engine.execute_multi_step(&sql);
    let mut final_sql = sql.clone();
    let mut retried = false;

    if final_result.is_err() {
        for retry in 0..2 {
            let err_msg = final_result.as_ref().unwrap_err().to_string();
            println!(
                "[query_excel_table] SQL failed (retry {}): {}",
                retry + 1,
                err_msg
            );

            let fix_prompt_text =
                knot_excel::SqlGenerator::build_fix_prompt(&final_sql, &err_msg, &schemas);
            let fix_prompt = format!(
                "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                sql_system, fix_prompt_text
            );

            match llm_client.generate_content_stream(&fix_prompt, 256).await {
                Ok(mut fix_rx) => {
                    let mut fixed_raw = String::new();
                    while let Some(token) = fix_rx.recv().await {
                        fixed_raw.push_str(&token);
                    }
                    // 过滤 <think>...</think>
                    let fixed_raw = if let Some(end_idx) = fixed_raw.find("</think>") {
                        fixed_raw[end_idx + 8..].trim().to_string()
                    } else {
                        fixed_raw.trim().to_string()
                    };
                    let fixed = fixed_raw
                        .trim()
                        .trim_start_matches("```sql")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim()
                        .to_string();
                    println!("[query_excel_table] Fixed SQL: {}", fixed);
                    final_result = engine.execute_multi_step(&fixed);
                    final_sql = fixed;
                    if final_result.is_ok() {
                        retried = true;
                        break;
                    }
                }
                Err(e) => {
                    println!("[query_excel_table] LLM fix failed: {}", e);
                    break;
                }
            }
        }
    }

    // 6. 处理结果
    let mut result = final_result.map_err(|e| format!("SQL execution failed: {}", e))?;
    result.retried = retried;

    let total_count = result.row_count;

    // 7. 分页处理
    let (paged_rows, paged_count) = if total_count > page_size {
        // 大结果集：分页
        let paged_sql = format!(
            "SELECT * FROM ({}) AS _paged LIMIT {} OFFSET {}",
            final_sql,
            page_size,
            (page - 1) * page_size
        );
        match engine.execute_multi_step(&paged_sql) {
            Ok(paged_result) => (paged_result.rows, paged_result.row_count),
            Err(_) => {
                // 分页失败，fallback 到截断
                let start = ((page - 1) * page_size).min(result.rows.len());
                let end = (start + page_size).min(result.rows.len());
                let rows = result.rows[start..end].to_vec();
                let count = rows.len();
                (rows, count)
            }
        }
    } else {
        (result.rows, result.row_count)
    };

    let ctx = knot_excel::ResultSummarizer::process_with_count(total_count);

    println!(
        "[query_excel_table] Success: {} total rows, page {}/{}, showing {}",
        total_count,
        page,
        (total_count + page_size - 1) / page_size,
        paged_count
    );

    // 8. 缓存 engine 到 AppState（供翻页复用）
    {
        let mut cache = state.excel_engine_cache.lock().await;
        cache.insert(
            file_path.clone(),
            (engine, final_sql.clone(), std::time::Instant::now()),
        );
    }

    Ok(ExcelQueryResponse {
        sql: final_sql,
        columns: result.columns,
        rows: paged_rows,
        row_count: paged_count,
        total_count,
        current_page: page,
        page_size,
        is_summarized: ctx.is_summarized(),
        summary_text: if ctx.is_summarized() {
            Some(ctx.to_prompt_text())
        } else {
            None
        },
        retried,
    })
}

#[derive(serde::Serialize)]
struct ExcelQueryResponse {
    sql: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    row_count: usize,
    total_count: usize,  // 总行数（分页前）
    current_page: usize, // 当前页（1-based）
    page_size: usize,    // 每页行数
    is_summarized: bool,
    summary_text: Option<String>,
    retried: bool,
}

/// 翻页查询：复用缓存的 DuckDB 会话，不重新生成 SQL
#[tauri::command]
async fn query_excel_page(
    state: State<'_, AppState>,
    file_path: String,
    page: usize,
    page_size: Option<usize>,
) -> Result<ExcelQueryResponse, String> {
    let page = page.max(1);
    let page_size = page_size.unwrap_or(20).min(200);

    println!(
        "[query_excel_page] file={}, page={}, page_size={}",
        file_path, page, page_size
    );

    // 从缓存获取 engine，执行分页查询
    let mut cache = state.excel_engine_cache.lock().await;
    let (engine, sql, last_access) = cache.get_mut(&file_path).ok_or("缓存已过期，请重新查询")?;
    *last_access = std::time::Instant::now();
    let sql = sql.clone();

    // 先获取总行数
    let count_sql = format!("SELECT COUNT(*) FROM ({}) AS _cnt", sql);
    let total_count = match engine.execute_multi_step(&count_sql) {
        Ok(r) => {
            if let Some(row) = r.rows.first() {
                row.first()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0)
            } else {
                0
            }
        }
        Err(_) => 0,
    };

    // 执行分页查询
    let paged_sql = format!(
        "SELECT * FROM ({}) AS _paged LIMIT {} OFFSET {}",
        sql,
        page_size,
        (page - 1) * page_size
    );

    let result = engine
        .execute_multi_step(&paged_sql)
        .map_err(|e| format!("分页查询失败: {}", e))?;

    println!(
        "[query_excel_page] Success: {} total, page {}/{}, showing {}",
        total_count,
        page,
        (total_count + page_size - 1) / page_size.max(1),
        result.row_count
    );

    Ok(ExcelQueryResponse {
        sql,
        columns: result.columns,
        rows: result.rows,
        row_count: result.row_count,
        total_count,
        current_page: page,
        page_size,
        is_summarized: false,
        summary_text: None,
        retried: false,
    })
}

/// 根据搜索上下文生成 LLM 回答 (Streaming Version)
#[tauri::command]
async fn rag_generate(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: String,
    context: String,
) -> Result<(), String> {
    // Note: generate_content_stream is an inherent method of LlamaClient, not part of LlmProvider trait currently.
    use knot_parser::LlmProvider;

    println!("[rag_generate] Starting generation for query: {}", query);

    let llm_client = {
        let guard = state.chat_client.read().await;
        guard.clone()
    }
    .ok_or("Chat LLM (Qwen3) not ready")?;

    // 加载配置
    let config = load_config(&app);

    // 使用共享预算计算函数（兜底：rag_search 已保证大多数情况不超限）
    let max_context_chars = compute_context_budget(&config);
    println!(
        "[rag_generate] Config: context_size={}, max_tokens={}, max_context_chars={}",
        config.llm_context_size, config.llm_max_tokens, max_context_chars
    );

    let context_len = context.chars().count();

    let final_context = if context_len > max_context_chars {
        println!(
            "[rag_generate] Context too long ({} chars), using two-stage compression...",
            context_len
        );

        // 第一阶段：让 LLM 从长上下文中提取与问题相关的关键信息
        let compress_prompt = format!(
            r#"<|im_start|>system
你是信息提取专家。请从以下文档中提取与用户问题最相关的关键信息。
只输出关键信息，不要回答问题本身。保持简洁，最多 500 字。
<|im_end|>
<|im_start|>user
用户问题: {}

文档内容:
{}
<|im_end|>
<|im_start|>assistant
"#,
            query,
            // 第一阶段可以使用更长的上下文，因为输出很短
            context.chars().take(12000).collect::<String>()
        );

        use knot_parser::LlmProvider;
        match llm_client.generate_content(&compress_prompt).await {
            Ok(compressed) => {
                println!(
                    "[rag_generate] Compression complete: {} -> {} chars",
                    context_len,
                    compressed.len()
                );
                compressed
            }
            Err(e) => {
                println!("[rag_generate] Compression failed: {}, using truncation", e);
                // 降级：压缩失败则使用简单截断
                context.chars().take(max_context_chars).collect::<String>()
            }
        }
    } else {
        context
    };

    // 构建 prompt
    // 注意：在 assistant 回复开头预填充空 <think></think> 块
    // 这告诉模型"思考阶段已完成"，直接输出答案
    // /no_think 指令对 /completion 原始接口无效，只在聊天模板引擎中生效
    let think_prefix = if config.llm_think_enabled {
        "" // 允许模型自由思考
    } else {
        "<think>\n</think>\n\n" // 预填充空 think 块，跳过思考
    };

    let prompt = format!(
        r#"<|im_start|>system
你是一个智能助手。请根据参考文档直接回答用户问题。

**回答原则**：
1. **直接回答**：开门见山，把答案放在第一句。优先从文档中提取具体的日期、时间、地点、数字等事实信息来回答问题。
2. **去除客套**：不要使用"根据参考文档..."、"综上所述..."等前缀。
3. **禁止输出思考过程**：不要写分析推理文字，直接给最终答案。
4. **详细展开**：在核心答案之后，引用文档细节进行补充说明。
5. 只有当文档完全不包含任何相关信息时，才说"无法找到答案"。如果文档中有任何相关内容，都要尽力回答。
6. **表格展示**：当回答包含多条结构化数据（如列表、记录、对比）时，使用 Markdown 表格格式展示。示例：
| 日期 | 收缩压 | 舒张压 | 心率 |
|------|--------|--------|------|
| 2024-06-18 | 102 | 69 | 75 |
<|im_end|>
<|im_start|>user
参考文档：
{}

用户问题: {}<|im_end|>
<|im_start|>assistant
{}"#,
        final_context, query, think_prefix
    );

    println!(
        "[rag_generate] Prompt Preview (first 500 chars):\n{:.500}...",
        prompt
    );

    // Check config for streaming preference
    let config = load_config(&app);
    if config.streaming_enabled {
        // Use streaming
        println!("[rag_generate] Streaming enabled. Starting stream...");
        let mut rx = llm_client
            .generate_content_stream(&prompt, config.llm_max_tokens)
            .await
            .map_err(|e| e.to_string())?;

        println!("[rag_generate] Stream started...");

        // 递增 generation_id，记住自己的 ID
        let my_gen_id = state
            .generation_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        println!("[rag_generate] Generation ID: {}", my_gen_id);

        while let Some(token) = rx.recv().await {
            // 检查是否被新搜索取消（全局 ID 不再匹配自己）
            let current_id = state
                .generation_id
                .load(std::sync::atomic::Ordering::Relaxed);
            if current_id != my_gen_id {
                println!(
                    "[rag_generate] Generation {} superseded by {}, stopping.",
                    my_gen_id, current_id
                );
                break;
            }
            // Emit token event directly - thinking is prevented at prompt level
            // via empty <think></think> prefix
            if let Err(e) = app.emit("llm-token", token) {
                println!("[rag_generate] Failed to emit token: {}", e);
                break;
            }
        }

        println!("[rag_generate] Stream finished.");
    } else {
        // Use synchronous generation
        println!("[rag_generate] Streaming disabled. Using sync generation...");
        // Add a small delay for UI to switch state if needed (optional)
        // tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let content = llm_client
            .generate_content(&prompt)
            .await
            .map_err(|e| e.to_string())?;

        // Emit the entire content as one token
        if let Err(e) = app.emit("llm-token", content) {
            println!("[rag_generate] Failed to emit content: {}", e);
        }
        println!("[rag_generate] Sync generation finished.");
    }

    // Emit done event
    let _ = app.emit("llm-done", ());

    Ok(())
}

#[tauri::command]
async fn rag_query(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: String,
) -> Result<RagResponse, String> {
    use knot_parser::LlmProvider;

    // 边缘情况：空查询直接返回空结果
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(RagResponse {
            answer: "请输入搜索内容".to_string(),
            sources: vec![],
        });
    }

    // 边缘情况：超长查询截断到 500 字符
    const MAX_QUERY_LENGTH: usize = 500;
    let query = if query.chars().count() > MAX_QUERY_LENGTH {
        println!(
            "[rag_query] Query truncated from {} to {} chars",
            query.chars().count(),
            MAX_QUERY_LENGTH
        );
        query.chars().take(MAX_QUERY_LENGTH).collect::<String>()
    } else {
        query
    };

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

    // 预处理查询文本（中英文边界、噪音去重）
    let query = knot_core::store::KnotStore::preprocess_query(&query);

    println!("[rag_query] Acquiring embedding lock...");
    let embedding_provider = {
        let guard = state.thread_safe_embedding.read().await;
        println!("[rag_query] Embedding lock acquired");
        guard.clone()
    }
    .ok_or("Embedding Engine not ready")?;

    // Generate Query Embedding
    use knot_parser::EmbeddingProvider;
    println!("[rag_query] Generating embedding...");
    let query_vec = embedding_provider
        .generate_embedding(&query)
        .await
        .map_err(|e| e.to_string())?;
    println!("[rag_query] Embedding generated");

    let config = load_config(&app);
    let distance_threshold = config.vector_distance_threshold;

    let mut search_results = store
        .search(query_vec, &query, distance_threshold)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "[rag_query] Search complete. Found {} results",
        search_results.len()
    );

    // Multi-Hop Search (if enabled)
    if config.multi_hop_enabled && !search_results.is_empty() {
        let key_terms = knot_core::store::KnotStore::extract_key_terms(&search_results, &query, 5);
        if !key_terms.is_empty() {
            let expanded_query = format!("{} {}", query, key_terms.join(" "));
            println!("[rag_query] Multi-hop expanded query: '{}'", expanded_query);
            let hop_vec = embedding_provider
                .generate_embedding(&expanded_query)
                .await
                .map_err(|e| e.to_string())?;
            let hop_results = store
                .search(hop_vec, &expanded_query, distance_threshold)
                .await
                .map_err(|e| e.to_string())?;
            search_results =
                knot_core::store::KnotStore::merge_search_results(search_results, hop_results);
        }
    }

    // Context Expansion (if enabled)
    if config.context_expansion_enabled {
        store.expand_search_context(&mut search_results);
    }

    // GraphRAG: 图查询增强 (if enabled)
    if config.graph_rag_enabled {
        let data_dir_str = config.data_dir.as_deref().unwrap_or("");
        let graph_base = get_index_base_dir(&app, data_dir_str);
        let graph_db_path = graph_base.join("knot_graph.db");
        if graph_db_path.exists() {
            if let Ok(graph) =
                knot_core::entity::EntityGraph::new(&graph_db_path.to_string_lossy()).await
            {
                let query_entities = knot_core::entity::extract_entities_rule_based(&query, "", "");
                let mut entity_names: Vec<String> =
                    query_entities.iter().map(|e| e.name.clone()).collect();
                for word in query.split_whitespace() {
                    let word_clean = word.trim_matches(|c: char| !c.is_alphanumeric());
                    if word_clean.len() >= 2
                        && !entity_names
                            .iter()
                            .any(|e| e.to_lowercase() == word_clean.to_lowercase())
                    {
                        entity_names.push(word_clean.to_string());
                    }
                }
                let mut graph_context = String::new();
                for entity_name in entity_names.iter().take(5) {
                    if let Ok(related) = graph.get_related_entities(entity_name).await {
                        if !related.is_empty() {
                            graph_context.push_str(&format!(
                                "[知识图谱] {} 关联: {}\n",
                                entity_name,
                                related
                                    .iter()
                                    .take(5)
                                    .map(|r| format!("{}({})", r.name, r.relation_type))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                    }
                }
                if !graph_context.is_empty() {
                    if let Some(first) = search_results.first_mut() {
                        let existing = first.expanded_context.take().unwrap_or_default();
                        first.expanded_context = Some(
                            format!("{}\n{}", existing, graph_context)
                                .trim()
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
    // 3. Format Context
    let mut context_str = String::new();
    let mut display_sources = Vec::new();

    for (i, res) in search_results.iter().take(5).enumerate() {
        let context_line = res.breadcrumbs.clone().unwrap_or_default();
        let expanded = res.expanded_context.as_deref().unwrap_or("");
        let content_with_context = if expanded.is_empty() {
            res.text.clone()
        } else {
            format!("{}\n---\n{}", res.text, expanded)
        };
        context_str.push_str(&format!(
            "[{}] (匹配度: {:.0}%) 文件: {} - 章节: {}\n内容: {}\n\n",
            i + 1,
            res.score,
            res.file_path,
            context_line,
            content_with_context
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
        "<|im_start|>system\n你是一个智能助手。请根据参考文档回答用户问题。\n\n**回答原则**：\n1. **开门见山**：直接把文档中找到的关键信息（如日期、地点、结论）放在第一句。\n2. **去除客套**：不要使用“根据参考文档…”、“综上所述…”等前缀。\n3. **详细展开**：在核心答案之后，引用文档细节进行说明。\n4. 只有当文档完全不包含相关信息时，才说“无法找到答案”。\n<|im_end|>\n<|im_start|>user\n参考文档：\n{}\n\n用户问题: {}<|im_end|>\n<|im_start|>assistant\n",
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
async fn get_model_status(state: State<'_, AppState>) -> Result<String, String> {
    let status = state.model_status.read().await;
    Ok(status.clone())
}

#[tauri::command]
async fn check_model_status(app: tauri::AppHandle, filename: String) -> Result<bool, String> {
    let manager = ModelPathManager::new(&app);
    Ok(manager.get_model_path(&filename).exists())
}

/// 所有核心模型的完整性检查
/// 返回 { all_ready: bool, missing: [String] }
#[tauri::command]
async fn check_all_models(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let manager = ModelPathManager::new(&app);

    let core_models = vec![
        "GLM-OCR-Q8_0.gguf",
        "mmproj-GLM-OCR-Q8_0.gguf",
        "Qwen3.5-4B-Q4_K_M.gguf",
        "ppocrv5/det.onnx",
        "ppocrv5/rec.onnx",
        "ppocrv5/ppocrv5_dict.txt",
    ];

    let mut missing = Vec::new();
    for model in &core_models {
        let path = manager.get_model_path(model);
        if !path.exists() {
            missing.push(model.to_string());
        }
    }

    let all_ready = missing.is_empty();

    Ok(serde_json::json!({
        "all_ready": all_ready,
        "missing": missing,
    }))
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

    // 所有核心模型列表
    let all_models = vec![
        "GLM-OCR-Q8_0.gguf",
        "mmproj-GLM-OCR-Q8_0.gguf",
        "Qwen3.5-4B-Q4_K_M.gguf",
        "ppocrv5/det.onnx",
        "ppocrv5/rec.onnx",
        "ppocrv5/ppocrv5_dict.txt",
    ];

    // 只下载缺失的模型
    let manager = ModelPathManager::new(&app);
    let mut added = 0;
    for model in &all_models {
        let path = manager.get_model_path(model);
        if !path.exists() {
            qm.add_to_queue(model.to_string()).await;
            added += 1;
            println!("[Queue] Added missing model: {}", model);
        } else {
            // 已存在的模型立即发出完成事件，让前端更新状态
            let _ = app.emit("queue-item-complete", model.to_string());
            println!("[Queue] Skipping existing model: {}", model);
        }
    }

    if added == 0 {
        // 全部已存在，直接完成
        let _ = app.emit("queue-finished", ());
        return Ok(());
    }

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
    {
        let mut status = state.model_status.write().await;
        *status = "loading".to_string();
    }
    let _ = app.emit("model-status", "loading");

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
        let chat_model_path = manager.get_model_path("Qwen3.5-4B-Q4_K_M.gguf");

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
                    *c_guard = Some(client.clone());
                    println!("[Reload] ✓ Chat LLM Started.");

                    // Fire and forget warmup
                    let app_handle = app.clone();
                    let model_status_clone = state.model_status.clone(); // Clone for task

                    tokio::spawn(async move {
                        if let Err(e) = client.warmup().await {
                            eprintln!("[Reload] Warmup failed: {}", e);
                            {
                                let mut status = model_status_clone.write().await;
                                *status = "error".to_string();
                            }
                            let _ = app_handle.emit("model-status", "error");
                        } else {
                            {
                                let mut status = model_status_clone.write().await;
                                *status = "ready".to_string();
                            }
                            let _ = app_handle.emit("model-status", "ready");
                        }
                    });
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

#[tauri::command]
async fn set_streaming_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = load_config(&app);
    config.streaming_enabled = enabled;
    save_config(&app, &config)?;
    Ok(())
}

#[tauri::command]
async fn set_vector_distance_threshold(
    app: tauri::AppHandle,
    threshold: f32,
) -> Result<(), String> {
    let mut config = load_config(&app);
    config.vector_distance_threshold = threshold;
    save_config(&app, &config)?;
    println!("[Config] Vector distance threshold set to: {}", threshold);
    Ok(())
}

#[tauri::command]
async fn set_llm_context_size(app: tauri::AppHandle, size: u32) -> Result<(), String> {
    let mut config = load_config(&app);
    config.llm_context_size = size;
    save_config(&app, &config)?;
    println!(
        "[Config] LLM context size set to: {} (restart required)",
        size
    );
    Ok(())
}

#[tauri::command]
async fn set_llm_max_tokens(app: tauri::AppHandle, max_tokens: u32) -> Result<(), String> {
    let mut config = load_config(&app);
    config.llm_max_tokens = max_tokens;
    save_config(&app, &config)?;
    println!("[Config] LLM max tokens set to: {}", max_tokens);
    Ok(())
}

#[tauri::command]
async fn set_llm_think_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = load_config(&app);
    config.llm_think_enabled = enabled;
    save_config(&app, &config)?;
    println!(
        "[Config] LLM think mode: {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

#[tauri::command]
async fn set_context_expansion_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = load_config(&app);
    config.context_expansion_enabled = enabled;
    save_config(&app, &config)?;
    println!(
        "[Config] Context expansion: {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

#[tauri::command]
async fn set_multi_hop_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = load_config(&app);
    config.multi_hop_enabled = enabled;
    save_config(&app, &config)?;
    println!(
        "[Config] Multi-hop search: {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

#[tauri::command]
async fn set_graph_rag_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = load_config(&app);
    config.graph_rag_enabled = enabled;
    save_config(&app, &config)?;
    println!(
        "[Config] Graph RAG: {}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

#[tauri::command]
async fn get_graph_data(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let config = load_config(&app);
    let data_dir = config.data_dir.ok_or("Data directory not set")?;
    let base_dir = get_index_base_dir(&app, &data_dir);
    let graph_db_path = base_dir.join("knot_graph.db");

    if !graph_db_path.exists() {
        return Ok(serde_json::json!({"nodes": [], "edges": []}));
    }

    let graph = knot_core::entity::EntityGraph::new(&graph_db_path.to_string_lossy())
        .await
        .map_err(|e| format!("Graph init error: {}", e))?;

    let data = graph
        .get_graph_data(50)
        .await
        .map_err(|e| format!("Graph query error: {}", e))?;

    serde_json::to_value(&data).map_err(|e| format!("Serialize error: {}", e))
}

/// 取消正在进行的 LLM 生成。
/// 前端在发起新搜索前调用此命令，rag_generate 的流式循环检测到 flag 后立即停止。
#[tauri::command]
async fn cancel_generation(state: State<'_, AppState>) -> Result<(), String> {
    // 递增 generation_id，使当前正在运行的 rag_generate 检测到 ID 不匹配并停止
    let new_id = state
        .generation_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1;
    println!(
        "[cancel_generation] Generation cancel requested. New ID: {}",
        new_id
    );
    Ok(())
}
