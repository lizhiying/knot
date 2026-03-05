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

    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { onMount } from "svelte";

    let modelStatus = $state("loading");
    let indexedDocs = $state(0);
    let startupTimeMs = $state(null);

    // Knowledge 统计数据
    let knowledgeTotalFiles = $state(0);
    let knowledgeIndexedFiles = $state(0);

    // 当切换到 Knowledge 视图时获取文件统计
    $effect(() => {
        if (navigation.view === VIEW_KNOWLEDGE) {
            invoke("list_knowledge_files")
                .then((files) => {
                    knowledgeTotalFiles = files.length;
                    knowledgeIndexedFiles = files.filter(
                        (f) => f.index_status === "Indexed",
                    ).length;
                })
                .catch((e) => {
                    console.error("Failed to get knowledge stats:", e);
                });
        }
    });

    onMount(() => {
        // Listen for model status updates
        const unlistenModelStatus = listen("model-status", (event) => {
            console.log("Model Status Event:", event.payload);
            modelStatus = event.payload;
        });

        // Listen for indexing status and refresh doc count
        const unlistenIndexingStatus = listen("indexing-status", (event) => {
            console.log("Indexing Status Event:", event.payload);
            if (event.payload === "ready") {
                refreshDocCount();
            }
        });

        // Get initial status
        invoke("get_model_status")
            .then((status) => {
                console.log("Initial Model Status:", status);
                modelStatus = status;
            })
            .catch((e) => {
                console.error("Failed to get model status:", e);
            });

        const refreshDocCount = () => {
            invoke("get_index_status")
                .then((status) => {
                    console.log("Index Status:", status);
                    indexedDocs = status.file_count;
                    // We can also track doc_count if needed for tooltips
                })
                .catch((e) => {
                    console.error("Failed to get index status:", e);
                });
        };

        refreshDocCount();

        // Get startup time (time from app click to page ready)
        invoke("get_startup_time")
            .then((ms) => {
                console.log("Startup Time:", ms, "ms");
                startupTimeMs = ms;
            })
            .catch((e) => {
                console.error("Failed to get startup time:", e);
            });

        return () => {
            unlistenModelStatus.then((unlisten) => unlisten());
            unlistenIndexingStatus.then((unlisten) => unlisten());
        };
    });
    const formatCount = (num) => {
        if (num >= 10000) return (num / 1000).toFixed(1) + "k";
        return num.toLocaleString();
    };

    const formatStartupTime = (ms) => {
        if (ms === null) return "";
        if (ms < 1000) return `${ms}ms`;
        return `${(ms / 1000).toFixed(2)}s`;
    };
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
            <div
                class="flex items-center gap-3 text-[var(--text-secondary)] opacity-60"
            >
                <div class="flex items-center gap-1.5">
                    <span class="material-symbols-outlined text-[14px]"
                        >database</span
                    >
                    <span class="text-[10px]"
                        >{formatCount(indexedDocs)} Docs</span
                    >
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

        <!-- Startup Time -->
        {#if startupTimeMs !== null}
            <div
                class="flex items-center gap-1.5 text-[var(--text-secondary)] opacity-60"
                title="App startup time"
            >
                <span class="material-symbols-outlined text-[14px]"
                    >rocket_launch</span
                >
                <span class="text-[10px]"
                    >{formatStartupTime(startupTimeMs)}</span
                >
            </div>
        {/if}

        <!-- Divider -->
        <div class="w-px h-3 bg-[var(--border-color)] mx-2"></div>

        <!-- Knowledge Stats (before ESC) -->
        {#if navigation.view === VIEW_KNOWLEDGE}
            <div
                class="flex items-center gap-3 text-[var(--text-secondary)] opacity-60"
            >
                <div class="flex items-center gap-1.5">
                    <span
                        class="text-[10px] font-semibold text-[var(--text-primary)]"
                        >{knowledgeTotalFiles}</span
                    >
                    <span class="text-[10px]">文件</span>
                </div>
                <div class="flex items-center gap-1.5">
                    <div class="w-1.5 h-1.5 rounded-full bg-green-500"></div>
                    <span
                        class="text-[10px] font-semibold text-[var(--text-primary)]"
                        >{knowledgeIndexedFiles}</span
                    >
                    <span class="text-[10px]">已索引</span>
                </div>
            </div>
            <div class="w-px h-3 bg-[var(--border-color)] mx-2"></div>
        {/if}

        <!-- Common shortcuts -->
        <div class="flex items-center gap-3 opacity-60">
            <div class="flex items-center gap-1">
                <kbd
                    class="px-1.5 py-0.5 rounded bg-[var(--bg-card)] border border-[var(--border-color)] font-mono text-[10px] text-[var(--text-secondary)]"
                    >ESC</kbd
                >
                <span class="text-[10px] text-[var(--text-secondary)]"
                    >Close</span
                >
            </div>

            <!-- Model Status -->
            <div
                class="flex items-center gap-1.5"
                title={modelStatus === "loading"
                    ? "Model Loading..."
                    : "Model Ready"}
            >
                {#if modelStatus === "loading"}
                    <!-- Loading Spinner -->
                    <span
                        class="material-symbols-outlined text-[14px] animate-spin text-[var(--text-secondary)]"
                        >sync</span
                    >
                {:else if modelStatus === "ready"}
                    <!-- Green Check -->
                    <span
                        class="material-symbols-outlined text-[14px] text-green-500"
                        >check_circle</span
                    >
                {:else if modelStatus === "error"}
                    <span
                        class="material-symbols-outlined text-[14px] text-red-500"
                        >error</span
                    >
                {/if}
            </div>
        </div>
    </div>
</div>
