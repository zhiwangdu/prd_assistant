# Case Store Spec

## 目标

Case Store 保存已确认故障 Case，并支持后续任务相似召回。

## 当前状态

未实现代码，已有设计方向。

## 输入

- LLM 分析结果
- 人工确认后的标题、现象、根因、解决方案
- 关键证据引用
- 产品和版本

## 输出

- 相似 Case 列表
- Case 详情
- 可编辑 Case 记录

## 存储

MVP 可先用本地文件或 SQLite。pgvector 不是第一版硬依赖。

建议字段：

- `case_id`
- `product`
- `versions`
- `title`
- `symptom`
- `root_cause`
- `solution`
- `evidence_refs`
- `created_at`
- `updated_at`
- `enabled`

## 验收标准

- 人工确认后可保存 Case。
- 新任务可按产品、关键词和相似度召回 Case。
- Case 可禁用而不是硬删除。
- README 和 SPEC 在存储结构或召回策略变更时同步更新。
