/**
 * 流式 Markdown 表格补全工具
 * 
 * 在流式输出过程中，Markdown 表格可能处于不完整状态：
 * - 只有表头行 `| col1 | col2 |`，没有分隔行 `|---|---|`
 * - marked.js 无法将其识别为表格，会显示为源码
 * 
 * 此函数在传给 marked.parse() 之前，检测未完成的表格并自动补全分隔行，
 * 使得表格在流式输出过程中也能正确渲染。
 */

/**
 * 检测并补全不完整的 Markdown 表格
 * @param {string} content - 流式输出的 Markdown 内容
 * @returns {string} 补全后的内容
 */
export function completeStreamingTable(content) {
    if (!content) return content;

    const lines = content.split('\n');

    // 从后往前查找连续的表格行块（以 | 开头的行）
    let tableEnd = -1;
    let tableStart = -1;

    for (let i = lines.length - 1; i >= 0; i--) {
        const trimmed = lines[i].trim();
        if (trimmed === '') continue; // 跳过空行

        if (trimmed.startsWith('|')) {
            if (tableEnd === -1) tableEnd = i;
            tableStart = i;
        } else {
            break; // 遇到非表格行，停止
        }
    }

    // 没有找到表格行
    if (tableStart === -1 || tableEnd === -1) return content;

    // 提取表格行（带行号）
    const tableLines = [];
    for (let i = tableStart; i <= tableEnd; i++) {
        if (lines[i].trim() !== '') {
            tableLines.push({ index: i, text: lines[i].trim() });
        }
    }

    if (tableLines.length === 0) return content;

    // 计算表头列数
    const headerLine = tableLines[0].text;
    const cols = headerLine.split('|').filter(c => c.trim() !== '').length;
    if (cols === 0) return content;

    // 生成正确列数的分隔行
    const correctSeparator = '| ' + Array(cols).fill('---').join(' | ') + ' |';

    // 判断一行是否为完整且列数正确的分隔行
    const isValidSeparator = (line) => {
        if (!/^\|[\s\-:| ]+\|$/.test(line)) return false;
        if (!line.includes('-')) return false;
        const sepCols = line.split('|').filter(c => c.trim() !== '').length;
        return sepCols === cols;
    };

    // 判断一行是否为正在输出中的不完整分隔行（只包含 |、-、:、空格）
    const isPartialSeparator = (line) => {
        // 去掉开头的 |，剩余部分只包含 -、:、|、空格
        return line.startsWith('|') && /^[\s\-:| ]*$/.test(line.substring(1)) && line.includes('-');
    };

    const result = [...lines];

    if (tableLines.length === 1) {
        // 只有表头，没有分隔行 → 插入分隔行
        result.splice(tableStart + 1, 0, correctSeparator);
        return result.join('\n');
    }

    // 检查第二行（应该是分隔行的位置）
    const secondLine = tableLines[1];

    if (isValidSeparator(secondLine.text)) {
        // 分隔行完整且列数正确，无需处理
        return content;
    }

    if (isPartialSeparator(secondLine.text)) {
        // 分隔行正在流式输出中（如 |--- 或 |---|），替换为完整的
        result[secondLine.index] = correctSeparator;
        return result.join('\n');
    }

    // 第二行不是分隔行（可能是数据行），在表头后插入分隔行
    result.splice(tableStart + 1, 0, correctSeparator);
    return result.join('\n');
}

/**
 * 将 HTML 中的 <table> 包裹在可横向滚动的容器中
 * 用于 marked.parse() 输出后的后处理
 * @param {string} html - marked.parse 生成的 HTML
 * @returns {string} 包裹后的 HTML
 */
export function wrapTablesForScroll(html) {
    if (!html) return html;
    return html.replace(
        /<table>/g,
        '<div class="table-scroll-wrapper"><table>'
    ).replace(
        /<\/table>/g,
        '</table></div>'
    );
}
