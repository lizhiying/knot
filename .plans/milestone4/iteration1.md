Milestone: milestone4
Iteration: iteration1

Goal: Implement the functional expand/collapse logic and layout structure.
Assumptions: The data for `Section` (heading) and `Quote` (content) is available in the search result object.
Scope: `src/lib/components/ChatMessage.svelte` or `HybridEvidence.svelte` (if separate).

Tasks:
- [x] Requirements Analysis: Check `HybridSearchResult` struct/interface to ensure `metadata` or `content` fields are available for display.
- [x] Component Structure: Add `isExpanded` state to the evidence card component.
- [x] UI Logic: Add specific Click Handler to toggle `isExpanded`.
- [x] UI Layout: Add the "Chevron" button to the right of the score.
- [x] UI Layout: Add the conditional block for `Section` and `Quote` that appears when `isExpanded` is true.

Exit criteria: Clicking the card toggles the visibility of the detailed content.
