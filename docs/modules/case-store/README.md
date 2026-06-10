# Case Store 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

当前 MVP 已用 Rust Server 内部模块实现本地 JSON 存储和关键词重叠召回；后续再接 embedding、SQLite 或 PostgreSQL + pgvector。

## Embedding 配置

embedding 模型必须配置化。

```yaml
embedding:
  provider: "openai_compatible"
  model: "text-embedding-3-small"
  api_key_env: "LOGAGENT_EMBEDDING_API_KEY"
  store: "sqlite"
```

## 职责

Case Store 负责把人工确认后的分析结果沉淀为可复用经验，并在新任务中召回相似历史 Case。

当前实现位于 `server/src/stores/case_store.rs` 和 `server/src/stores/case_import_store.rs`，通过受保护 API 暴露：

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

存储目录为 `storage.data_dir/cases/`，每个 Case 一个 JSON 文件。Server 启动时加载到内存，保存和更新使用临时文件 rename。当前开发阶段使用 Case schema v2，不做 v1 旧数据兼容；旧 JSON 需要清空或重新生成。

新任务创建时会按用户问题召回最多 5 个启用 Case，并把结果固化到 workspace 的 `case_context.json`。Artifacts API 返回 `caseContext`，LLM Gateway 会把该上下文加入 prompt 的“历史 Case 参考”段落，并明确要求历史 Case 只能作为参考，最终结论仍必须引用当前任务证据。

## 人工确认

任务分析完成后，WebUI 提供：

- 确认为 Case
- 修改后确认
- 放弃

顶部 `Cases` 页面提供独立 Case Store 管理入口：

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

- Case 写入本地 JSON 文件。
- Case import 草稿写入 `storage.data_dir/case_imports/`，确认后再写入 `storage.data_dir/cases/`。
- Case schema v2 使用 `source_type` 区分任务确认 Case 和手工录入 Case。
- 服务端内存加载后做关键词重叠评分。
- WebUI 在成功任务结果下方提供可编辑确认表单和相似 Case 列表，顶部 `Cases` 页面提供 LLM-assisted import 和管理入口。
- 新任务保存 `case_context.json` 并在分析 prompt 中提供历史 Case 参考。
- Case 可禁用，不做硬删除。

后续：

- 生成 embedding。
- 将 Case 引用升级为更正式的 Analysis Agent evidence bundle。
- 迁移到 PostgreSQL + pgvector。

## 迭代位置

Case 基础功能当前状态：

- 人工确认：已实现。
- 手工录入 API：已实现。
- WebUI 管理页面：已实现基础搜索、录入、编辑和启用状态切换。
- Case 存储：已实现本地 JSON。
- Top N 相似召回：已实现关键词重叠评分。
- embedding 生成：后续实现。

Case 合并、批量管理和 embedding 召回可以后续增强。

## 召回流程

1. 新任务开始分析前，根据用户问题、日志摘要和错误模式生成查询向量。
2. 召回 Top 5 相似 Case。
3. 将相似 Case 加入 Analysis Agent evidence bundle，由 LLM Gateway 作为参考输入。
4. 分析结果中标明历史 Case 只是参考，不替代当前任务证据。

Case Store 只接收人工确认后的 Case。任务来源 Case 必须来自 Analysis Agent 最终结果并经过人工确认；手工录入 Case 由用户直接确认。中间假设、待验证信息、隐藏推理和被用户否定的结论不得沉淀为 Case。
