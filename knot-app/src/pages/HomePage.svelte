<script>
    /**
     * 首页 - Spotlight 界面
     */
    import SpotlightContainer from "$lib/components/Spotlight/SpotlightContainer.svelte";
    import ShortcutHint from "$lib/components/Common/ShortcutHint.svelte";
    import { onMount } from "svelte";

    const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

    let spotlightVisible = $state(true);

    // 键盘快捷键处理
    function handleKeydown(e) {
        if (e.altKey && e.code === "Space") {
            e.preventDefault();
            spotlightVisible = !spotlightVisible;
        }
        if (e.key === "Escape") {
            spotlightVisible = false;
            // 隐藏整个窗口
            const appWindow = getCurrentWebviewWindow();
            appWindow.hide();
        }
    }

    onMount(() => {
        window.addEventListener("keydown", handleKeydown);

        // 监听窗口 focus 事件，重新显示时恢复内容
        const appWindow = getCurrentWebviewWindow();
        const unlistenFocus = appWindow.onFocusChanged(
            ({ payload: focused }) => {
                if (focused) {
                    spotlightVisible = true;
                }
            },
        );

        return () => {
            window.removeEventListener("keydown", handleKeydown);
            unlistenFocus.then((fn) => fn());
        };
    });
</script>

<SpotlightContainer visible={spotlightVisible} />

<style>
    .home-page {
        font-family: "Inter", sans-serif;
        overflow: hidden;
    }
</style>
