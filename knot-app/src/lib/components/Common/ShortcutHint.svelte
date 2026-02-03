<script>
    /**
     * 全局快捷键提示组件
     */
    let { shortcut = "Alt + Space", action = "to toggle Spotlight" } = $props();
</script>

<div
    class="fixed bottom-6 left-1/2 -translate-x-1/2 text-zinc-500 text-xs opacity-60 flex items-center gap-3"
>
    <span>
        Press
        <kbd
            class="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 font-mono text-zinc-300"
        >
            {shortcut}
        </kbd>
        {action}
    </span>
</div>
