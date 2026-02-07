Milestone: milestone6
Iteration: iteration1

Goal:
Enable robust multilingual search by implementing Dual Indexing (Jieba + ICU) in `knot-core` and updating the search logic to query both fields.

Assumptions:
- We can add `tantivy-icu` (or `tantivy` with `icu` feature) to `knot-core` dependencies.
- Re-indexing is acceptable for this update (breaking schema change).

Scope:
- `knot-core`: Dependency updates, Schema modification, Search logic update.
- `knot-app`: Trigger re-index UI/Notification (handled by existing re-index flow).

Tasks:
- [x] Update `knot-core/Cargo.toml` (Tantivy 0.22, ICU Deferred).
- [x] Modify `KnotStore::ensure_tantivy_index` to define `text_zh` and `text_std` fields (text_std uses "default" for now).
- [x] Update `KnotStore::add_records` to populate both fields from the same source text.
- [x] Update `KnotStore::search` to construct a BooleanQuery aiming at both fields.
- [x] Add `reset_index` call or version check to force re-index (Implemented Auto-Reset).

Exit criteria:
- Searching for Chinese specific terms works (via `text_zh`).
- Searching for English/mixed terms works (via `text_std` tokenization).
- Index size increase is within acceptable limits.
