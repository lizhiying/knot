# Milestone 9: CLI 独立可用性

## 项目架构概述

当前项目包含三个主要 Crate：

```
knot-workspaces/
├── knot-core/    # 核心业务逻辑库
├── knot-cli/     # 命令行工具
├── knot-app/     # Tauri 桌面应用
└── pageindex-rs/ # 文档解析与索引库
```

### 各模块职责

| 模块           | 类型       | 职责                                                                              |
| :------------- | :--------- | :-------------------------------------------------------------------------------- |
| `knot-core`    | Rust 库    | 核心逻辑：Embedding 引擎、LLM 客户端/Sidecar、索引存储(LanceDB+Tantivy)、文件监控 |
| `knot-cli`     | 可执行文件 | 命令行工具：Index/Query/Watch 命令                                                |
| `knot-app`     | Tauri 应用 | 桌面 GUI：Spotlight 界面、模型管理、设置、RAG 交互                                |
| `pageindex-rs` | Rust 库    | 文档解析：Markdown/Office/PDF 解析、PageNode 结构、Embedding Provider trait       |

### 依赖关系

```
knot-app ──────┬──▶ knot-core
               │          │
               │          ├── embedding.rs   (ONNX Embedding 引擎)
               │          ├── llm.rs         (LlamaClient + LlamaSidecar)
               │          ├── store.rs       (LanceDB + Tantivy 存储)
               │          └── index.rs       (KnotIndexer)
               │
               └──▶ pageindex-rs
                          │
                          ├── IndexDispatcher (文档解析)
                          └── EmbeddingProvider (trait)

knot-cli ──────▶ knot-core
                       │
                       ├── index.rs  (KnotIndexer)
                       └── store.rs  (KnotStore)
```

## 模型加载位置分析

| 模型类型        | 加载位置           | 实现文件                          |
| :-------------- | :----------------- | :-------------------------------- |
| **Embedding**   | `knot-app/main.rs` | `EmbeddingEngine::init_onnx()`    |
| **LLM (Qwen3)** | `knot-app/main.rs` | `LlamaSidecar::spawn_with_mmap()` |

当前问题：**模型加载逻辑完全在 `knot-app` 中实现**，`knot-cli` 没有加载真实模型的能力。

## `knot-cli` 当前状态

| 功能     | 状态   | 说明                                        |
| :------- | :----- | :------------------------------------------ |
| `index`  | ⚠️ 部分 | 可索引文件，但使用 MockEmbedding (零向量)   |
| `query`  | ⚠️ 部分 | 可查询，但向量搜索无意义 (query 也用零向量) |
| `watch`  | ⚠️ 部分 | 可监控，但同样使用 MockEmbedding            |
| RAG 问答 | ❌ 缺失 | 没有 LLM 支持，无法生成回答                 |

## 让 `knot-cli` 独立可用需要完成的任务

### P0: 核心功能 (必须)

| 任务                    | 复杂度 | 说明                                                                                                  |
| :---------------------- | :----- | :---------------------------------------------------------------------------------------------------- |
| 集成真实 Embedding 模型 | 中     | CLI 需加载 ONNX 模型 (BGE-small-zh)，替换 MockEmbedding                                               |
| 默认模型路径约定        | 低     | 默认使用 `~/.knot/models/bge-small-zh-v1.5.onnx` 和 `~/.knot/models/bge-small-zh-v1.5-tokenizer.json` |
| 修复 Query 向量维度     | 低     | 当前硬编码 384 维，需与实际模型维度 (512) 匹配                                                        |
| 添加 `status` 命令      | 低     | 显示：模型是否可用、已索引文件数量                                                                    |
| 添加 `download` 命令    | 中     | 当模型缺失时，自动下载所需模型到 `~/.knot/models/`                                                    |

### P1: LLM 支持 (完整 RAG)

| 任务              | 复杂度 | 说明                                                  |
| :---------------- | :----- | :---------------------------------------------------- |
| 集成 LlamaSidecar | 中     | 添加 `ask` 命令，启动 llama-server 并调用 LlamaClient |
| 默认 LLM 模型约定 | 低     | 默认使用 `~/.knot/models/Qwen3-1.7B-Q4_K_M.gguf`      |
| 流式输出支持      | 低     | 在终端显示打字机效果                                  |

### P2: 体验优化 (可选)

| 任务             | 复杂度 | 说明                                    |
| :--------------- | :----- | :-------------------------------------- |
| 配置文件支持     | 低     | 支持 `~/.knot/config.toml` 覆盖默认路径 |
| 进度条显示       | 低     | 索引过程显示进度                        |
| 交互式 REPL 模式 | 中     | 启动后持续等待查询输入                  |

## 默认路径约定

所有数据默认存储在 `~/.knot/` 下，无需用户指定：

```
~/.knot/
├── models/
│   ├── bge-small-zh-v1.5.onnx           # Embedding 模型 (必需)
│   ├── bge-small-zh-v1.5-tokenizer.json # Embedding Tokenizer (必需)
│   ├── Qwen3-1.7B-Q4_K_M.gguf           # LLM 对话模型 (RAG 问答需要)
│   ├── OCRFlux-3B.Q4_K_M.gguf           # PDF/图片 OCR 模型 (PDF 解析需要)
│   └── OCRFlux-3B.mmproj-f16.gguf       # OCR 视觉投影模型 (PDF 解析需要)
├── indexes/
│   └── <hash>/                           # 按数据源路径 hash 分隔
│       ├── knot_index.lance/
│       └── tantivy/
└── config.toml                           # 可选配置文件
```

### 模型清单

| 模型文件                           | 用途         | 大小   | 必需性         |
| :--------------------------------- | :----------- | :----- | :------------- |
| `bge-small-zh-v1.5.onnx`           | 文本向量化   | ~90MB  | ✅ 必需         |
| `bge-small-zh-v1.5-tokenizer.json` | 分词器       | ~1MB   | ✅ 必需         |
| `Qwen3-1.7B-Q4_K_M.gguf`           | RAG 问答生成 | ~1.1GB | ⚠️ ask 命令需要 |
| `OCRFlux-3B.Q4_K_M.gguf`           | PDF/图片 OCR | ~2GB   | ⚠️ PDF 解析需要 |
| `OCRFlux-3B.mmproj-f16.gguf`       | OCR 视觉投影 | ~600MB | ⚠️ PDF 解析需要 |

**模型选择逻辑**：
- CLI 默认使用固定的模型文件名（如 `bge-small-zh-v1.5.onnx`）
- 如需切换模型，可通过 `--embedding-model` 或配置文件指定

## 建议实施顺序

1. **Phase 1**: 集成 Embedding 模型 + `status` 命令 → 让用户知道系统状态
2. **Phase 2**: 让 `index` 和 `query` 能产生有意义的结果
3. **Phase 3**: 添加 `ask` 命令 → 完整 RAG 闭环
4. **Phase 4**: 配置文件 + 体验优化

## 预期命令示例

```bash
# 下载缺失的模型
knot-cli download

# 下载特定模型
knot-cli download --model embedding   # 仅下载 Embedding 模型
knot-cli download --model llm         # 仅下载 LLM 对话模型
knot-cli download --model ocr         # 仅下载 OCR 模型

# 查看状态 (模型是否就绪、索引文件数)
knot-cli status

# 索引 (默认使用 ~/.knot/models 下的模型)
knot-cli index -i ~/Documents/notes

# 查询 (纯检索，默认使用 ~/.knot 下的索引)
knot-cli query -t "如何使用 Rust 的生命周期？"

# RAG 问答 (需要 LLM)
knot-cli ask -q "总结一下 Rust 的所有权规则"

# 高级：指定自定义模型
knot-cli index -i ~/Documents/notes --embedding-model ~/.knot/models/custom.onnx
```

## `status` 命令输出示例

```
Knot CLI Status
===============

Models:
  ✓ Embedding:  bge-small-zh-v1.5.onnx (512 dim)
  ✓ LLM:        Qwen3-1.7B-Q4_K_M.gguf (1.7B params)
  ✗ OCR:        OCRFlux-3B.Q4_K_M.gguf (missing)
    → Run 'knot-cli download --model ocr' to install

Indexed Sources (2):
  1. ~/Documents/notes
     Files: 234 | Chunks: 1,890 | Last Sync: 2026-02-08 13:40:00
  2. ~/Projects/rust-book-cn
     Files: 1,000 | Chunks: 7,011 | Last Sync: 2026-02-08 10:15:00

Paths:
  Knot Root:      ~/.knot
  Config Home:    ~/.knot/config.toml
  Index Storage:  ~/.knot/indexes/<hash>/
  Models:         ~/.knot/models/
```
