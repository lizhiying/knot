<script>
    /**
     * 证据面板组件
     */
    import EvidenceCard from "./EvidenceCard.svelte";

    let {
        results = [],
        matchText = "0 hits",
        onHighlight = () => {},
        onUnhighlight = () => {},
    } = $props();
</script>

<div
    class="w-[38%] min-w-[200px] flex flex-col border-r border-[var(--border-color)]"
>
    <!-- 面板头部 -->
    <div class="px-5 py-2 mt-2 flex justify-between items-center">
        <span
            class="text-[10px] font-bold text-[var(--text-muted)] uppercase tracking-widest"
            >Hybrid Evidence</span
        >
        <span
            class="text-[10px] font-medium text-[var(--text-secondary)] px-2 py-0.5 rounded-full border border-[var(--border-color)]"
        >
            {matchText}
        </span>
    </div>

    <!-- 证据列表 -->
    <div class="flex-1 overflow-y-auto p-4 space-y-3 scroll-hide">
        {#each results as result (result.id)}
            <EvidenceCard
                {result}
                onMouseEnter={() => onHighlight(result.id)}
                onMouseLeave={() => onUnhighlight(result.id)}
            />
        {/each}
    </div>
</div>
