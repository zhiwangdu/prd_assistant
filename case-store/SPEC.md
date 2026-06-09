# Case Store Spec

## 目标

Case Store 保存已确认故障 Case，并支持后续任务相似召回。

## 当前状态

已实现 MVP：

- Server 内部模块 `server/src/case_store.rs`。
- 本地 JSON 文件存储，目录为 `storage.data_dir/cases/`。
- Case schema v2 使用 `sourceType` 区分 `task` 和 `manual` 来源；开发阶段不兼容 v1 旧数据。
- 成功任务可通过 `POST /api/tasks/:task_id/case` 人工确认保存为 Case。
- 手工 Case 可通过 `POST /api/cases` 直接录入，不绑定任务。
- `GET /api/cases` 支持关键词召回，默认只返回 `enabled=true` 的 Case。
- `PATCH /api/cases/:case_id` 支持编辑文本、元信息、证据引用和禁用 Case。
- WebUI 在成功任务最终结果下方提供确认表单、相似 Case 列表和禁用操作。
- WebUI 顶部 `Cases` 页面支持搜索、手工录入、详情编辑和启用/禁用。

未实现：

- embedding 生成。
- embedding 召回。
- 将 Case 引用升级为更正式的 Analysis Agent evidence bundle。
- Case 合并和批量管理。

## 输入

- Analysis Agent 最终结果
- 人工确认后的标题、现象、根因、解决方案
- 手工录入的标题、现象、根因、解决方案
- 关键证据引用
- 产品和版本

## 输出

- 相似 Case 列表
- Case 详情
- 可编辑 Case 记录
- Case Store 管理页面

## API

```http
POST /api/tasks/:task_id/case
POST /api/cases
GET /api/cases?query=<text>&limit=5&includeDisabled=false
GET /api/cases/:case_id
PATCH /api/cases/:case_id
```

`POST /api/tasks/:task_id/case` 只接受 `SUCCEEDED` 任务。请求可覆盖 `title`、`symptom`、`rootCause`、`solution`、`evidenceRefs`、`product`、`version` 和 `environment`；未提供字段从最终 `AnalysisResult` 和 `metadata_context.json` 派生。生成记录为 `sourceType=task`，必须包含 `taskId` 和 `sourceResultPath`。

`POST /api/cases` 创建 `sourceType=manual` 记录。请求必须包含 `title`、`symptom`、`rootCause` 和 `solution`；可选 `product`、`version`、`environment`、`instanceId`、`nodeId`、`evidenceRefs` 和 `enabled`。手工 Case 不包含 `taskId` 和 `sourceResultPath`。

新任务创建时会写入：

```text
workspaces/<task_id>/case_context.json
```

`GET /api/tasks/:task_id/artifacts` 返回 `caseContextPath` 和 `caseContext`。LLM Gateway 会读取该上下文并加入 prompt，但历史 Case 只作为参考，不替代当前任务证据。

## 存储

MVP 当前使用本地 JSON 文件。pgvector 不是第一版硬依赖。

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

当前召回策略为关键词重叠评分：

- 查询文本按空白、逗号和分号切分。
- 检索字段包括 title、symptom、rootCause、solution、product、version、environment、instanceId、nodeId 和 evidenceRefs。
- 未提供 query 时按创建时间返回最近启用 Case。
- 禁用 Case 默认不返回，除非 `includeDisabled=true`。
- 新任务创建使用用户问题作为 query，召回最多 5 个启用 Case。

## 验收标准

- 人工确认后可保存 Case。
- 手工录入可保存为 `sourceType=manual` Case，且不需要任务 ID。
- 新任务可按产品、关键词和相似度召回 Case。
- Case 可禁用而不是硬删除。
- 未完成、未确认或仅包含中间假设的分析不可保存为 Case。
- 重复确认同一 task 时返回已有 Case，不创建重复记录。
- `sourceType=task` Case 必须有 `taskId` 和 `sourceResultPath`；`sourceType=manual` Case 禁止带这两个字段。
- 新任务 artifacts 能返回 `caseContext`，LLM prompt 包含历史 Case 参考段落。
- WebUI 顶部 `Cases` 页面能完成手工录入、搜索、编辑和启用状态切换。
- README 和 SPEC 在存储结构或召回策略变更时同步更新。
