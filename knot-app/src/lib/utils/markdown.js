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

    // 提取表格行
    const tableLines = lines.slice(tableStart, tableEnd + 1).filter(l => l.trim() !== '');

    if (tableLines.length === 0) return content;

    // 检查是否已有分隔行（|---|---|）
    const hasSeparator = tableLines.some(line => {
        const trimmed = line.trim();
        // 分隔行：只包含 |, -, :, 空格
        return trimmed.startsWith('|') && /^\|[\s\-:| ]+\|$/.test(trimmed) && trimmed.includes('-');
    });

    // 已有分隔行，表格完整，无需补全
    if (hasSeparator) return content;

    // 需要补全：取第一行作为表头，计算列数
    const headerLine = tableLines[0].trim();
    const cols = headerLine.split('|').filter(c => c.trim() !== '').length;

    if (cols === 0) return content;

    // 生成分隔行
    const separator = '| ' + Array(cols).fill('---').join(' | ') + ' |';

    // 在表头行后插入分隔行
    const result = [...lines];
    result.splice(tableStart + 1, 0, separator);

    return result.join('\n');
}
