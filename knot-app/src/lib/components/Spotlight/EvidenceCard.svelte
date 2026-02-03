<script>
    /**
     * 证据卡片组件
     */
    let {
        result,
        highlighted = false,
        onMouseEnter = () => {},
        onMouseLeave = () => {},
    } = $props();
</script>

<div
    class="evidence-card p-3 rounded-xl bg-[var(--bg-card)] border border-[var(--border-light)] hover:bg-[var(--bg-card-hover)] cursor-pointer group transition-colors"
    class:highlighted
    id="card-{result.id}"
    onmouseenter={onMouseEnter}
    onmouseleave={onMouseLeave}
    role="button"
    tabindex="0"
>
    <div class="flex justify-between items-start mb-2">
        <div class="flex items-center gap-2 min-w-0">
            <span
                class="material-symbols-outlined text-[var(--highlight)] text-lg flex-shrink-0"
                >article</span
            >
            <span
                class="text-xs font-semibold text-[var(--text-primary)] truncate"
                >{result.title}</span
            >
        </div>
        <span
            class="text-[9px] font-bold px-2 py-0.5 rounded-md bg-[var(--accent-glow)] text-[var(--accent-primary)] border border-[var(--accent-primary)]/20"
        >
            {result.score}
        </span>
    </div>

    <p
        class="text-[11px] text-[var(--text-secondary)] leading-relaxed line-clamp-2"
    >
        {result.snippet}
    </p>

    <div
        class="mt-2 text-[9px] text-[var(--text-muted)] flex items-center gap-1 font-medium"
    >
        <span class="material-symbols-outlined text-xs">folder</span>
        <span class="truncate">{result.path}</span>
    </div>
</div>

<style>
    .line-clamp-2 {
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }
</style>
