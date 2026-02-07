# Milestone 6: Advanced Indexing & Search

## Goal
Implement advanced multilingual search using Dual Indexing (Jieba + ICU), improve search relevance with Metadata Boosting (Filename/Path), and enhance vector search with Context Injection.

## Success Metrics
- [ ] searching mixed CJK/English text returns relevant results from both languages.
- [ ] Searching for a filename (e.g. "Knot") ranks the file itself higher than files just mentioning it.
- [ ] Vector search retrieves relevant chunks even if they don't explicitly contain the query keywords, thanks to injected context.

## Iterations
- [ ] **Iteration 1: Dual Indexing & ICU Integration**
    - Implement `text_zh` (Jieba) and `text_std` (ICU) schema.
    - Enable mixed logic search.
- [ ] **Iteration 2: Metadata & Boosting**
    - Add `file_name` and `path_tags` to Schema.
    - Implement Path Processing (Stripping, Root Detection).
    - Apply Query Boosting.
- [ ] **Iteration 3: Vector Context & Refinement**
    - Inject metadata into Vector Embeddings.
    - Verify and Tuning.
