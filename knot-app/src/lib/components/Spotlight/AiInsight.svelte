<script>
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
</script>

<div class="w-[62%] flex flex-col">
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
            <div class="markdown-content text-[14px]">
                {@html content}
            </div>
        {/if}

        <!-- 打字光标 -->
        {#if showCursor}
            <span class="thinking-cursor"></span>
        {/if}
    </div>
</div>
