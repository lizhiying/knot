# Milestone: Milestone 5 (Search Optimization)
Iteration: 2 (Advanced Tokenization - Jieba)

Goal: Implement high-quality multilingual tokenization (`jieba-rs`) and normalization, ensuring static linking.

Assumptions:
- `jieba-rs` integrates correctly with `tantivy` as a custom tokenizer.

Scope:
- Add `jieba-rs` dependency.
- Create `stopwords.txt` loader (`include_str!`):
    - Read from bundled resources.
    - Parse whitespace-separated words.
- Configure `TextAnalyzer` chain:
    - Tokenizer: `JiebaTokenizer` (Custom implementation)
    - Filter: `StopWordFilter` (using loaded list)
    - Filter: `LowerCaser`, `RemoveLongFilter`
- Verify "什么是vanna" is correctly segmented.
- Verify "café" matches "cafe".

Tasks:
- [x] Add `jieba-rs` to Cargo.toml (Replaced `tantivy-icu`)
- [x] Implement `JiebaTokenizer` struct in `knot-core` (Custom `Tokenizer` trait imp)
- [x] Integrate `StopWordFilter` with `stopwords.txt`
- [x] Add `LowerCaser` and `RemoveLongFilter`
- [x] Register tokenizer as "jieba" in `KnotStore`
- [x] Verify mixed language tokenization via unit tests

Exit criteria:
- "什么是vanna" returns `vanna.md` (via "vanna" token match).
- `cargo test` passes.
- No runtime dynamic link errors.
