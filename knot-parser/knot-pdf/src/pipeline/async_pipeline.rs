//! 异步 Pipeline：基于 tokio 的异步解析 API
//!
//! 提供两种异步接口：
//! - `parse_pdf_async`：异步获取完整 DocumentIR
//! - `parse_pdf_stream`：通过有界 channel 逐页推送 PageIR

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{mpsc, Semaphore};

use crate::config::Config;
use crate::error::PdfError;
use crate::ir::{DocumentIR, PageIR};
use crate::pipeline::Pipeline;

/// 异步解析结果（通过 channel 逐页推送）
pub struct AsyncParseHandle {
    /// 接收 PageIR 的 channel
    pub receiver: mpsc::Receiver<Result<PageIR, PdfError>>,
    /// 后台任务 handle
    pub handle: tokio::task::JoinHandle<Result<AsyncParseStats, PdfError>>,
}

/// 异步解析统计信息
#[derive(Debug, Clone)]
pub struct AsyncParseStats {
    /// 总页数
    pub total_pages: usize,
    /// 成功页数
    pub success_pages: usize,
    /// 失败页数
    pub failed_pages: usize,
    /// 文档 ID
    pub doc_id: String,
}

/// 异步解析 PDF，返回完整的 DocumentIR
///
/// 在 tokio 的 blocking 线程池中执行解析，不阻塞异步运行时。
///
/// # Example
/// ```ignore
/// use knot_pdf::{Config, pipeline::async_pipeline::parse_pdf_async};
///
/// let doc = parse_pdf_async("example.pdf", Config::default()).await?;
/// println!("Pages: {}", doc.pages.len());
/// ```
pub async fn parse_pdf_async<P: AsRef<Path> + Send + 'static>(
    path: P,
    config: Config,
) -> Result<DocumentIR, PdfError> {
    let path = path.as_ref().to_path_buf();

    tokio::task::spawn_blocking(move || {
        let pipeline = Pipeline::new(config);
        pipeline.parse(&path)
    })
    .await
    .map_err(|e| PdfError::Backend(format!("Async task failed: {}", e)))?
}

/// 异步流式解析 PDF，通过有界 channel 逐页推送 PageIR
///
/// - `channel_size`：有界 channel 的容量（backpressure 控制）
/// - 返回 `AsyncParseHandle`，包含 receiver 和后台 JoinHandle
///
/// # Example
/// ```ignore
/// use knot_pdf::{Config, pipeline::async_pipeline::parse_pdf_stream};
///
/// let handle = parse_pdf_stream("example.pdf", Config::default(), 4).await?;
/// while let Some(result) = handle.receiver.recv().await {
///     match result {
///         Ok(page) => println!("Page {}", page.page_index),
///         Err(e) => eprintln!("Error: {}", e),
///     }
/// }
/// let stats = handle.handle.await??;
/// println!("Done: {} pages", stats.total_pages);
/// ```
pub async fn parse_pdf_stream<P: AsRef<Path> + Send + 'static>(
    path: P,
    config: Config,
    channel_size: usize,
) -> Result<AsyncParseHandle, PdfError> {
    let path = path.as_ref().to_path_buf();
    let channel_size = channel_size.max(1);

    // 验证文件存在
    if !path.exists() {
        return Err(PdfError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {}", path.display()),
        )));
    }

    let (tx, rx) = mpsc::channel(channel_size);

    let handle = tokio::task::spawn(async move { stream_parse_inner(path, config, tx).await });

    Ok(AsyncParseHandle {
        receiver: rx,
        handle,
    })
}

/// 内部流式解析实现
async fn stream_parse_inner(
    path: PathBuf,
    config: Config,
    tx: mpsc::Sender<Result<PageIR, PdfError>>,
) -> Result<AsyncParseStats, PdfError> {
    // 在 blocking 线程池中完成解析（因为 PDF 后端不是 Send-safe 的异步操作）
    let doc = tokio::task::spawn_blocking(move || {
        let pipeline = Pipeline::new(config);
        pipeline.parse(&path)
    })
    .await
    .map_err(|e| PdfError::Backend(format!("Blocking task panicked: {}", e)))??;

    let total_pages = doc.pages.len();
    let doc_id = doc.doc_id.clone();
    let mut success_pages = 0;
    let failed_pages = 0;

    // 逐页通过 channel 推送
    for page in doc.pages {
        success_pages += 1;
        if tx.send(Ok(page)).await.is_err() {
            // receiver 已关闭，停止发送
            log::warn!(
                "Stream receiver dropped, stopping at page {}",
                success_pages
            );
            break;
        }
    }

    Ok(AsyncParseStats {
        total_pages,
        success_pages,
        failed_pages,
        doc_id,
    })
}

/// 带回调的异步解析：解析完成后对每页调用 handler
///
/// handler 在异步上下文中调用，可以执行异步操作。
/// 使用 Semaphore 控制并发回调数量。
///
/// # Example
/// ```ignore
/// use knot_pdf::{Config, pipeline::async_pipeline::parse_pdf_with_handler};
///
/// parse_pdf_with_handler("example.pdf", Config::default(), |page| {
///     println!("Page {}: {} blocks", page.page_index, page.blocks.len());
/// }).await?;
/// ```
pub async fn parse_pdf_with_handler<P, F>(
    path: P,
    config: Config,
    handler: F,
) -> Result<AsyncParseStats, PdfError>
where
    P: AsRef<Path> + Send + 'static,
    F: Fn(PageIR) + Send + Sync + 'static,
{
    let max_concurrent = config.ocr_workers.max(1);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let handler = Arc::new(handler);
    let channel_size = config.page_queue_size.max(1);

    let mut handle = parse_pdf_stream(path, config, channel_size).await?;
    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = handle.receiver.recv().await {
        match result {
            Ok(page) => {
                let _permit = semaphore
                    .acquire()
                    .await
                    .map_err(|e| PdfError::Backend(format!("Semaphore closed: {}", e)))?;
                handler(page);
                success += 1;
            }
            Err(e) => {
                log::warn!("Page processing error: {}", e);
                failed += 1;
            }
        }
    }

    let stats = handle
        .handle
        .await
        .map_err(|e| PdfError::Backend(format!("Stream task failed: {}", e)))??;

    Ok(AsyncParseStats {
        total_pages: stats.total_pages,
        success_pages: success,
        failed_pages: failed,
        doc_id: stats.doc_id,
    })
}
