# Case Store 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

MVP 阶段可用 Rust 管理 JSONL/SQLite 存储和余弦相似度计算；后续再接 PostgreSQL + pgvector。

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

## 人工确认

任务分析完成后，WebUI 提供：

- 确认为 Case
- 修改后确认
- 放弃

## Case 字段

- `id`
- `title`
- `symptom`
- `root_cause`
- `solution`
- `confirmed`
- `created_at`

## embedding 文本

```text
title + symptom + root_cause + solution
```

## MVP 存储策略

第一版：

- embedding 写入本地 JSONL 或 SQLite。
- 服务端内存加载后做余弦相似度。

后续：

- 迁移到 PostgreSQL + pgvector。

## 迭代位置

Case 基础功能应在第 1 阶段并行完成：

- 人工确认
- Case 存储
- embedding 生成
- Top 5 相似召回

完整 Case 编辑和高级管理可以后续增强。

## 召回流程

1. 新任务开始分析前，根据用户问题、日志摘要和错误模式生成查询向量。
2. 召回 Top 5 相似 Case。
3. 将相似 Case 加入 LLM Agent 输入。
4. 分析结果中标明历史 Case 只是参考，不替代当前任务证据。
