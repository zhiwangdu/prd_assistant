# Memory / Case Store Spec

## 目标

Memory 保存已确认故障 Case，并支持后续任务相似召回。现有 Case Store API 是 Memory 的兼容层。

## 当前状态

已实现 MVP：

- Python V2 clean-room Server 使用 `LOGAGENT_V2_DATA_DIR/logagent.sqlite`
  的 `cases` 表作为主索引，并保留同级 `cases/*.json` legacy 层；初始化
  时按 `caseId` 幂等 upsert schema v2 JSON，创建或编辑 Case 后先更新
  SQLite、FTS 和本地 vector，再原子写回 `cases/<caseId>.json`。
- Case import 草稿存储在 `storage.data_dir/case_imports/`。
- Case schema v2 使用 `sourceType` 区分 `task` 和 `manual` 来源；开发阶段不兼容 v1 旧数据。
- 成功任务可通过 `POST /api/tasks/:task_id/case` 人工确认保存为 Case。
- 手工 Case 可通过 `POST /api/cases` 直接录入，不绑定任务。
- `GET /api/cases` 支持 SQLite FTS/BM25 + 关键词 fallback 召回，默认只返回 `enabled=true` 的 Case。
- `PATCH /api/cases/:case_id` 支持编辑文本、元信息、证据引用和禁用 Case。
- WebUI 在成功任务最终结果下方提供确认表单、相似 Case 列表和禁用操作。
- WebUI 顶部 `Memory` 页面支持搜索、LLM-assisted 文本导入、缺失信息追问、确认保存、详情编辑和启用/禁用。
- Python V2 clean-room Server 已实现本地 hash-vector recall：Case 主表保存 `vector_json`，搜索合并 SQLite FTS5/BM25、关键词 fallback 和 vector 相似度，必要时返回 vector-only 近似召回。

未实现：

- 外部 embedding provider 生成。
- sqlite-vec、pgvector 等外部 vector index。
- 将 Case 引用升级为更正式的 analysis evidence bundle。
- Case 合并和批量管理。

## 输入

- Analysis Orchestrator 最终结果
- 人工确认后的标题、现象、根因、解决方案
- 手工录入的标题、现象、根因、解决方案
- 关键证据引用
- 产品和版本

## 输出

- 相似 Case 列表
- Case 详情
- 可编辑 Case 记录
- Memory 管理页面

## API

```http
POST /api/tasks/:task_id/case
POST /api/cases
POST /api/cases/imports
GET /api/cases/imports/:draft_id
PATCH /api/cases/imports/:draft_id
POST /api/cases/imports/:draft_id/messages
POST /api/cases/imports/:draft_id/confirm
GET /api/cases?query=<text>&limit=5&includeDisabled=false
GET /api/cases/:case_id
PATCH /api/cases/:case_id
POST /api/mcp/readonly
```

Python V2 clean-room Server 使用 `/api/v2/cases*` 命名空间提供等价能力：

```http
POST /api/v2/tasks/:task_id/case
POST /api/v2/runs/:run_id/case
POST /api/v2/cases
POST /api/v2/cases/imports
POST /api/v2/cases/imports/preview
POST /api/v2/cases/imports/:import_id/messages
PATCH /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/:import_id/confirm
GET /api/v2/cases
GET /api/v2/cases/:case_id
PATCH /api/v2/cases/:case_id
```

`POST /api/tasks/:task_id/case` 只接受 `SUCCEEDED` 任务。请求可覆盖 `title`、`symptom`、`rootCause`、`solution`、`evidenceRefs`、`product`、`version` 和 `environment`；未提供字段从最终 `AnalysisResult` 和 `metadata_context.json` 派生。生成记录为 `sourceType=task`，必须包含 `taskId` 和 `sourceResultPath`。Python V2 保留原生 `POST /api/v2/runs/:run_id/case`，并提供 Rust/V1-style `POST /api/v2/tasks/:task_id/case` alias；两者共享同一任务确认和重复确认幂等逻辑。

`POST /api/cases` 创建 `sourceType=manual` 记录。请求必须包含 `title`、`symptom`、`rootCause` 和 `solution`；可选 `product`、`version`、`environment`、`instanceId`、`nodeId`、`evidenceRefs` 和 `enabled`。手工 Case 不包含 `taskId` 和 `sourceResultPath`。

Case import API 创建未确认草稿，不直接写入 Case Store。`POST /api/cases/imports` 支持 JSON 文本和 multipart UTF-8 文本类文件；PDF/DOCX 暂不解析。LLM Gateway 将原始材料整理为 `structuredCase`，如果缺少 `title`、`symptom`、`rootCause` 或 `solution`，返回 `missingFields` 和 `assistantQuestion`。`POST /api/cases/imports/:draft_id/messages` 追加用户补充并重新整理，`PATCH /api/cases/imports/:draft_id` 保存手工修正，`POST /api/cases/imports/:draft_id/confirm` 只有在必填字段完整时才创建 `sourceType=manual` Case。

V2 Case import 使用同样的产品语义：`POST /api/v2/cases/imports` 是 Rust/V1-style
create alias，接受 JSON V1 `text` / V2 `content`、multipart `text`/`content`
字段或 UTF-8 文本文件 `file`，返回 HTTP 201 并同时提供 `import` 和 `draft`
字段；文件导入只接受文本/json/yaml content type 或 `.txt`、`.text`、`.md`、
`.markdown`、`.log`、`.json`、`.yaml`、`.yml`、`.csv` 文件名；preview 持久化
source text、draft、validation errors 和 messages；
messages endpoint 追加用户补充，合并原文与消息重新解析，并在仍缺字段时生成下一轮
assistant question；PATCH 保存未确认草稿的人工修正并重算 validation errors；confirm 仍只在
必填字段完整时创建 `manual` Case，已确认草稿拒绝继续修改。

新任务创建时会写入：

```text
workspaces/<task_id>/case_context.json
```

`GET /api/tasks/:task_id/artifacts` 返回 `caseContextPath` 和 `caseContext`。LLM Gateway 会读取该上下文并加入 prompt，但历史 Case 只作为参考，不替代当前任务证据。

只读 HTTP MCP 只允许读取 recent/search/get Case，不允许写入、确认、编辑或禁用 Case。`logagent://cases/recent` 返回 Rust/V1 默认的最近 20 个 enabled Case；`logagent.search_cases` 使用 Rust/V1 只读入口的默认 `limit=5` 和 `limit=1..50` 上限。
Python V2 task MCP 额外提供 Rust V1 兼容 `logagent.recall_cases`，只召回 enabled
Case，并把结果作为 `case_context` background evidence 持久化；响应返回
`artifactPath`、`caseCount` 和逐 Case `evidenceRefs`，默认 `limit=5` 且按 1..20 裁剪；该背景不能作为最终根因证据引用。

## 存储

MVP 当前使用本地 SQLite。pgvector 不是第一版硬依赖。

SQLite 表：

- `memory_items`：Memory item 元数据和完整 `CaseRecord` JSON，第一阶段 `memoryType=case`。
- `memory_chunks`：用于检索的文本 chunk，Case 当前写入单 chunk。
- `memory_chunks_fts`：FTS5 索引。创建或查询失败时不阻断 Case API，Server fallback 到关键词重叠评分。
- V2 `cases.vector_json`：由 searchable text 生成的本地 hash-vector，用于轻量相似召回和 FTS 结果重排；不需要外部服务。

建议字段：

- `schema_version`: 固定为 2
- `case_id`
- `source_type`: `task` / `manual`
- `task_id`: 仅任务来源 Case 必填
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
- `source_result_path`: 仅任务来源 Case 必填
- `created_at`
- `updated_at`
- `enabled`

## 召回策略

当前召回策略：

- 先过滤 `memoryType=case`、`status=active`、`enabled`。
- 查询文本按空白、逗号和分号切分用于关键词 fallback；FTS 查询会进一步拆分 hyphen 等符号。
- SQLite FTS/BM25 分数和关键词重叠分数合并排序。
- V2 同时计算本地 hash-vector 相似度，并把 vector 结果与 FTS/关键词结果合并；FTS 命中可返回 `searchBackend=hybrid` 和 `vectorScore`，纯向量召回返回 `searchBackend=vector`。
- 检索字段包括 title、symptom、rootCause、solution、product、version、environment、instanceId、nodeId 和 evidenceRefs。
- 未提供 query 时按创建时间返回最近启用 Case。
- 禁用 Case 默认不返回，除非 `includeDisabled=true`。
- 新任务创建使用用户问题作为 query，召回最多 5 个启用 Case。

## 验收标准

- 人工确认后可保存 Case。
- 文本导入或直接 API 手工录入可保存为 `sourceType=manual` Case，且不需要任务 ID。
- Case import 缺少必填字段时必须阻止确认保存，并提供可继续回答的问题。
- 新任务可按产品、关键词和 FTS 相似度召回 Case。
- V2 新任务可通过本地 hash-vector recall 召回语义相近但精确 token 不完全一致的 Case。
- Case 可禁用而不是硬删除。
- 未完成、未确认或仅包含中间假设的分析不可保存为 Case。
- 重复确认同一 task 时返回已有 Case，不创建重复记录。
- `sourceType=task` Case 必须有 `taskId` 和 `sourceResultPath`；`sourceType=manual` Case 禁止带这两个字段。
- 新任务 artifacts 能返回 `caseContext`，LLM prompt 包含历史 Case 参考段落。
- 只读 HTTP MCP 可以搜索和读取 Case，且不能修改 Case Store。
- WebUI 顶部 `Memory` 页面能完成手工录入、搜索、编辑和启用状态切换。
- 启动迁移重复执行时不能创建重复 Case，legacy JSON 文件必须保留。
- README 和 SPEC 在存储结构或召回策略变更时同步更新。
