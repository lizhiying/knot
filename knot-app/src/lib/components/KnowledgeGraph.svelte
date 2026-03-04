<script>
    import { invoke } from "@tauri-apps/api/core";
    import { onMount, onDestroy } from "svelte";

    let canvas;
    let ctx;
    let nodes = $state([]);
    let edges = $state([]);
    let loading = $state(true);
    let error = $state(null);
    let hoveredNode = $state(null);
    let selectedNode = $state(null);
    let animFrame;
    let dragging = null;
    let offsetX = 0;
    let offsetY = 0;
    let scale = 1;

    // 颜色映射
    const typeColors = {
        Person: "#f472b6",
        Organization: "#60a5fa",
        Technology: "#34d399",
        Concept: "#fbbf24",
    };

    async function loadGraphData() {
        loading = true;
        error = null;
        try {
            const data = await invoke("get_graph_data");
            // 初始化节点位置
            const w = canvas?.width || 800;
            const h = canvas?.height || 600;
            nodes = (data.nodes || []).map((n, i) => ({
                ...n,
                x: w / 2 + (Math.random() - 0.5) * w * 0.6,
                y: h / 2 + (Math.random() - 0.5) * h * 0.6,
                vx: 0,
                vy: 0,
                radius: Math.max(8, Math.min(24, 6 + n.weight * 2)),
            }));
            edges = data.edges || [];
        } catch (e) {
            error = String(e);
            nodes = [];
            edges = [];
        }
        loading = false;
    }

    function simulate() {
        if (nodes.length === 0) return;
        const w = canvas?.width || 800;
        const h = canvas?.height || 600;

        // 力导向模拟
        for (let i = 0; i < nodes.length; i++) {
            const a = nodes[i];
            // 引力（中心吸引）
            a.vx += (w / 2 - a.x) * 0.001;
            a.vy += (h / 2 - a.y) * 0.001;

            // 斥力（节点互斥）
            for (let j = i + 1; j < nodes.length; j++) {
                const b = nodes[j];
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                let dist = Math.sqrt(dx * dx + dy * dy) || 1;
                let force = 800 / (dist * dist);
                a.vx += (dx / dist) * force;
                a.vy += (dy / dist) * force;
                b.vx -= (dx / dist) * force;
                b.vy -= (dy / dist) * force;
            }
        }

        // 弹簧力（边连接的节点相互吸引）
        for (const edge of edges) {
            const a = nodes.find((n) => n.id === edge.source);
            const b = nodes.find((n) => n.id === edge.target);
            if (a && b) {
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let dist = Math.sqrt(dx * dx + dy * dy) || 1;
                let force = (dist - 120) * 0.005;
                a.vx += (dx / dist) * force;
                a.vy += (dy / dist) * force;
                b.vx -= (dx / dist) * force;
                b.vy -= (dy / dist) * force;
            }
        }

        // 更新位置（阻尼）
        for (const node of nodes) {
            if (node === dragging) continue;
            node.vx *= 0.85;
            node.vy *= 0.85;
            node.x += node.vx;
            node.y += node.vy;
            // 边界限制
            node.x = Math.max(node.radius, Math.min(w - node.radius, node.x));
            node.y = Math.max(node.radius, Math.min(h - node.radius, node.y));
        }
    }

    function draw() {
        if (!ctx || !canvas) return;
        const w = canvas.width;
        const h = canvas.height;

        ctx.clearRect(0, 0, w, h);
        ctx.save();

        // 画边
        for (const edge of edges) {
            const a = nodes.find((n) => n.id === edge.source);
            const b = nodes.find((n) => n.id === edge.target);
            if (a && b) {
                const isHighlighted =
                    selectedNode &&
                    (selectedNode.id === a.id || selectedNode.id === b.id);
                ctx.beginPath();
                ctx.moveTo(a.x, a.y);
                ctx.lineTo(b.x, b.y);
                ctx.strokeStyle = isHighlighted
                    ? "rgba(255,255,255,0.6)"
                    : "rgba(255,255,255,0.12)";
                ctx.lineWidth = isHighlighted ? 2 : 1;
                ctx.stroke();

                // 关系标签（仅高亮时显示）
                if (isHighlighted) {
                    const mx = (a.x + b.x) / 2;
                    const my = (a.y + b.y) / 2;
                    ctx.font = "10px system-ui";
                    ctx.fillStyle = "rgba(255,255,255,0.7)";
                    ctx.textAlign = "center";
                    ctx.fillText(edge.relation_type, mx, my - 4);
                }
            }
        }

        // 画节点
        for (const node of nodes) {
            const color = typeColors[node.entity_type] || "#94a3b8";
            const isHovered = hoveredNode?.id === node.id;
            const isSelected = selectedNode?.id === node.id;

            // 光晕效果
            if (isHovered || isSelected) {
                ctx.beginPath();
                ctx.arc(node.x, node.y, node.radius + 6, 0, Math.PI * 2);
                ctx.fillStyle = color + "33";
                ctx.fill();
            }

            // 节点
            ctx.beginPath();
            ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
            ctx.fillStyle = isSelected ? color : color + "cc";
            ctx.fill();
            ctx.strokeStyle = isHovered ? "#fff" : color;
            ctx.lineWidth = isHovered ? 2 : 1;
            ctx.stroke();

            // 标签
            ctx.font = `${isHovered ? "bold " : ""}11px system-ui`;
            ctx.fillStyle = "#e2e8f0";
            ctx.textAlign = "center";
            ctx.fillText(node.label, node.x, node.y + node.radius + 14);
        }

        ctx.restore();

        simulate();
        animFrame = requestAnimationFrame(draw);
    }

    function getNodeAt(x, y) {
        for (const node of nodes) {
            const dx = x - node.x;
            const dy = y - node.y;
            if (dx * dx + dy * dy <= (node.radius + 4) ** 2) {
                return node;
            }
        }
        return null;
    }

    function handleMouseMove(e) {
        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;

        if (dragging) {
            dragging.x = x;
            dragging.y = y;
            dragging.vx = 0;
            dragging.vy = 0;
        } else {
            hoveredNode = getNodeAt(x, y);
            canvas.style.cursor = hoveredNode ? "pointer" : "default";
        }
    }

    function handleMouseDown(e) {
        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;
        const node = getNodeAt(x, y);
        if (node) {
            dragging = node;
            selectedNode = node;
        }
    }

    function handleMouseUp() {
        dragging = null;
    }

    function handleClick(e) {
        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;
        const node = getNodeAt(x, y);
        selectedNode = node;
    }

    onMount(async () => {
        if (canvas) {
            const dpr = window.devicePixelRatio || 1;
            const rect = canvas.getBoundingClientRect();
            canvas.width = rect.width * dpr;
            canvas.height = rect.height * dpr;
            ctx = canvas.getContext("2d");
            ctx.scale(dpr, dpr);
            canvas.style.width = rect.width + "px";
            canvas.style.height = rect.height + "px";
        }
        await loadGraphData();
        draw();
    });

    onDestroy(() => {
        if (animFrame) cancelAnimationFrame(animFrame);
    });
</script>

<div class="graph-container">
    <div class="graph-header">
        <h3>知识图谱</h3>
        <div class="graph-legend">
            {#each Object.entries(typeColors) as [type, color]}
                <span class="legend-item">
                    <span class="legend-dot" style="background:{color}"></span>
                    {type}
                </span>
            {/each}
        </div>
        <button class="refresh-btn" onclick={loadGraphData} disabled={loading}>
            {loading ? "加载中..." : "刷新"}
        </button>
    </div>

    {#if error}
        <div class="graph-error">{error}</div>
    {:else if nodes.length === 0 && !loading}
        <div class="graph-empty">
            <p>暂无图谱数据</p>
            <p class="hint">请先开启知识图谱开关并索引文档</p>
        </div>
    {:else}
        <canvas
            bind:this={canvas}
            class="graph-canvas"
            onmousemove={handleMouseMove}
            onmousedown={handleMouseDown}
            onmouseup={handleMouseUp}
            onclick={handleClick}
        ></canvas>
    {/if}

    {#if selectedNode}
        <div class="node-detail">
            <div class="detail-name">{selectedNode.label}</div>
            <div
                class="detail-type"
                style="color:{typeColors[selectedNode.entity_type] ||
                    '#94a3b8'}"
            >
                {selectedNode.entity_type}
            </div>
            <div class="detail-weight">
                关联数: {Math.round(selectedNode.weight)}
            </div>
        </div>
    {/if}
</div>

<style>
    .graph-container {
        position: relative;
        width: 100%;
        height: 400px;
        background: rgba(15, 23, 42, 0.8);
        border-radius: 12px;
        border: 1px solid rgba(255, 255, 255, 0.1);
        overflow: hidden;
    }

    .graph-header {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 12px 16px;
        border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    }

    .graph-header h3 {
        margin: 0;
        font-size: 14px;
        font-weight: 600;
        color: #e2e8f0;
    }

    .graph-legend {
        display: flex;
        gap: 10px;
        flex: 1;
    }

    .legend-item {
        display: flex;
        align-items: center;
        gap: 4px;
        font-size: 11px;
        color: #94a3b8;
    }

    .legend-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
    }

    .refresh-btn {
        padding: 4px 12px;
        font-size: 11px;
        border-radius: 6px;
        border: 1px solid rgba(255, 255, 255, 0.15);
        background: rgba(255, 255, 255, 0.05);
        color: #94a3b8;
        cursor: pointer;
        transition: all 0.2s;
    }

    .refresh-btn:hover:not(:disabled) {
        background: rgba(255, 255, 255, 0.1);
        color: #e2e8f0;
    }

    .graph-canvas {
        width: 100%;
        height: calc(100% - 45px);
        display: block;
    }

    .graph-error {
        padding: 40px;
        text-align: center;
        color: #f87171;
        font-size: 13px;
    }

    .graph-empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: calc(100% - 45px);
        color: #64748b;
    }

    .graph-empty p {
        margin: 4px 0;
        font-size: 13px;
    }

    .graph-empty .hint {
        font-size: 11px;
        color: #475569;
    }

    .node-detail {
        position: absolute;
        bottom: 12px;
        left: 12px;
        padding: 8px 14px;
        background: rgba(30, 41, 59, 0.95);
        border-radius: 8px;
        border: 1px solid rgba(255, 255, 255, 0.12);
        backdrop-filter: blur(8px);
    }

    .detail-name {
        font-size: 13px;
        font-weight: 600;
        color: #e2e8f0;
    }

    .detail-type {
        font-size: 11px;
        margin-top: 2px;
    }

    .detail-weight {
        font-size: 11px;
        color: #64748b;
        margin-top: 2px;
    }
</style>
