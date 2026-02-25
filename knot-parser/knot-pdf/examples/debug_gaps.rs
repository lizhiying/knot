use knot_pdf::backend::{PdfBackend, RawChar};
use std::path::Path;

fn main() {
    // 初始化 pdfium 后端
    #[cfg(feature = "pdfium")]
    {
        let mut backend = knot_pdf::backend::PdfiumBackend::new().unwrap();
        let _ = backend
            .open(Path::new("tests/fixtures/Attention_Is_All_You_Need.pdf"))
            .unwrap();

        let chars = backend.extract_chars(0).unwrap();

        // 按 y 分组成行
        let mut lines: Vec<Vec<&RawChar>> = Vec::new();
        for ch in &chars {
            let ch_y = ch.bbox.y + ch.bbox.height / 2.0;
            let mut found = false;
            for line in lines.iter_mut() {
                let line_y = line[0].bbox.y + line[0].bbox.height / 2.0;
                if (line_y - ch_y).abs() < 3.0 {
                    line.push(ch);
                    found = true;
                    break;
                }
            }
            if !found {
                lines.push(vec![ch]);
            }
        }

        // 排序并输出前 10 行的间距信息
        lines.sort_by(|a, b| a[0].bbox.y.partial_cmp(&b[0].bbox.y).unwrap());

        for (li, line) in lines.iter_mut().enumerate().take(15) {
            line.sort_by(|a, b| a.bbox.x.partial_cmp(&b.bbox.x).unwrap());
            let text: String = line.iter().map(|c| c.unicode).collect();

            // 只显示超过 3 个字符的行
            if line.len() < 3 {
                println!(
                    "--- Short line {} (y≈{:.1}): {:?} [SKIP - likely sidebar]",
                    li, line[0].bbox.y, text
                );
                continue;
            }

            println!(
                "=== Line {} (y≈{:.1}, {} chars) ===",
                li,
                line[0].bbox.y,
                line.len()
            );

            // 统计所有 gap
            let mut gaps: Vec<(char, char, f32, f32)> = Vec::new();
            for i in 1..line.len() {
                let prev = &line[i - 1];
                let curr = &line[i];
                let gap = curr.bbox.x - (prev.bbox.x + prev.bbox.width);
                let ratio = gap / prev.font_size.max(1.0);
                gaps.push((prev.unicode, curr.unicode, gap, ratio));
            }

            // 找到最大 gap（这可能是词间距）
            let word_gaps: Vec<_> = gaps.iter().filter(|(_, _, gap, _)| *gap > 0.5).collect();

            println!("  Text: {}", text);
            println!("  Word-size gaps (>0.5pt):");
            for (p, c, gap, ratio) in &word_gaps {
                println!(
                    "    '{}' → '{}': gap={:.2}pt, ratio={:.3}",
                    p, c, gap, ratio
                );
            }
            println!();
        }
    }

    #[cfg(not(feature = "pdfium"))]
    {
        println!("This example requires the 'pdfium' feature");
    }
}
