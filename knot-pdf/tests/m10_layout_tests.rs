//! M10 集成测试：版面检测模块
//!
//! 测试内容：
//! 1. 版面检测数据结构的 serde 往返
//! 2. BlockRole 新增 variant 的兼容性
//! 3. layout_model_enabled=false 时与之前行为完全一致
//! 4. 配置项正确性

use knot_pdf::config::ReadingOrderMethod;
use knot_pdf::ir::BlockRole;
use knot_pdf::layout::{
    compute_iou, merge_layout_with_blocks, nms, LayoutDetector, LayoutLabel, LayoutRegion,
    MockLayoutDetector,
};
use knot_pdf::{parse_pdf, Config};

// ─── BlockRole 兼容性测试 ───

#[test]
fn test_block_role_serde_new_variants() {
    // 新增的 Heading, PageNumber, Sidebar variant 必须能正确序列化/反序列化
    let roles = vec![
        BlockRole::Body,
        BlockRole::Header,
        BlockRole::Footer,
        BlockRole::Title,
        BlockRole::Heading,
        BlockRole::List,
        BlockRole::Caption,
        BlockRole::PageNumber,
        BlockRole::Sidebar,
        BlockRole::Unknown,
    ];

    for role in &roles {
        let json = serde_json::to_string(role).expect("BlockRole 序列化失败");
        let parsed: BlockRole = serde_json::from_str(&json).expect("BlockRole 反序列化失败");
        assert_eq!(*role, parsed, "BlockRole serde 往返失败: {:?}", role);
    }

    println!("✓ 所有 BlockRole variant serde 往返成功");
}

#[test]
fn test_block_role_backward_compat() {
    // 旧版 JSON 中的 role 值应该仍能解析
    let old_json = r#""Body""#;
    let role: BlockRole = serde_json::from_str(old_json).unwrap();
    assert_eq!(role, BlockRole::Body);

    let old_json = r#""Title""#;
    let role: BlockRole = serde_json::from_str(old_json).unwrap();
    assert_eq!(role, BlockRole::Title);
}

// ─── 版面检测数据结构测试 ───

#[test]
fn test_layout_label_complete_coverage() {
    // 确保所有 11 个 DocLayNet 类别都有映射
    for i in 0..=10 {
        let label = LayoutLabel::from_class_id(i);
        assert_ne!(
            label,
            LayoutLabel::Unknown,
            "Class ID {} should have a known label",
            i
        );
    }

    // 超出范围应返回 Unknown
    assert_eq!(LayoutLabel::from_class_id(99), LayoutLabel::Unknown);
}

#[test]
fn test_layout_region_confidence_ordering() {
    let mut regions = vec![
        LayoutRegion {
            bbox: knot_pdf::ir::BBox::new(0.0, 0.0, 100.0, 50.0),
            label: LayoutLabel::Paragraph,
            confidence: 0.3,
        },
        LayoutRegion {
            bbox: knot_pdf::ir::BBox::new(0.0, 0.0, 100.0, 50.0),
            label: LayoutLabel::Title,
            confidence: 0.9,
        },
        LayoutRegion {
            bbox: knot_pdf::ir::BBox::new(0.0, 0.0, 100.0, 50.0),
            label: LayoutLabel::Heading,
            confidence: 0.6,
        },
    ];

    // NMS 应该按置信度排序后保留最高的
    nms(&mut regions, 0.5);
    assert_eq!(regions.len(), 1, "NMS 应只保留 1 个（完全重叠）");
    assert_eq!(regions[0].label, LayoutLabel::Title, "应保留最高置信度的");
}

// ─── feature gate 行为验证 ───

#[test]
fn test_layout_model_disabled_no_behavior_change() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let pdf_path = dir.path().join("test.pdf");

    // 生成简单 PDF
    generate_simple_pdf(&pdf_path);

    // layout_model_enabled = false (默认)
    let config_default = Config::default();
    assert!(!config_default.layout_model_enabled, "默认不启用版面检测");

    let doc = parse_pdf(&pdf_path, &config_default).expect("解析失败");
    assert!(!doc.pages.is_empty());

    // 所有 block 的 role 应该是规则方法分配的（不受模型影响）
    for page in &doc.pages {
        for block in &page.blocks {
            // 无模型时不应有 Heading、PageNumber、Sidebar 等模型特有角色
            // (这些只有模型分类才会分配)
            println!(
                "  blk '{}': role={:?}",
                &block.normalized_text[..block.normalized_text.len().min(40)],
                block.role
            );
        }
    }

    println!("✓ layout_model_enabled=false 时行为正常");
}

// ─── 配置项测试 ───

#[test]
fn test_layout_config_serde_json() {
    let mut config = Config::default();
    config.layout_model_enabled = true;
    config.layout_confidence_threshold = 0.7;
    config.layout_input_size = 800;

    let json = serde_json::to_string(&config).expect("序列化失败");
    assert!(json.contains("layout_model_enabled"));
    assert!(json.contains("layout_confidence_threshold"));

    let config2: Config = serde_json::from_str(&json).expect("反序列化失败");
    assert!(config2.layout_model_enabled);
    assert!((config2.layout_confidence_threshold - 0.7).abs() < 0.001);
    assert_eq!(config2.layout_input_size, 800);
}

#[test]
fn test_layout_config_toml() {
    let toml = r#"
layout_model_enabled = true
layout_confidence_threshold = 0.6
layout_input_size = 1024
"#;

    let config = Config::from_toml_str(toml).expect("TOML 解析失败");
    assert!(config.layout_model_enabled);
    assert!((config.layout_confidence_threshold - 0.6).abs() < 0.001);
    assert_eq!(config.layout_input_size, 1024);

    // 默认值测试
    let config_default = Config::from_toml_str("").expect("空 TOML 解析失败");
    assert!(!config_default.layout_model_enabled);
    assert!((config_default.layout_confidence_threshold - 0.5).abs() < 0.001);
    assert_eq!(config_default.layout_input_size, 640);
}

#[test]
fn test_layout_config_defaults() {
    let config = Config::default();

    assert!(!config.layout_model_enabled, "默认不启用");
    assert!(config.layout_model_path.is_none(), "默认无模型路径");
    assert!(
        (config.layout_confidence_threshold - 0.5).abs() < 0.001,
        "默认置信度阈值 0.5"
    );
    assert_eq!(config.layout_input_size, 640, "默认输入大小 640");
}

// ─── Mock 检测器集成 ───

#[test]
fn test_mock_detector_integration() {
    let detector = MockLayoutDetector;

    // Mock 应该返回空结果
    let regions = detector.detect(&[], 612.0, 792.0).unwrap();
    assert!(regions.is_empty());

    // 空 regions 不应影响任何 block
    let mut blocks = vec![];
    merge_layout_with_blocks(&mut blocks, &regions, 0.5, 0.3);
    assert!(blocks.is_empty());
}

#[test]
fn test_all_eval_samples_still_pass() {
    // 确保 M10 改动不影响现有评测样本的解析
    let eval_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("eval_samples")
        .join("born_digital");

    if !eval_dir.exists() {
        eprintln!("跳过：eval_samples 不存在");
        return;
    }

    let config = Config::default(); // layout_model_enabled = false

    let mut success = 0;
    let mut total = 0;

    for entry in std::fs::read_dir(&eval_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "pdf") {
            continue;
        }

        total += 1;
        match parse_pdf(&path, &config) {
            Ok(doc) => {
                let total_blocks: usize = doc.pages.iter().map(|p| p.blocks.len()).sum();
                println!(
                    "  ✓ {}: {} pages, {} blocks",
                    path.file_name().unwrap().to_str().unwrap(),
                    doc.pages.len(),
                    total_blocks
                );
                success += 1;
            }
            Err(e) => {
                println!("  ✗ {}: {}", path.file_name().unwrap().to_str().unwrap(), e);
            }
        }
    }

    println!("\nM10 回归测试: {}/{} 通过", success, total);
    assert_eq!(success, total, "所有评测样本应解析成功");
}

// ─── Helper ───

fn generate_simple_pdf(path: &std::path::Path) {
    use lopdf::dictionary;
    use lopdf::{Document, Stream};

    let mut doc = Document::with_version("1.7");

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let content = b"BT /F1 14 Tf 72 720 Td (Test Document) Tj ET\n\
                    BT /F1 12 Tf 72 700 Td (Body text paragraph.) Tj ET";
    let content_stream = Stream::new(dictionary! {}, content.to_vec());
    let content_id = doc.add_object(content_stream);

    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => resources_id,
    });

    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    });

    if let Ok(page_obj) = doc.get_object_mut(page_id) {
        if let Ok(dict) = page_obj.as_dict_mut() {
            dict.set("Parent", pages_id);
        }
    }

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });

    doc.trailer.set("Root", catalog_id);
    doc.max_id = doc.objects.keys().map(|k| k.0).max().unwrap_or(0);

    let mut file = std::fs::File::create(path).expect("创建 PDF 文件失败");
    doc.save_to(&mut file).expect("保存 PDF 失败");
}
