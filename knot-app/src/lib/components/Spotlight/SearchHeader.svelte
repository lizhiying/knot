<script>
    /**
     * 搜索头部组件
     */
    import { onMount } from "svelte";
    import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
    import appIcon from "../../../app-icon-grey.png";

    let {
        isLoading = false,
        iconName = "search",
        placeholder = "What can I help you find today?",
        onSearch = () => {},
        value = $bindable(""),
    } = $props();

    // let inputValue = $state(""); // Removed local state
    let inputRef = $state(null);

    function handleKeydown(e) {
        if (e.key === "Enter" && value.trim().length > 0) {
            onSearch(value);
        }
    }

    // 聚焦输入框
    function focusInput() {
        if (inputRef) {
            inputRef.focus();
        }
    }

    onMount(() => {
        // 组件挂载后自动聚焦
        focusInput();

        let unlistenFocus;

        // 监听窗口 focus 事件，窗口重新获得焦点时自动聚焦输入框
        const appWindow = getCurrentWebviewWindow();
        appWindow
            .onFocusChanged(({ payload: focused }) => {
                if (focused) {
                    focusInput();
                }
            })
            .then((unlisten) => {
                unlistenFocus = unlisten;
            });

        return () => {
            if (unlistenFocus) unlistenFocus();
        };
    });
</script>

<div
    class="flex items-center pl-24 pr-6 py-4 border-b border-[var(--border-color)] bg-[var(--bg-primary)] group"
    data-tauri-drag-region
>
    <!-- 搜索图标/加载动画 -->
    <div class="flex items-center justify-center mr-2">
        {#if isLoading}
            <svg
                class="animate-spin h-7 w-7 text-[var(--accent-primary)]"
                fill="none"
                viewBox="0 0 24 24"
            >
                <circle
                    class="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="4"
                ></circle>
                <path
                    class="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                ></path>
            </svg>
        {:else}
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                class="lucide lucide-search text-[var(--text-muted)] group-focus-within:text-[var(--accent-primary)] transition-colors"
                aria-hidden="true"
                ><path d="m21 21-4.34-4.34"></path><circle cx="11" cy="11" r="8"
                ></circle></svg
            >
        {/if}
    </div>

    <!-- 搜索输入框 (Auto Width) -->
    <div
        class="relative grid items-center ml-2"
        style="min-width: 70px; max-width: 600px;"
    >
        <span
            class="invisible row-start-1 col-start-1 whitespace-pre text-2xl font-light tracking-tight px-0"
            aria-hidden="true">{value || placeholder}</span
        >
        <input
            bind:this={inputRef}
            bind:value
            type="text"
            class="row-start-1 col-start-1 w-full bg-transparent border-none outline-none text-2xl text-[var(--text-primary)] placeholder-[var(--text-muted)] font-light tracking-tight px-0"
            {placeholder}
            autocomplete="off"
            spellcheck="false"
            onkeydown={handleKeydown}
        />
    </div>

    <!-- Drag Spacer -->
    <div class="flex-1 self-stretch" data-tauri-drag-region></div>

    <!-- App Icon -->
    <div
        class="flex items-center justify-center pl-4 opacity-70"
        data-tauri-drag-region
    >
        <img src={appIcon} alt="App Icon" class="w-6 h-6 object-contain" />
    </div>
</div>
