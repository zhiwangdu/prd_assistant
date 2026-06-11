# WebUI Spec

## 目标

提供 System Context 管理、日志上传、调查交互、证据查看和 openGemini Metadata 可视化。前端使用 React + Vite + TypeScript + Tailwind CSS，shadcn/ui 负责基础交互组件。

## Metadata ViewModel

输入为 Server 归一化快照：

- `instance`
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
- Metadata Explorer 不渲染 Graph，而是提供两个可切换视角。Node / DBPT / Shards 视角派生可筛选、可级联展开的 PT 视图：

```text
Database
  -> DataNode
    -> DBPT
      -> Shard rows with time range and IndexID / Index status
```

- `Shard.IndexID -> Index.ID` 作为 Shard 到 Index 的交叉关联。
- 支持 Database、DataNode、时间范围、仅异常、Shard 行和 Index 信息显隐筛选。
- 点击 DBPT 展示聚合指标、异常和时间范围。
- DB / RP / Shards / Indexes 视角必须按 `Database -> RP -> ShardGroup/IndexGroup -> Shard/Index` 级联展开，不默认铺开全部明细表。
- Schemas 页面默认选择第一个非 `_internal` DB 及其第一个 RP；如果只有 `_internal` 则选择 `_internal`；RP 筛选必须随 DB 联动，Measurement/field 筛选用于缩小结果。
- Schema field type 必须按 openGemini 类型码展示：0 unknown、1 int、2 uint、3 float、4 string、5 boolean、6 tag、7 last。
- Metadata 明细表必须在长列表滚动时固定表头，确保用户下翻大量节点、分片、索引或字段时仍能看到字段含义。
- Diagnostics。

`Shard.Owners` 和 `Index.Owners` 必须解释为 PT ID，禁止直接当作 NodeID。

## 页面

- 顶部栏全局产品名为 `LogAgent Analysis Workbench`，副标题描述 evidence、memory、system context 和 tools，避免把整个 WebUI 限定为 Metadata Console。
- 顶部导航默认选中 `Log Analysis`，可见顺序必须是 `Log Analysis`、`Memory`、`System Context`、`Tools`。
- Metadata 不再作为顶层导航项；Metadata 拓扑和导入能力仍通过 System Context 内的 Metadata tab 进入。
- Metadata 导入区必须提供三种导入方式：实时 URL 加载、JSON 文件上传、手动 JSON 文本输入。三种方式都先生成导入预览，再由用户确认写入 Metadata Store。
- 实时 URL 加载面向 openGemini `/getdata`，必须要求 InstanceID；JSON 文件和 JSON 文本使用 `templateType=json`，完整模板 JSON 可不填写 InstanceID，openGemini 原始 JSON 仍要求 InstanceID。
- Nodes 页面中 MetaNode 状态固定展示 none；Data/SQL 节点按 0 none、1 alive、2 leaving、3 left、4 failed 映射状态。
- Log Analysis
- Memory
- System Context
- Overview
- Nodes
- Partitions
- Metadata Explorer
- Schemas
- Diagnostics
- Raw JSON
- Tools
- Evidence timeline

## Analysis 交互

- 状态展示使用 `QUEUED`、`RUNNING`、`WAITING_FOR_USER`、`WAITING_FOR_APPROVAL`、`SUCCEEDED`、`FAILED`。
- 执行阶段作为次级进度展示，不能由前端直接修改。
- `WAITING_FOR_USER` 按 `questionId` 提交回答，重复提交使用幂等 key。
- `WAITING_FOR_APPROVAL` 展示动作类型、原因、目标范围和风险；拒绝时可填写原因。
- 当前 WebUI 已在 Task execution 卡片内展示 pending prompt / pending approval，并通过 Server API 恢复任务。
- 时间线来自服务端事件摘要，不渲染隐藏思维链或未经清洗的 Provider 原始响应。
- Log Analysis 必须以 Session 为唯一历史入口。未选择 Session 时只显示新建入口；选择后展示 Session draft editor、uploads、active run、历史 runs 和 Evidence timeline。
- `title/question/sourceUrl/instanceId/nodeId` 草稿输入 debounce PATCH 到 `/api/sessions/:session_id`，刷新页面后从 Server 恢复。
- `Session draft` 必须支持展开/收起；启动分析 run 后自动收起，收起态展示 title、question、source URL、metadata 绑定、upload/run 数量和 session 状态摘要。
- 上传仍使用 `/api/uploads*`，上传完成后调用 `/api/sessions/:session_id/uploads` 附加到当前 Session；上传是可选输入，用户只填写 `question` 也可以启动分析。
- Session draft 可选择 System Context resources，选择结果 debounce PATCH 到 `systemContextIds`；创建 run 后 artifacts 展示 `system_context.json` 摘要。
- `Start analysis` 调用 `/api/sessions/:session_id/tasks`，每次创建新的 task run；同一 Session 可以多次运行。
- Runs 面板、收起态 timeline 和 Case 确认区优先展示 task `alias`，没有 alias 时使用状态/时间回退标题；不能把 `task_...` 作为主要显示名称。
- WebUI 选择 Session 时 best-effort 调用 Native Agent `PUT /workspace/current` 设置 Chrome 导入目标；失败只提示，不阻断 WebUI 上传。
- Evidence timeline 使用 `/api/sessions/:session_id/timeline`，合并 Session events 和 task analysis events。
- Evidence timeline 必须支持展开/收起；task 到达 `SUCCEEDED` 或 `FAILED` 后自动收起，收起态只展示最终结果摘要、失败摘要或当前 run 状态，用户可手动重新展开查看完整事件。
- `Task execution` 必须实时轮询 `/api/tasks/:task_id/analysis`，展示 loop revision、预算、最近事件、LLM callId/attempt/schema retry、model decision、action 和 evidence 摘要。
- 最终结果按 evidence ref 跳转到对应 artifact。
- 最终结果下方提供 Case 确认表单，允许用户在保存前修改 title、symptom、rootCause 和 solution。
- WebUI 通过 `GET /api/cases` 展示相似 Case，支持关键词搜索和禁用 Case，并展示 schema v2 的 `sourceType` 来源。
- 顶部 `Memory` 页面必须支持 `GET /api/cases` 搜索列表、Case import 文本/文本文件导入、LLM 结构化草稿、缺失信息追问、确认保存、`PATCH /api/cases/:case_id` 详情编辑和启用/禁用。直接 `POST /api/cases` 保留为后端兼容能力，不再作为主录入 UI。
- 成功任务 artifacts 中存在 `caseContext` 时，WebUI 必须展示任务创建时召回的历史 Case，并说明其仅作分析参考。
- 成功任务 artifacts 中存在 `textInput` 时，WebUI 必须展示任务创建时固化的对话框输入，并支持 `session_text_input.json#question` evidence ref 跳转。
- 最终结果中的 `case_context.json#cases/<index>` evidence ref 必须能跳转到对应历史 Case context 条目。
- 页面初始化从 `GET /api/sessions` 读取最近 Session，不以 localStorage 作为任务真源。
- 创建任务后每秒读取任务详情，终态停止；`SUCCEEDED` 再读取 artifacts，`FAILED` 展示失败阶段和消息。
- 创建任务提交用户问题，展示 `GENERATE_RESULT` 阶段，并在成功后读取结构化 LLM 结果。
- 创建任务可提交 `instanceId` / `nodeId`，历史任务详情展示 Server 解析值及 Metadata context artifact。`clusterId` 已从用户输入中移除，仅作为后端兼容字段存在。
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
- `scripts/build-webui.sh` 要求 `LOGAGENT_WORK_DIR`，构建后把静态产物同步到 `$LOGAGENT_WORK_DIR/webui/out`；`scripts/server-service.sh` 从该工作目录启动 Server。
- API Key 保存在本机 localStorage。
- LLM debug 开关不持久化在浏览器侧，页面刷新时以 Server 返回值为准。
- Raw JSON 来自 Server 返回的原始快照，不在浏览器执行任何内容；Raw JSON UI 必须按需展开，不能在初始 render 时全量 stringify 大对象。

## 验收

- `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/` 返回 Vite 构建页面。
- Metadata 页面展示已导入 Instance 列表，并按 InstanceID 读取已存快照。
- openGemini 实时加载和导入预览必须要求用户手工输入 InstanceID，并支持可选备注名。
- Metadata 页面能上传 `.json` 文件生成导入预览并确认写入。
- Metadata 页面能粘贴 JSON 文本生成导入预览并确认写入。
- Metadata 列表展示备注名时必须单行省略，Overview 展示备注字段。
- 能在用户输入 InstanceID 后从 `127.0.0.1:8091/getdata` 加载真实数据。
- `Owners:[0]` 显示为 PT 0，并经 PtView 映射到 DataNode。
- `testmst` 映射到 `testmst_0000` 并展示 Schema。
- Metadata Explorer 在大集群下通过 `Database -> DataNode -> DBPT -> Shards` 级联展开可用，不渲染 Graph。
- Metadata Explorer 的 DB/RP 视角按 Database/RP/ShardGroup/IndexGroup 层级折叠，展开后能看到 Shard 和 Index 明细。
- Schemas 页面默认选择非 `_internal` DB 和首个 RP，RP 随 DB 联动，field type 显示为实际类型名称。
- Raw JSON 页面打开大对象时不应卡死，默认只展示顶层结构并按需展开子树。
- Metadata 和 Tools 长表格下翻时表头固定在表格顶部。
- 缺失 DataNode/PT 以异常容器展示，不能静默丢弃。
- 原有日志上传和 evidence 查看能力保持可用。
- 页面刷新后能恢复 Server Session 列表、草稿、uploads、run 列表和运行状态；没有 uploads 的 Session 也能点击 `Start analysis`。
- 点击 `Start analysis` 后 Session draft 自动收起；运行中的 timeline 可展开查看事件，任务成功或失败后 timeline 自动收起并展示结果摘要或错误摘要。
- 成功任务轮询到 alias 后，Runs 列表和详情标题展示 alias 而不是裸 task ID。
- 顶部栏展示 `LogAgent Analysis Workbench`，不再展示 `LogAgent Metadata Console`。
- stub 任务能展示 LLM 最终结果，真实 Provider 失败能展示 `GENERATE_RESULT` 错误。
- 运行中任务能实时展示 analysis loop 事件摘要和预算计数。
- LLM debug 开关能通过 API 开启/关闭 Server 侧 response content 日志。
- 带 Metadata 的任务能展示产品、版本、环境、节点状态、数据库和 PT 摘要。
- 带 Tool Runner 产物的任务能展示 tool result；存在 `findings` 时展示 severity、file、line 和 message。
- stub 任务能完成追问、审批、恢复和最终结果展示。
- 成功任务能保存为 Case；保存后相似 Case 列表能召回该 Case，禁用后默认列表不再展示。
- 新任务召回到历史 Case 时，成功任务详情能展示 `caseContext`。
- Memory 背后的 Case schema v2 响应中的 `taskId` 和 `sourceResultPath` 可为空，前端不能假设所有 Case 都绑定任务。
- 顶部导航能进入 `Memory` 页面；粘贴文本或上传 UTF-8 文本类文件后可生成结构化草稿，缺失必填字段时能提交补充回答，确认保存后可在列表中搜索、选择、编辑并切换启用状态。
- 顶部导航能进入 `Tools` 页面；工具目录来自 `GET /api/tools`，工具运行来自 `/api/tools/runs`，不能混入 Log analysis 任务列表。
- Tools 页面首版支持 `pprof_analyzer` 上传 profile、设置 sample index/node count/SVG 开关、创建 `tool_run` task、轮询状态并展示 top 函数表和 artifact 路径。
- 顶部导航能进入 `System Context` 页面；可创建和激活 Prompt Pack、Architecture Mermaid、Runbook/Knowledge，并能从 Metadata tab 继续使用原 Metadata 拓扑页面。
- Log Analysis 选择 System Context 后刷新能恢复选择，创建 run 后能在 artifacts 中看到固化的 System Context snapshot。
