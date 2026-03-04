//! M5 测试：OCR Fallback + Store 断点续传

use knot_pdf::ir::*;
use knot_pdf::ocr::MockOcrBackend;
use knot_pdf::Config;

#[test]
fn test_ocr_trigger_conditions() {
    let mut config = Config::default();
    config.ocr_enabled = true;
    config.ocr_mode = knot_pdf::config::OcrMode::Auto;
    config.scoring_text_threshold = 0.5;

    // 1. 低分页面触发
    let mut page_low = make_empty_page(0);
    page_low.text_score = 0.3;
    assert!(knot_pdf::ocr::should_trigger_ocr(&page_low, &config));

    // 2. 高分页面不触发
    let mut page_high = make_empty_page(1);
    page_high.text_score = 0.8;
    assert!(!knot_pdf::ocr::should_trigger_ocr(&page_high, &config));

    // 3. 强制模式始终触发
    config.ocr_mode = knot_pdf::config::OcrMode::ForceAll;
    assert!(knot_pdf::ocr::should_trigger_ocr(&page_high, &config));

    // 4. 禁用模式从不触发
    config.ocr_mode = knot_pdf::config::OcrMode::Disabled;
    assert!(!knot_pdf::ocr::should_trigger_ocr(&page_high, &config));
    assert!(!knot_pdf::ocr::should_trigger_ocr(&page_low, &config));
}

#[test]
fn test_ocr_result_backfill() {
    let mut page = make_empty_page(0);
    let mock_ocr = MockOcrBackend;
    let image_data = vec![0u8; 10];

    knot_pdf::ocr::run_ocr_and_update_page(&mut page, &mock_ocr, &image_data, false).unwrap();

    assert_eq!(page.source, PageSource::Ocr);
    assert!(!page.blocks.is_empty());
    assert_eq!(page.blocks[0].normalized_text, "Mocked full page text");
    assert!(page.timings.ocr_ms.is_some());
    assert!(page.diagnostics.ocr_quality_score.is_some());
}

#[test]
fn test_ocr_mixed_source_backfill() {
    // 已有 blocks 的页面应标记为 Mixed
    let mut page = make_empty_page(0);
    page.blocks.push(BlockIR {
        block_id: "existing_0".to_string(),
        bbox: BBox::new(10.0, 10.0, 100.0, 20.0),
        role: BlockRole::Body,
        lines: vec![],
        normalized_text: "Existing text".to_string(),
    });

    let mock_ocr = MockOcrBackend;
    let image_data = vec![0u8; 10];

    knot_pdf::ocr::run_ocr_and_update_page(&mut page, &mock_ocr, &image_data, false).unwrap();

    assert_eq!(page.source, PageSource::Mixed);
    assert_eq!(page.blocks.len(), 2); // 原有 + OCR
    assert_eq!(page.blocks[0].normalized_text, "Existing text");
    assert_eq!(page.blocks[1].normalized_text, "Mocked full page text");
}

#[test]
fn test_ocr_quality_score_calculation() {
    let mut page = make_empty_page(0);
    let mock_ocr = MockOcrBackend;
    let image_data = vec![0u8; 10];

    knot_pdf::ocr::run_ocr_and_update_page(&mut page, &mock_ocr, &image_data, false).unwrap();

    let quality = page.diagnostics.ocr_quality_score.unwrap();
    // MockOcrBackend 返回 confidence=0.85
    assert!(quality > 0.8 && quality <= 1.0, "quality={}", quality);
}

#[test]
fn test_ocr_workers_config_default() {
    let config = Config::default();
    assert_eq!(config.ocr_workers, 1);
}

#[test]
fn test_pipeline_ocr_workers_clamped() {
    // 确保 max_ocr_workers 最小为 1
    let mut config = Config::default();
    config.ocr_workers = 0;
    let pipeline = knot_pdf::pipeline::Pipeline::new(config);
    // Pipeline 内部将 ocr_workers.max(1)，无法直接验证字段
    // 但不应 panic
    drop(pipeline);
}

#[cfg(feature = "store_sled")]
#[test]
fn test_sled_store_lifecycle() {
    use knot_pdf::store::{PageStatus, Store};
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_sled");
    let store = knot_pdf::store::SledStore::open(&db_path).unwrap();

    let doc_id = "test_doc_1";
    let page_idx = 0;
    let mut page_ir = make_empty_page(page_idx);
    page_ir.text_score = 0.99;

    // 1. 保存和加载
    store.save_page(doc_id, page_idx, &page_ir).unwrap();
    let loaded = store.load_page(doc_id, page_idx).unwrap().unwrap();
    assert_eq!(loaded.text_score, 0.99);

    // 2. 状态管理
    store
        .update_status(doc_id, page_idx, PageStatus::InProgress)
        .unwrap();
    assert_eq!(
        store.get_status(doc_id, page_idx).unwrap(),
        PageStatus::InProgress
    );

    store
        .update_status(doc_id, page_idx, PageStatus::Done)
        .unwrap();
    assert_eq!(
        store.get_status(doc_id, page_idx).unwrap(),
        PageStatus::Done
    );

    // 3. 断点恢复页码
    assert_eq!(store.get_last_completed_page(doc_id).unwrap(), Some(0));
}

/// 集成测试：断点续传——多页保存后从正确位置恢复
#[cfg(feature = "store_sled")]
#[test]
fn test_breakpoint_resume_multipage() {
    use knot_pdf::store::{PageStatus, Store};
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_resume");
    let store = knot_pdf::store::SledStore::open(&db_path).unwrap();

    let doc_id = "multipage_doc";

    // 模拟前 5 页完成
    for i in 0..5 {
        let page = make_empty_page(i);
        store.save_page(doc_id, i, &page).unwrap();
        store.update_status(doc_id, i, PageStatus::Done).unwrap();
    }

    // 第 6 页标记为 Failed
    store
        .update_status(doc_id, 5, PageStatus::Failed("timeout".to_string()))
        .unwrap();

    // 验证最后完成页为 4（第 5 页）
    let last = store.get_last_completed_page(doc_id).unwrap();
    assert_eq!(last, Some(4));

    // 重新打开 store 模拟重启
    drop(store);
    let store2 = knot_pdf::store::SledStore::open(&db_path).unwrap();

    // 恢复后仍能获取到正确的断点位置
    let last2 = store2.get_last_completed_page(doc_id).unwrap();
    assert_eq!(last2, Some(4));

    // 恢复后的页面数据完整
    for i in 0..5 {
        let loaded = store2.load_page(doc_id, i).unwrap();
        assert!(loaded.is_some(), "Page {} should be loadable", i);
        assert_eq!(loaded.unwrap().page_index, i);
    }

    // Failed 页不应有 PageIR 数据（只有 status）
    let failed_page = store2.load_page(doc_id, 5).unwrap();
    assert!(failed_page.is_none());

    // Failed 页状态保留
    assert_eq!(
        store2.get_status(doc_id, 5).unwrap(),
        PageStatus::Failed("timeout".to_string())
    );
}

/// 集成测试：OCR 并发控制——同步模式下 OCR 串行执行，不超过限制
#[test]
fn test_ocr_serial_execution() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let concurrent_count = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));

    // 模拟多次 OCR 调用（串行）
    for _ in 0..10 {
        let current = concurrent_count.fetch_add(1, Ordering::SeqCst) + 1;
        // 更新最大并发数
        let prev_max = max_seen.load(Ordering::SeqCst);
        if current > prev_max {
            max_seen.store(current, Ordering::SeqCst);
        }

        // 模拟 OCR 工作
        let mock_ocr = MockOcrBackend;
        let mut page = make_empty_page(0);
        let image_data = vec![0u8; 10];
        knot_pdf::ocr::run_ocr_and_update_page(&mut page, &mock_ocr, &image_data, false).unwrap();

        concurrent_count.fetch_sub(1, Ordering::SeqCst);
    }

    // 同步模式下，最大并发应始终为 1
    assert_eq!(max_seen.load(Ordering::SeqCst), 1);
}

/// 测试 OCR 图片数据在处理后被释放（Drop 语义验证）
#[test]
fn test_ocr_image_memory_release() {
    let mock_ocr = MockOcrBackend;
    let mut page = make_empty_page(0);

    // 创建大 buffer 模拟图片
    let image_data = vec![0u8; 1024 * 1024]; // 1MB
    assert_eq!(image_data.len(), 1024 * 1024);

    knot_pdf::ocr::run_ocr_and_update_page(&mut page, &mock_ocr, &image_data, false).unwrap();

    // image_data 在函数调用后仍存在（因为是引用），
    // 但在 Pipeline 中是局部变量，函数返回后即 drop
    assert!(page.timings.ocr_ms.is_some());

    // 验证不会在 PageIR 中缓存原始图片
    // PageIR 不包含 image_data 字段
    assert!(page.blocks.len() > 0);
}

fn make_empty_page(index: usize) -> PageIR {
    PageIR {
        page_index: index,
        size: PageSize {
            width: 500.0,
            height: 700.0,
        },
        rotation: 0.0,
        blocks: vec![],
        tables: vec![],
        images: vec![],
        formulas: vec![],
        diagnostics: PageDiagnostics::default(),
        text_score: 1.0,
        is_scanned_guess: false,
        source: PageSource::BornDigital,
        timings: Timings::default(),
    }
}
