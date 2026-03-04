//! 输出写入工具

use knot_pdf::PdfError;
use std::io::Write;

/// 写入输出（到文件或 stdout）
pub fn write_output(data: &[u8], output_path: Option<&std::path::Path>) -> Result<(), PdfError> {
    if let Some(path) = output_path {
        // 确保父目录存在
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, data)?;
    } else {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        handle.write_all(data)?;
        handle.flush()?;
    }
    Ok(())
}

/// 检查输入文件是否存在
pub fn check_input_exists(path: &std::path::Path) -> Result<(), PdfError> {
    if !path.exists() {
        return Err(PdfError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("文件不存在: {}", path.display()),
        )));
    }
    Ok(())
}
