// 测试脚本：验证 node_id 唯一性
use pageindex_rs::{IndexDispatcher, PageIndexConfig};
use std::path::Path;

#[tokio::main]
async fn main() {
    let path = Path::new("/Users/lizhiying/Projects/mynotes/test/云南旅游.md");

    let dispatcher = IndexDispatcher::new();
    let config = PageIndexConfig {
        vision_provider: None,
        llm_provider: None,
        embedding_provider: None,
        min_token_threshold: 50,
        summary_token_threshold: 200,
        enable_auto_summary: false,
        default_language: "en".to_string(),
        progress_callback: None,
    };

    match dispatcher.index_file(path, &config).await {
        Ok(root) => {
            println!("✅ 解析成功!\n");
            println!("--- node_id 列表 ---");
            println!("Root: {}", root.node_id);
            for (i, child) in root.children.iter().enumerate() {
                println!("Child {}: {} -> {}", i + 1, child.node_id, child.title);
            }
        }
        Err(e) => {
            println!("❌ 解析失败: {}", e);
        }
    }
}
