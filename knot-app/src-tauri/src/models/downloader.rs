use futures_util::StreamExt;
use std::path::Path;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::io::AsyncWriteExt;

pub struct Downloader;

#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    filename: String,
    current: u64,
    total: u64,
    percentage: f64,
}

impl Downloader {
    pub async fn download_file<R: Runtime>(
        app: &AppHandle<R>,
        url: &str,
        path: &Path,
        filename_for_event: &str, // 用于前端显示的标识
    ) -> Result<(), String> {
        let client = reqwest::Client::new();
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        let mut last_emit = std::time::Instant::now(); // To throttle events

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| format!("Error while downloading chunk: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Error while writing to file: {}", e))?;

            downloaded += chunk.len() as u64;

            // Emit progress
            // Throttle: Max 10 events per second to avoid clogging channel
            if last_emit.elapsed().as_millis() > 100 {
                let percentage = if total_size > 0 {
                    (downloaded as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                };

                let _ = app.emit(
                    "download-progress",
                    DownloadProgress {
                        filename: filename_for_event.to_string(),
                        current: downloaded,
                        total: total_size,
                        percentage,
                    },
                );
                last_emit = std::time::Instant::now();
            }
        }

        // Final Event (100%)
        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                filename: filename_for_event.to_string(),
                current: downloaded,
                total: total_size,
                percentage: 100.0,
            },
        );

        println!("[Downloader] Download complete: {:?}", path);
        Ok(())
    }
}
