# AGENTS.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

**Knot** is a desktop AI assistant (similar to Spotlight) built with Tauri v2. It provides local document parsing, semantic search, and RAG capabilities. The project uses three main Rust workspaces:

1. **pageindex-rs**: Core document parsing library that converts unstructured documents into semantic trees
2. **undoc**: Microsoft Office document extraction library (DOCX, XLSX, PPTX to Markdown/JSON)
3. **knot-app**: Tauri desktop application that integrates AI models and provides the user interface

## Build Commands

### Build the entire workspace
```bash
cargo build --release
```

### Build specific workspace members
```bash
# Build pageindex-rs
cargo build -p pageindex-rs --release

# Build undoc library
cargo build -p undoc --release

# Build undoc CLI
cargo build -p undoc-cli --release

# Build knot-app (Tauri app)
cargo build -p knot-app --release
```

### Build knot-app with Tauri
```bash
cd knot-app
npm run tauri build
```

### Development mode (knot-app)
```bash
cd knot-app
npm run tauri dev
```

## Testing

### Run all tests in workspace
```bash
cargo test
```

### Run tests for specific packages
```bash
cargo test -p pageindex-rs
cargo test -p undoc
```

### Run a single test
```bash
# Find test files in pageindex-rs/examples/ or use cargo test <test_name>
cargo test --test <test_name>
```

## Linting and Formatting

### Format code
```bash
cargo fmt --all
```

### Check formatting
```bash
cargo fmt --all -- --check
```

### Run clippy
```bash
cargo clippy -- -D warnings
```

## Architecture

### Data Flow

1. **Document Ingestion**: Files (PDF/DOCX/XLSX/PPTX/Markdown) → `pageindex-rs` → Structured `PageNode` tree
2. **Semantic Processing**: `PageNode` tree → AI models (LLM/Embedding) → Enriched with summaries and vectors
3. **Storage**: Enriched trees → LanceDB (vector database)
4. **Retrieval**: User query → Vector search → Context assembly → LLM generation

### Key Components

#### pageindex-rs (`/pageindex-rs`)

Core library that defines the document parsing pipeline:

- **`IndexDispatcher`** (`core/dispatcher.rs`): Main entry point that routes files to format-specific parsers
- **Format parsers** (`formats/`):
  - `MarkdownParser`: Stack-based state machine for Markdown AST with heading hierarchy
  - `DocxParser`: Wraps `undoc` library, converts DOCX to Markdown then to PageNode tree
  - `PdfParser`: Uses PDFium + LLM vision model (OCRFlux-3B) to extract structured content
- **Provider traits** (`lib.rs`): `VisionProvider`, `LlmProvider`, `EmbeddingProvider` - implemented by application layer

**Important**: `pageindex-rs` is a pure library. It does NOT contain model implementations. Models are injected via `PageIndexConfig` at runtime.

#### undoc (`/undoc`)

High-performance Microsoft Office document extraction library:

- **Core parsers**: `docx/`, `xlsx/`, `pptx/` - Extract content from Office Open XML formats
- **Renderers** (`render/`): Convert to Markdown, plain text, or JSON
- **CLI** (`cli/`): Standalone `undoc` command-line tool with self-update support
- **FFI** (`ffi.rs`): C-ABI bindings for C#/.NET integration (enabled with `ffi` feature)

Parallel processing with Rayon for multi-section documents.

#### knot-app (`/knot-app`)

Tauri v2 desktop application:

- **Frontend** (`src/`): Svelte UI components
- **Backend** (`src-tauri/src/`):
  - `engine/`: AI model wrappers
    - `llm.rs`: Llama.cpp integration (via sidecar process) for LLM inference
    - `embedding.rs`: ONNX Runtime embedding engine (bge-small-zh-v1.5)
  - `main.rs`: Tauri commands, application setup, model initialization
- **Binary dependencies** (`bin/`): llama-server executables for different platforms

### Provider Trait Implementation Pattern

When implementing document parsing with AI models:

```rust
// 1. Initialize models
let embedding_engine = EmbeddingEngine::init_onnx("path/to/model.onnx")?;
let llm_client = LlamaClient::new("http://127.0.0.1:8080");

// 2. Create config with provider references
let config = PageIndexConfig::new()
    .with_llm_provider(&llm_client)
    .with_embedding_provider(&embedding_engine)
    .with_vision_provider(&vision_engine);

// 3. Parse document
let dispatcher = IndexDispatcher::new();
let tree = dispatcher.index_file(path, &config).await?;

// 4. Post-processing (optional, can also be done in-pipeline)
dispatcher.inject_summaries(&mut tree, &config).await;
dispatcher.inject_embeddings(&mut tree, &config).await;
```

## Feature Flags

### pageindex-rs
- `office`: Enable Office document support (via `undoc` dependency)
- `vision`: Enable vision-based parsing features

### undoc
- `docx`: DOCX support (enabled by default)
- `xlsx`: XLSX support (enabled by default)  
- `pptx`: PPTX support (enabled by default)
- `async`: Async file I/O support
- `ffi`: C-ABI foreign function interface

### knot-app/src-tauri
Build with all features: `pageindex-rs` with `["office", "vision"]`

## Models Directory

Models are stored in `/models` (relative to workspace root):

- `bge-small-zh-v1.5.onnx`: Chinese/English embedding model
- Other GGUF models (LLM) should be placed here

The `get_models_dir()` function in `knot-app/src-tauri/src/main.rs` computes the path dynamically.

## Common Development Patterns

### Adding a new document format parser

1. Create parser struct in `pageindex-rs/src/formats/`
2. Implement `DocumentParser` trait with `can_handle()` and `parse()` methods
3. Register parser in `IndexDispatcher::new()` constructor
4. Ensure `parse()` returns a `PageNode` tree with proper metadata

### Token counting

Token count is approximated as `content.len() / 4` throughout the codebase. This is a simplification for Chinese/English mixed content.

### Tree optimization (Thinning)

The `apply_tree_thinning()` method in `IndexDispatcher` merges nodes with token count below `min_token_threshold` to ensure each RAG chunk has sufficient semantic information. This happens automatically after parsing.

## Debugging

### Enable verbose logging for Tauri
```bash
cd knot-app
RUST_LOG=debug npm run tauri dev
```

### Inspect PageNode tree structure
Parse results are JSON-serializable `PageNode` trees. Use `serde_json::to_string_pretty()` to inspect.

## Language Conventions

- **Primary language**: Code comments and documentation use Chinese where domain-specific (e.g., `docs/` folder)
- **Identifiers**: English for all code symbols (functions, structs, variables)
- **User-facing strings**: Chinese in knot-app UI

## Dependencies Note

- **llama.cpp integration**: The `knot-app` uses llama.cpp via sidecar process (not Rust bindings). The `llm-server` binary is launched on demand.
- **ONNX Runtime**: Used for embedding generation. Model must support `load-dynamic` feature.
- **PDFium**: PDF rendering requires PDFium library. `pdfium-render` crate provides bindings.

## Important Constraints

- **No standalone models**: `pageindex-rs` is model-agnostic. Never add model loading code to this library.
- **Async boundaries**: Document parsing is async. Use `tokio::runtime` if calling from sync context.
- **Memory management**: LLM models use mmap strategy for lazy loading. Embedding models are preloaded at startup.
- **Thread safety**: Provider implementations must be `Send + Sync` for use in async context.
