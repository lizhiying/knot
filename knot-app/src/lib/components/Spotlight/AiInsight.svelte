<script>
    import { marked } from "marked";
    import { open } from "@tauri-apps/plugin-shell";

    /**
     * AI 洞察区域组件
     */
    let {
        status = "Ready",
        statusType = "ready",
        isThinking = false,
        content = "",
        showCursor = false,
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
            Promise.resolve(marked.parse(content)).then((res) => {
                htmlContent = res;
            });
        } else {
            htmlContent = "";
        }
    });
</script>

<div class="w-[62%] flex flex-col h-full min-h-0">
    <div class="p-6 overflow-y-auto scroll-hide flex-1">
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
                onclick={handleLinkClick}
                role="presentation"
            >
                {@html htmlContent}
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
