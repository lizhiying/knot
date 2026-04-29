<script>
    import { marked } from "marked";
    import { open } from "@tauri-apps/plugin-shell";
    import {
        completeStreamingTable,
        wrapTablesForScroll,
        sortableTables,
    } from "$lib/utils/markdown.js";

    /**
     * AI 洞察区域组件
     */
    let {
        status = "Ready",
        statusType = "ready",
        isThinking = false,
        content = "",
        showCursor = false,
        sqlPagination = null,
        onGoToPage = () => {},
        candidateTables = null,
        onSelectTable = () => {},
    } = $props();

    // 状态点颜色
    let statusDotClass = $derived(
        (() => {
            switch (statusType) {
                case "analyzing":
                    return "bg-blue-500 animate-pulse";
                case "complete":
                    return "bg-green-500";
                default:
                    return "bg-blue-500";
            }
        })(),
    );

    // Render Markdown
    let htmlContent = $state("");

    // Handle link clicks
    async function handleLinkClick(event) {
        const target = event.target.closest("a");
        if (target && target.href) {
            event.preventDefault();
            try {
                await open(target.href);
            } catch (error) {
                console.error("Failed to open link:", error);
            }
        }
    }

    $effect(() => {
        if (content) {
            const completed = completeStreamingTable(content);
            Promise.resolve(marked.parse(completed)).then((res) => {
                htmlContent = wrapTablesForScroll(res);
            });
        } else {
            htmlContent = "";
        }
    });
</script>

<div class="w-[62%] flex flex-col h-full min-h-0 overflow-x-hidden">
    <div class="p-6 overflow-y-auto overflow-x-hidden scroll-hide flex-1">
        <!-- 状态指示 -->
        <div
            class="flex items-center gap-2 text-[10px] font-bold mb-6 uppercase tracking-widest"
        >
            <span class="w-2 h-2 rounded-full {statusDotClass}"></span>
            <span>{status}</span>
        </div>

        <!-- 思考骨架屏 -->
        {#if isThinking}
            <div class="space-y-5">
                <div class="h-4 w-4/5 skeleton rounded-lg opacity-40"></div>
                <div class="h-4 w-full skeleton rounded-lg opacity-40"></div>
                <div class="h-4 w-11/12 skeleton rounded-lg opacity-40"></div>
                <div class="h-4 w-3/4 skeleton rounded-lg opacity-40"></div>
            </div>
        {/if}

        <!-- 洞察内容 -->
        {#if !isThinking && content}
            <div
                class="markdown-content text-[14px] prose prose-sm dark:prose-invert max-w-none"
                style="overflow-wrap: break-word; word-break: break-word;"
                onclick={handleLinkClick}
                role="presentation"
                use:sortableTables
            >
                {@html htmlContent}
            </div>
        {/if}

        <!-- 候选表选择器 (多表拦截) -->
        {#if candidateTables && candidateTables.length > 0}
            <div class="mt-4 p-4 rounded-xl border border-[var(--border-color)] bg-[var(--bg-card)] shadow-sm">
                <div class="text-sm font-semibold text-[var(--text-primary)] mb-3 flex items-center gap-2">
                    <span class="material-symbols-outlined text-[18px] text-amber-500">alt_route</span>
                    找到了多个相关表格，请明确选择一个：
                </div>
                <div class="flex flex-col gap-2">
                    {#each candidateTables as table}
                        <button 
                            class="flex items-center justify-between p-3 rounded-lg border border-[var(--border-color)] hover:border-[var(--accent-primary)] hover:bg-[var(--bg-hover)] transition-all text-left group"
                            onclick={() => onSelectTable(table.file_path)}
                        >
                            <div class="flex items-center gap-3 overflow-hidden">
                                <span class="material-symbols-outlined tracking-wider text-[#10b981] opacity-90">table_chart</span>
                                <span class="text-sm flex-1 truncate">{table.file_name}</span>
                            </div>
                            <span class="material-symbols-outlined text-[16px] opacity-0 group-hover:opacity-100 transition-opacity text-[var(--accent-primary)] transform group-hover:translate-x-1 duration-200">arrow_forward</span>
                        </button>
                    {/each}
                </div>
            </div>
        {/if}

        <!-- SQL 分页控件 -->
        {#if sqlPagination && sqlPagination.totalPages > 1}
            <div
                class="flex items-center justify-center gap-3 mt-4 py-3 border-t border-[var(--border-color)]"
            >
                <button
                    class="px-2 py-1 rounded-lg border border-[var(--border-color)] bg-[var(--bg-card)] hover:bg-[var(--bg-hover)] disabled:opacity-30 disabled:cursor-not-allowed transition-colors text-[var(--text-secondary)]"
                    disabled={sqlPagination.currentPage <= 1}
                    onclick={() => onGoToPage(sqlPagination.currentPage - 1)}
                >
                    <span class="material-symbols-outlined text-[16px]"
                        >chevron_left</span
                    >
                </button>
                <span class="text-xs text-[var(--text-secondary)]">
                    第 {sqlPagination.currentPage} / {sqlPagination.totalPages} 页
                    <span class="opacity-60"
                        >（共 {sqlPagination.totalRows} 行）</span
                    >
                </span>
                <button
                    class="px-2 py-1 rounded-lg border border-[var(--border-color)] bg-[var(--bg-card)] hover:bg-[var(--bg-hover)] disabled:opacity-30 disabled:cursor-not-allowed transition-colors text-[var(--text-secondary)]"
                    disabled={sqlPagination.currentPage >=
                        sqlPagination.totalPages}
                    onclick={() => onGoToPage(sqlPagination.currentPage + 1)}
                >
                    <span class="material-symbols-outlined text-[16px]"
                        >chevron_right</span
                    >
                </button>
            </div>
        {/if}

        <!-- 打字光标 -->
        {#if showCursor}
            <span class="thinking-cursor"></span>
        {/if}
    </div>

    <!-- Follow-up Input -->
    {#if !isThinking && content}
        <div
            class="p-4 border-t border-[var(--border-color)] bg-[var(--bg-primary)]/50 backdrop-blur-sm"
        >
            <div class="relative flex items-center">
                <span
                    class="absolute left-3 material-symbols-outlined text-[18px] opacity-40"
                    >chat_bubble</span
                >
                <input
                    type="text"
                    placeholder="Ask a follow-up..."
                    class="w-full bg-[var(--bg-card)] border border-[var(--border-color)] rounded-xl py-2.5 pl-10 pr-4 text-sm focus:outline-none focus:border-[var(--accent-primary)] transition-colors opacity-80 hover:opacity-100"
                    onkeydown={(e) => {
                        if (e.key === "Enter") {
                            // TODO: Emit follow-up event
                            console.log("Follow up:", e.target.value);
                            e.target.value = "";
                        }
                    }}
                />
            </div>
        </div>
    {/if}
</div>
