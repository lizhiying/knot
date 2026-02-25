//! SledStore 实现

use crate::error::PdfError;
use crate::ir::{PageDiagnostics, PageIR};
use crate::store::traits::{PageStatus, Store};
use sled::Db;
use std::path::Path;

/// 基于 Sled 的存储后端
pub struct SledStore {
    db: Db,
}

impl SledStore {
    /// 打开或创建 Sled 数据库
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PdfError> {
        let db = sled::open(path).map_err(|e| PdfError::Store(e.to_string()))?;
        Ok(Self { db })
    }

    fn page_key(doc_id: &str, page_index: usize) -> String {
        format!("{}:{}:ir", doc_id, page_index)
    }

    fn status_key(doc_id: &str, page_index: usize) -> String {
        format!("{}:{}:status", doc_id, page_index)
    }

    fn diag_key(doc_id: &str, page_index: usize) -> String {
        format!("{}:{}:diag", doc_id, page_index)
    }

    fn last_page_key(doc_id: &str) -> String {
        format!("{}:last_page", doc_id)
    }
}

impl Store for SledStore {
    fn save_page(&self, doc_id: &str, page_index: usize, page_ir: &PageIR) -> Result<(), PdfError> {
        let key = Self::page_key(doc_id, page_index);
        let data = serde_json::to_vec(page_ir).map_err(|e| PdfError::Store(e.to_string()))?;
        self.db
            .insert(key, data)
            .map_err(|e| PdfError::Store(e.to_string()))?;

        // 同时更新最后完成页
        let last_key = Self::last_page_key(doc_id);
        self.db
            .insert(last_key, page_index.to_string().as_bytes())
            .map_err(|e| PdfError::Store(e.to_string()))?;

        Ok(())
    }

    fn load_page(&self, doc_id: &str, page_index: usize) -> Result<Option<PageIR>, PdfError> {
        let key = Self::page_key(doc_id, page_index);
        let data = self
            .db
            .get(key)
            .map_err(|e| PdfError::Store(e.to_string()))?;
        match data {
            Some(vec) => {
                let ir =
                    serde_json::from_slice(&vec).map_err(|e| PdfError::Store(e.to_string()))?;
                Ok(Some(ir))
            }
            None => Ok(None),
        }
    }

    fn get_status(&self, doc_id: &str, page_index: usize) -> Result<PageStatus, PdfError> {
        let key = Self::status_key(doc_id, page_index);
        let data = self
            .db
            .get(key)
            .map_err(|e| PdfError::Store(e.to_string()))?;
        match data {
            Some(vec) => serde_json::from_slice(&vec).map_err(|e| PdfError::Store(e.to_string())),
            None => Ok(PageStatus::NotStarted),
        }
    }

    fn update_status(
        &self,
        doc_id: &str,
        page_index: usize,
        status: PageStatus,
    ) -> Result<(), PdfError> {
        let key = Self::status_key(doc_id, page_index);
        let data = serde_json::to_vec(&status).map_err(|e| PdfError::Store(e.to_string()))?;
        self.db
            .insert(key, data)
            .map_err(|e| PdfError::Store(e.to_string()))?;
        Ok(())
    }

    fn save_diagnostics(
        &self,
        doc_id: &str,
        page_index: usize,
        diagnostics: &PageDiagnostics,
    ) -> Result<(), PdfError> {
        let key = Self::diag_key(doc_id, page_index);
        let data = serde_json::to_vec(diagnostics).map_err(|e| PdfError::Store(e.to_string()))?;
        self.db
            .insert(key, data)
            .map_err(|e| PdfError::Store(e.to_string()))?;
        Ok(())
    }

    fn get_last_completed_page(&self, doc_id: &str) -> Result<Option<usize>, PdfError> {
        let key = Self::last_page_key(doc_id);
        let data = self
            .db
            .get(key)
            .map_err(|e| PdfError::Store(e.to_string()))?;
        match data {
            Some(vec) => {
                let s = String::from_utf8_lossy(&vec);
                let idx = s
                    .parse::<usize>()
                    .map_err(|e| PdfError::Store(e.to_string()))?;
                Ok(Some(idx))
            }
            None => Ok(None),
        }
    }
}
