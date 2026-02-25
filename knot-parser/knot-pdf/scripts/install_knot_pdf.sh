# vim /Users/lizhiying/.config/knot-pdf/knot-pdf.toml
cargo install --path . --features "cli pdfium ocr_paddle vision" --bin knot-pdf-cli --force 2>&1 | tail -5