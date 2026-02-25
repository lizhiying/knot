#!/usr/bin/env python3
"""
生成 M6 评测样本集 PDF

样本集包含：
1. 10 份多样化 born-digital PDF（各种布局和内容）
2. 5 份扫描件 PDF（文本渲染为图片）
3. 20 个 stream 表格样本（无线框表格）
4. 20 个 ruled 表格样本（有线框表格）
5. 10 个复杂/失败表格样本（合并单元格、嵌套、超宽等）

输出到 tests/fixtures/eval_samples/ 目录
"""

from fpdf import FPDF
import random
import os
import math

random.seed(2024)

OUTPUT_DIR = "tests/fixtures/eval_samples"


# ============================================================
# 通用工具
# ============================================================

def ensure_dir(path):
    os.makedirs(path, exist_ok=True)


def rand_amount():
    return random.randint(100, 99999)


def rand_pct():
    return round(random.uniform(-30, 50), 1)


# ============================================================
# 1. Born-Digital PDF（10 份）
# ============================================================

def gen_born_digital():
    """生成 10 份多样化 born-digital PDF"""
    out_dir = os.path.join(OUTPUT_DIR, "born_digital")
    ensure_dir(out_dir)

    specs = [
        ("bd01_text_only.pdf", "纯文本", gen_bd_text_only),
        ("bd02_multi_column.pdf", "双栏布局", gen_bd_multi_column),
        ("bd03_financial_report.pdf", "财报", gen_bd_financial),
        ("bd04_academic_paper.pdf", "学术论文", gen_bd_academic),
        ("bd05_legal_contract.pdf", "法律合同", gen_bd_legal),
        ("bd06_invoice.pdf", "发票", gen_bd_invoice),
        ("bd07_form_fields.pdf", "表单", gen_bd_form),
        ("bd08_mixed_lang.pdf", "中英混排", gen_bd_mixed_lang),
        ("bd09_list_heavy.pdf", "列表密集", gen_bd_list_heavy),
        ("bd10_toc_outline.pdf", "目录大纲", gen_bd_toc),
    ]

    for filename, desc, gen_func in specs:
        pdf = FPDF()
        pdf.set_auto_page_break(auto=True, margin=20)
        gen_func(pdf)
        path = os.path.join(out_dir, filename)
        pdf.output(path)
        size_kb = os.path.getsize(path) / 1024
        print(f"  [born-digital] {filename} ({desc}): {pdf.pages_count}p, {size_kb:.1f}KB")


def gen_bd_text_only(pdf):
    for i in range(3):
        pdf.add_page()
        pdf.set_font("Helvetica", "B", 16)
        pdf.cell(0, 10, f"Chapter {i+1}: Document Processing", new_x="LMARGIN", new_y="NEXT")
        pdf.set_font("Helvetica", size=11)
        for _ in range(5):
            pdf.multi_cell(0, 6, f"The field of document processing has seen remarkable advances. "
                           f"Current systems achieve {random.randint(85,99)}% accuracy on standard benchmarks. "
                           f"This paragraph discusses the implications for enterprise workflows.")
            pdf.ln(3)


def gen_bd_multi_column(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, "Two-Column Layout Example", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(5)
    pdf.set_font("Helvetica", size=9)
    col_w = 85
    x_start = pdf.l_margin
    y_start = pdf.get_y()
    for col in range(2):
        pdf.set_xy(x_start + col * (col_w + 10), y_start)
        for _ in range(4):
            x = pdf.get_x()
            pdf.multi_cell(col_w, 5, f"Column {col+1} content. Machine learning models process "
                           f"{random.randint(1000,9999)} documents per hour with {random.randint(90,99)}% accuracy. "
                           f"The throughput depends on document complexity and hardware.")
            pdf.set_x(x)
            pdf.ln(2)


def gen_bd_financial(pdf):
    for q in range(4):
        pdf.add_page()
        pdf.set_font("Helvetica", "B", 14)
        pdf.cell(0, 10, f"Q{q+1} 2024 Financial Highlights", new_x="LMARGIN", new_y="NEXT")
        pdf.set_font("Helvetica", size=10)
        pdf.multi_cell(0, 5, "All figures in thousands USD.")
        pdf.ln(3)
        headers = ["Item", "Actual", "Budget", "Variance"]
        cw = [60, 35, 35, 35]
        pdf.set_font("Helvetica", "B", 9)
        pdf.set_fill_color(220, 220, 220)
        for i, h in enumerate(headers):
            pdf.cell(cw[i], 7, h, border=1, fill=True, align="C")
        pdf.ln()
        pdf.set_font("Helvetica", size=9)
        items = ["Revenue", "COGS", "Gross Profit", "OpEx", "EBITDA", "Net Income"]
        for item in items:
            a, b = rand_amount(), rand_amount()
            pdf.cell(cw[0], 6, item, border=1)
            pdf.cell(cw[1], 6, f"${a:,}", border=1, align="R")
            pdf.cell(cw[2], 6, f"${b:,}", border=1, align="R")
            pdf.cell(cw[3], 6, f"${a-b:,}", border=1, align="R")
            pdf.ln()


def gen_bd_academic(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 16)
    pdf.cell(0, 10, "A Study on PDF Table Extraction Methods", new_x="LMARGIN", new_y="NEXT", align="C")
    pdf.set_font("Helvetica", "I", 10)
    pdf.cell(0, 6, "Author A, Author B, Author C", new_x="LMARGIN", new_y="NEXT", align="C")
    pdf.ln(5)
    pdf.set_font("Helvetica", "B", 12)
    pdf.cell(0, 8, "Abstract", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=10)
    pdf.multi_cell(0, 5, "This paper presents a comprehensive evaluation of table extraction techniques "
                   "applied to PDF documents. We compare ruled-based and stream-based approaches across "
                   "diverse document types including financial reports, academic papers, and invoices.")
    pdf.ln(3)
    for sec_i, sec in enumerate(["1. Introduction", "2. Related Work", "3. Methodology", "4. Results"]):
        pdf.set_font("Helvetica", "B", 12)
        pdf.cell(0, 8, sec, new_x="LMARGIN", new_y="NEXT")
        pdf.set_font("Helvetica", size=10)
        for _ in range(2):
            pdf.multi_cell(0, 5, f"Content for section {sec_i+1}. "
                           f"Our experiments show improvements of {random.randint(5,25)}% over baseline methods. "
                           f"The dataset contains {random.randint(100,5000)} annotated tables.")
            pdf.ln(2)


def gen_bd_legal(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, "SERVICE AGREEMENT", new_x="LMARGIN", new_y="NEXT", align="C")
    pdf.ln(5)
    pdf.set_font("Helvetica", size=10)
    clauses = [
        "1. DEFINITIONS. In this Agreement, unless the context otherwise requires:",
        '   1.1 "Service" means the software-as-a-service platform provided by the Provider.',
        '   1.2 "Customer Data" means all data submitted by or on behalf of Customer.',
        "2. TERM AND TERMINATION.",
        "   2.1 This Agreement commences on the Effective Date and continues for 12 months.",
        "   2.2 Either party may terminate with 30 days written notice.",
        "3. FEES AND PAYMENT.",
        f"   3.1 Customer shall pay ${random.randint(1000,50000):,} per month.",
        "   3.2 Late payments incur interest at 1.5% per month.",
        "4. LIMITATION OF LIABILITY.",
        "   4.1 Neither party's aggregate liability shall exceed the fees paid in the prior 12 months.",
    ]
    for c in clauses:
        pdf.multi_cell(0, 6, c)
        pdf.ln(2)


def gen_bd_invoice(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 18)
    pdf.cell(0, 12, "INVOICE", new_x="LMARGIN", new_y="NEXT", align="R")
    pdf.set_font("Helvetica", size=10)
    pdf.cell(0, 6, f"Invoice #: INV-{random.randint(10000,99999)}", new_x="LMARGIN", new_y="NEXT", align="R")
    pdf.cell(0, 6, "Date: 2024-06-15", new_x="LMARGIN", new_y="NEXT", align="R")
    pdf.ln(10)
    headers = ["Item", "Qty", "Unit Price", "Total"]
    cw = [80, 20, 35, 35]
    pdf.set_font("Helvetica", "B", 10)
    for i, h in enumerate(headers):
        pdf.cell(cw[i], 8, h, border=1, fill=False, align="C")
    pdf.ln()
    pdf.set_font("Helvetica", size=10)
    subtotal = 0
    for item in ["Software License", "Support Plan", "Training (3 days)", "Custom Integration", "Data Migration"]:
        qty = random.randint(1, 10)
        price = random.randint(500, 5000)
        total = qty * price
        subtotal += total
        pdf.cell(cw[0], 7, item, border=1)
        pdf.cell(cw[1], 7, str(qty), border=1, align="C")
        pdf.cell(cw[2], 7, f"${price:,}", border=1, align="R")
        pdf.cell(cw[3], 7, f"${total:,}", border=1, align="R")
        pdf.ln()
    tax = int(subtotal * 0.08)
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(cw[0] + cw[1], 7, "", border=0)
    pdf.cell(cw[2], 7, "Subtotal:", border=1, align="R")
    pdf.cell(cw[3], 7, f"${subtotal:,}", border=1, align="R")
    pdf.ln()
    pdf.cell(cw[0] + cw[1], 7, "", border=0)
    pdf.cell(cw[2], 7, "Tax (8%):", border=1, align="R")
    pdf.cell(cw[3], 7, f"${tax:,}", border=1, align="R")
    pdf.ln()
    pdf.cell(cw[0] + cw[1], 7, "", border=0)
    pdf.cell(cw[2], 7, "TOTAL:", border=1, align="R", fill=False)
    pdf.cell(cw[3], 7, f"${subtotal + tax:,}", border=1, align="R", fill=False)


def gen_bd_form(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, "APPLICATION FORM", new_x="LMARGIN", new_y="NEXT", align="C")
    pdf.ln(5)
    pdf.set_font("Helvetica", size=10)
    fields = [
        ("Full Name:", "John Smith"),
        ("Date of Birth:", "1990-05-15"),
        ("Email:", "john.smith@example.com"),
        ("Phone:", "+1-555-0123"),
        ("Address:", "123 Main St, Suite 400, New York, NY 10001"),
        ("Position Applied:", "Senior Software Engineer"),
        ("Experience (years):", "8"),
        ("Expected Salary:", "$150,000"),
    ]
    for label, val in fields:
        pdf.set_font("Helvetica", "B", 10)
        pdf.cell(50, 8, label)
        pdf.set_font("Helvetica", size=10)
        pdf.cell(0, 8, val, new_x="LMARGIN", new_y="NEXT")


def gen_bd_mixed_lang(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, "Mixed Language Document", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=11)
    # Note: fpdf2 with Helvetica can't render CJK, so we describe what would be there
    texts = [
        "Section 1: English content about document processing systems.",
        "The PDF format was developed by Adobe in the early 1990s.",
        "Table extraction requires understanding both visual layout and logical structure.",
        f"Performance: {random.randint(100,999)} pages/min on modern hardware.",
        "Section 2: Technical specifications and API documentation.",
        "The parse_pdf() function accepts a path and Config parameter.",
        "Returns DocumentIR containing PageIR, BlockIR, TableIR, and ImageIR.",
        f"Memory usage: peak {random.randint(50,200)}MB for a 100-page document.",
    ]
    for t in texts:
        pdf.multi_cell(0, 6, t)
        pdf.ln(3)


def gen_bd_list_heavy(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, "Feature Checklist", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=10)
    categories = {
        "Text Extraction": ["Character extraction", "Font info", "Reading order", "Multi-column"],
        "Table Extraction": ["Stream detection", "Ruled detection", "Cell merging", "Header detection"],
        "OCR": ["Page scoring", "Region detection", "PaddleOCR", "Result merging"],
        "Performance": ["Per-page timing", "Memory tracking", "Async API", "Backpressure"],
    }
    for cat, items in categories.items():
        pdf.set_font("Helvetica", "B", 11)
        pdf.cell(0, 8, cat, new_x="LMARGIN", new_y="NEXT")
        pdf.set_font("Helvetica", size=10)
        for item in items:
            status = random.choice(["DONE", "DONE", "DONE", "TODO", "IN PROGRESS"])
            pdf.cell(8, 6, "-")
            pdf.cell(80, 6, item)
            pdf.cell(0, 6, f"[{status}]", new_x="LMARGIN", new_y="NEXT")


def gen_bd_toc(pdf):
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 16)
    pdf.cell(0, 10, "TABLE OF CONTENTS", new_x="LMARGIN", new_y="NEXT", align="C")
    pdf.ln(8)
    chapters = [
        ("1", "Introduction", 1),
        ("2", "System Architecture", 5),
        ("3", "Text Extraction Pipeline", 12),
        ("4", "Table Detection and Extraction", 23),
        ("5", "OCR Integration", 38),
        ("6", "Performance Evaluation", 45),
        ("7", "API Reference", 52),
        ("A", "Appendix: Sample Outputs", 60),
        ("B", "Appendix: Benchmark Data", 70),
    ]
    for num, title, page in chapters:
        pdf.set_font("Helvetica", "B" if not num.isalpha() else "", 11)
        pdf.cell(15, 7, num)
        pdf.cell(130, 7, title)
        pdf.cell(0, 7, str(page), new_x="LMARGIN", new_y="NEXT", align="R")


# ============================================================
# 2. 扫描件 PDF（5 份）— 用极低质量文本模拟
# ============================================================

def gen_scanned_pdfs():
    """生成 5 份模拟扫描件 PDF（极少可提取文本，触发 OCR）"""
    out_dir = os.path.join(OUTPUT_DIR, "scanned")
    ensure_dir(out_dir)

    specs = [
        ("scan01_letter.pdf", "扫描信件", 2),
        ("scan02_receipt.pdf", "扫描收据", 1),
        ("scan03_form.pdf", "扫描表单", 3),
        ("scan04_report.pdf", "扫描报告", 5),
        ("scan05_mixed.pdf", "混合扫描", 4),
    ]

    for filename, desc, pages in specs:
        pdf = FPDF()
        pdf.set_auto_page_break(auto=True, margin=20)
        for p in range(pages):
            pdf.add_page()
            # 用灰色背景模拟扫描效果
            pdf.set_fill_color(245, 242, 235)
            pdf.rect(0, 0, 210, 297, style="F")
            # 添加极少量文本（模拟扫描噪声，PageScore 会很低）
            pdf.set_font("Helvetica", size=6)
            pdf.set_text_color(200, 200, 200)
            pdf.set_xy(10, 10)
            pdf.cell(0, 3, f"[scan artifact p{p+1}]")
            # 添加一些模拟的图像区域标记
            pdf.set_draw_color(180, 180, 180)
            pdf.rect(20, 30, 170, 240, style="D")
            pdf.set_font("Helvetica", "I", 8)
            pdf.set_text_color(150, 150, 150)
            pdf.set_xy(60, 140)
            pdf.cell(0, 5, f"[Scanned image content - page {p+1}]")
        path = os.path.join(out_dir, filename)
        pdf.output(path)
        size_kb = os.path.getsize(path) / 1024
        print(f"  [scanned] {filename} ({desc}): {pdf.pages_count}p, {size_kb:.1f}KB")


# ============================================================
# 3. Stream 表格样本（20 个）— 无线框
# ============================================================

def gen_stream_tables():
    """生成 20 个 stream 表格（无线框，靠文本对齐构成表格）"""
    out_dir = os.path.join(OUTPUT_DIR, "tables_stream")
    ensure_dir(out_dir)

    for idx in range(1, 21):
        pdf = FPDF()
        pdf.set_auto_page_break(auto=True, margin=20)
        pdf.add_page()
        pdf.set_font("Helvetica", "B", 12)
        pdf.cell(0, 8, f"Stream Table Sample #{idx:02d}", new_x="LMARGIN", new_y="NEXT")
        pdf.ln(3)

        # 随机参数
        cols = random.randint(2, 6)
        rows = random.randint(5, 20)
        col_w = 170 / cols

        # 表头（无边框，靠空格对齐）
        pdf.set_font("Helvetica", "B", 9)
        headers = [f"Col_{chr(65+c)}" for c in range(cols)]
        if idx <= 5:
            headers = ["Name", "Value", "Unit", "Note", "Status", "Date"][:cols]
        elif idx <= 10:
            headers = ["Product", "Q1", "Q2", "Q3", "Q4", "Total"][:cols]
        elif idx <= 15:
            headers = ["Region", "Sales", "Target", "Pct", "Rating", "Comment"][:cols]
        else:
            headers = ["ID", "Amount", "Tax", "Discount", "Net", "Currency"][:cols]

        for h in headers:
            pdf.cell(col_w, 7, h, align="C")
        pdf.ln()

        # 分隔线（用虚线模拟，非 ruled 线段）
        pdf.set_font("Helvetica", size=6)
        pdf.cell(170, 2, "-" * 120, new_x="LMARGIN", new_y="NEXT")

        # 数据行
        pdf.set_font("Helvetica", size=8)
        for r in range(rows):
            for c in range(cols):
                if c == 0:
                    val = f"Item_{r+1}" if idx <= 5 else f"R{r+1}"
                else:
                    val = f"${random.randint(10, 9999):,}" if random.random() > 0.3 else f"{random.randint(1,100)}%"
                pdf.cell(col_w, 6, val, align="R" if c > 0 else "L")
            pdf.ln()

        path = os.path.join(out_dir, f"stream_{idx:02d}.pdf")
        pdf.output(path)
    print(f"  [stream] 20 stream table PDFs generated")


# ============================================================
# 4. Ruled 表格样本（20 个）— 有线框
# ============================================================

def gen_ruled_tables():
    """生成 20 个 ruled 表格（有线框/网格）"""
    out_dir = os.path.join(OUTPUT_DIR, "tables_ruled")
    ensure_dir(out_dir)

    for idx in range(1, 21):
        pdf = FPDF()
        pdf.set_auto_page_break(auto=True, margin=20)
        pdf.add_page()
        pdf.set_font("Helvetica", "B", 12)
        pdf.cell(0, 8, f"Ruled Table Sample #{idx:02d}", new_x="LMARGIN", new_y="NEXT")
        pdf.ln(3)

        cols = random.randint(3, 7)
        rows = random.randint(5, 15)

        # 不等宽列
        if idx % 3 == 0:
            raw = [random.randint(15, 50) for _ in range(cols)]
            total = sum(raw)
            col_widths = [int(w / total * 170) for w in raw]
            col_widths[-1] = 170 - sum(col_widths[:-1])
        else:
            col_widths = [170 // cols] * cols
            col_widths[-1] = 170 - sum(col_widths[:-1])

        # 选择不同的表头
        header_sets = [
            ["Category", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Total"],
            ["Metric", "2022", "2023", "2024", "YoY%", "Target", "Gap"],
            ["Department", "Budget", "Actual", "Variance", "Pct", "Status", "Notes"],
            ["Item", "Quantity", "Price", "Discount", "Tax", "Total", "Notes"],
        ]
        headers = header_sets[idx % len(header_sets)][:cols]

        # 表头
        pdf.set_font("Helvetica", "B", 8)
        pdf.set_fill_color(200, 210, 230)
        for c in range(cols):
            pdf.cell(col_widths[c], 8, headers[c], border=1, fill=True, align="C")
        pdf.ln()

        # 数据行
        pdf.set_font("Helvetica", size=8)
        for r in range(rows):
            is_total_row = (r == rows - 1 and idx % 4 == 0)
            if is_total_row:
                pdf.set_font("Helvetica", "B", 8)
                pdf.set_fill_color(240, 240, 240)

            for c in range(cols):
                if c == 0:
                    val = "TOTAL" if is_total_row else f"Row {r+1}"
                else:
                    val = f"${random.randint(100, 99999):,}" if random.random() > 0.2 else f"{rand_pct()}%"
                pdf.cell(col_widths[c], 7, val, border=1,
                         fill=is_total_row, align="R" if c > 0 else "L")
            pdf.ln()
            if is_total_row:
                pdf.set_font("Helvetica", size=8)

        path = os.path.join(out_dir, f"ruled_{idx:02d}.pdf")
        pdf.output(path)
    print(f"  [ruled] 20 ruled table PDFs generated")


# ============================================================
# 5. 复杂/失败表格样本（10 个）
# ============================================================

def gen_complex_tables():
    """生成 10 个复杂表格（边界情况、预期可能失败）"""
    out_dir = os.path.join(OUTPUT_DIR, "tables_complex")
    ensure_dir(out_dir)

    generators = [
        ("complex_01_wide.pdf", "超宽表格（15列）", gen_cx_wide),
        ("complex_02_tall.pdf", "超长表格（100行跨页）", gen_cx_tall),
        ("complex_03_nested_text.pdf", "单元格含多行文本", gen_cx_multiline),
        ("complex_04_sparse.pdf", "稀疏表格（大量空单元格）", gen_cx_sparse),
        ("complex_05_no_header.pdf", "无表头表格", gen_cx_no_header),
        ("complex_06_mixed_borders.pdf", "混合边框（部分有线部分无线）", gen_cx_mixed_border),
        ("complex_07_tiny.pdf", "微型表格（2x2）", gen_cx_tiny),
        ("complex_08_adjacent.pdf", "相邻多表格", gen_cx_adjacent),
        ("complex_09_text_between.pdf", "表格间穿插文本", gen_cx_text_between),
        ("complex_10_numeric_only.pdf", "纯数字表格", gen_cx_numeric_only),
    ]

    for filename, desc, gen_func in generators:
        pdf = FPDF()
        pdf.set_auto_page_break(auto=True, margin=20)
        gen_func(pdf)
        path = os.path.join(out_dir, filename)
        pdf.output(path)
        size_kb = os.path.getsize(path) / 1024
        print(f"  [complex] {filename} ({desc}): {pdf.pages_count}p, {size_kb:.1f}KB")


def gen_cx_wide(pdf):
    """15 列超宽表格"""
    pdf.add_page("L")  # 横向
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Wide Table (15 columns)", new_x="LMARGIN", new_y="NEXT")
    cols = 15
    cw = 277 // cols  # landscape width
    pdf.set_font("Helvetica", "B", 6)
    for c in range(cols):
        pdf.cell(cw, 6, f"C{c+1}", border=1, align="C")
    pdf.ln()
    pdf.set_font("Helvetica", size=6)
    for r in range(20):
        for c in range(cols):
            pdf.cell(cw, 5, str(random.randint(1, 999)), border=1, align="R")
        pdf.ln()


def gen_cx_tall(pdf):
    """100 行跨页表格"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Tall Table (100 rows, spans pages)", new_x="LMARGIN", new_y="NEXT")
    cw = [20, 50, 40, 40]
    headers = ["#", "Description", "Amount", "Status"]
    pdf.set_font("Helvetica", "B", 8)
    for i, h in enumerate(headers):
        pdf.cell(cw[i], 7, h, border=1, align="C")
    pdf.ln()
    pdf.set_font("Helvetica", size=8)
    for r in range(100):
        pdf.cell(cw[0], 6, str(r+1), border=1, align="C")
        pdf.cell(cw[1], 6, f"Item description #{r+1}", border=1)
        pdf.cell(cw[2], 6, f"${random.randint(10, 9999):,}", border=1, align="R")
        pdf.cell(cw[3], 6, random.choice(["Active", "Pending", "Done"]), border=1, align="C")
        pdf.ln()


def gen_cx_multiline(pdf):
    """单元格含多行文本"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Multi-line Cell Content", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=8)
    data = [
        ["Product A", "High-performance computing\nsolution for enterprise", "$12,500"],
        ["Product B", "Cloud-native platform\nwith auto-scaling\nand monitoring", "$8,900"],
        ["Product C", "Desktop application\nfor data analysis", "$3,200"],
    ]
    cw = [30, 90, 30]
    pdf.set_font("Helvetica", "B", 8)
    for i, h in enumerate(["Name", "Description", "Price"]):
        pdf.cell(cw[i], 7, h, border=1, align="C")
    pdf.ln()
    pdf.set_font("Helvetica", size=8)
    for row in data:
        max_lines = max(len(cell.split("\n")) for cell in row)
        row_h = max_lines * 5
        y0 = pdf.get_y()
        for i, cell in enumerate(row):
            pdf.set_xy(pdf.l_margin + sum(cw[:i]), y0)
            pdf.multi_cell(cw[i], 5, cell, border=1)
        pdf.set_y(y0 + row_h)


def gen_cx_sparse(pdf):
    """稀疏表格"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Sparse Table (many empty cells)", new_x="LMARGIN", new_y="NEXT")
    cols, rows = 5, 10
    cw = 34
    pdf.set_font("Helvetica", "B", 8)
    for c in range(cols):
        pdf.cell(cw, 7, f"Field {c+1}", border=1, align="C")
    pdf.ln()
    pdf.set_font("Helvetica", size=8)
    for r in range(rows):
        for c in range(cols):
            val = str(random.randint(1, 999)) if random.random() > 0.6 else ""
            pdf.cell(cw, 6, val, border=1, align="R")
        pdf.ln()


def gen_cx_no_header(pdf):
    """无表头表格"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Table Without Headers", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=9)
    for r in range(8):
        pdf.cell(40, 7, f"Category {r+1}", border=1)
        pdf.cell(35, 7, f"${random.randint(1000, 50000):,}", border=1, align="R")
        pdf.cell(35, 7, f"{random.randint(1, 100)}%", border=1, align="R")
        pdf.cell(40, 7, random.choice(["Good", "Fair", "Excellent"]), border=1, align="C")
        pdf.ln()


def gen_cx_mixed_border(pdf):
    """混合边框"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Mixed Border Table", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=9)
    for r in range(10):
        border = 1 if r < 3 else ("T" if r == 3 else 0)
        pdf.cell(50, 7, f"Item {r+1}", border=border)
        pdf.cell(40, 7, f"${random.randint(100, 9999):,}", border=1, align="R")
        pdf.cell(40, 7, f"{random.randint(1, 100)} units", border=border, align="R")
        pdf.ln()


def gen_cx_tiny(pdf):
    """微型 2x2 表格"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Tiny 2x2 Table", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(5)
    pdf.set_font("Helvetica", size=10)
    pdf.cell(50, 8, "Revenue", border=1)
    pdf.cell(50, 8, "$1,234,567", border=1, align="R")
    pdf.ln()
    pdf.cell(50, 8, "Expenses", border=1)
    pdf.cell(50, 8, "$987,654", border=1, align="R")


def gen_cx_adjacent(pdf):
    """相邻多表格"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Adjacent Tables", new_x="LMARGIN", new_y="NEXT")

    for t in range(3):
        pdf.ln(5)
        pdf.set_font("Helvetica", "B", 9)
        pdf.cell(0, 6, f"Table {t+1}:", new_x="LMARGIN", new_y="NEXT")
        pdf.set_font("Helvetica", size=8)
        for r in range(4):
            pdf.cell(50, 6, f"T{t+1}_R{r+1}", border=1)
            pdf.cell(40, 6, f"${random.randint(100, 9999):,}", border=1, align="R")
            pdf.cell(40, 6, f"{random.randint(1, 100)}%", border=1, align="R")
            pdf.ln()


def gen_cx_text_between(pdf):
    """表格间穿插文本"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Tables with Text Between", new_x="LMARGIN", new_y="NEXT")

    pdf.set_font("Helvetica", size=10)
    pdf.multi_cell(0, 5, "The first table shows Q1 results:")
    pdf.ln(2)
    pdf.set_font("Helvetica", size=8)
    for r in range(3):
        pdf.cell(60, 6, f"Q1 Item {r+1}", border=1)
        pdf.cell(40, 6, f"${random.randint(1000, 9999):,}", border=1, align="R")
        pdf.ln()

    pdf.ln(5)
    pdf.set_font("Helvetica", size=10)
    pdf.multi_cell(0, 5, "The following table shows Q2 improvement over Q1:")
    pdf.ln(2)
    pdf.set_font("Helvetica", size=8)
    for r in range(3):
        pdf.cell(60, 6, f"Q2 Item {r+1}", border=1)
        pdf.cell(40, 6, f"${random.randint(1000, 9999):,}", border=1, align="R")
        pdf.cell(30, 6, f"+{random.randint(1, 30)}%", border=1, align="R")
        pdf.ln()


def gen_cx_numeric_only(pdf):
    """纯数字表格"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 10)
    pdf.cell(0, 8, "Numeric-Only Table", new_x="LMARGIN", new_y="NEXT")
    cols = 8
    cw = 170 // cols
    pdf.set_font("Helvetica", "B", 7)
    for c in range(cols):
        pdf.cell(cw, 6, f"V{c+1}", border=1, align="C")
    pdf.ln()
    pdf.set_font("Helvetica", size=7)
    for r in range(25):
        for c in range(cols):
            val = f"{random.uniform(0.01, 999.99):.2f}"
            pdf.cell(cw, 5, val, border=1, align="R")
        pdf.ln()


# ============================================================
# Main
# ============================================================

def main():
    print("Generating M6 Evaluation Sample Set...")
    print(f"Output: {OUTPUT_DIR}/\n")

    ensure_dir(OUTPUT_DIR)

    print("1. Born-Digital PDFs (10):")
    gen_born_digital()

    print("\n2. Scanned PDFs (5):")
    gen_scanned_pdfs()

    print("\n3. Stream Table Samples (20):")
    gen_stream_tables()

    print("\n4. Ruled Table Samples (20):")
    gen_ruled_tables()

    print("\n5. Complex/Edge-case Table Samples (10):")
    gen_complex_tables()

    # 統計
    total = 0
    for root, dirs, files in os.walk(OUTPUT_DIR):
        total += len([f for f in files if f.endswith(".pdf")])
    print(f"\nTotal: {total} PDF files generated")


if __name__ == "__main__":
    main()
