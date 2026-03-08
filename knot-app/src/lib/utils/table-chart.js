/**
 * 表格图表生成工具
 * 解析 HTML 表格数据，自动选择合适的图表类型并渲染
 */
import { Chart, registerables } from 'chart.js';

// 注册所有 Chart.js 组件
Chart.register(...registerables);

// 柔和的图表配色方案（紫色主题）
const CHART_COLORS = [
    'rgba(139, 92, 246, 0.8)',   // 紫色
    'rgba(59, 130, 246, 0.8)',   // 蓝色
    'rgba(16, 185, 129, 0.8)',   // 绿色
    'rgba(245, 158, 11, 0.8)',   // 橙色
    'rgba(239, 68, 68, 0.8)',    // 红色
    'rgba(236, 72, 153, 0.8)',   // 粉色
    'rgba(99, 102, 241, 0.8)',   // 靛蓝
    'rgba(20, 184, 166, 0.8)',   // 青色
];

const CHART_BORDERS = CHART_COLORS.map(c => c.replace('0.8)', '1)'));
const CHART_BG = CHART_COLORS.map(c => c.replace('0.8)', '0.15)'));

/**
 * 从 HTML table 元素解析数据
 */
function parseTableData(table) {
    const headers = [...table.querySelectorAll('thead th')].map(th => th.textContent.trim());
    const rows = [...table.querySelectorAll('tbody tr')].map(tr =>
        [...tr.querySelectorAll('td')].map(td => td.textContent.trim())
    );
    return { headers, rows };
}

/**
 * 判断一列是否为数值列
 */
function isNumericColumn(rows, colIdx) {
    let numCount = 0;
    for (const row of rows) {
        const val = row[colIdx];
        if (val && !isNaN(Number(val))) numCount++;
    }
    return numCount > rows.length * 0.6; // 60% 以上是数字
}

/**
 * 判断一列是否为日期/分类列（作为 X 轴标签）
 */
function isLabelColumn(rows, colIdx) {
    return !isNumericColumn(rows, colIdx);
}

/**
 * 自动选择图表类型并生成配置
 */
function buildChartConfig(headers, rows) {
    if (headers.length < 2 || rows.length === 0) return null;

    // 找到第一个非数值列作为 X 轴标签
    let labelColIdx = 0;
    for (let i = 0; i < headers.length; i++) {
        if (isLabelColumn(rows, i)) {
            labelColIdx = i;
            break;
        }
    }

    // 收集所有数值列
    const numericCols = [];
    for (let i = 0; i < headers.length; i++) {
        if (i !== labelColIdx && isNumericColumn(rows, i)) {
            numericCols.push(i);
        }
    }

    if (numericCols.length === 0) return null;

    const labels = rows.map(r => r[labelColIdx] || '');
    const datasets = numericCols.map((colIdx, i) => ({
        label: headers[colIdx],
        data: rows.map(r => {
            const v = Number(r[colIdx]);
            return isNaN(v) ? null : v;
        }),
        borderColor: CHART_COLORS[i % CHART_COLORS.length],
        backgroundColor: CHART_BG[i % CHART_BG.length],
        borderWidth: 2.5,
        pointRadius: 4,
        pointHoverRadius: 6,
        pointBackgroundColor: CHART_COLORS[i % CHART_COLORS.length],
        tension: 0.3,
        fill: numericCols.length === 1, // 单列时填充
    }));

    // 选择图表类型
    // 日期列或行数 > 3 → 折线图，否则 → 柱状图
    const isDateLike = labels.some(l => /\d{4}[-/]\d{1,2}[-/]\d{1,2}/.test(l));
    const chartType = (isDateLike || rows.length > 5) ? 'line' : 'bar';

    return {
        type: chartType,
        data: { labels, datasets },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    position: 'top',
                    align: 'start',
                    labels: {
                        color: '#e0e0e0',
                        font: { size: 12, weight: '500' },
                        boxWidth: 14,
                        boxHeight: 3,
                        padding: 16,
                        usePointStyle: false,
                    }
                },
                tooltip: {
                    backgroundColor: 'rgba(30, 30, 40, 0.95)',
                    titleColor: '#fff',
                    bodyColor: 'rgba(255, 255, 255, 0.9)',
                    borderColor: 'rgba(139, 92, 246, 0.3)',
                    borderWidth: 1,
                    cornerRadius: 6,
                    padding: 10,
                    titleFont: { size: 12 },
                    bodyFont: { size: 12 },
                }
            },
            scales: {
                x: {
                    ticks: { color: 'rgba(255, 255, 255, 0.7)', font: { size: 11 } },
                    grid: { color: 'rgba(255, 255, 255, 0.06)' },
                },
                y: {
                    ticks: { color: 'rgba(255, 255, 255, 0.7)', font: { size: 11 } },
                    grid: { color: 'rgba(255, 255, 255, 0.08)' },
                }
            },
            interaction: {
                intersect: false,
                mode: 'index',
            },
        }
    };
}

/**
 * 在指定表格下方创建图表切换按钮和图表容器
 * @param {HTMLTableElement} table - 表格元素
 * @returns {Function|null} cleanup 函数
 */
export function attachChartToggle(table) {
    const { headers, rows } = parseTableData(table);
    const config = buildChartConfig(headers, rows);
    if (!config) return null; // 无法生成图表（没有数值列）

    // 找到表格的父容器（可能是 table-scroll-wrapper）
    const parent = table.closest('.table-scroll-wrapper') || table.parentElement;

    // 创建"显示图表"链接
    const toggleLink = document.createElement('div');
    toggleLink.className = 'chart-toggle-link';
    toggleLink.innerHTML = '<span class="chart-icon">📊</span> 显示图表';
    toggleLink.style.cssText = `
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 4px 0;
        margin: 2px 0 8px;
        font-size: 12px;
        color: rgba(139, 92, 246, 0.8);
        cursor: pointer;
        user-select: none;
        transition: color 0.15s;
    `;
    toggleLink.addEventListener('mouseenter', () => {
        toggleLink.style.color = '#8b5cf6';
    });
    toggleLink.addEventListener('mouseleave', () => {
        toggleLink.style.color = 'rgba(139, 92, 246, 0.8)';
    });

    // 创建图表容器（初始隐藏）
    const chartContainer = document.createElement('div');
    chartContainer.className = 'table-chart-container';
    chartContainer.style.cssText = `
        display: none;
        height: 260px;
        margin: 0 0 12px;
        padding: 12px;
        background: rgba(255, 255, 255, 0.03);
        border: 1px solid rgba(139, 92, 246, 0.15);
        border-radius: 8px;
    `;

    const canvas = document.createElement('canvas');
    chartContainer.appendChild(canvas);

    // 插入到表格/wrapper 后面
    parent.insertAdjacentElement('afterend', chartContainer);
    parent.insertAdjacentElement('afterend', toggleLink);

    let chartInstance = null;
    let visible = false;

    toggleLink.addEventListener('click', () => {
        visible = !visible;
        chartContainer.style.display = visible ? 'block' : 'none';
        toggleLink.innerHTML = visible
            ? '<span class="chart-icon">📊</span> 隐藏图表'
            : '<span class="chart-icon">📊</span> 显示图表';

        if (visible && !chartInstance) {
            // 延迟一帧创建图表（等 DOM 显示完毕）
            requestAnimationFrame(() => {
                chartInstance = new Chart(canvas.getContext('2d'), config);
            });
        }
    });

    return () => {
        if (chartInstance) {
            chartInstance.destroy();
            chartInstance = null;
        }
        toggleLink.remove();
        chartContainer.remove();
    };
}
