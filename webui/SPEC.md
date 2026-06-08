# WebUI Spec

## 目标

提供日志上传、调查交互、证据查看和 openGemini Metadata 可视化。前端使用 React + Vite + TypeScript + Tailwind CSS，shadcn/ui 负责基础交互组件，React Flow 负责拓扑图。

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
- DataNode-centric 容器拓扑：

```text
DataNode
  -> Database
    -> DBPT
      -> ShardGroup -> Shard
      -> IndexGroup -> Index
```

- `Shard.IndexID -> Index.ID` 作为 Shard 到 Index 的交叉关联。
- 支持 Database、DataNode、时间范围、仅异常、Shard/Index 显隐筛选。
- 点击实体展示完整字段和关联对象。
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
- Analysis timeline（规划）

## Analysis 交互

- 状态展示使用 `QUEUED`、`RUNNING`、`WAITING_FOR_USER`、`WAITING_FOR_APPROVAL`、`SUCCEEDED`、`FAILED`。
- 执行阶段作为次级进度展示，不能由前端直接修改。
- `WAITING_FOR_USER` 按 `questionId` 提交回答，重复提交使用幂等 key。
- `WAITING_FOR_APPROVAL` 展示动作类型、原因、目标范围和风险；拒绝时可填写原因。
- 当前 WebUI 已在 Task execution 卡片内展示 pending prompt / pending approval，并通过 Server API 恢复任务。
- 时间线来自服务端事件摘要，不渲染隐藏思维链或未经清洗的 Provider 原始响应。
- `Task execution` 必须实时轮询 `/api/tasks/:task_id/analysis`，展示 loop revision、预算、最近事件、LLM callId/attempt/schema retry、model decision、action 和 evidence 摘要。
- 最终结果按 evidence ref 跳转到对应 artifact。
- 页面初始化从 `GET /api/tasks` 读取最近任务，不以 localStorage 作为任务真源。
- 创建任务后每秒读取任务详情，终态停止；`SUCCEEDED` 再读取 artifacts，`FAILED` 展示失败阶段和消息。
- 创建任务提交用户问题，展示 `GENERATE_RESULT` 阶段，并在成功后读取结构化 LLM 结果。
- 创建任务可提交 `instanceId` / `clusterId` / `nodeId`，历史任务详情展示 Server 解析值及 Metadata context artifact。
- 成功任务 artifacts 展示 `toolResults`，包括工具名、状态、退出码、耗时、摘要、结构化 findings 和 stdout/stderr 路径。
- 结果中的 grep evidence ref 可跳转到当前页面对应 match。
- 历史成功任务可重新选择并读取 artifacts；上传进度和 Server 执行进度必须独立。
- 页面顶部提供 LLM debug 开关，读写 `/api/debug/llm`。开关只影响 Server 日志中的 LLM response content，不在页面展示 Provider 原始响应。

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
- Index Owner PT 不存在。
- PT 无 Shard 或无 Index。
- DataNode 无 PT。
- ShardGroup 与 IndexGroup 时间范围不一致。

MetaNode 的 `Status=0` 不直接判定为离线；状态码必须按节点类型解释。

## 构建和部署

- Vite 输出目录固定为 `webui/out`。
- Rust Server 使用 `ServeDir("webui/out")` 托管。
- API Key 保存在本机 localStorage。
- LLM debug 开关不持久化在浏览器侧，页面刷新时以 Server 返回值为准。
- Raw JSON 来自 Server 返回的原始快照，不在浏览器执行任何内容。

## 验收

- `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/` 返回 Vite 构建页面。
- 能从 `127.0.0.1:8091/getdata` 加载真实数据。
- `Owners:[0]` 显示为 PT 0，并经 PtView 映射到 DataNode。
- `testmst` 映射到 `testmst_0000` 并展示 Schema。
- React Flow 拓扑、Diagnostics 和 Raw JSON 可用。
- 缺失 DataNode/PT 以异常容器展示，不能静默丢弃。
- 原有日志上传和 evidence 查看能力保持可用。
- 页面刷新后能恢复 Server 任务列表和运行状态。
- stub 任务能展示 LLM 最终结果，真实 Provider 失败能展示 `GENERATE_RESULT` 错误。
- 运行中任务能实时展示 analysis loop 事件摘要和预算计数。
- LLM debug 开关能通过 API 开启/关闭 Server 侧 response content 日志。
- 带 Metadata 的任务能展示产品、版本、环境、节点状态、数据库和 PT 摘要。
- 带 Tool Runner 产物的任务能展示 tool result；存在 `findings` 时展示 severity、file、line 和 message。
- stub 任务能完成追问、审批、恢复和最终结果展示。
