use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use tokio::sync::mpsc;

pub struct DirectoryWatcher {
    // Keep watcher alive
    _watcher: RecommendedWatcher,
    pub rx: mpsc::UnboundedReceiver<Result<Event, notify::Error>>,
}

impl DirectoryWatcher {
    pub fn new(path: &Path) -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Create a watcher that sends events to the channel
        // notify calls this closure on a separate thread
        let mut watcher = RecommendedWatcher::new(
            move |res: std::result::Result<Event, notify::Error>| {
                match &res {
                    Ok(event) => {
                        let is_ignored = event.paths.iter().any(|p| {
                            let s = p.to_string_lossy();
                            s.contains("knot_index.lance") || s.contains("/.") || s.contains("\\.")
                            // simple check for hidden files/dirs in path
                        });

                        if !is_ignored {
                            let _ = tx.send(res);
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(res);
                    }
                }
            },
            Config::default(),
        )?;

        // Start watching recursively
        watcher.watch(path, RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }
}

pub fn should_index_file(path: &Path) -> bool {
    // Ignore hidden files (starting with dot)
    if path
        .file_name()
        .map(|s| s.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
    {
        // println!("[Monitor] Skipping hidden file: {:?}", path);
        return false;
    }

    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy();
        let should = matches!(
            ext_str.as_ref(),
            "md" | "txt" | "pdf" | "docx" | "pptx" | "xlsx" | "xls" | "xlsm" | "xlsb" | "html"
        );
        if !should {
            // println!(
            //     "[Monitor] Skipping unsupported extension: {:?} ({})",
            //     path, ext_str
            // );
        }
        should
    } else {
        // println!("[Monitor] Skipping no extension: {:?}", path);
        false
    }
}
