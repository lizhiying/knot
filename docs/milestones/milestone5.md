# Milestone 5: Search Optimization (Tantivy & ICU)

## Goal
Implement a robust, production-grade Full-Text Search (FTS) engine using **Tantivy** to replace the current naive keyword matching. The system must support high-quality Chinese/English mixed search, static linking for single-binary distribution, and advanced normalization.

## Requirements

### 1. Tantivy Integration (FTS Engine)
- **Replace**: Replace `knot-core`'s current naive substring matching (`contains`) with `tantivy`.
- **Dual Index Strategy**:
    - **LanceDB**: Stores Vectors (Semantic Search).
    - **Tantivy**: Stores Text/Metadata (Keyword Search / BM25).
- **Consolidated Search**: `rag_query` must merge results from both engines (Reciprocal Rank Fusion or Linear Weighted).

### 2. Advanced Tokenization (ICU & Stopwords)
- **Library**: Use `tantivy-icu`.
- **Goal**: Correctly segment mixed Chinese/English text.
- **StopWords Optimization**:
    - **External File**: Load stop words from `knot-app/stopwords.txt` (bundled resource).
    - **Benefits**:
        - **Quality**: Removes noise (e.g., "的", "is") to prevent high-frequency matches from diluting scores.
        - **Performance**: Reduces intersection cost in inverted index.
        - **Flexibility**: Runtime updates by editing the text file (no recompile needed).
    - **Implementation**: Create a `load_stopwords` helper function to parse the file (handling comments `#` and multiple items per line).

### 3. Static Linking
- **Constraint**: The app is distributed as a single `dmg` / binary.
- **Requirement**: `icu` libraries must be statically linked or bundled such that no external `.dylib` / `.dll` installation is required by the end user.

### 4. Normalization
- **AsciiFoldingFilter**: Handle diacritics (e.g., `café` <-> `cafe`).
- **LowerCasing**: Case-insensitive search.
- **Normalization**: Unicode normalization (NFKC).

## Technical Architecture

### `KnotStore` Update
- **Fields**:
    - `lancedb_table`: For vector ops.
    - `tantivy_index`: `tantivy::Index` (on disk, parallel to lancedb).
- **Write Path**:
    - `add_records`: Write vector to LanceDB, write text to Tantivy.
    - `commit`: Ensure both are committed.
- **Read Path**:
    - `search`: Parallel query (LanceDB Vector Search + Tantivy Boolean Search).
    - `merge`: Combine scores.

## Tasks
See `.plans/milestone5/` for iteration details.
