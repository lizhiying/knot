Milestone: milestone1
Iteration: iteration2

Goal: 引入状态管理和结构化上下文，支持增量更新，大幅提升索引效率和检索上下文质量。
Assumptions: Iteration 1 已完成，LanceDB 读写正常。
Scope: SQLite Registry, Hash Check, Parent-Child Context, Breadcrumbs.

Tasks:
- [ ] 引入 `sqlx` + `sqlite`: 创建 `file_registry` 表
- [ ] 实现 File Hash 计算与比对逻辑 (Skipping unchanged files)
- [ ] 实现 Node-level Diff Logic (基于 `node_id` + `content_hash`)
- [ ] 优化 Flatten Logic: 注入 Parent Context 和 Breadcrumbs 到 VectorRecord
- [ ] 实现 Delete Logic: 处理文件删除或移动的情况
- [ ] 集成 `notify` crate: 监听文件系统变更事件

Exit criteria:
- 修改一个文件，Index 仅处理该文件（看日志）。
- 检索结果中包含 `breadcrumbs` 信息（如 "Doc > Chapter 1 > Section A"）。
