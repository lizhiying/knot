<script>
    /**
     * 主应用组件
     * 仅渲染 HomePage (包含 SpotlightContainer)
     */
    import HomePage from "./pages/HomePage.svelte";
    import { theme } from "$lib/stores/theme.svelte.js";
    import { onMount } from "svelte";

    onMount(() => {
        // 初始化主题
        theme.init();
    });
</script>

<HomePage />
