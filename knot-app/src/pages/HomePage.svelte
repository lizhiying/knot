<script>
    /**
     * 首页 - Spotlight 界面
     */
    import SpotlightContainer from "$lib/components/Spotlight/SpotlightContainer.svelte";
    import ShortcutHint from "$lib/components/Common/ShortcutHint.svelte";
    import Onboarding from "$lib/components/Onboarding/Onboarding.svelte";
    import { onMount } from "svelte";
    import { invoke } from "@tauri-apps/api/core";

    const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

    let spotlightVisible = $state(true);
    let isSetupComplete = $state(false);
    let isChecking = $state(true);

    // 键盘快捷键处理
    function handleKeydown(e) {
        // Only allow Spotlight toggle if setup is complete
        if (!isSetupComplete) return;

        if (e.altKey && e.code === "Space") {
            e.preventDefault();
            spotlightVisible = !spotlightVisible;
        }
        // ESC 或 Cmd+W (macOS) / Ctrl+W (Windows/Linux) 关闭窗口
        if (e.key === "Escape" || ((e.metaKey || e.ctrlKey) && e.key === "w")) {
            e.preventDefault();
            spotlightVisible = false;
            // 隐藏整个窗口
            const appWindow = getCurrentWebviewWindow();
            appWindow.hide();
        }
    }

    async function checkSetup() {
        try {
            // Check Qwen (Core Chat Model)
            const exists = await invoke("check_model_status", {
                filename: "Qwen3-1.7B-Q4_K_M.gguf",
            });
            isSetupComplete = exists;
        } catch (e) {
            console.error("Setup check failed", e);
            isSetupComplete = false;
        } finally {
            isChecking = false;
        }
    }

    onMount(() => {
        checkSetup();

        window.addEventListener("keydown", handleKeydown);

        // 监听窗口 focus 事件，重新显示时恢复内容
        const appWindow = getCurrentWebviewWindow();
        const unlistenFocus = appWindow.onFocusChanged(
            ({ payload: focused }) => {
                if (focused && isSetupComplete) {
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

{#if !isChecking}
    {#if isSetupComplete}
        <SpotlightContainer visible={spotlightVisible} />
    {:else}
        <Onboarding onComplete={() => (isSetupComplete = true)} />
    {/if}
{/if}

<style>
    .home-page {
        font-family: "Inter", sans-serif;
        overflow: hidden;
    }
</style>
