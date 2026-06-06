# WebUI Spec

## 目标

提供日志上传/证据查看和 openGemini Metadata 可视化。前端使用 React + Vite + TypeScript + Tailwind CSS，shadcn/ui 负责基础交互组件，React Flow 负责拓扑图。

## Metadata ViewModel

输入为 Server 归一化快照：

- `cluster`
- `nodes`
- `cluster.rawSnapshot`
- `databases[].retentionPolicies[]`
- `partitionViews[]`
- `measurements[]`
- `shardGroups[].shards[]`
- `indexGroups[].indexes[]`

前端派生：

- 节点、DB、RP、PT、Shard、Measurement、Index 数量。
- Duration 的可读格式。
- MstVersions 逻辑表/物理表映射。
- Shard -> PT -> DataNode 拓扑。
- Diagnostics。

`Shard.Owners` 和 `Index.Owners` 必须解释为 PT ID，禁止直接当作 NodeID。

## 页面

- Overview
- Nodes
- Partitions
- Topology
- Databases
- Schemas
- Diagnostics
- Raw JSON
- Log analysis

## Diagnostics

- Data/SQL 节点离线。
- `ConnID != AliveConnID`。
- PT Owner 找不到 DataNode。
- Shard Owner PT 在同 Database 的 PtView 中不存在。
- Database 默认 RP 为空或不存在。
- RP 无 ShardGroup。
- Measurement 无 Schema。
- Shard IndexID 找不到 Index。
- 未被 Shard 引用的 Index。

MetaNode 的 `Status=0` 不直接判定为离线；状态码必须按节点类型解释。

## 构建和部署

- Vite 输出目录固定为 `webui/out`。
- Rust Server 使用 `ServeDir("webui/out")` 托管。
- API Key 保存在本机 localStorage。
- Raw JSON 来自 Server 返回的原始快照，不在浏览器执行任何内容。

## 验收

- `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/` 返回 Vite 构建页面。
- 能从 `127.0.0.1:8091/getdata` 加载真实数据。
- `Owners:[0]` 显示为 PT 0，并经 PtView 映射到 DataNode。
- `testmst` 映射到 `testmst_0000` 并展示 Schema。
- React Flow 拓扑、Diagnostics 和 Raw JSON 可用。
- 原有日志上传和 evidence 查看能力保持可用。
