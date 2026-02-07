# Milestone 6 Summary: Advanced Indexing & Search Optimization

## Overview
Milestone 6 focused on significantly enhancing the search capabilities of Knot by introducing a hybrid search engine (Dual Indexing), improved linguistic support (Jieba for Chinese), and context-aware indexing (Metadata & Vector Injection).

## Key Features Implemented

### 1. Dual Indexing (Hybrid Search)
-   **Architecture**: Combines **Tantivy** (Keyword Search) and **LanceDB** (Vector Search).
-   **Mechanism**:
    -   `KnotStore` maintains both indices in sync.
    -   Search queries are executed in parallel and results are fused using Reciprocal Rank Fusion (RRF).
-   **Benefit**: Balances exact keyword matching with semantic understanding.

### 2. Advanced Tokenization
-   **Jieba Tokenizer**: Integrated `jieba-rs` for high-quality Chinese word segmentation.
-   **Schema**:
    -   `text_zh`: Indexed with Jieba (for Chinese queries).
    -   `text_std`: Indexed with Standard Tokenizer (for multilingual support).
-   **Benefit**: "什么是vanna" changes from character-based matching to semantic word matching.

### 3. Metadata Indexing & Boosting (Iteration 2)
-   **New Fields**:
    -   `file_name`: Stores filename (e.g., "manual.md"). **Boost: 3.0**.
    -   `path_tags`: Stores directory path tokens (e.g., "docs api v1"). **Boost: 1.5**.
-   **Path Processing**: Automatically strips system prefixes and extracts meaningful folder names as tags.
-   **Benefit**: Searching for "manual" or "api" prioritizes files with those names/paths over files just containing the word.

### 4. Vector Context Injection (Iteration 3)
-   **Strategy**: "Embedding-Only" Injection.
-   **Implementation**:
    -   **Stored Text**: Remains clean (original content).
    -   **Embedding Text**: Prepend metadata: `File: [name] | Path: [tags] \n [content]`.
-   **Benefit**: Vector search understands that a chunk belongs to "API Documentation" even if the chunk text is generic, improving semantic retrieval.

---

## How to Test

### Prerequisite: Reset Index
**Crucial Step**: Since the Schema and Embedding format have changed, you **MUST** reset the index to see the effects.
1.  Open App -> Settings.
2.  Click **"Reset Index"**.
3.  Wait for the app to restart and re-index your workspace.

### Test Scenarios

#### Scenario A: Chinese Search Quality
-   **Query**: `什么是vanna` (or similar Chinese query).
-   **Expectation**:
    -   Should match documents containing "vanna" and related Chinese context.
    -   Results should be relevant (Jieba segmentation working).

#### Scenario B: Metadata Boosting (Filename)
-   **Query**: Search for a specific filename, e.g., `store` (matching `store.rs`).
-   **Expectation**: 
    -   `knot-core/src/store.rs` should appear at the **Top** of the list.
    -   Evidence should show "Match: file_name" (if UI supports debug, otherwise judge by rank).

#### Scenario C: Path Context (Vector)
-   **Query**: Search for a concept implied by a folder name, e.g., `core logic` (where "core" is a folder `knot-core`).
-   **Expectation**:
    -   Files inside `knot-core` should be retrieved even if they don't explicitly say "core logic" in every chunk, due to vector context injection.

#### Scenario D: Mixed Language
-   **Query**: `rust async trait`.
-   **Expectation**:
    -   Should correctly retrieve Rust code files.
    -   Hybrid search should rank these well using `text_std`.

## Verification Checklist
- [x] Application builds and runs without panic.
- [x] "Reset Index" successfully clears old data and starts re-indexing.
- [x] Search returns results for both English and Chinese.
- [x] Filename matches feel "sticky" (stay at top).
- [x] No "dirty" metadata text visible in the UI snippets (verifying clean storage).
