<script>
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { onMount } from "svelte";

    let region = $state("Detecting...");
    let isDownloading = $state(false);
    let currentFile = $state("");
    let downloadProgress = $state(0);

    // Model Status State
    let models = $state([
        {
            name: "GLM-OCR-Q8_0.gguf",
            label: "OCR Model (GLM-OCR)",
            size: "950MB",
            exists: false,
        },
        {
            name: "mmproj-GLM-OCR-Q8_0.gguf",
            label: "OCR Vision (Projector)",
            size: "484MB",
            exists: false,
        },
        {
            name: "Qwen3.5-4B-Q4_K_M.gguf",
            label: "Chat Logic (Qwen3.5)",
            size: "3.1GB",
            exists: false,
        },
        {
            name: "ppocrv5/det.onnx",
            label: "PDF OCR Detection",
            size: "5MB",
            exists: false,
        },
        {
            name: "ppocrv5/rec.onnx",
            label: "PDF OCR Recognition",
            size: "14MB",
            exists: false,
        },
        {
            name: "ppocrv5/ppocrv5_dict.txt",
            label: "PDF OCR Dictionary",
            size: "200KB",
            exists: false,
        },
    ]);

    async function checkStatus() {
        try {
            region = await invoke("get_detected_region");

            // Check each model
            for (let i = 0; i < models.length; i++) {
                models[i].exists = await invoke("check_model_status", {
                    filename: models[i].name,
                });
            }
        } catch (e) {
            console.error("Failed to check status:", e);
        }
    }

    async function startQueue() {
        isDownloading = true;
        currentFile = "Initializing...";
        downloadProgress = 0;
        try {
            // Start Queue
            await invoke("start_download_queue", { region: null }); // Use auto-detected or current
        } catch (e) {
            console.error(e);
            alert("Queue failed: " + e);
            isDownloading = false;
        }
    }

    onMount(() => {
        checkStatus();

        const unlistenProgress = listen("download-progress", (event) => {
            currentFile = event.payload.filename;
            downloadProgress = event.payload.percentage;
        });

        const unlistenQueueStatus = listen("queue-status", (event) => {
            console.log("Queue Status:", event.payload);
            // event.payload is string message "Starting filename..."
        });

        const unlistenQueueItemDone = listen("queue-item-complete", (event) => {
            console.log("Item Done:", event.payload);
            // Update model existence immediately
            const idx = models.findIndex((m) => m.name === event.payload);
            if (idx !== -1) models[idx].exists = true;
        });

        const unlistenQueueFinished = listen("queue-finished", () => {
            isDownloading = false;
            currentFile = "";
            checkStatus();
        });

        const unlistenError = listen("download-error", (event) => {
            alert(event.payload);
            isDownloading = false;
        });

        return () => {
            unlistenProgress.then((u) => u());
            unlistenQueueStatus.then((u) => u());
            unlistenQueueItemDone.then((u) => u());
            unlistenQueueFinished.then((u) => u());
            unlistenError.then((u) => u());
        };
    });
</script>

<div class="mb-8">
    <h3 class="text-base font-semibold mb-4 pb-1">Model Management</h3>

    <div
        class="bg-[var(--bg-secondary)] rounded-xl border border-[var(--border-color)] p-6 mb-6"
    >
        <!-- Region Info -->
        <div
            class="flex items-center justify-between mb-4 border-b border-[var(--border-color)] pb-4"
        >
            <div>
                <h3 class="font-medium text-sm text-[var(--text-primary)]">
                    Download Source
                </h3>
                <p class="text-[var(--text-secondary)] text-xs mt-1">
                    Current detecting region: <span class="font-mono"
                        >{region}</span
                    >
                </p>
            </div>
            <div>
                {#if isDownloading}
                    <button
                        class="px-3 py-1 text-xs rounded border border-[var(--border-color)] text-[var(--text-secondary)]"
                        disabled
                    >
                        Downloading...
                    </button>
                {:else}
                    <button
                        class="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-[var(--accent-primary)] text-white hover:brightness-110 shadow-sm disabled:opacity-50"
                        onclick={startQueue}
                        disabled={models.every((m) => m.exists)}
                    >
                        {models.every((m) => m.exists)
                            ? "All Installed"
                            : "Download All Models"}
                    </button>
                {/if}
            </div>
        </div>

        <!-- Models List -->
        <div class="flex flex-col gap-3">
            {#each models as model}
                <div
                    class="flex items-center justify-between p-3 bg-[var(--bg-card)] rounded-lg border border-[var(--border-color)]"
                >
                    <div class="flex flex-col">
                        <span
                            class="text-sm font-medium text-[var(--text-primary)]"
                            >{model.label}</span
                        >
                        <span
                            class="text-[10px] text-[var(--text-secondary)] font-mono"
                            >{model.name} • {model.size}</span
                        >
                    </div>
                    <div class="flex items-center">
                        {#if model.exists}
                            <span
                                class="text-xs text-green-500 font-medium flex items-center gap-1"
                            >
                                <span
                                    class="material-symbols-outlined text-[14px]"
                                    >check_circle</span
                                >
                                Installed
                            </span>
                        {:else}
                            <span
                                class="text-xs text-[var(--text-secondary)] flex items-center gap-1"
                            >
                                <span
                                    class="material-symbols-outlined text-[14px]"
                                    >cloud_off</span
                                >
                                Missing
                            </span>
                        {/if}
                    </div>
                </div>
            {/each}
        </div>

        <!-- Progress Bar -->
        {#if isDownloading}
            <div
                class="mt-4 pt-4 border-t border-dashed border-[var(--border-color)]"
            >
                <div class="flex justify-between text-xs mb-1">
                    <span class="text-[var(--text-primary)]"
                        >Downloading: {currentFile}</span
                    >
                    <span class="text-[var(--text-secondary)]"
                        >{downloadProgress.toFixed(1)}%</span
                    >
                </div>
                <div
                    class="h-1.5 w-full bg-[var(--bg-card)] rounded-full overflow-hidden"
                >
                    <div
                        class="h-full bg-[var(--accent-primary)] transition-all duration-300"
                        style="width: {downloadProgress}%"
                    ></div>
                </div>
            </div>
        {/if}
    </div>
</div>
