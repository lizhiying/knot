# Milestone: Milestone 5 (Search Optimization)
Iteration: 1 (Basic Tantivy Integration)

Goal: Integrate `tantivy` into `knot-core` and implement basic FTS (Standard Tokenizer) alongside LanceDB.

Assumptions:
- We can run Tantivy and LanceDB side-by-side in `KnotStore`.
- Initial implementation uses standard tokenizer (no ICU yet).

Scope:
- Add `tantivy` dependency.
- Define Tantivy Schema (Text, Path, ID).
- Update `KnotStore` to manage Tantivy `IndexWriter` and `IndexReader`.
- Implement `add_records` to write to BOTH indices.
- Implement `search` to query Tantivy (BM25) and mix with Vector score.

Tasks:
- [x] Add `tantivy` to `knot-core/Cargo.toml`
- [x] Initialize Tantivy Index in `~/.knot/indexes/<hash>/tantivy`
- [x] Implement `index_writer` logic in `KnotStore::add_records`
- [x] Implement `search` logic using Tantivy `QueryParser`
- [x] Verify basic English search ("vanna") works better than substring

Exit criteria:
- `knot-app` compiles and runs.
- Indexing a file populates both LanceDB and Tantivy folders.
- Search returns results from Tantivy.
