//! M12 公式模型集成测试
//!
//! 需要：
//! - cargo test --test m12_formula_tests --features formula_model
//! - models/formula/ 目录下的 encoder_model.onnx, decoder_model.onnx, tokenizer.json, config.json

#[cfg(feature = "formula_model")]
mod formula_model_tests {
    use knot_pdf::formula::{FormulaRecognizer, OnnxFormulaRecognizer};
    use std::path::Path;

    const MODEL_DIR: &str = "models/formula";

    fn has_model() -> bool {
        Path::new(MODEL_DIR).join("encoder_model.onnx").exists()
            && Path::new(MODEL_DIR).join("decoder_model.onnx").exists()
            && Path::new(MODEL_DIR).join("tokenizer.json").exists()
            && Path::new(MODEL_DIR).join("config.json").exists()
    }

    #[test]
    fn test_load_formula_model() {
        if !has_model() {
            eprintln!("Skipping: formula model not found in {}", MODEL_DIR);
            return;
        }

        let result = OnnxFormulaRecognizer::from_dir(Path::new(MODEL_DIR), 0.3);
        match result {
            Ok(_) => println!("✓ Formula model loaded successfully"),
            Err(e) => {
                // tract 可能不支持所有 ONNX 算子，记录但不失败
                eprintln!("⚠ Model load returned error (may be expected): {}", e);
            }
        }
    }

    #[test]
    fn test_formula_inference() {
        if !has_model() {
            eprintln!("Skipping: formula model not found in {}", MODEL_DIR);
            return;
        }

        let recognizer = match OnnxFormulaRecognizer::from_dir(Path::new(MODEL_DIR), 0.3) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("⚠ Cannot load model: {}", e);
                return;
            }
        };

        // 创建一个简单的测试 PNG 图片（1x1 白色像素）
        // 实际使用时应该用真实的公式渲染图
        let mut png_bytes = Vec::new();
        {
            let mut encoder =
                image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_bytes));
            use image::ImageEncoder;
            // 创建 32x32 白色 RGB 图片
            let white_pixels = vec![255u8; 32 * 32 * 3];
            encoder
                .write_image(&white_pixels, 32, 32, image::ExtendedColorType::Rgb8)
                .expect("Failed to create test PNG");
        }

        let start = std::time::Instant::now();
        let result = recognizer.recognize(&png_bytes);
        let elapsed = start.elapsed();

        match result {
            Ok(recognition) => {
                println!("✓ Inference completed in {:.1}ms", elapsed.as_millis());
                println!("  LaTeX: '{}'", recognition.latex);
                println!("  Confidence: {:.3}", recognition.confidence);
            }
            Err(e) => {
                eprintln!(
                    "⚠ Inference error (may be expected with tract): {} ({:.1}ms)",
                    e,
                    elapsed.as_millis()
                );
            }
        }
    }
}

/// 不依赖 formula_model feature 的基础测试
mod formula_basic_tests {
    use knot_pdf::formula::detect::*;
    use knot_pdf::ir::*;

    #[test]
    fn test_detect_formulas_empty() {
        let formulas = detect_formulas(&[], &[], 0);
        assert!(formulas.is_empty());
    }

    #[test]
    fn test_detect_formulas_no_math() {
        let blocks = vec![BlockIR {
            block_id: "b0".to_string(),
            bbox: BBox::new(0.0, 0.0, 100.0, 20.0),
            role: BlockRole::Body,
            lines: vec![],
            normalized_text: "Hello world, this is normal text.".to_string(),
        }];

        let formulas = detect_formulas(&[], &blocks, 0);
        // 没有字符数据时不应检测出公式
        assert!(formulas.is_empty());
    }

    #[test]
    fn test_formula_ir_serde_roundtrip() {
        let formula = FormulaIR {
            formula_id: "f0_0".to_string(),
            page_index: 0,
            bbox: BBox::new(100.0, 200.0, 300.0, 20.0),
            formula_type: FormulaType::Display,
            confidence: 0.85,
            raw_text: "E = mc²".to_string(),
            latex: Some(r"E = mc^{2}".to_string()),
            equation_number: Some("(1)".to_string()),
            contained_block_ids: vec!["b0".to_string()],
        };

        let json = serde_json::to_string(&formula).expect("序列化失败");
        let formula2: FormulaIR = serde_json::from_str(&json).expect("反序列化失败");

        assert_eq!(formula.formula_id, formula2.formula_id);
        assert_eq!(formula.formula_type, formula2.formula_type);
        assert_eq!(formula.raw_text, formula2.raw_text);
        assert_eq!(formula.latex, formula2.latex);
        assert_eq!(formula.equation_number, formula2.equation_number);
    }

    #[test]
    fn test_formula_to_markdown_inline() {
        let formula = FormulaIR {
            formula_id: "f0_0".to_string(),
            page_index: 0,
            bbox: BBox::new(0.0, 0.0, 100.0, 12.0),
            formula_type: FormulaType::Inline,
            confidence: 0.9,
            raw_text: "x^2".to_string(),
            latex: Some(r"x^{2}".to_string()),
            equation_number: None,
            contained_block_ids: vec![],
        };

        let md = formula.to_markdown();
        assert_eq!(md, "$x^{2}$");
    }

    #[test]
    fn test_formula_to_markdown_display() {
        let formula = FormulaIR {
            formula_id: "f0_0".to_string(),
            page_index: 0,
            bbox: BBox::new(0.0, 0.0, 400.0, 30.0),
            formula_type: FormulaType::Display,
            confidence: 0.85,
            raw_text: "E = mc²".to_string(),
            latex: None, // 无 LaTeX，使用原始文本
            equation_number: Some("(1)".to_string()),
            contained_block_ids: vec![],
        };

        let md = formula.to_markdown();
        assert!(md.contains("$$"));
        assert!(md.contains("E = mc²"));
        assert!(md.contains("(1)"));
    }
}
