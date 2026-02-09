<script>
    /**
     * 证据面板组件
     */
    import EvidenceCard from "./EvidenceCard.svelte";

    let {
        results = [],
        matchText = "0 hits",
        isSearching = false,
        searchDuration = 0,
        onHighlight = () => {},
        onUnhighlight = () => {},
    } = $props();
</script>

<div
    class="w-[38%] min-w-[200px] flex flex-col border-r border-[var(--border-color)]"
>
    <!-- 面板头部 -->
    <div class="px-5 py-2 mt-2 flex justify-between items-center">
        <div class="flex items-center gap-2">
            <span
                class="text-[10px] font-bold text-[var(--text-muted)] uppercase tracking-widest"
                >Hybrid Evidence</span
            >
            {#if !isSearching && searchDuration > 0}
                <span class="text-[10px] text-[var(--text-muted)] opacity-70"
                    >{searchDuration}s</span
                >
            {/if}
        </div>
        <span
            class="text-[10px] font-medium text-[var(--text-secondary)] px-2 py-0.5 rounded-full border border-[var(--border-color)]"
        >
            {isSearching ? "Searching..." : matchText}
        </span>
    </div>

    <!-- 证据列表 -->
    <div class="flex-1 overflow-y-auto p-4 space-y-3 scroll-hide">
        {#if isSearching}
            <!-- 骨架屏 -->
            {#each Array(4) as _}
                <div
                    class="p-3 bg-[var(--bg-card)]/50 rounded-lg border border-[var(--border-color)]/50 space-y-2 animate-pulse"
                >
                    <div class="flex justify-between items-center">
                        <div
                            class="h-3 w-1/3 bg-[var(--border-color)]/30 rounded"
                        ></div>
                        <div
                            class="h-3 w-8 bg-[var(--border-color)]/30 rounded"
                        ></div>
                    </div>
                    <div
                        class="h-12 w-full bg-[var(--border-color)]/20 rounded"
                    ></div>
                </div>
            {/each}
        {:else if results.length === 0}
            <!-- 空结果提示 -->
            <div
                class="flex flex-col items-center justify-center h-full text-center px-4 py-8"
            >
                <div
                    class="w-12 h-12 rounded-xl bg-[var(--bg-secondary)] border border-[var(--border-color)] flex items-center justify-center mb-4"
                >
                    <span
                        class="material-symbols-outlined text-xl text-[var(--text-muted)]"
                        >search_off</span
                    >
                </div>
                <p class="text-xs text-[var(--text-secondary)] mb-1">
                    未找到相关结果
                </p>
                <p
                    class="text-[10px] text-[var(--text-muted)] max-w-[180px] leading-relaxed"
                >
                    请尝试其他关键词或检查拼写
                </p>
            </div>
        {:else}
            <!-- 真实列表 -->
            {#each results as result, i (result.id)}
                <EvidenceCard
                    {result}
                    index={i + 1}
                    onMouseEnter={() => onHighlight(result.id)}
                    onMouseLeave={() => onUnhighlight(result.id)}
                />
            {/each}
        {/if}
    </div>
</div>
