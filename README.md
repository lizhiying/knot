<div align="right">
  <a href="#english">English</a> | <a href="#简体中文">简体中文</a>
</div>

---

<h1 id="english">Knot</h1>

Knot is a localized, privacy-first desktop AI assistant (similar to macOS Spotlight), featuring a powerful built-in RAG (Retrieval-Augmented Generation) system. It can deeply parse, semantically search, and intelligently interact with various local documents. The project is primarily built with Rust and Tauri, focusing on high performance and data privacy security.

## 📦 Project Modules

Knot's codebase uses a modular Workspace structure, divided into several core projects by function:

### 🖥️ knot-app (Desktop Application)
Knot's main program entry, a cross-platform desktop client built with Tauri v2.
- **Frontend**: Developed with Svelte 5 + Vite, providing a smooth, modern User Interface (UI).
- **Backend (Rust)**: Handles system-level interactions, local model calls (ONNX Runtime integration), and communication with LLM background processes (such as Llama.cpp Sidecar).

### 🧠 knot-core (Core RAG Engine)
Knot's core logic layer, responsible for combining document parsing results with AI search capabilities.
- Manages underlying storage and retrieval systems: integrates `LanceDB` (Vector Database) and `Tantivy` (Full-text Search Engine) to achieve Hybrid Search.
- Contains text Chunking, Tokenization (Jieba, ICU paragraph segmentation), and core logic for interacting with Embedding models.

### 🔄 knot-parser (Document Parsing Dispatcher)
A unified document parsing facade, serving as the scheduling center for other format-specific parsers.
- Aggregates support for PDF, Excel, Markdown, and Office documents (such as docx, pptx, etc.).
- Provides a unified interface to the upper layer, converting unstructured documents into Knot's standard hierarchical semantic tree (PageNode Tree).

### 📄 knot-pdf (Advanced PDF Parser)
A Rust-native offline PDF parser built specifically for RAG scenarios.
- Supports conventional text extraction while featuring rich built-in Computer Vision (CV) and Deep Learning extension capabilities.
- Supports integration with PDFium rendering, OCR engines (Tesseract, ONNX-based PaddleOCR), Layout Analysis models (Layout Model), and Formula Recognition models (Formula Model), precisely restoring complex PDF document layouts.

### 📊 knot-excel (Spreadsheet Parser)
A parsing module focused on Excel structured data.
- High-performance reading of `.xlsx` and other spreadsheet files based on `calamine`.
- Optional `DuckDB` integration: supports querying and processing complex spreadsheet data via SQL, ideal for data-intensive knowledge Q&A scenarios.

### 📝 knot-markdown (Markdown Parser)
A semantic Markdown parsing tool.
- Parses Markdown text into an Abstract Syntax Tree based on `pulldown-cmark`.
- Automatically builds semantic Heading Trees based on heading levels, preserving the original context structure of the document to significantly improve RAG retrieval quality.

## 🛠️ Technology Stack

- **Core Programming Language**: Rust
- **Desktop Application Framework**: Tauri v2
- **Frontend Technologies**: Svelte 5, Vite, JavaScript/TypeScript
- **Local AI Inference**: ONNX Runtime (for Embedding and Layout Analysis), Llama.cpp (for Large Language Model Inference)
- **Database Components**: LanceDB (Vector Storage), Tantivy (Inverted Index Search), DuckDB (Table Querying), SQLite (Metadata Storage)

## 🚀 Build and Run

Ensure you have the Rust (Cargo) and Node.js environments installed on your system.

### Build the entire Workspace
```bash
cargo build --release
```

### Run Tauri Desktop Application (Development Mode)
```bash
cd knot-app
npm install
npm run tauri dev
```

### Run specific module tests
```bash
cargo test -p knot-pdf
cargo test -p knot-core
```

## 💻 Development and Testing Environment

The current project is primarily developed and extensively tested in the following environment:
- **Hardware**: Apple Silicon (Mac M1, etc.)
- **OS**: macOS

## 📧 Contact

If you have any suggestions, questions, or wish to collaborate, feel free to contact via email:
- **Email**: [timdoctli@gmail.com](mailto:timdoctli@gmail.com)

## 📄 License

This project is open-sourced under the [MIT License](LICENSE).

---

<h1 id="简体中文">Knot</h1>

Knot 是一个专注于本地化、隐私优先的桌面 AI 助手（类似于 macOS 的 Spotlight），内置了强大的 RAG（检索增强生成）系统，能够对各类本地文档进行深度解析、语义搜索和智能交互。项目主要基于 Rust 和 Tauri 构建，主打高性能与数据隐私安全。

## 📦 项目模块介绍

Knot 的代码库采用模块化的 Workspace 结构，按照功能拆分为多个核心工程：

### 🖥️ knot-app (桌面应用程序)
Knot 的主程序入口，基于 Tauri v2 构建的跨平台桌面客户端。
- **前端**：采用 Svelte 5 + Vite 开发，提供流畅、现代化的用户界面（UI）。
- **后端 (Rust)**：处理系统级交互、本地模型调用（ONNX Runtime 集成）以及与 LLM 后台进程（如 Llama.cpp Sidecar）的通信。

### 🧠 knot-core (核心 RAG 引擎)
Knot 的核心逻辑层，负责将文档解析结果与 AI 搜索能力结合。
- 管理底层存储与检索系统：集成了 `LanceDB`（向量数据库）与 `Tantivy`（全文检索引擎）实现混合检索（Hybrid Search）。
- 包含文本切片（Chunking）、分词（Jieba, ICU 段落分割）以及与 Embedding 模型交互的核心逻辑。

### 🔄 knot-parser (文档解析调度器)
统一的文档解析门面（Facade），作为其他特定格式解析器的调度中心。
- 聚合了对 PDF、Excel、Markdown 以及 Office 文档（如 docx, pptx 等）的支持。
- 向上层提供统一的接口，将非结构化文档转换为 Knot 标准的层级化语义树（PageNode Tree）。

### 📄 knot-pdf (高级 PDF 解析器)
专为 RAG 场景打造的 Rust 原生离线 PDF 解析器。
- 支持常规文本提取，同时内置了丰富的计算机视觉（CV）与深度学习扩展能力。
- 支持集成 PDFium 渲染、OCR 引擎（Tesseract, 基于 ONNX 的 PaddleOCR）、版面分析模型（Layout Model）以及公式识别模型（Formula Model），能够精准还原复杂的 PDF 文档排版。

### 📊 knot-excel (表格解析器)
专注于 Excel 结构化数据的解析模块。
- 基于 `calamine` 实现高性能的 `.xlsx` 等表格文件读取。
- 可选集成 `DuckDB`：支持通过 SQL 对复杂的电子表格数据进行查询和处理，非常适合数据密集型的知识问答场景。

### 📝 knot-markdown (Markdown 解析器)
Markdown 语义化解析工具。
- 基于 `pulldown-cmark` 将 Markdown 文本解析为抽象语法树。
- 自动构建基于标题层级的语义树（Heading Trees），保留文档原有的上下文结构，显著提升 RAG 的检索质量。

## 🛠️ 技术栈

- **核心编程语言**: Rust
- **桌面应用框架**: Tauri v2
- **前端技术**: Svelte 5, Vite, JavaScript/TypeScript
- **本地 AI 推理**: ONNX Runtime (用于 Embedding 和版面分析), Llama.cpp (用于大语言模型推理)
- **数据库组件**: LanceDB (向量存储), Tantivy (倒排索引搜索), DuckDB (表格查询), SQLite (元数据存储)

## 🚀 构建与运行

确保你的系统中已安装 Rust (Cargo) 和 Node.js 环境。

### 构建整个 Workspace
```bash
cargo build --release
```

### 运行 Tauri 桌面应用 (开发模式)
```bash
cd knot-app
npm install
npm run tauri dev
```

### 运行特定模块的测试
```bash
cargo test -p knot-pdf
cargo test -p knot-core
```

## 💻 开发与测试环境

当前项目主要在以下环境进行开发和充分测试：
- **硬件**: Apple Silicon (Mac M1 等)
- **操作系统**: macOS

## 📧 联系方式

如果有任何建议、问题或合作交流，欢迎通过邮件联系：
- **Email**: [timdoctli@gmail.com](mailto:timdoctli@gmail.com)

## 📄 协议 (License)

本项目采用 [MIT License](LICENSE) 开源协议。
