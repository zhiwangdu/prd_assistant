# Memory Spec

## 目标

Memory 为 LogAgent 提供本地、可迁移的跨任务知识索引。Phase 1 只实现 confirmed case 记忆，保持现有 Case API、Case schema 和 `case_context.json` 引用兼容。

## 存储

```text
storage.data_dir/
  memory/
    memory.sqlite
  cases/
    case_xxx.json
```

SQLite 表：

- `memory_items(memory_id, memory_type, status, enabled, source_id, record_json, searchable_text, created_at, updated_at)`
- `memory_chunks(chunk_id, item_id, chunk_index, content)`
- `memory_chunks_fts(item_id, chunk_id, content)`

第一阶段：

- `memory_type = case`
- `memory_id = caseId`
- `source_id = taskId` for task cases
- `record_json` 保存完整 `CaseRecord`

## 兼容

- `/api/cases*` 不改路径或响应 shape。
- `CaseRecord`、`CaseSearchHit` 不改字段。
- `case_context.json#cases/<index>` evidence refs 保持有效。
- `storage.data_dir/cases/*.json` 启动导入必须按 `caseId` 幂等，不能删除源文件。

## 召回

- 默认只返回 `enabled=true`、`status=active`、`memoryType=case`。
- FTS/BM25 命中与关键词重叠分数合并排序。
- FTS 不可用时 fallback 到关键词重叠。
- 未提供 query 时按创建时间返回最近 Case。

## 配置

Embedding 配置已预留但默认关闭：

```yaml
embedding:
  enabled: false
  provider: "openai_compatible"
  model: "text-embedding-3-small"
  api_key_env: "LOGAGENT_EMBEDDING_API_KEY"
  store: "sqlite"
```

Phase 1 不要求生成 embedding，也不要求安装 sqlite-vec。

## 验收

- Legacy JSON Case 启动后可在 SQLite Memory 中检索。
- 重复启动迁移不产生重复 Case。
- 创建、更新、禁用 Case 通过现有 API 继续工作。
- 新任务仍写入兼容的 `case_context.json`。
- FTS 查询可召回简单关键词重叠不易命中的 Case；FTS 失败时服务保持可用。
