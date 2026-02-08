# Milestone 9: CLI 独立可用性 - 完成总结

## 概述

Milestone 9 的目标是让 `knot-cli` 成为一个独立可用的命令行工具，无需依赖 Tauri 应用即可完成完整的 RAG（检索增强生成）工作流程。

**完成日期**: 2026-02-08

---

## 修改内容

### 1. 核心功能 (Iteration 1)

| 修改项              | 说明                                               |
| :------------------ | :------------------------------------------------- |
| 真实 Embedding 支持 | 替换 MockEmbedding，集成 ThreadSafeEmbeddingEngine |
| `status` 命令       | 显示模型状态、索引状态、路径信息                   |
| `index` 命令        | 索引指定目录，支持自定义 embedding 模型路径        |
| `query` 命令        | 向量 + 关键词混合搜索，支持 --json 输出            |

### 2. LLM 集成 (Iteration 2)

| 修改项          | 说明                                |
| :-------------- | :---------------------------------- |
| `download` 命令 | 从 HuggingFace 下载模型，支持进度条 |
| `ask` 命令      | 完整 RAG：检索 + LLM 生成回答       |
| 流式输出        | 打字机效果实时显示 LLM 回答         |
| `<think>` 过滤  | 自动过滤 LLM 的思考过程标签         |
| JSON 输出       | query/ask 命令支持 --json 格式      |

### 3. 体验优化 (Iteration 3)

| 修改项            | 说明                                       |
| :---------------- | :----------------------------------------- |
| 配置文件          | ~/.knot/config.toml 支持自定义模型路径     |
| 进度条            | 索引过程显示 spinner 和 progress bar       |
| 错误提示          | 用户友好的错误信息，自动提示 download 命令 |
| `index-list` 命令 | 查看所有已索引的源目录                     |
| `index --remove`  | 移除指定目录的索引                         |
| `--verbose` 参数  | 全局参数，显示详细调试日志                 |
| 静默模式          | 默认隐藏调试日志，输出更整洁               |

### 4. LLM 服务常驻优化 (Iteration 4)

| 修改项         | 说明                                    |
| :------------- | :-------------------------------------- |
| 端口更改       | LLM 端口从 8081 改为 28081              |
| `chat` 命令    | REPL 交互模式，一次加载多次提问         |
| `serve` 命令   | 启动 LLM 服务守护进程                   |
| `serve --stop` | 停止运行中的 LLM 服务                   |
| 自动服务复用   | ask 命令自动检测并复用已运行的 LLM 服务 |
| PID 文件管理   | ~/.knot/llm-server.pid 用于进程管理     |

---

## 文件变更

```
knot-cli/
├── Cargo.toml        # 新增依赖: ort, tokenizers, reqwest, indicatif, toml
└── src/main.rs       # 主程序，约 1400 行

knot-core/
├── src/llm.rs        # 新增 spawn(), spawn_quiet(), get_pid(), debug_println! 宏
├── src/store.rs      # 添加 KNOT_QUIET 日志控制
└── src/index.rs      # 添加 KNOT_QUIET 日志控制

.plans/milestone9/
├── cli-independence.md  # 里程碑计划
├── iteration1.md        # 迭代1计划
├── iteration2.md        # 迭代2计划
├── iteration3.md        # 迭代3计划
├── iteration4.md        # 迭代4计划 (LLM 服务优化)
└── summary.md           # 本文件
```

---

## 使用说明

### 前置准备

```bash
# 1. 下载所需模型
cargo run -q -p knot-cli -- download --model all

# 或分别下载
cargo run -q -p knot-cli -- download --model embedding
cargo run -q -p knot-cli -- download --model llm
```

### 基本命令

```bash
# 查看系统状态
cargo run -q -p knot-cli -- status

# 索引目录
cargo run -q -p knot-cli -- index -i /path/to/docs

# 查看已索引目录
cargo run -q -p knot-cli -- index-list

# 移除索引
cargo run -q -p knot-cli -- index -i /path/to/docs --remove

# 搜索
cargo run -q -p knot-cli -- query -t "搜索关键词"
cargo run -q -p knot-cli -- query -t "搜索关键词" --json

# 问答 (RAG)
cargo run -q -p knot-cli -- ask -q "你的问题"
cargo run -q -p knot-cli -- ask -q "你的问题" --json

# 详细日志模式
cargo run -q -p knot-cli -- -v ask -q "问题"
```

### 交互模式 (推荐)

```bash
# 启动交互式聊天 (只加载一次模型)
cargo run -q -p knot-cli -- chat

# 启动 LLM 服务守护进程 (终端 1)
cargo run -q -p knot-cli -- serve

# 使用已运行的服务快速查询 (终端 2)
cargo run -q -p knot-cli -- ask -q "问题"  # 自动复用服务，无需重新加载

# 停止守护进程
cargo run -q -p knot-cli -- serve --stop
```

### 配置文件 (可选)

创建 `~/.knot/config.toml`:

```toml
[models]
embedding = "/custom/path/to/embedding.onnx"
llm = "/custom/path/to/model.gguf"

[paths]
models_dir = "/custom/models"
indexes_dir = "/custom/indexes"
```

---

## 验证步骤

### 1. 验证模型下载

```bash
# 确认模型文件存在
ls -la ~/.knot/models/
# 应该看到:
# - bge-small-zh-v1.5.onnx
# - bge-small-zh-v1.5-tokenizer.json
# - Qwen3-1.7B-Q4_K_M.gguf
```

### 2. 验证索引功能

```bash
# 索引测试目录
cargo run -q -p knot-cli -- index -i ./docs/milestones

# 验证索引列表
cargo run -q -p knot-cli -- index-list
# 应该显示刚才索引的目录
```

### 3. 验证搜索功能

```bash
# 测试向量+关键词搜索
cargo run -q -p knot-cli -- query -t "性能优化"
# 应该返回相关结果

# 测试 JSON 输出
cargo run -q -p knot-cli -- query -t "性能优化" --json | head
# 应该返回有效 JSON
```

### 4. 验证 RAG 问答

```bash
# 测试完整 RAG 流程
cargo run -q -p knot-cli -- ask -q "Milestone 8 做了什么优化?"
# 应该:
# 1. 搜索相关文档
# 2. 调用 LLM 生成回答
# 3. 显示引用的文档块

# 测试 JSON 输出
cargo run -q -p knot-cli -- ask -q "Rust" --json
# 应该返回包含 answer 和 references 的 JSON
```

### 5. 验证配置文件

```bash
# 创建测试配置
echo '[paths]
indexes_dir = "/tmp/knot-test-indexes"' > ~/.knot/config.toml

# 索引后检查路径
cargo run -q -p knot-cli -- index -i ./docs/milestones
ls -la /tmp/knot-test-indexes/

# 清理测试配置
rm ~/.knot/config.toml
```

---

## 退出标准验证

| 标准                | 状态 | 验证命令                                      |
| :------------------ | :--- | :-------------------------------------------- |
| status 显示模型状态 | ✅    | `cargo run -q -p knot-cli -- status`          |
| index 可索引目录    | ✅    | `cargo run -q -p knot-cli -- index -i ./docs` |
| query 可搜索        | ✅    | `cargo run -q -p knot-cli -- query -t "test"` |
| download 可下载模型 | ✅    | `cargo run -q -p knot-cli -- download`        |
| ask 可 RAG 问答     | ✅    | `cargo run -q -p knot-cli -- ask -q "test"`   |
| 配置文件可覆盖路径  | ✅    | 创建 config.toml 后验证                       |
| 索引有可视化进度    | ✅    | 新建索引时观察 spinner                        |
| 错误有友好提示      | ✅    | 删除模型后运行命令观察                        |
| chat 交互模式       | ✅    | `cargo run -q -p knot-cli -- chat`            |
| serve 守护进程      | ✅    | `cargo run -q -p knot-cli -- serve`           |
| ask 服务复用        | ✅    | 启动 serve 后运行 ask，显示 "using existing"  |

---

## 后续建议

1. **安装到系统**: `cargo install --path knot-cli` 后可直接用 `knot-cli` 命令
2. **Shell 补全**: 可考虑添加 bash/zsh 自动补全支持
3. **Watch 模式**: 现有 watch 命令可进一步完善
4. **OCR 支持**: 如需 OCR 功能，需要额外配置 OCR 模型

---

## 第三方集成指南 (Rust + Tauri)

如果你的 Rust + Tauri 应用想集成 Knot 的 RAG 能力，有以下三种方式：

### 方式1: 作为 Library 依赖 (推荐)

直接在 `Cargo.toml` 中引入 `knot-core` 作为依赖：

```toml
[dependencies]
knot-core = { path = "../knot-workspaces/knot-core" }
# 或发布到 crates.io 后:
# knot-core = "0.1"
```

使用示例：

```rust
use knot_core::{KnotStore, KnotIndexer, ThreadSafeEmbeddingEngine};
use knot_core::llm::{LlamaSidecar, LlamaClient};
use std::sync::Arc;

// 1. 初始化 Embedding 引擎
let embedding_engine = ThreadSafeEmbeddingEngine::new(
    "/path/to/model.onnx",
    "/path/to/tokenizer.json",
)?;

// 2. 索引文档
let indexer = KnotIndexer::new("/path/to/registry.db", Some(Arc::new(embedding_engine))).await;
let (records, deleted) = indexer.index_directory("/path/to/docs").await?;

let store = KnotStore::new("/path/to/index.lance").await?;
store.add_records(records).await?;
store.create_fts_index().await?;

// 3. 搜索
let query_vec = embedding_engine.generate_embedding("搜索关键词").await?;
let results = store.search(query_vec, "搜索关键词").await?;

// 4. LLM 生成 (可选)
let sidecar = LlamaSidecar::spawn_quiet("/path/to/model.gguf", &bin_dir, None, 28081)?;
let client = LlamaClient::new(28081);
let mut rx = client.generate_content_stream(&prompt).await?;
while let Some(token) = rx.recv().await {
    print!("{}", token);
}
```

### 方式2: 作为 CLI 子进程调用

在 Tauri 中通过 `std::process::Command` 调用 CLI：

```rust
use std::process::Command;

// 索引
let output = Command::new("knot-cli")
    .args(["index", "-i", "/path/to/docs"])
    .output()?;

// 搜索 (JSON 输出便于解析)
let output = Command::new("knot-cli")
    .args(["query", "-t", "搜索关键词", "--json"])
    .output()?;
let results: serde_json::Value = serde_json::from_slice(&output.stdout)?;

// RAG 问答
let output = Command::new("knot-cli")
    .args(["ask", "-q", "你的问题", "--json"])
    .output()?;
let answer: serde_json::Value = serde_json::from_slice(&output.stdout)?;
```

### 方式3: 通过 LLM HTTP 服务

先启动 LLM 服务，然后通过 HTTP 调用：

```rust
// 启动服务 (单独进程或后台线程)
let _ = Command::new("knot-cli")
    .args(["serve"])
    .spawn()?;

// 等待服务就绪
tokio::time::sleep(Duration::from_secs(5)).await;

// HTTP 调用
let client = reqwest::Client::new();
let resp = client.post("http://127.0.0.1:28081/completion")
    .json(&serde_json::json!({
        "prompt": "<|im_start|>user\n你好<|im_end|>\n<|im_start|>assistant\n",
        "stream": true
    }))
    .send()
    .await?;

// 处理流式响应...
```

### 集成注意事项

| 项目                    | 说明                                           |
| :---------------------- | :--------------------------------------------- |
| **模型文件**            | 需要将 embedding 和 LLM 模型打包或让用户下载   |
| **llama-server 二进制** | 需要在 `bin` 目录下提供对应平台的 llama-server |
| **数据目录**            | 默认 `~/.knot/`，可通过 `config.toml` 自定义   |
| **端口**                | LLM 服务默认 28081，避免与其他服务冲突         |
| **平台兼容**            | 支持 macOS (arm64/x64)、Linux、Windows         |

### Tauri 特定配置

如果使用 Tauri，需要在 `tauri.conf.json` 中添加权限：

```json
{
  "tauri": {
    "allowlist": {
      "shell": {
        "execute": true,
        "sidecar": true
      },
      "fs": {
        "all": true
      }
    }
  }
}
```

---

## 相关提交

```
fbc93a9 feat(knot-cli): add download and ask commands for full RAG workflow
7092db0 feat(knot-cli): add quiet mode for clean output
ee8ea8e feat(knot-cli): add UX improvements for iteration 3
01cc817 feat(knot-cli): add LLM server reuse and interactive chat mode
```
