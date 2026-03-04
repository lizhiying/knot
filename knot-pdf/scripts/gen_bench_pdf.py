#!/usr/bin/env python3
"""
生成 100 页 born-digital 测试 PDF

内容包括：
- 纯文本页面（多段落、中英混排）
- 包含表格的页面（模拟财报）
- 包含页眉页脚的页面
- 多列布局页面
- 数字密集页面

用途：knot-pdf 基准测试 & 端到端评测
"""

from fpdf import FPDF
import random
import os

random.seed(42)  # 可复现

class BenchmarkPDF(FPDF):
    """自定义 PDF 生成器"""

    def __init__(self):
        super().__init__()
        self.set_auto_page_break(auto=True, margin=25)

    def header(self):
        """页眉"""
        self.set_font("Helvetica", "I", 8)
        self.cell(0, 5, f"knot-pdf Benchmark Document | Page {self.page_no()}/100", align="C")
        self.ln(8)

    def footer(self):
        """页脚"""
        self.set_y(-15)
        self.set_font("Helvetica", "I", 8)
        self.cell(0, 10, f"Confidential - Generated for testing purposes only | {self.page_no()}", align="C")


def add_text_page(pdf, page_num):
    """纯文本页面"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 16)
    pdf.cell(0, 10, f"Chapter {page_num}: Text Content Analysis", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(5)

    pdf.set_font("Helvetica", size=11)
    paragraphs = [
        "The rapid advancement of artificial intelligence and machine learning technologies has fundamentally transformed how organizations process and analyze large volumes of documents. In the context of Retrieval-Augmented Generation (RAG), the quality of document parsing directly impacts the accuracy and relevance of generated responses.",
        "PDF documents remain the most widely used format for distributing financial reports, academic papers, legal contracts, and technical documentation. Despite their ubiquity, extracting structured information from PDFs presents significant challenges due to the format's inherent complexity.",
        f"This section examines the performance characteristics of various parsing approaches, with particular attention to text extraction accuracy, table structure preservation, and processing throughput. The benchmark results presented here were collected using a standardized test suite comprising {random.randint(50, 200)} sample documents.",
        "Key metrics include: characters per second (CPS), structural accuracy percentage, and memory utilization patterns. These measurements provide a comprehensive view of system performance under realistic workloads.",
        f"The total processing time for this document is expected to be under {random.randint(5, 15)} seconds on modern hardware, with peak memory consumption not exceeding {random.randint(100, 250)}MB.",
    ]

    for p in paragraphs:
        pdf.multi_cell(0, 6, p)
        pdf.ln(3)


def add_table_page(pdf, page_num):
    """包含表格的页面（模拟财报）"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, f"Financial Summary - Q{(page_num % 4) + 1} {2023 + page_num // 4}", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(5)

    pdf.set_font("Helvetica", size=10)
    pdf.multi_cell(0, 5, "The following table presents key financial metrics for the reporting period. All figures are in thousands of USD unless otherwise noted.")
    pdf.ln(5)

    # 表格数据
    headers = ["Category", "Current Period", "Prior Period", "Change", "Change %"]
    col_widths = [50, 35, 35, 30, 30]

    # 表头
    pdf.set_font("Helvetica", "B", 9)
    pdf.set_fill_color(220, 220, 220)
    for i, h in enumerate(headers):
        pdf.cell(col_widths[i], 8, h, border=1, fill=True, align="C")
    pdf.ln()

    # 数据行
    categories = [
        "Revenue", "Cost of Goods Sold", "Gross Profit", "Operating Expenses",
        "R&D Expenses", "Marketing", "General & Admin", "Operating Income",
        "Interest Income", "Interest Expense", "Pre-tax Income", "Income Tax",
        "Net Income", "EPS (Basic)", "EPS (Diluted)",
    ]

    pdf.set_font("Helvetica", size=9)
    for cat in categories:
        curr = random.randint(1000, 50000)
        prev = random.randint(1000, 50000)
        change = curr - prev
        pct = (change / prev * 100) if prev != 0 else 0

        row = [cat, f"${curr:,}", f"${prev:,}", f"${change:,}", f"{pct:.1f}%"]
        aligns = ["L", "R", "R", "R", "R"]
        for i, val in enumerate(row):
            pdf.cell(col_widths[i], 7, val, border=1, align=aligns[i])
        pdf.ln()

    pdf.ln(5)
    pdf.set_font("Helvetica", "I", 9)
    pdf.multi_cell(0, 5, f"Note: Figures are unaudited and subject to revision. The year-over-year comparison reflects organic growth excluding acquisitions made in {2022 + page_num // 4}.")


def add_mixed_page(pdf, page_num):
    """混合内容页面（文字 + 小表格 + 列表）"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 14)
    pdf.cell(0, 10, f"Section {page_num}: Product Performance Overview", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(3)

    pdf.set_font("Helvetica", size=10)
    pdf.multi_cell(0, 5, "This section provides a comprehensive overview of product performance metrics across all major business segments. The data has been aggregated from multiple internal reporting systems.")
    pdf.ln(3)

    # 小表格
    pdf.set_font("Helvetica", "B", 9)
    metrics = [
        ("Active Users", f"{random.randint(10, 500)}M"),
        ("Monthly Revenue", f"${random.randint(50, 500)}M"),
        ("Retention Rate", f"{random.randint(70, 98)}%"),
        ("NPS Score", f"{random.randint(30, 80)}"),
        ("Support Tickets", f"{random.randint(1000, 50000):,}"),
    ]

    pdf.set_fill_color(240, 240, 240)
    pdf.cell(80, 7, "Metric", border=1, fill=True, align="C")
    pdf.cell(60, 7, "Value", border=1, fill=True, align="C")
    pdf.ln()

    pdf.set_font("Helvetica", size=9)
    for metric, value in metrics:
        pdf.cell(80, 7, metric, border=1)
        pdf.cell(60, 7, value, border=1, align="R")
        pdf.ln()

    pdf.ln(5)

    # 列表
    pdf.set_font("Helvetica", "B", 11)
    pdf.cell(0, 8, "Key Highlights:", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=10)

    highlights = [
        f"Revenue grew {random.randint(5, 30)}% year-over-year, exceeding analyst expectations.",
        f"Customer acquisition cost decreased by {random.randint(10, 25)}% through improved targeting.",
        f"Product launch in {random.choice(['APAC', 'EMEA', 'LATAM'])} region achieved {random.randint(80, 120)}% of first-year targets.",
        "Infrastructure optimization reduced cloud computing costs by $12M annually.",
        f"Employee headcount increased to {random.randint(5000, 20000):,}, with {random.randint(200, 1000)} new engineering hires.",
    ]

    for h in highlights:
        pdf.cell(5, 6, "-")
        pdf.multi_cell(0, 6, f" {h}")
        pdf.ln(1)


def add_dense_numbers_page(pdf, page_num):
    """数字密集页面（模拟数据表）"""
    pdf.add_page()
    pdf.set_font("Helvetica", "B", 12)
    pdf.cell(0, 10, f"Appendix {page_num}: Detailed Data Table", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(3)

    # 宽表格
    headers = ["ID", "Date", "Amount", "Units", "Price", "Tax", "Total"]
    col_w = [15, 25, 25, 20, 25, 20, 30]

    pdf.set_font("Helvetica", "B", 7)
    pdf.set_fill_color(200, 200, 200)
    for i, h in enumerate(headers):
        pdf.cell(col_w[i], 6, h, border=1, fill=True, align="C")
    pdf.ln()

    pdf.set_font("Helvetica", size=7)
    for row_idx in range(30):
        amount = random.uniform(100, 10000)
        units = random.randint(1, 100)
        price = amount / units
        tax = amount * 0.08
        total = amount + tax

        row_data = [
            f"{page_num * 100 + row_idx + 1}",
            f"2024-{random.randint(1,12):02d}-{random.randint(1,28):02d}",
            f"${amount:,.2f}",
            str(units),
            f"${price:,.2f}",
            f"${tax:,.2f}",
            f"${total:,.2f}",
        ]
        for i, val in enumerate(row_data):
            pdf.cell(col_w[i], 5, val, border=1, align="R" if i > 0 else "C")
        pdf.ln()


def main():
    pdf = BenchmarkPDF()
    pdf.set_title("knot-pdf Benchmark Document - 100 Pages Born-Digital")
    pdf.set_author("knot-pdf Benchmark Suite")
    pdf.set_subject("Performance and accuracy benchmarking for PDF parsing")

    # 生成 100 页，按模式分布
    for page_num in range(1, 101):
        mod = page_num % 5
        if mod == 0:
            add_dense_numbers_page(pdf, page_num)
        elif mod == 1:
            add_text_page(pdf, page_num)
        elif mod == 2:
            add_table_page(pdf, page_num)
        elif mod == 3:
            add_mixed_page(pdf, page_num)
        elif mod == 4:
            add_text_page(pdf, page_num)

    output_dir = "tests/fixtures"
    os.makedirs(output_dir, exist_ok=True)
    output_path = os.path.join(output_dir, "bench_100pages.pdf")
    pdf.output(output_path)
    print(f"Generated: {output_path}")
    print(f"Pages: {pdf.pages_count}")
    file_size = os.path.getsize(output_path)
    print(f"Size: {file_size / 1024:.1f} KB")


if __name__ == "__main__":
    main()
