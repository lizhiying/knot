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

| 任务                    | 复杂度 | 说明                                                    |
| :---------------------- | :----- | :------------------------------------------------------ |
| 集成真实 Embedding 模型 | 中     | CLI 需加载 ONNX 模型 (BGE-small-zh)，替换 MockEmbedding |
| 模型路径配置            | 低     | 添加 `--models-dir` 参数，指定模型文件位置              |
| 修复 Query 向量维度     | 低     | 当前硬编码 384 维，需与实际模型维度 (512) 匹配          |

### P1: LLM 支持 (完整 RAG)

| 任务              | 复杂度 | 说明                                                  |
| :---------------- | :----- | :---------------------------------------------------- |
| 集成 LlamaSidecar | 中     | 添加 `ask` 命令，启动 llama-server 并调用 LlamaClient |
| LLM 模型路径配置  | 低     | 添加 `--llm-model` 参数                               |
| 流式输出支持      | 低     | 在终端显示打字机效果                                  |

### P2: 体验优化 (可选)

| 任务             | 复杂度 | 说明                       |
| :--------------- | :----- | :------------------------- |
| 配置文件支持     | 低     | 支持 `~/.knot/config.toml` |
| 进度条显示       | 低     | 索引过程显示进度           |
| 交互式 REPL 模式 | 中     | 启动后持续等待查询输入     |

## 建议实施顺序

1. **Phase 1**: 集成 Embedding 模型 → 让 `index` 和 `query` 能产生有意义的结果
2. **Phase 2**: 添加 `ask` 命令 → 完整 RAG 闭环
3. **Phase 3**: 配置文件 + 体验优化

## 预期命令示例

```bash
# 索引
knot-cli index -i ~/Documents/notes --models-dir ~/.knot/models

# 查询 (纯检索)
knot-cli query -t "如何使用 Rust 的生命周期？" --data-dir ~/.knot

# RAG 问答 (需要 LLM)
knot-cli ask -q "总结一下 Rust 的所有权规则" --llm-model ~/.knot/models/qwen3-1.5b-q4.gguf
```
