# Memory / Case Store 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

当前 MVP 已用 Rust Server 内部模块实现 Memory 后端，第一阶段只启用 `memoryType=case`。兼容的 Case API、`CaseRecord`、`CaseSearchHit` 和 `case_context.json` 保持不变；主索引存储在 `storage.data_dir/memory/memory.sqlite`，legacy JSON Case 文件保留在 `storage.data_dir/cases/` 作为迁移和回滚源。Python V2 clean-room Server 额外在本地 SQLite Case 表中维护 `vector_json`，用 dependency-light hash-vector recall 与 FTS/BM25 合并排序，不依赖 PostgreSQL、Redis、pgvector 或外部 embedding 服务。

## Embedding 配置

外部 embedding 模型后续必须配置化；当前 V2 已先使用本地 hash-vector recall 覆盖轻量相似召回。

```yaml
embedding:
  provider: "openai_compatible"
  model: "text-embedding-3-small"
  api_key_env: "LOGAGENT_EMBEDDING_API_KEY"
  store: "sqlite"
```

## 职责

Memory 负责把人工确认后的分析结果沉淀为可复用经验，并在新任务中召回相似历史 Case。Case Store 当前是 Memory 上的兼容 facade。

当前实现位于 `server/src/stores/memory_store.rs`、`server/src/stores/case_store.rs` 和 `server/src/stores/case_import_store.rs`，通过受保护 API 暴露：

- `POST /api/tasks/:task_id/case`：成功任务人工确认后保存为 Case。
- `POST /api/cases`：手工录入一个不绑定任务的 Case。
- `POST /api/cases/imports`：从粘贴文本或 UTF-8 文本文件创建导入草稿并调用 LLM 整理。
- `GET /api/cases/imports/:draft_id`：读取导入草稿。
- `PATCH /api/cases/imports/:draft_id`：保存用户对草稿字段的修正。
- `POST /api/cases/imports/:draft_id/messages`：提交缺失信息回答并继续整理。
- `POST /api/cases/imports/:draft_id/confirm`：确认草稿并保存为 `manual` Case。
- `GET /api/cases?query=<text>&limit=5`：按关键词召回启用 Case。
- `GET /api/cases/:case_id`：读取 Case 详情。
- `PATCH /api/cases/:case_id`：编辑 Case 字段或设置 `enabled=false` 禁用。

Python V2 clean-room Server 提供等价 Case Memory 入口：`POST /api/v2/cases/imports/preview`
创建导入草稿，`POST /api/v2/cases/imports/:import_id/messages` 追加缺失信息并重新整理，
`PATCH /api/v2/cases/imports/:import_id` 保存人工草稿修正并重算校验错误，
`POST /api/v2/cases/imports/:import_id/confirm` 在必填字段完整后保存为 `manual` Case。
V2 草稿、校验错误和消息历史都持久化在 SQLite `case_imports` 表中；已确认草稿不再允许 PATCH。V2 Case 搜索会合并 SQLite FTS5/BM25、关键词 fallback 和本地 hash-vector 相似度；搜索结果可返回 `searchBackend=fts5|keyword|recent|hybrid|vector`，并在命中向量召回时带 `vectorScore`。

主存储为 `storage.data_dir/memory/memory.sqlite`，包含 `memory_items`、`memory_chunks` 和 `memory_chunks_fts`。Server 启动时会把 `storage.data_dir/cases/*.json` 按 `caseId` idempotent 导入 SQLite；旧 JSON 文件不删除，新增和更新 Case 也会同步写 JSON。当前开发阶段使用 Case schema v2，不做 v1 旧数据兼容。

新任务创建时会按用户问题召回最多 5 个启用 Case，并把结果固化到 workspace 的 `case_context.json`。Artifacts API 返回 `caseContext`，Claude Code 会通过 `analysis_package.json` 和 MCP resource 读取“历史 Case 参考”，并明确要求历史 Case 只能作为参考，最终结论仍必须引用当前任务证据。

只读 HTTP MCP 通过 `logagent://cases/recent`、`logagent.search_cases` 和 `logagent.get_case` 暴露已确认 Case，供个人本地 Claude Code 读取；该入口不能创建、编辑、禁用或确认 Case。

Python V2 clean-room Server 的 task MCP 额外暴露 Rust V1 兼容
`logagent.recall_cases`，只返回 enabled Case，并把召回结果写入
background-only `case_context` evidence；响应返回 Rust/V1 兼容的
`artifactPath`、`caseCount` 和逐 Case `evidenceRefs`。Readonly MCP 继续只暴露
`logagent.search_cases` 和 `logagent.get_case`。

## 人工确认

任务分析完成后，WebUI 提供：

- 确认为 Case
- 修改后确认
- 放弃

顶部 `Memory` 页面提供独立 Case 管理入口：

- 搜索和查看已保存 Case。
- 粘贴 Case 文档/文字或上传 UTF-8 文本类文件，调用 LLM 整理为 `manual` Case 草稿。
- 缺少标题、现象、根因或解决方案时，通过连续对话补充信息。
- 编辑标题、现象、根因、解决方案、产品、版本、环境、InstanceID、NodeID 和 evidence refs。
- 启用或禁用 Case。

## Case 字段

- `case_id`
- `schema_version`: 固定为 2
- `source_type`: `task` 或 `manual`
- `task_id`: 仅 `source_type=task` 时存在
- `product`
- `version`
- `environment`
- `instance_id`
- `node_id`
- `title`
- `symptom`
- `root_cause`
- `solution`
- `evidence_refs`
- `source_result_path`: 仅 `source_type=task` 时存在
- `enabled`
- `created_at`
- `updated_at`

## embedding 文本

```text
title + symptom + root_cause + solution
```

## MVP 存储策略

第一版：

- Case 主记录写入 `storage.data_dir/memory/memory.sqlite`。
- Case import 草稿写入 `storage.data_dir/case_imports/`，确认后写入 Memory，并同步 legacy JSON 到 `storage.data_dir/cases/`。
- Case schema v2 使用 `source_type` 区分任务确认 Case 和手工录入 Case。
- 搜索优先使用 SQLite FTS/BM25，并合并关键词重叠评分；FTS 不可用或查询失败时回退到关键词重叠评分。
- V2 Case 主表额外维护 `vector_json`，按同一份 searchable text 生成本地 hash-vector；搜索合并 FTS 和 vector 分数，也可返回 vector-only 近似召回。
- WebUI 在成功任务结果下方提供可编辑确认表单和相似 Case 列表，顶部 `Memory` 页面提供 LLM-assisted import 和管理入口。
- 新任务保存 `case_context.json` 并在分析 prompt 中提供历史 Case 参考。
- Case 可禁用，不做硬删除。

后续：

- 接入外部 embedding provider。
- 将 Case 引用升级为更正式的 analysis evidence bundle。
- 按需要接入 sqlite-vec 或迁移到 PostgreSQL + pgvector，替换或增强当前 V2 本地 hash-vector recall。

## 迭代位置

Case 基础功能当前状态：

- 人工确认：已实现。
- 手工录入 API：已实现。
- WebUI 管理页面：已实现基础搜索、录入、编辑和启用状态切换。
- Case 存储：已实现 SQLite Memory 主索引和 legacy JSON 同步。
- Top N 相似召回：已实现 SQLite FTS/BM25 + 关键词 fallback 评分。
- V2 本地 vector 召回：已实现 hash-vector recall、hybrid/vector-only 搜索和 Case 编辑后向量刷新。
- 外部 embedding 生成：后续实现。

Case 合并、批量管理和外部 embedding/vector index 召回可以后续增强。

## 召回流程

1. 新任务开始分析前，根据用户问题、日志摘要和错误模式构造检索文本；V2 同时生成本地 hash-vector query。
2. 召回 Top 5 相似 Case，优先合并 FTS/BM25、关键词和本地 vector 相似度。
3. 将相似 Case 写入 `case_context.json`，由 Claude Code 作为背景参考输入。
4. 分析结果中标明历史 Case 只是参考，不替代当前任务证据。

Case Store 只接收人工确认后的 Case。任务来源 Case 必须来自 Analysis Orchestrator 最终结果并经过人工确认；手工录入 Case 由用户直接确认。中间假设、待验证信息、隐藏推理和被用户否定的结论不得沉淀为 Case。
