<script>
    /**
     * 证据卡片组件
     */
    import { slide } from "svelte/transition";

    let {
        result,
        index = 1,
        highlighted = false,
        onMouseEnter = () => {},
        onMouseLeave = () => {},
    } = $props();
    let isExpanded = $state(false);

    function toggleExpand(e) {
        // Prevent event bubbling if clicking nested buttons (though we don't have many)
        // But we want the whole card clickable.
        isExpanded = !isExpanded;
    }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
<div
    class="evidence-card p-3 rounded-xl bg-[var(--bg-card)] border border-[var(--border-light)] hover:border-[var(--accent-primary)] hover:bg-[var(--bg-card-hover)] cursor-pointer group transition-all duration-300 select-none items-start focus:outline-none outline-none"
    class:highlighted
    class:shadow-sm={isExpanded}
    id="card-{result.id}"
    onmouseenter={onMouseEnter}
    onmouseleave={onMouseLeave}
    onclick={toggleExpand}
    role="button"
    tabindex="0"
>
    <!-- Header: Title + Score + Chevron -->
    <div class="flex justify-between items-start mb-2">
        <div class="flex items-center gap-2 min-w-0 flex-1">
            <div
                class="w-5 h-5 rounded bg-[var(--bg-secondary)] text-[var(--text-primary)] group-hover:bg-[var(--accent-primary)] group-hover:text-white flex items-center justify-center font-bold text-xs flex-shrink-0 shadow-sm transition-colors duration-300"
            >
                {index}
            </div>

            <span
                class="text-sm font-semibold text-[var(--text-primary)] group-hover:text-[var(--accent-primary)] truncate transition-colors duration-300"
                >{result.title}</span
            >
        </div>

        <div class="flex items-center gap-2">
            <span
                class="text-[10px] font-bold px-1.5 py-0.5 rounded-md text-[var(--accent-primary)] border border-[var(--accent-primary)]/20 shadow-sm bg-[var(--accent-glow)]"
            >
                {result.score}
            </span>
            <button
                class="text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-transform duration-300"
                style="transform: rotate({isExpanded ? '180deg' : '0deg'})"
                aria-label="Toggle Details"
            >
                <span class="material-symbols-outlined text-lg"
                    >expand_more</span
                >
            </button>
        </div>
    </div>

    <!-- Content Area -->
    <div class="text-[11px] text-[var(--text-secondary)] leading-relaxed">
        {#if isExpanded}
            <!-- Expanded View with Slide Animation -->
            <div transition:slide={{ duration: 300, axis: "y" }}>
                <!-- Source Section Info -->
                <div class="flex items-center gap-2 mb-2 mt-2">
                    <span
                        class="px-2 py-0.5 rounded bg-[var(--bg-secondary)] border border-[var(--border-light)] text-[10px] font-mono text-[var(--text-primary)] flex items-center gap-1.5"
                    >
                        <span class="material-symbols-outlined text-[10px]"
                            >menu_book</span
                        >
                        SOURCE: {result.source_section ||
                            "Page ? • Unknown Section"}
                    </span>
                </div>

                <!-- Quote Block -->
                <div
                    class="pl-3 py-2 pr-2 border-l-2 border-[var(--highlight)] group-hover:border-[var(--accent-primary)] bg-[var(--highlight)]/5 group-hover:bg-[var(--accent-primary)]/5 rounded-r-md italic text-[var(--text-primary)] transition-colors duration-300"
                >
                    "{result.quote || result.snippet}"
                </div>
            </div>
        {:else}
            <!-- Collapsed View: Snippet -->
            <p class="line-clamp-2">
                {result.snippet}
            </p>
        {/if}
    </div>

    <!-- Footer: Breadcrumbs -->
    <div
        class="mt-3 text-[9px] text-[var(--text-muted)] flex items-center gap-1.5 font-medium opacity-80"
    >
        <span class="material-symbols-outlined text-[12px]">folder</span>
        <span class="truncate">{result.path || "Unknown Path"}</span>
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
