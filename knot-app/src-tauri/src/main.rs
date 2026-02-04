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
        .invoke_handler(tauri::generate_handler![parse_file, open_doc_parser_window])
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
