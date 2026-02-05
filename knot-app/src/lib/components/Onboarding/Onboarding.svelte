<script>
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { onMount } from "svelte";

    let { onComplete = () => {} } = $props();

    let isDownloading = $state(false);
    let downloadProgress = $state(0);
    let currentFile = $state("");
    let statusMessage = $state("Checking system requirements...");

    // Core model that is required
    const CORE_MODEL = "Qwen3-1.7B-Q4_K_M.gguf";

    async function startDownload() {
        isDownloading = true;
        statusMessage = "Initializing download...";
        downloadProgress = 0;

        try {
            // Start download queue
            await invoke("start_download_queue", { region: null });
        } catch (e) {
            console.error(e);
            statusMessage = "Download failed: " + e;
            isDownloading = false;
        }
    }

    import { LogicalSize } from "@tauri-apps/api/dpi";

    onMount(async () => {
        // Resize window for onboarding
        const win = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
        await win.setSize(new LogicalSize(896, 600));

        const unlistenProgress = listen("download-progress", (event) => {
            currentFile = event.payload.filename;
            downloadProgress = event.payload.percentage;
            statusMessage = `Downloading ${event.payload.filename}...`;
        });

        const unlistenQueueFinished = listen("queue-finished", () => {
            isDownloading = false;
            statusMessage = "Setup complete!";
            // Double check if model exists now
            invoke("check_model_status", { filename: CORE_MODEL }).then(
                (exists) => {
                    if (exists) {
                        try {
                            const win =
                                window.__TAURI__.webviewWindow.getCurrentWebviewWindow();
                            win.setSize(new LogicalSize(896, 153)).then(() => {
                                onComplete();
                            });
                        } catch (e) {
                            onComplete();
                        }
                    } else {
                        statusMessage =
                            "Verification failed. Please try again.";
                    }
                },
            );
        });

        const unlistenError = listen("download-error", (event) => {
            statusMessage = "Error: " + event.payload;
            isDownloading = false;
        });

        return () => {
            unlistenProgress.then((u) => u());
            unlistenQueueFinished.then((u) => u());
            unlistenError.then((u) => u());
        };
    });
</script>

<div
    class="fixed inset-0 h-screen w-screen bg-[#FAFAFA] dark:bg-[#1c1c1e] text-[var(--text-primary)] select-none overflow-hidden font-sans flex flex-col items-center justify-start select-none"
    style="border-radius: 12px;"
>
    <!-- Window Border (Separate layer to ensure visibility) -->
    <div
        class="absolute inset-0 rounded-[12px] border border-black/5 dark:border-white/10 pointer-events-none z-50"
    ></div>

    <!-- Drag Region -->
    <div
        data-tauri-drag-region
        class="absolute top-0 left-0 right-0 h-14 z-50 cursor-default"
    ></div>

    <!-- Background Decor -->
    <div
        class="absolute inset-0 overflow-hidden opacity-40 pointer-events-none"
    >
        <div
            class="absolute -top-[20%] -left-[10%] w-[50%] h-[50%] bg-blue-500/10 dark:bg-blue-500/5 blur-[100px] rounded-full"
        ></div>
        <div
            class="absolute top-[30%] -right-[10%] w-[40%] h-[40%] bg-purple-500/10 dark:bg-purple-500/5 blur-[100px] rounded-full"
        ></div>
    </div>

    <!-- Content Card -->
    <div
        class="relative z-10 w-full max-w-xl px-8 text-center flex flex-col items-center justify-start h-full pt-20"
    >
        <!-- Logo/Icon -->
        <div class="mb-8 relative">
            <div
                class="w-24 h-24 rounded-[22px] bg-white dark:bg-[#27272a] shadow-2xl relative z-10 flex items-center justify-center overflow-hidden"
            >
                <img
                    src="/app-icon.png"
                    alt="Knot"
                    class="w-full h-full object-cover"
                />
            </div>
            <!-- Glow behind logo -->
            <div
                class="absolute inset-0 bg-[#6366f1] blur-3xl opacity-20 dark:opacity-40 transform scale-125 z-0"
            ></div>
        </div>

        <h1
            class="text-3xl font-bold tracking-tight mb-3 text-slate-900 dark:text-[#f4f4f5]"
        >
            Welcome to Knot
        </h1>
        <p
            class="text-[15px] text-slate-500 dark:text-[#a1a1aa] mb-10 max-w-sm leading-relaxed"
        >
            Your private, local-first AI workspace. <br />
            Let's get your inference engine ready.
        </p>

        <!-- Status Card -->
        <div
            class="w-full bg-white dark:bg-[#27272a] border border-slate-200 dark:border-[#3f3f46] rounded-xl p-5 shadow-sm text-left relative overflow-hidden backdrop-blur-sm"
        >
            <div class="flex items-center gap-4 mb-5">
                <div
                    class="p-2.5 rounded-lg bg-indigo-50 dark:bg-[#312e81]/30 text-indigo-600 dark:text-[#818cf8]"
                >
                    <span class="material-symbols-outlined text-[24px]"
                        >download_for_offline</span
                    >
                </div>
                <div>
                    <div
                        class="font-medium text-sm text-slate-900 dark:text-[#f4f4f5]"
                    >
                        Retrieval Engine & Models
                    </div>
                    <div
                        class="text-xs text-slate-500 dark:text-[#a1a1aa] mt-0.5"
                    >
                        Required (~4.5 GB) • On-Device Storage
                    </div>
                </div>
            </div>

            {#if isDownloading}
                <div class="space-y-3">
                    <div
                        class="flex justify-between text-xs font-medium text-slate-600 dark:text-[#d4d4d8]"
                    >
                        <span class="truncate pr-4">{statusMessage}</span>
                        <span class="font-mono"
                            >{downloadProgress.toFixed(0)}%</span
                        >
                    </div>

                    <!-- Progress Bar -->
                    <div
                        class="h-1.5 w-full bg-slate-100 dark:bg-[#3f3f46] rounded-full overflow-hidden"
                    >
                        <div
                            class="h-full bg-indigo-600 dark:bg-[#6366f1] transition-all duration-300 ease-out relative"
                            style="width: {downloadProgress}%"
                        >
                            <!-- Shimmer effect -->
                            <div
                                class="absolute inset-0 bg-gradient-to-r from-transparent via-white/20 to-transparent w-full -translate-x-full animate-[shimmer_1.5s_infinite]"
                            ></div>
                        </div>
                    </div>
                </div>
            {:else}
                <button
                    class="w-full py-3 rounded-lg font-medium bg-slate-900 dark:bg-[#f4f4f5] text-white dark:text-black hover:opacity-90 transition-all shadow-md active:scale-[0.99] flex items-center justify-center gap-2"
                    onclick={startDownload}
                >
                    <span>Start Installation</span>
                    <span class="material-symbols-outlined text-lg"
                        >arrow_forward</span
                    >
                </button>
            {/if}
        </div>

        <div
            class="mt-8 flex items-center gap-2 text-[10px] text-slate-400 dark:text-[#71717a] font-medium uppercase tracking-wider"
        >
            <span class="material-symbols-outlined text-sm">lock</span>
            <span>100% Local Processing</span>
            <span class="mx-1">•</span>
            <span>No Cloud Sync</span>
        </div>
    </div>
</div>

<style>
    :global(body) {
        background: transparent !important;
    }

    @keyframes shimmer {
        100% {
            transform: translateX(100%);
        }
    }
</style>
