//! M11 表格结构模型集成测试

#[cfg(feature = "table_model")]
mod table_model_tests {
    use knot_pdf::ir::BBox;
    use knot_pdf::table::onnx_structure::{OnnxTableStructureDetector, TableModelArch};
    use knot_pdf::table::structure_detect::*;

    #[test]
    fn test_onnx_table_model_load_and_infer() {
        let model_path = std::path::Path::new("models/table_structure.onnx");
        if !model_path.exists() {
            eprintln!("Skipping: models/table_structure.onnx not found");
            return;
        }

        // 加载模型（YOLO 架构，输入 640x640）
        let detector =
            OnnxTableStructureDetector::from_file(model_path, 640, 640, TableModelArch::Yolo, 0.5);
        assert!(detector.is_ok(), "模型应能成功加载: {:?}", detector.err());
        let detector = detector.unwrap();
        assert_eq!(detector.name(), "onnx-table-structure");

        // 创建一个虚拟的 PNG 图片（1x1 白色像素）
        let mut png_data = Vec::new();
        {
            use std::io::Cursor;
            let img = image::RgbImage::from_pixel(100, 100, image::Rgb([255u8, 255, 255]));
            let mut cursor = Cursor::new(&mut png_data);
            img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
        }

        let table_bbox = BBox::new(50.0, 100.0, 400.0, 200.0);
        let start = std::time::Instant::now();
        let result = detector.detect(&png_data, &table_bbox);
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "推理应成功: {:?}", result.err());
        let result = result.unwrap();

        println!("推理耗时: {:?}", elapsed);
        println!("检测到 {} 个元素", result.elements.len());
        println!("行分割线: {}", result.row_separators.len());
        println!("列分割线: {}", result.col_separators.len());

        // 空白图像，不应检测到太多元素（取决于置信度阈值）
        // 主要验证不崩溃
        assert!(elapsed.as_millis() < 5000, "推理应在 5 秒内完成");
    }

    #[test]
    fn test_merge_grid_lines_integration() {
        // 模拟规则方法和模型方法的输出
        let rule_lines = vec![
            TableGridLine {
                direction: GridLineDirection::Horizontal,
                position: 100.0,
                start: 50.0,
                end: 450.0,
                confidence: 0.7,
            },
            TableGridLine {
                direction: GridLineDirection::Horizontal,
                position: 150.0,
                start: 50.0,
                end: 450.0,
                confidence: 0.6,
            },
        ];

        let model_lines = vec![
            TableGridLine {
                direction: GridLineDirection::Horizontal,
                position: 101.0, // 接近规则线 100
                start: 50.0,
                end: 450.0,
                confidence: 0.9, // 更高置信度
            },
            TableGridLine {
                direction: GridLineDirection::Horizontal,
                position: 200.0, // 新发现的分割线
                start: 50.0,
                end: 450.0,
                confidence: 0.85,
            },
            TableGridLine {
                direction: GridLineDirection::Vertical,
                position: 200.0, // 列分割线
                start: 100.0,
                end: 300.0,
                confidence: 0.8,
            },
        ];

        let merged = merge_grid_lines(&rule_lines, &model_lines, 5.0, 0.5);

        // 应有: 100→被101替换, 150保留, 200新增, 垂直200新增 = 4条
        assert_eq!(merged.len(), 4);

        // 检查去重后保留了模型的高置信度线
        let h_lines: Vec<_> = merged
            .iter()
            .filter(|l| l.direction == GridLineDirection::Horizontal)
            .collect();
        assert_eq!(h_lines.len(), 3);

        // 第一条线应被模型线替换(101, 0.9)
        assert!((h_lines[0].position - 101.0).abs() < 0.1);
        assert!((h_lines[0].confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_mock_detector_as_fallback() {
        let detector = MockTableStructureDetector;
        let result = detector
            .detect(&[], &BBox::new(0.0, 0.0, 100.0, 100.0))
            .unwrap();

        assert!(result.elements.is_empty());
        assert!(result.row_separators.is_empty());
        assert!(result.col_separators.is_empty());
        assert!(result.header_bbox.is_none());
        assert!(result.spanning_cells.is_empty());
        assert_eq!(result.num_rows(), 0);
        assert_eq!(result.num_cols(), 0);
    }
}
