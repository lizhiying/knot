use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone)]
pub struct ModelPathManager {
    app_data_dir: PathBuf,
    resource_dir: PathBuf,
}

impl ModelPathManager {
    pub fn new(app: &tauri::AppHandle) -> Self {
        // User requested: ~/.knot/models
        let home_dir = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| app.path().app_data_dir().unwrap_or(PathBuf::from(".")));

        let app_data_dir = home_dir.join(".knot").join("models");

        // Ensure directory exists immediately
        if !app_data_dir.exists() {
            let _ = std::fs::create_dir_all(&app_data_dir);
        }

        // Using correct resource resolution logic similar to main.rs
        let resource_dir = app.path().resource_dir().unwrap_or(PathBuf::from("."));

        Self {
            app_data_dir,
            resource_dir: resource_dir.join("models"),
        }
    }

    /// 获取模型的最终路径
    /// 优先级：AppData (下载版) -> Resource (内嵌版)
    pub fn get_model_path(&self, filename: &str) -> PathBuf {
        let external_path = self.app_data_dir.join(filename);
        if external_path.exists() {
            println!("[ModelManager] Using external model: {:?}", external_path);
            return external_path;
        }

        let internal_path = self.resolve_internal_resource(filename);
        println!(
            "[ModelManager] Using internal model (fallback): {:?}",
            internal_path
        );

        // 如果内部也没找到，仍然返回内部路径（让调用者去处理 NotFound）
        // 或者返回一个 Result? 目前保持简单，返回 PathBuf
        internal_path
    }

    /// 获取用于下载的目标路径
    pub fn get_download_target_path(&self, filename: &str) -> PathBuf {
        // 确保目录存在
        if !self.app_data_dir.exists() {
            let _ = std::fs::create_dir_all(&self.app_data_dir);
        }
        self.app_data_dir.join(filename)
    }

    fn resolve_internal_resource(&self, filename: &str) -> PathBuf {
        // 复制 main.rs 中的 resolve_resource 逻辑的简化版
        // 因为 main.rs 在 fallback 处理上比较复杂，这里简化处理
        // 假设在单一 binary 环境下，resource_dir 是可靠的
        // 如果是开发环境，可能需要回退到 Cargo Manifest

        let p = self.resource_dir.join(filename);
        if p.exists() {
            return p;
        }

        // Dev fallback logic similar to main.rs could be added here if needed
        // For now, assume deployed app or correct dev setup
        p
    }
}
