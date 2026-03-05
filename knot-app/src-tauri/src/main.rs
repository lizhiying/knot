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
            unignore_file
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
        "xlsx" => KnowledgeFileType::Excel,
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
        if file_path
            .file_name()
            .map(|s| s.to_string_lossy().starts_with('.'))
            .unwrap_or(false)
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
            let _ = reg.update_file(&file_path, &hash, modified).await;
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

    // 3. Scan & Index (Initial Pass)
    let input_path = std::path::Path::new(&data_dir);

    // Initial Scan
    match indexer.index_directory(&input_path).await {
        Ok((records, deleted)) => {
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
                            Ok((new_records, _)) if !new_records.is_empty() => {
                                println!("[GraphRAG] Re-scan found {} records", new_records.len());
                                let (entities, relations) =
                                    knot_core::entity::extract_from_records(&new_records);
                                let deduped = knot_core::entity::dedup_entities(entities);
                                // 保存 records 到 store
                                if let Ok(store) = KnotStore::new(&index_path).await {
                                    let _ = store.add_records(new_records).await;
                                    let _ = store.create_fts_index().await;
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

            // Invalidate cached KnotStore so next search sees new Tantivy segments
            // The pre-warmed store's Tantivy Index doesn't see segments written
            // by the background indexer's separate KnotStore instance
            let app_state = app.state::<AppState>();
            let mut guard = app_state.knot_store.write().await;
            *guard = None;
            println!("[Indexer] Invalidated cached KnotStore (will be re-created on next search)");
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
                             }

                            let _ = app.emit("indexing-status", "ready");

                            // Invalidate cached KnotStore after monitor indexing
                            {
                                let app_state = app.state::<AppState>();
                                let mut guard = app_state.knot_store.write().await;
                                *guard = None;
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
    let mut context_str = String::new();
    let mut display_sources = Vec::new();

    for (i, res) in search_results.iter().take(5).enumerate() {
        let context_line = res.breadcrumbs.clone().unwrap_or_default();
        // 拼入扩展上下文
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

    println!("[rag_search] Total: {:?}", total_start.elapsed());

    Ok(RagSearchResponse {
        sources: display_sources,
        context: context_str,
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
    let llm_context_size = config.llm_context_size;
    let llm_max_tokens = config.llm_max_tokens;

    // 动态计算最大上下文字符数
    // 公式：(context_size / parallel - max_tokens - prompt_overhead) * chars_per_token
    // parallel=2, prompt_overhead=300, chars_per_token≈2
    let tokens_for_context = (llm_context_size / 2)
        .saturating_sub(llm_max_tokens)
        .saturating_sub(300);
    let max_context_chars = (tokens_for_context as usize) * 2;
    println!(
        "[rag_generate] Config: context_size={}, max_tokens={}, max_context_chars={}",
        llm_context_size, llm_max_tokens, max_context_chars
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
            .generate_content_stream(&prompt, llm_max_tokens)
            .await
            .map_err(|e| e.to_string())?;

        println!("[rag_generate] Stream started...");

        while let Some(token) = rx.recv().await {
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
