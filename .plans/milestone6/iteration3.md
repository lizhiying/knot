Milestone: milestone6
Iteration: iteration3

Goal:
Enhance semantic search (Vector) by injecting document context (Title/Path) into the embedding text.

Assumptions:
- Vector model supports input text length with added context (512 tokens is usually plenty for chunks).

Scope:
- `knot-core`: `KnotIndexer` text preparation logic.

Tasks:
- [x] Modify `KnotIndexer::enrich_node` (or similar) to prepend metadata.
    - Format: `File: [name] | Path: [tags] \n [content]`
- [x] Verify that new embeddings distinctively capture filename semantics.
- [x] Run full regression test on Search Quality.

Exit criteria:
- Vector search finds a document when query matches filename but not body text.
- Overall search feel is "smarter".
