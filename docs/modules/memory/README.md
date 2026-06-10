# Memory 方案

Memory 是 Server 内部的本地知识后端。第一阶段只启用 `memoryType=case`，把已确认故障 Case 存入 SQLite 并继续通过现有 `/api/cases*` 接口对外兼容。

## 当前实现

- Rust 实现，位于 `server/src/stores/memory_store.rs`。
- SQLite 主库：`storage.data_dir/memory/memory.sqlite`。
- 表：`memory_items`、`memory_chunks`、`memory_chunks_fts`。
- 启动时从 `storage.data_dir/cases/*.json` 按 `caseId` idempotent 导入。
- 旧 JSON 文件保留，新增和更新 Case 继续同步写 JSON，作为迁移和回滚源。
- 搜索先过滤 `memoryType=case`、`status=active`、`enabled`，再合并 FTS/BM25 和关键词重叠分数。
- FTS 创建或查询失败时回退到关键词重叠召回。

## 边界

Memory 不替代 System Context。System Context 仍负责 Prompt Pack、架构、Runbook、Metadata adapter 等背景资源；Memory 当前只承载人工确认后的可复用 Case。

## 后续

- 接入 embedding 配置和向量索引，当前配置默认 disabled。
- 扩展非 Case 的 memory type。
- 将召回结果升级为更正式的 Analysis Agent evidence bundle。
