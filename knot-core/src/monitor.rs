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
    let file_name = match path.file_name() {
        Some(name) => name.to_string_lossy(),
        None => return false,
    };

    // 1. 跳过隐藏文件（以 . 开头）
    if file_name.starts_with('.') {
        return false;
    }

    // 2. 跳过 Office 临时锁文件（~$开头，Excel/Word/PPT 编辑时产生）
    if file_name.starts_with("~$") {
        return false;
    }

    // 3. 跳过常见临时/备份文件
    if file_name.ends_with('~')           // Emacs/编辑器备份文件 (file.txt~)
        || file_name.starts_with('~')      // 其他波浪线临时文件
        || file_name == "Thumbs.db"        // Windows 缩略图缓存
        || file_name == "desktop.ini"
    // Windows 文件夹配置
    {
        return false;
    }

    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy();

        // 4. 跳过临时/交换/备份文件扩展名
        if matches!(
            ext_str.as_ref(),
            "tmp" | "temp" | "bak" | "swp" | "swo" | "swn"  // 通用临时 + Vim 交换
            | "lock" | "lck"                                   // 锁文件
            | "part" | "crdownload" | "download" // 下载中的文件
        ) {
            return false;
        }

        // 5. 检查是否为可索引格式
        matches!(
            ext_str.as_ref(),
            "md" | "txt" | "pdf" | "docx" | "pptx" | "xlsx" | "xls" | "xlsm" | "xlsb" | "html"
        )
    } else {
        false
    }
}
