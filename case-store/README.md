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

当前实现位于 `server/src/case_store.rs`，通过受保护 API 暴露：

- `POST /api/tasks/:task_id/case`：成功任务人工确认后保存为 Case。
- `GET /api/cases?query=<text>&limit=5`：按关键词召回启用 Case。
- `GET /api/cases/:case_id`：读取 Case 详情。
- `PATCH /api/cases/:case_id`：编辑 Case 字段或设置 `enabled=false` 禁用。

存储目录为 `storage.data_dir/cases/`，每个 Case 一个 JSON 文件。Server 启动时加载到内存，保存和更新使用临时文件 rename。

## 人工确认

任务分析完成后，WebUI 提供：

- 确认为 Case
- 修改后确认
- 放弃

## Case 字段

- `case_id`
- `task_id`
- `product`
- `version`
- `environment`
- `instance_id`
- `cluster_id`
- `node_id`
- `title`
- `symptom`
- `root_cause`
- `solution`
- `evidence_refs`
- `source_result_path`
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
- 服务端内存加载后做关键词重叠评分。
- WebUI 在成功任务结果下方提供可编辑确认表单和相似 Case 列表。
- Case 可禁用，不做硬删除。

后续：

- 生成 embedding。
- 相似召回接入 Analysis Agent evidence bundle。
- 迁移到 PostgreSQL + pgvector。

## 迭代位置

Case 基础功能当前状态：

- 人工确认：已实现。
- Case 存储：已实现本地 JSON。
- Top N 相似召回：已实现关键词重叠评分。
- embedding 生成：后续实现。

完整 Case 编辑和高级管理可以后续增强。

## 召回流程

1. 新任务开始分析前，根据用户问题、日志摘要和错误模式生成查询向量。
2. 召回 Top 5 相似 Case。
3. 将相似 Case 加入 Analysis Agent evidence bundle，由 LLM Gateway 作为参考输入。
4. 分析结果中标明历史 Case 只是参考，不替代当前任务证据。

Case Store 只接收 Analysis Agent 最终结果且必须经过人工确认。中间假设、待验证信息、隐藏推理和被用户否定的结论不得沉淀为 Case。
