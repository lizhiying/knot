use super::config::{ModelSourceConfig, Region};
use super::downloader::Downloader;
use super::manager::ModelPathManager;
use std::collections::VecDeque;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

#[derive(Clone, serde::Serialize)]
pub struct QueueItem {
    pub filename: String,
    pub status: String, // "pending", "downloading", "completed", "failed"
}

pub struct QueueManager {
    queue: Arc<Mutex<VecDeque<String>>>,
    is_processing: Arc<Mutex<bool>>,
    source_config: Arc<Mutex<ModelSourceConfig>>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            is_processing: Arc::new(Mutex::new(false)),
            source_config: Arc::new(Mutex::new(ModelSourceConfig::new())),
        }
    }

    pub async fn set_region(&self, region: Region) {
        let mut config = self.source_config.lock().await;
        config.region = region;
    }

    pub async fn add_to_queue(&self, filename: String) {
        let mut queue = self.queue.lock().await;
        if !queue.contains(&filename) {
            queue.push_back(filename);
        }
    }

    // Clear queue
    pub async fn clear_queue(&self) {
        let mut queue = self.queue.lock().await;
        queue.clear();
    }

    pub async fn process_queue(&self, app: AppHandle) -> Result<(), String> {
        let mut is_processing = self.is_processing.lock().await;
        if *is_processing {
            return Ok(()); // Already running
        }
        *is_processing = true;

        // Release lock before async loop
        drop(is_processing);

        let manager = ModelPathManager::new(&app);

        // Check disk space ONLY ONCE before starting batch?
        // Or before each? Milestone says "Download check mount point... if < 5GB block".
        // Let's check before loop starts for the whole batch assumption, or just check.

        if let Err(e) = self
            .check_disk_space(&manager.get_download_target_path(""))
            .await
        {
            let _ = app.emit("download-error", format!("Disk Check Failed: {}", e));
            let mut is_processing = self.is_processing.lock().await;
            *is_processing = false;
            return Err(e);
        }

        loop {
            // 1. Snapshot Queue (Fetch Process Batch)
            let batch: Vec<String> = {
                let mut queue = self.queue.lock().await;
                if queue.is_empty() {
                    break;
                }
                queue.drain(..).collect()
            };

            if batch.is_empty() {
                break;
            }

            // 2. Prepare Tasks Data (Pre-calculate paths/URLs to avoid locking in async block)
            let mut tasks_data = Vec::new();
            for filename in batch {
                let target_path = manager.get_download_target_path(&filename);
                let url = {
                    let config = self.source_config.lock().await;
                    config.get_url(&filename)
                };
                tasks_data.push((filename, target_path, url));
            }

            println!(
                "[Queue] Processing batch of {} files concurrently...",
                tasks_data.len()
            );

            // 3. Execute Concurrently
            let futures = tasks_data.into_iter().map(|(filename, target_path, url)| {
                let app = app.clone();
                async move {
                    let _ = app.emit("queue-status", format!("Starting {}", filename));
                    println!("[Queue] Downloading {} from {}", filename, url);

                    match Downloader::download_file(&app, &url, &target_path, &filename).await {
                        Ok(_) => {
                            let _ = app.emit("queue-item-complete", filename.clone());
                        }
                        Err(e) => {
                            eprintln!("[Queue] Failed {}: {}", filename, e);
                            let _ =
                                app.emit("download-error", format!("Failed {}: {}", filename, e));
                        }
                    }
                }
            });

            futures_util::future::join_all(futures).await;
        }

        let mut is_processing = self.is_processing.lock().await;
        *is_processing = false;
        let _ = app.emit("queue-finished", ());

        Ok(())
    }

    async fn check_disk_space(&self, path: &std::path::Path) -> Result<(), String> {
        use fs2::available_space;

        // Find a valid path to check (parent dir usually)
        let check_path = if path.exists() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        };

        // Ensure dir exists to check space on it
        if !check_path.exists() {
            std::fs::create_dir_all(&check_path).map_err(|e| e.to_string())?;
        }

        match available_space(&check_path) {
            Ok(bytes) => {
                let gb = bytes as f64 / 1024.0 / 1024.0 / 1024.0;
                println!("[Queue] Available space: {:.2} GB", gb);
                if gb < 5.0 {
                    return Err(format!(
                        "Insufficient disk space. Required: 5GB, Available: {:.2}GB",
                        gb
                    ));
                }
                Ok(())
            }
            Err(e) => {
                println!("[Queue] Failed to check space: {}", e);
                // On some systems/sandboxes this might fail. We should ideally not block if check fails unless critical.
                // But requirement says "Check disk space".
                Err(format!("Failed to check disk space: {}", e))
            }
        }
    }
}
