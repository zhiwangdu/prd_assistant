# Memory 方案

Memory 是 Server 内部的本地知识后端。第一阶段只启用 `memoryType=case`，把已确认故障 Case 存入 SQLite 并继续通过现有 `/api/cases*` 接口对外兼容。

## 当前实现

- Python V2 实现，主库为 `LOGAGENT_V2_DATA_DIR/logagent.sqlite`。
- Case 表维护 `vector_json`，基于同一份 searchable text 生成本地 hash-vector，并把 vector 相似度与 FTS/关键词召回合并。
- 同级 `cases/*.json` 作为 legacy Case JSON 迁移和回滚层；初始化时按 `caseId` 幂等导入。
- 搜索先过滤 `memoryType=case`、`status=active`、`enabled`，再合并 FTS/BM25、关键词重叠和本地 vector 分数。
- FTS 创建或查询失败时回退到关键词重叠召回；结果可标记为 `hybrid` 或 `vector`，并携带 `vectorScore`。

## 边界

Memory 不替代 System Context。System Context 仍负责 Prompt Pack、架构、Runbook、Metadata adapter 等背景资源；Memory 当前只承载人工确认后的可复用 Case。

## 后续

- 接入外部 embedding provider 和可选 vector index，当前 V2 已用本地 hash-vector recall 覆盖轻量相似召回。
- 扩展非 Case 的 memory type。
- 将召回结果升级为更正式的 analysis evidence bundle。
