<script>
    /**
     * 结果面板容器组件
     */
    import AiInsight from "./AiInsight.svelte";
    import EvidencePanel from "./EvidencePanel.svelte";

    let {
        visible = false,
        results = [],
        insightState = {},
        isSearching = false,
        searchDuration = 0,
        highlightedCardId = null,
        onHighlightCard = () => {},
        onUnhighlightCard = () => {},
        sqlPagination = null,
        onGoToPage = () => {},
    } = $props();

    let matchText = $derived(
        results.length > 0 ? `${results.length} results match` : "No results",
    );
</script>

{#if visible}
    <div class="flex h-full min-h-0">
        <EvidencePanel
            {results}
            {matchText}
            {isSearching}
            {searchDuration}
            onHighlight={onHighlightCard}
            onUnhighlight={onUnhighlightCard}
        />
        <AiInsight
            status={insightState.status}
            statusType={insightState.statusType}
            isThinking={insightState.isThinking}
            content={insightState.content}
            showCursor={insightState.showCursor}
            {sqlPagination}
            {onGoToPage}
        />
    </div>
{:else}
    <div
        class="flex flex-col items-center justify-center h-full text-[var(--text-secondary)] opacity-60 pb-20 fade-in"
    >
        <div
            class="w-16 h-16 rounded-2xl bg-[var(--bg-card)] border border-[var(--border-color)] flex items-center justify-center mb-6 shadow-sm"
        >
            <span class="material-symbols-outlined text-3xl">search</span>
        </div>
        <h3 class="text-sm font-medium text-[var(--text-primary)] mb-2">
            Ready to search
        </h3>
        <p class="text-xs max-w-[240px] text-center leading-relaxed">
            Type anything to search across your local documents, knowledges, and
            settings.
        </p>

        <div class="mt-8 flex gap-3">
            <div
                class="flex items-center gap-1.5 px-2 py-1 rounded bg-[var(--bg-card)] border border-[var(--border-color)]"
            >
                <kbd class="font-mono text-[10px]">⏎</kbd>
                <span class="text-[10px]">to search</span>
            </div>
            <div
                class="flex items-center gap-1.5 px-2 py-1 rounded bg-[var(--bg-card)] border border-[var(--border-color)]"
            >
                <kbd class="font-mono text-[10px]">↑↓</kbd>
                <span class="text-[10px]">to navigate</span>
            </div>
        </div>
    </div>
{/if}

<style>
    .fade-in {
        animation: fadeIn 0.3s ease-out forwards;
    }

    @keyframes fadeIn {
        from {
            opacity: 0;
            transform: translateY(5px);
        }
        to {
            opacity: 0.6;
            transform: translateY(0);
        }
    }
</style>
