<script>
    /**
     * Spotlight 底部组件
     */
    import {
        navigation,
        VIEW_SEARCH,
        VIEW_DOC_PARSER,
        VIEW_KNOWLEDGE,
        VIEW_SETTINGS,
    } from "$lib/stores/navigation.svelte.js";

    let {
        docCount = "12.4k Docs",
        ragActive = true,
        hasTopBorder = true,
        onNavigate = () => {},
    } = $props();
</script>

<div
    class="px-6 py-2 flex justify-between items-center bg-[var(--bg-primary)] text-[11px] font-medium {hasTopBorder
        ? 'border-t border-[var(--border-color)]'
        : ''}"
>
    <!-- Left Navigation Menu -->
    <div class="flex items-center gap-1">
        <button
            class="px-3 py-1.5 rounded-md transition-colors {navigation.view ===
            VIEW_SEARCH
                ? 'bg-[var(--bg-card)] text-[var(--accent-primary)] shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-card-hover)]'}"
            onmousedown={(e) => {
                e.preventDefault();
                navigation.setActiveView(VIEW_SEARCH);
                onNavigate();
            }}
        >
            Search
        </button>
        <button
            class="px-3 py-1.5 rounded-md transition-colors {navigation.view ===
            VIEW_DOC_PARSER
                ? 'bg-[var(--bg-card)] text-[var(--accent-primary)] shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-card-hover)]'}"
            onmousedown={(e) => {
                e.preventDefault();
                navigation.setActiveView(VIEW_DOC_PARSER);
                onNavigate();
            }}
        >
            Doc Parser Demo
        </button>
        <button
            class="px-3 py-1.5 rounded-md transition-colors {navigation.view ===
            VIEW_KNOWLEDGE
                ? 'bg-[var(--bg-card)] text-[var(--accent-primary)] shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-card-hover)]'}"
            onmousedown={(e) => {
                e.preventDefault();
                navigation.setActiveView(VIEW_KNOWLEDGE);
                onNavigate();
            }}
        >
            Knowledges
        </button>
        <button
            class="px-3 py-1.5 rounded-md transition-colors {navigation.view ===
            VIEW_SETTINGS
                ? 'bg-[var(--bg-card)] text-[var(--accent-primary)] shadow-sm'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-card-hover)] '}"
            onmousedown={(e) => {
                e.preventDefault();
                navigation.setActiveView(VIEW_SETTINGS);
                onNavigate();
            }}
        >
            Settings
        </button>
    </div>

    <!-- Right Side: Context Aware Status -->
    <div class="flex items-center gap-4">
        {#if navigation.view === VIEW_SEARCH}
            <div class="flex items-center gap-3 text-[var(--text-secondary)]">
                <div class="flex items-center gap-1.5">
                    <span class="material-symbols-outlined text-[16px]"
                        >database</span
                    >
                    <span>{docCount}</span>
                </div>
                <div class="flex items-center gap-1.5">
                    <span class="material-symbols-outlined text-[16px]"
                        >bolt</span
                    >
                    <span>{ragActive ? "RAG Active" : "RAG Inactive"}</span>
                </div>
            </div>
        {:else if navigation.view === VIEW_DOC_PARSER}
            <div class="flex items-center gap-3 text-[var(--text-secondary)]">
                <span class="flex items-center gap-1.5">
                    <div
                        class="w-1.5 h-1.5 rounded-full bg-[var(--accent-primary)]"
                    ></div>
                    Ready
                </span>
            </div>
        {/if}

        <!-- Divider -->
        <div class="w-px h-3 bg-[var(--border-color)] mx-2"></div>

        <!-- Common shortcuts -->
        <div class="flex items-center gap-3 opacity-60">
            {#if navigation.view === VIEW_SEARCH}
                <div class="flex items-center gap-1">
                    <kbd
                        class="px-1.5 py-0.5 rounded bg-[var(--bg-card)] border border-[var(--border-color)] font-mono text-[10px] text-[var(--text-secondary)]"
                        >⏎</kbd
                    >
                    <span class="text-[10px] text-[var(--text-secondary)]"
                        >Search</span
                    >
                </div>
            {/if}
            <div class="flex items-center gap-1">
                <kbd
                    class="px-1.5 py-0.5 rounded bg-[var(--bg-card)] border border-[var(--border-color)] font-mono text-[10px] text-[var(--text-secondary)]"
                    >ESC</kbd
                >
                <span class="text-[10px] text-[var(--text-secondary)]"
                    >Close</span
                >
            </div>
        </div>
    </div>
</div>
