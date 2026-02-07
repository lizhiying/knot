Milestone: milestone6
Iteration: iteration2

Goal:
Improve search relevance by indexing File Names and Paths with specific boosting weights.

Assumptions:
- Iteration 1 is complete and Dual Indexing is working.

Scope:
- `knot-core`: Schema update, Path processing logic, Query boosting logic.

Tasks:
- [x] Modify `KnotStore` Schema to add `file_name` (ICU, Boost 3.0) and `path_tags` (Simple, Boost 1.5).
- [x] Implement `PathProcessor` struct/logic in `knot-core` to:
    - Strip prefixes (~/, /Users/...).
    - Detect Project Root (.git, Cargo.toml).
    - Extract meaningful tags.
- [x] Update `KnotIndexer` to compute `file_name` and `path_tags` and pass to `KnotStore`.
- [x] Update `KnotStore::search` to include these fields in the query with boosts.

Exit criteria:
- Searching for a filename returns the file as the top result.
- Searching for a folder name returns files within that folder with boosted rank.
