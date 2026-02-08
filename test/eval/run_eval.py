#!/usr/bin/env python3
"""
Knot RAG 评测驱动器
通过 HTTP 接口调用 Knot App 的 RAG 功能，评估问答质量。

使用前请确保：
1. Knot App 正在运行
2. 已配置 data_dir 并完成索引
3. HTTP 评测 API 在 18765 端口运行

使用方法:
    python run_eval.py
"""

import json
import time
import html
import requests
from pathlib import Path
from typing import Optional
from dataclasses import dataclass, field, asdict
from difflib import SequenceMatcher

# 配置
EVAL_API_URL = "http://127.0.0.1:18765"
EVAL_DATA_FILE = Path(__file__).parent / "eval.jsonl"
RESULT_FILE = Path(__file__).parent / "eval_result.json"
HTML_REPORT_FILE = Path(__file__).parent / "eval_report.html"


@dataclass
class EvalQuestion:
    """评测问题"""
    id: str
    doc: str
    question: str
    answer_gold: str
    evidence_gold: list
    type: str
    must_refuse: bool


@dataclass
class EvalResult:
    """单个问题的评测结果"""
    id: str
    question: str
    type: str
    answer_gold: str
    answer_pred: str
    citations_pred: list
    scores: dict
    passed: bool
    duration: float = 0.0  # 耗时（秒）


def load_questions() -> list[EvalQuestion]:
    """加载评测问题"""
    questions = []
    with open(EVAL_DATA_FILE, 'r', encoding='utf-8') as f:
        for line in f:
            if line.strip():
                data = json.loads(line)
                questions.append(EvalQuestion(**data))
    return questions


def call_rag_api(query: str) -> Optional[dict]:
    """调用 RAG API"""
    try:
        response = requests.post(
            f"{EVAL_API_URL}/rag/query",
            json={"query": query},
            timeout=60
        )
        if response.status_code == 200:
            return response.json()
        else:
            print(f"  [Error] HTTP {response.status_code}: {response.text[:200]}")
            return None
    except requests.exceptions.ConnectionError:
        print("  [Error] 无法连接到评测 API，请确保 Knot App 正在运行")
        return None
    except Exception as e:
        print(f"  [Error] API 调用失败: {e}")
        return None


def calculate_similarity(text1: str, text2: str) -> float:
    """计算两个文本的相似度（备用方案）"""
    return SequenceMatcher(None, text1.lower(), text2.lower()).ratio()


def call_llm_judge(question: str, answer_gold: str, answer_pred: str) -> Optional[dict]:
    """调用 LLM 评判 API"""
    try:
        response = requests.post(
            f"{EVAL_API_URL}/llm/judge",
            json={
                "question": question,
                "answer_gold": answer_gold,
                "answer_pred": answer_pred
            },
            timeout=60
        )
        if response.status_code == 200:
            return response.json()
        else:
            print(f"  [Error] LLM Judge HTTP {response.status_code}")
            return None
    except Exception as e:
        print(f"  [Error] LLM Judge 调用失败: {e}")
        return None


def check_refusal(answer: str) -> bool:
    """检查是否是拒答回复"""
    refusal_keywords = [
        "无法找到", "未找到", "没有相关信息", "无法回答",
        "文档中未提及", "没有提到", "无法确定"
    ]
    return any(kw in answer for kw in refusal_keywords)


def evaluate_question(q: EvalQuestion) -> EvalResult:
    """评估单个问题"""
    print(f"  评测: {q.id} - {q.question[:30]}...")
    
    start_time = time.time()
    
    # 调用 RAG API
    response = call_rag_api(q.question)
    
    if response is None:
        duration = time.time() - start_time
        return EvalResult(
            id=q.id,
            question=q.question,
            type=q.type,
            answer_gold=q.answer_gold,
            answer_pred="[API 调用失败]",
            citations_pred=[],
            scores={"answer_match": 0, "citation_hit": False},
            passed=False,
            duration=round(duration, 2)
        )
    
    answer_pred = response.get("answer", "")
    citations_pred = response.get("citations", [])
    
    # 计算分数
    scores = {}
    
    if q.must_refuse:
        # 拒答题：检查是否正确拒答
        is_refusal = check_refusal(answer_pred)
        scores["refusal_correct"] = is_refusal
        passed = is_refusal
    else:
        # 常规题：使用 LLM 评判
        judge_result = call_llm_judge(q.question, q.answer_gold, answer_pred)
        
        if judge_result:
            scores["llm_score"] = round(judge_result.get("score", 0), 3)
            scores["llm_correct"] = judge_result.get("correct", False)
            scores["llm_reasoning"] = judge_result.get("reasoning", "")
            passed = judge_result.get("correct", False)
        else:
            # 备用：使用字符串相似度
            similarity = calculate_similarity(q.answer_gold, answer_pred)
            scores["answer_match"] = round(similarity, 3)
            scores["llm_score"] = None
            passed = similarity > 0.3
        
        # 检查引用是否命中 gold 文档
        citation_docs = [c.get("doc_path", "") for c in citations_pred]
        if q.evidence_gold:
            gold_doc = q.evidence_gold[0].get("doc", "")
            scores["citation_hit"] = any(gold_doc in doc for doc in citation_docs)
        else:
            scores["citation_hit"] = False
    
    duration = time.time() - start_time
    
    return EvalResult(
        id=q.id,
        question=q.question,
        type=q.type,
        answer_gold=q.answer_gold,
        answer_pred=answer_pred,
        citations_pred=citations_pred,
        scores=scores,
        passed=passed,
        duration=round(duration, 2)
    )


def generate_html_report(final_result: dict):
    """生成 HTML 评测报告"""
    summary = final_result["summary"]
    by_type = final_result["by_type"]
    results = final_result["results"]
    
    # 生成结果行
    result_rows = ""
    for r in results:
        status_class = "passed" if r["passed"] else "failed"
        status_text = "✓ 通过" if r["passed"] else "✗ 失败"
        
        # 格式化分数
        scores_html = ""
        for k, v in r["scores"].items():
            if isinstance(v, float):
                scores_html += f'<span class="score-item">{k}: {v:.1%}</span>'
            else:
                scores_html += f'<span class="score-item">{k}: {v}</span>'
        
        # 截断过长答案
        answer_gold = html.escape(r["answer_gold"][:200] + "..." if len(r["answer_gold"]) > 200 else r["answer_gold"])
        answer_pred = html.escape(r["answer_pred"][:300] + "..." if len(r["answer_pred"]) > 300 else r["answer_pred"])
        
        # 使用 HTML data-* 属性存储数据，避免引号冲突
        question_attr = html.escape(r["question"], quote=True)
        answer_gold_attr = html.escape(r["answer_gold"], quote=True)
        row_id = html.escape(r['id'])
        
        # 获取耗时，默认为 0.0
        duration = r.get("duration", 0.0)
        
        result_rows += f"""
        <tr class="{status_class}" id="row-{row_id}" data-question="{question_attr}" data-gold="{answer_gold_attr}">
            <td class="id">{row_id}</td>
            <td class="type">{html.escape(r["type"])}</td>
            <td class="question">{html.escape(r["question"])}</td>
            <td class="answer-gold">{answer_gold}</td>
            <td class="answer-pred" id="pred-{row_id}">{answer_pred}</td>
            <td class="scores" id="scores-{row_id}">{scores_html}</td>
            <td class="duration">{duration:.1f}s</td>
            <td class="status" id="status-{row_id}">{status_text}</td>
            <td class="action">
                <button class="retry-btn" onclick="retryTest('{row_id}')">
                    🔄
                </button>
            </td>
        </tr>
        """
    
    # 生成类型统计
    type_stats_html = ""
    for t, stats in by_type.items():
        acc_pct = stats["accuracy"] * 100
        bar_color = "#4ade80" if acc_pct >= 70 else "#facc15" if acc_pct >= 50 else "#f87171"
        type_stats_html += f"""
        <div class="type-stat">
            <div class="type-name">{html.escape(t)}</div>
            <div class="type-bar-container">
                <div class="type-bar" style="width: {acc_pct}%; background: {bar_color};"></div>
            </div>
            <div class="type-value">{stats["passed"]}/{stats["count"]} ({acc_pct:.0f}%)</div>
        </div>
        """
    
    html_content = f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Knot RAG 评测报告</title>
    <style>
        :root {{
            --bg-primary: #0f172a;
            --bg-secondary: #1e293b;
            --bg-card: #334155;
            --text-primary: #f1f5f9;
            --text-secondary: #94a3b8;
            --accent-green: #4ade80;
            --accent-red: #f87171;
            --accent-yellow: #facc15;
            --accent-blue: #60a5fa;
        }}
        
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'PingFang SC', sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2rem;
        }}
        
        .container {{
            max-width: 1400px;
            margin: 0 auto;
        }}
        
        header {{
            text-align: center;
            margin-bottom: 3rem;
        }}
        
        h1 {{
            font-size: 2.5rem;
            font-weight: 700;
            background: linear-gradient(135deg, var(--accent-blue), var(--accent-green));
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
            margin-bottom: 0.5rem;
        }}
        
        .timestamp {{
            color: var(--text-secondary);
            font-size: 0.9rem;
        }}
        
        .summary-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1.5rem;
            margin-bottom: 2rem;
        }}
        
        .summary-card {{
            background: var(--bg-secondary);
            border-radius: 12px;
            padding: 1.5rem;
            text-align: center;
        }}
        
        .summary-value {{
            font-size: 2.5rem;
            font-weight: 700;
            color: var(--accent-blue);
        }}
        
        .summary-value.accuracy {{
            color: {('#4ade80' if summary['overall_accuracy'] >= 0.7 else '#facc15' if summary['overall_accuracy'] >= 0.5 else '#f87171')};
        }}
        
        .summary-label {{
            color: var(--text-secondary);
            font-size: 0.9rem;
            margin-top: 0.5rem;
        }}
        
        .section {{
            background: var(--bg-secondary);
            border-radius: 12px;
            padding: 1.5rem;
            margin-bottom: 2rem;
        }}
        
        .section-title {{
            font-size: 1.25rem;
            font-weight: 600;
            margin-bottom: 1rem;
            color: var(--text-primary);
        }}
        
        .type-stat {{
            display: flex;
            align-items: center;
            gap: 1rem;
            margin-bottom: 0.75rem;
        }}
        
        .type-name {{
            width: 100px;
            color: var(--text-secondary);
        }}
        
        .type-bar-container {{
            flex: 1;
            height: 8px;
            background: var(--bg-card);
            border-radius: 4px;
            overflow: hidden;
        }}
        
        .type-bar {{
            height: 100%;
            border-radius: 4px;
            transition: width 0.5s ease;
        }}
        
        .type-value {{
            width: 120px;
            text-align: right;
            font-family: monospace;
        }}
        
        table {{
            width: 100%;
            border-collapse: collapse;
            font-size: 0.875rem;
        }}
        
        th, td {{
            padding: 0.75rem;
            text-align: left;
            border-bottom: 1px solid var(--bg-card);
        }}
        
        th {{
            background: var(--bg-card);
            font-weight: 600;
            color: var(--text-secondary);
            position: sticky;
            top: 0;
        }}
        
        tr.passed td.status {{
            color: var(--accent-green);
        }}
        
        tr.failed {{
            background: rgba(248, 113, 113, 0.1);
        }}
        
        tr.failed td.status {{
            color: var(--accent-red);
        }}
        
        .id, .type, .status {{
            white-space: nowrap;
        }}
        
        .question {{
            max-width: 200px;
        }}
        
        .answer-gold, .answer-pred {{
            max-width: 250px;
            font-size: 0.8rem;
            color: var(--text-secondary);
        }}
        
        .scores {{
            min-width: 120px;
        }}
        
        .score-item {{
            display: block;
            font-family: monospace;
            font-size: 0.75rem;
        }}
        
        .duration {{
            font-family: monospace;
            color: var(--accent-yellow);
            white-space: nowrap;
        }}
        
        .table-container {{
            overflow-x: auto;
        }}
        
        .action {{
            text-align: center;
        }}
        
        .retry-btn {{
            background: var(--bg-card);
            border: 1px solid var(--text-secondary);
            border-radius: 6px;
            padding: 4px 8px;
            cursor: pointer;
            font-size: 0.9rem;
            transition: all 0.2s;
        }}
        
        .retry-btn:hover {{
            background: var(--accent-blue);
            border-color: var(--accent-blue);
        }}
        
        .retry-btn:disabled {{
            opacity: 0.5;
            cursor: not-allowed;
        }}
        
        .retry-btn.loading {{
            animation: spin 1s linear infinite;
        }}
        
        @keyframes spin {{
            from {{ transform: rotate(0deg); }}
            to {{ transform: rotate(360deg); }}
        }}
        
        footer {{
            text-align: center;
            color: var(--text-secondary);
            font-size: 0.8rem;
            margin-top: 2rem;
        }}
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>🔮 Knot RAG 评测报告</h1>
            <p class="timestamp">生成时间: {summary["timestamp"]}</p>
        </header>
        
        <div class="summary-grid">
            <div class="summary-card">
                <div class="summary-value">{summary["total_questions"]}</div>
                <div class="summary-label">总测试题数</div>
            </div>
            <div class="summary-card">
                <div class="summary-value">{summary["passed"]}</div>
                <div class="summary-label">通过题数</div>
            </div>
            <div class="summary-card">
                <div class="summary-value accuracy">{summary["overall_accuracy"]:.1%}</div>
                <div class="summary-label">整体准确率</div>
            </div>
            <div class="summary-card">
                <div class="summary-value">{len(by_type)}</div>
                <div class="summary-label">题目类型</div>
            </div>
        </div>
        
        <div class="section">
            <h2 class="section-title">📊 按类型统计</h2>
            {type_stats_html}
        </div>
        
        <div class="section">
            <h2 class="section-title">📝 详细结果</h2>
            <div class="table-container">
                <table>
                    <thead>
                        <tr>
                            <th>ID</th>
                            <th>类型</th>
                            <th>问题</th>
                            <th>标准答案</th>
                            <th>模型回答</th>
                            <th>评分</th>
                            <th>耗时</th>
                            <th>状态</th>
                            <th>操作</th>
                        </tr>
                    </thead>
                    <tbody>
                        {result_rows}
                    </tbody>
                </table>
            </div>
        </div>
        
        <footer>
            Knot RAG Evaluation System • Powered by 🦀 Rust + 🐍 Python
        </footer>
    </div>
    
    <script>
        const API_URL = 'http://127.0.0.1:18765';
        
        async function retryTest(id) {{
            const row = document.getElementById(`row-${{id}}`);
            const question = row.dataset.question;
            const answerGold = row.dataset.gold;
            
            const btn = event.target;
            btn.disabled = true;
            btn.classList.add('loading');
            btn.textContent = '⌛';
            
            try {{
                // 1. 调用 RAG 查询
                const ragResp = await fetch(`${{API_URL}}/rag/query`, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ query: question }})
                }});
                
                if (!ragResp.ok) throw new Error('RAG API 调用失败');
                const ragData = await ragResp.json();
                
                // 2. 调用 LLM 评判
                const judgeResp = await fetch(`${{API_URL}}/llm/judge`, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{
                        question: question,
                        answer_gold: answerGold,
                        answer_pred: ragData.answer
                    }})
                }});
                
                if (!judgeResp.ok) throw new Error('LLM Judge API 调用失败');
                const judgeData = await judgeResp.json();
                
                // 3. 更新界面
                const row = document.getElementById(`row-${{id}}`);
                const predCell = document.getElementById(`pred-${{id}}`);
                const scoresCell = document.getElementById(`scores-${{id}}`);
                const statusCell = document.getElementById(`status-${{id}}`);
                
                predCell.textContent = ragData.answer.substring(0, 300) + (ragData.answer.length > 300 ? '...' : '');
                
                scoresCell.innerHTML = `
                    <span class="score-item">llm_score: ${{(judgeData.score * 100).toFixed(0)}}%</span>
                    <span class="score-item">llm_correct: ${{judgeData.correct}}</span>
                    <span class="score-item">llm_reasoning: ${{judgeData.reasoning}}</span>
                `;
                
                if (judgeData.correct) {{
                    row.className = 'passed';
                    statusCell.textContent = '✓ 通过';
                    statusCell.style.color = '#4ade80';
                }} else {{
                    row.className = 'failed';
                    statusCell.textContent = '✗ 失败';
                    statusCell.style.color = '#f87171';
                }}
                
                btn.textContent = '✔';
                btn.style.background = '#4ade80';
                
            }} catch (err) {{
                console.error(err);
                btn.textContent = '✗';
                btn.style.background = '#f87171';
                alert('重试失败: ' + err.message);
            }} finally {{
                btn.disabled = false;
                btn.classList.remove('loading');
                setTimeout(() => {{
                    btn.textContent = '🔄';
                    btn.style.background = '';
                }}, 2000);
            }}
        }}
    </script>
</body>
</html>
"""
    
    with open(HTML_REPORT_FILE, 'w', encoding='utf-8') as f:
        f.write(html_content)
    
    print(f"  ✓ HTML 报告已生成: {HTML_REPORT_FILE}")


def run_evaluation():
    """运行完整评测"""
    print("=" * 60)
    print("Knot RAG 评测系统")
    print("=" * 60)
    
    # 检查 API 可用性
    print("\n[1/5] 检查 API 连接...")
    try:
        resp = requests.get(f"{EVAL_API_URL}/health", timeout=5)
        if resp.status_code == 200:
            print("  ✓ API 连接正常")
        else:
            print(f"  ✗ API 返回非正常状态: {resp.status_code}")
            return
    except requests.exceptions.ConnectionError:
        print("  ✗ 无法连接到评测 API")
        print("    请确保 Knot App 正在运行并等待 5 秒后 API 启动")
        return
    
    # 加载问题
    print("\n[2/5] 加载评测数据...")
    questions = load_questions()
    print(f"  ✓ 加载了 {len(questions)} 个问题")
    
    # 执行评测
    print("\n[3/5] 执行评测...")
    results = []
    for q in questions:
        result = evaluate_question(q)
        results.append(result)
        time.sleep(0.5)  # 避免请求过快
    
    # 计算统计
    print("\n[4/5] 生成 JSON 报告...")
    
    total = len(results)
    passed = sum(1 for r in results if r.passed)
    
    # 按类型统计
    by_type = {}
    for r in results:
        if r.type not in by_type:
            by_type[r.type] = {"count": 0, "passed": 0}
        by_type[r.type]["count"] += 1
        if r.passed:
            by_type[r.type]["passed"] += 1
    
    for t in by_type:
        by_type[t]["accuracy"] = round(by_type[t]["passed"] / by_type[t]["count"], 3)
    
    # 收集失败样例
    failures = [asdict(r) for r in results if not r.passed]
    
    # 构建最终结果
    final_result = {
        "summary": {
            "total_questions": total,
            "passed": passed,
            "overall_accuracy": round(passed / total, 3) if total > 0 else 0,
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S")
        },
        "by_type": by_type,
        "results": [asdict(r) for r in results],
        "failures": failures
    }
    
    # 保存 JSON 结果
    with open(RESULT_FILE, 'w', encoding='utf-8') as f:
        json.dump(final_result, f, ensure_ascii=False, indent=2)
    print(f"  ✓ JSON 结果已保存: {RESULT_FILE}")
    
    # 生成 HTML 报告
    print("\n[5/5] 生成 HTML 报告...")
    generate_html_report(final_result)
    
    # 打印摘要
    print("\n" + "=" * 60)
    print("评测完成！")
    print("=" * 60)
    print(f"总计: {total} 题")
    print(f"通过: {passed} 题")
    print(f"准确率: {final_result['summary']['overall_accuracy']:.1%}")
    print()
    print("按类型统计:")
    for t, stats in by_type.items():
        print(f"  {t}: {stats['passed']}/{stats['count']} ({stats['accuracy']:.1%})")
    print()
    print(f"📄 JSON 结果: {RESULT_FILE}")
    print(f"📊 HTML 报告: {HTML_REPORT_FILE}")


if __name__ == "__main__":
    run_evaluation()
