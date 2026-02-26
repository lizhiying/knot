//! M13：后处理 Pipeline 框架
//!
//! 可插拔的后处理管线，用于对已解析的 PageIR 进行噪声过滤和质量提升。
//! 处理器按注册顺序依次执行。

pub mod footnote;
pub mod list;
pub mod paragraph;
pub mod url;
pub mod watermark;

use crate::config::Config;
use crate::ir::PageIR;

/// 后处理器 trait
pub trait PostProcessor: Send + Sync {
    /// 处理器名称
    fn name(&self) -> &str;

    /// 对单个页面 IR 进行后处理（原地修改）
    fn process_page(&self, page: &mut PageIR, config: &Config);
}

/// 后处理管线
pub struct PostProcessPipeline {
    processors: Vec<Box<dyn PostProcessor>>,
}

impl PostProcessPipeline {
    /// 创建空管线
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    /// 创建默认管线（包含所有内置处理器，按推荐顺序）
    pub fn default_pipeline() -> Self {
        let mut pipeline = Self::new();
        // 1. 水印检测与过滤（最先执行，避免影响其他检测）
        pipeline.add(Box::new(watermark::WatermarkFilter::new()));
        // 2. 脚注检测与分离
        pipeline.add(Box::new(footnote::FootnoteDetector::new()));
        // 3. 列表识别增强
        pipeline.add(Box::new(list::ListDetector::new()));
        // 4. URL 碎片修复
        pipeline.add(Box::new(url::UrlFixer::new()));
        pipeline
    }

    /// 添加处理器
    pub fn add(&mut self, processor: Box<dyn PostProcessor>) {
        self.processors.push(processor);
    }

    /// 对单个页面执行所有后处理器
    pub fn process_page(&self, page: &mut PageIR, config: &Config) {
        for processor in &self.processors {
            processor.process_page(page, config);
        }
    }

    /// 获取处理器列表（调试用）
    pub fn processor_names(&self) -> Vec<&str> {
        self.processors.iter().map(|p| p.name()).collect()
    }
}

impl Default for PostProcessPipeline {
    fn default() -> Self {
        Self::default_pipeline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pipeline_has_processors() {
        let pipeline = PostProcessPipeline::default_pipeline();
        let names = pipeline.processor_names();
        assert!(names.contains(&"watermark_filter"));
        assert!(names.contains(&"footnote_detector"));
        assert!(names.contains(&"list_detector"));
        assert!(names.contains(&"url_fixer"));
    }

    #[test]
    fn test_empty_pipeline() {
        let pipeline = PostProcessPipeline::new();
        assert_eq!(pipeline.processor_names().len(), 0);
    }

    #[test]
    fn test_custom_pipeline() {
        let mut pipeline = PostProcessPipeline::new();
        pipeline.add(Box::new(url::UrlFixer::new()));
        assert_eq!(pipeline.processor_names(), vec!["url_fixer"]);
    }
}
