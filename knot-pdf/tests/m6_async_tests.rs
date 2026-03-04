#![cfg(feature = "async")]
//! M6 异步 API 测试
//!
//! 测试 `parse_pdf_async`、`parse_pdf_stream`、`parse_pdf_with_handler` 三种异步接口
//!
//! 需要 `async` feature 启用：`cargo test --features async`

use knot_pdf::Config;

// === parse_pdf_async ===

#[tokio::test]
async fn test_parse_pdf_async_nonexistent() {
    let result = knot_pdf::parse_pdf_async("/nonexistent/fake.pdf", Config::default()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_parse_pdf_async_success() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );
    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("Skip: {} not found", pdf_path);
        return;
    }

    let config = Config::default();
    let start = std::time::Instant::now();
    let doc = knot_pdf::parse_pdf_async(pdf_path, config).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(doc.pages.len(), 100);
    assert!(!doc.doc_id.is_empty());
    println!(
        "parse_pdf_async: 100 pages in {:.2}s",
        elapsed.as_secs_f64()
    );
}

// === parse_pdf_stream ===

#[tokio::test]
async fn test_parse_pdf_stream_nonexistent() {
    let result = knot_pdf::parse_pdf_stream("/nonexistent/fake.pdf", Config::default(), 4).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_parse_pdf_stream_success() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );
    if !std::path::Path::new(pdf_path).exists() {
        return;
    }

    let config = Config::default();
    let mut handle = knot_pdf::parse_pdf_stream(pdf_path, config, 4)
        .await
        .unwrap();

    let mut page_count = 0;
    let mut errors = 0;

    while let Some(result) = handle.receiver.recv().await {
        match result {
            Ok(page) => {
                assert!(!page.blocks.is_empty() || page.tables.is_empty());
                page_count += 1;
            }
            Err(_) => errors += 1,
        }
    }

    let stats = handle.handle.await.unwrap().unwrap();
    assert_eq!(page_count, 100);
    assert_eq!(errors, 0);
    assert_eq!(stats.total_pages, 100);
    println!(
        "parse_pdf_stream: received {} pages, stats: {:?}",
        page_count, stats
    );
}

#[tokio::test]
async fn test_parse_pdf_stream_backpressure() {
    // 使用极小 channel (size=1) 验证 backpressure 不死锁
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );
    if !std::path::Path::new(pdf_path).exists() {
        return;
    }

    let config = Config::default();
    let mut handle = knot_pdf::parse_pdf_stream(pdf_path, config, 1)
        .await
        .unwrap();

    let mut count = 0;
    while let Some(Ok(_page)) = handle.receiver.recv().await {
        count += 1;
        // 模拟慢消费者
        if count % 20 == 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }
    assert_eq!(count, 100);
    let stats = handle.handle.await.unwrap().unwrap();
    assert_eq!(stats.total_pages, 100);
}

// === parse_pdf_with_handler ===

#[tokio::test]
async fn test_parse_pdf_with_handler() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );
    if !std::path::Path::new(pdf_path).exists() {
        return;
    }

    let config = Config::default();
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let stats = knot_pdf::parse_pdf_with_handler(pdf_path, config, move |page| {
        counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(page.page_index < 100);
    })
    .await
    .unwrap();

    let processed = counter.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(processed, 100);
    assert_eq!(stats.total_pages, 100);
    assert_eq!(stats.success_pages, 100);
    println!("parse_pdf_with_handler: {:?}", stats);
}

// === 行为一致性 ===

#[tokio::test]
async fn test_async_sync_consistency() {
    let pdf_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bench_100pages.pdf"
    );
    if !std::path::Path::new(pdf_path).exists() {
        return;
    }

    let config = Config::default();

    // 同步解析
    let sync_doc = knot_pdf::parse_pdf(pdf_path, &config).unwrap();

    // 异步解析
    let async_doc = knot_pdf::parse_pdf_async(pdf_path, config.clone())
        .await
        .unwrap();

    // 验证一致性
    assert_eq!(sync_doc.pages.len(), async_doc.pages.len());
    assert_eq!(sync_doc.doc_id, async_doc.doc_id);

    // 比较每页的 block 数量
    for (i, (s, a)) in sync_doc
        .pages
        .iter()
        .zip(async_doc.pages.iter())
        .enumerate()
    {
        assert_eq!(
            s.blocks.len(),
            a.blocks.len(),
            "Page {} block count mismatch",
            i
        );
        assert_eq!(
            s.tables.len(),
            a.tables.len(),
            "Page {} table count mismatch",
            i
        );
    }

    println!(
        "Sync/Async consistency verified for {} pages",
        sync_doc.pages.len()
    );
}

// === 无 fixture 的轻量测试 ===

#[tokio::test]
async fn test_async_api_exports() {
    // 验证 AsyncParseStats 可以 Debug
    let stats = knot_pdf::AsyncParseStats {
        total_pages: 10,
        success_pages: 10,
        failed_pages: 0,
        doc_id: "test".to_string(),
    };
    let debug_str = format!("{:?}", stats);
    assert!(debug_str.contains("total_pages"));
    assert!(debug_str.contains("10"));
}
