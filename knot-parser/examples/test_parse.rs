// 测试脚本：验证 node_id 唯一性
use knot_parser::{IndexDispatcher, PageIndexConfig};
use std::path::Path;

#[tokio::main]
async fn main() {
    let path = Path::new("/Users/lizhiying/Projects/mynotes/test/云南旅游.md");

    let dispatcher = IndexDispatcher::new();
    let config = PageIndexConfig::new();

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
