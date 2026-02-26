#!/bin/bash
# ============================================================
# knot-pdf 安装脚本
#
# 将 knot-pdf-cli 编译并安装到 ~/.cargo/bin/，全局可用。
# 配置文件位于：~/.config/knot-pdf/knot-pdf.toml
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFIG_DIR="$HOME/.config/knot-pdf"
CONFIG_FILE="$CONFIG_DIR/knot-pdf.toml"

echo "╔══════════════════════════════════════════════════╗"
echo "║         knot-pdf 安装                             ║"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# ── 1. 选择 Features ───────────────────────────────────
# 根据需要调整，默认推荐配置：
#   cli          — 命令行工具（必需）
#   pdfium       — Pdfium PDF 后端（文本提取更准确）
#   ocr_paddle   — PaddleOCR 引擎（扫描件支持）
#   vision       — Vision LLM 图片描述
#   formula_model — 公式 OCR（纯 Rust ONNX 推理）
#   layout_model — ONNX 版面检测模型（改善分栏识别）

FEATURES="cli,pdfium,ocr_paddle,vision,formula_model,layout_model"

echo "📦 编译 features: $FEATURES"
echo "📂 项目目录: $PROJECT_DIR"
echo ""

# ── 2. 编译安装到 ~/.cargo/bin/ ───────────────────────
echo "🔨 开始编译安装..."
cargo install \
    --path "$PROJECT_DIR" \
    --features "$FEATURES" \
    --bin knot-pdf-cli \
    --force \
    2>&1 | tail -10

echo ""

# ── 3. 验证安装 ─────────────────────────────────────
if command -v knot-pdf-cli &>/dev/null; then
    echo "✅ 安装成功！"
    echo "   位置: $(which knot-pdf-cli)"
    echo ""
    knot-pdf-cli --version 2>/dev/null || true
else
    echo "⚠️  安装完成，但 knot-pdf-cli 未在 PATH 中找到"
    echo "   请确保 ~/.cargo/bin 在你的 PATH 中："
    echo "   export PATH=\"\$HOME/.cargo/bin:\$PATH\""
fi

echo ""

# ── 4. 配置文件 ─────────────────────────────────────
if [ ! -f "$CONFIG_FILE" ]; then
    echo "📝 创建默认配置文件: $CONFIG_FILE"
    mkdir -p "$CONFIG_DIR"
    cp "$SCRIPT_DIR/knot-pdf.default.toml" "$CONFIG_FILE" 2>/dev/null || \
    echo "   （请手动创建配置文件，参考 docs/architecture.md）"
else
    echo "📋 配置文件已存在: $CONFIG_FILE"
fi

echo ""
echo "🎉 完成！使用方式："
echo "   knot-pdf-cli markdown input.pdf -o output.md"
echo "   knot-pdf-cli rag input.pdf -o output.txt"
echo "   knot-pdf-cli parse input.pdf -o output.json"
echo "   knot-pdf-cli info input.pdf"
echo "   knot-pdf-cli config show"