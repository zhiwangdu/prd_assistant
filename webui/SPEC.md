# WebUI Spec

## 目标

提供 Skill-backed System Context、日志上传、调查交互、证据查看和 openGemini Metadata 可视化。前端使用 React + Vite + TypeScript + Tailwind CSS，shadcn/ui 负责基础交互组件。

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
- Schema field type 必须按 openGemini 类型码展示：`Field_Type_Unknown=0` -> `Unknown`、`Field_Type_Int=1` -> `Integer`、`Field_Type_UInt=2` -> `Unsigned`、`Field_Type_Float=3` -> `Float`、`Field_Type_String=4` -> `String`、`Field_Type_Boolean=5` -> `Boolean`、`Field_Type_Tag=6` -> `Tag`、`Field_Type_Last=7` -> `Unknown`。
- Metadata 明细表必须在长列表滚动时固定表头，确保用户下翻大量节点、分片、索引或字段时仍能看到字段含义。
- Diagnostics。

`Shard.Owners` 和 `Index.Owners` 必须解释为 PT ID，禁止直接当作 NodeID。

## 页面

- 顶部栏全局产品名为 `LogAgent Analysis Workbench`，副标题描述 evidence、memory、system context 和 tools，避免把整个 WebUI 限定为 Metadata Console。
- 顶部导航默认选中 `Analyze`，可见顺序必须是 `Analyze`、`Memory`、`System Context`、`Tools`、`Settings`。
- 默认 V2 页面用户可见标题必须使用 Workbench 或 Console 语义，不能继续称为临时 bridge/桥接层。源码文件名可暂时保留 `V2*Bridge.tsx` 以避免无关重命名。
- 顶部必须提供 WebUI 语言选择，当前支持 `zh-CN` 和 `en-US`，默认 `zh-CN`。语言选择保存在浏览器 localStorage；在 `Analyze` 中会同步到当前 Session 的 `analysisLanguage`，创建新 run 时由 Server 快照到 task。
- `Analyze` 中固定 UI 文案、状态、阶段、置信度和常见 timeline event 必须优先使用简体中文展示；仅当专业名词无法准确翻译时保留英文，例如 `Session`、`Case`、`Claude Code`、`MCP`、`Metadata`、`Tool Runner`、`grep`、`artifact`、`evidence ref`、`InstanceID`、`NodeID`、产品名和 JSON/path。切换到 `en-US` 时这些固定展示改为英文。
- Metadata 不再作为顶层导航项；V2 Metadata 导入、实例管理和 snapshot 查看通过 System Context 页面内的 V2 Metadata 工作台区块进入。
- Metadata 导入区必须提供三种导入方式：实时 URL 加载、JSON 文件上传、手动 JSON 文本输入。三种方式都先生成导入预览，再由用户确认写入 Metadata Store。
- 实时 URL 加载面向 openGemini `/getdata`，必须要求 InstanceID；JSON 文件和 JSON 文本使用 `templateType=json`，完整模板 JSON 可不填写 InstanceID，openGemini 原始 JSON 仍要求 InstanceID。
- Metadata 导入区必须提供 Raw JSON 刷新入口，对当前 Instance 调用 `/api/metadata/instances/:instance_id/refresh`；刷新成功后更新右侧快照和已导入列表，失败时展示 Server 错误。
- Imported Instances 列表必须提供单条删除入口，调用 `DELETE /api/metadata/instances/:instance_id`；删除当前选中 Instance 后清空右侧快照和 InstanceID 输入。
- 重复确认导入相同 InstanceID 时，UI 必须按 Server 返回的覆盖后快照展示，不能继续显示旧节点残留。
- Nodes 页面中 MetaNode 状态固定展示 none；Data/SQL 节点按 0 none、1 alive、2 leaving、3 left、4 failed 映射状态。
- Analyze
- Memory
- System Context
- System Context / Skills import
- Overview
- Nodes
- Partitions
- Metadata Explorer
- Schemas
- Diagnostics
- Raw JSON
- Tools
- Tools / Fetch
- Tools / Executors
- Settings
- Evidence timeline

## Analysis 交互

- Analyze 页面是 V2 原生入口，默认直接调用 Python V2 后端能力，不再默认渲染旧 Rust Session-first 面板。
- V2 Analyze 页面必须调用 `/api/v2/workspaces` 创建和列出 Workspace，支持选择历史 Workspace，并展示 Workspace question、mode 和 created time。
- V2 Analyze 页面必须支持读取单个 Workspace 后回填 question/mode，调用 `PATCH /api/v2/workspaces/:workspace_id` 保存选中 Workspace 的 question/mode/language，并调用 `DELETE /api/v2/workspaces/:workspace_id` 软删除历史 Workspace；删除后前端清空当前选择并刷新历史列表。
- V2 Analyze 页面必须支持对选中 Workspace 上传文件：小文件可直接调用 `/api/v2/workspaces/:workspace_id/uploads`，超过前端 chunk 阈值的文件必须走 `/api/v2/workspaces/:workspace_id/uploads/init`、`/api/v2/uploads/:session_id/chunks?offset=...` 和 `/api/v2/uploads/:session_id/complete`。
- V2 Analyze 页面必须调用 `/api/v2/workspaces/:workspace_id/runs` 创建 Run，轮询 `/api/v2/runs/:run_id/analysis`，并展示 run status、phase、timeline、evidence count、resource count、artifact count 和最终结果摘要。
- V2 Analyze 页面必须从 `/api/v2/runs/:run_id/analysis.resources` 展示运行资源摘要，包括 `analysis_state.json` 中的 LangGraph engine/graph/nodes、Agent request/response provider/model/validation、`claude_mcp_config.json`、`claude_session.json` 和 `mcp_calls.jsonl` 的 call count / last call。
- V2 Analyze 页面的 timeline 事件标签必须优先使用 Python V2 后端返回的 `kind` 字段，并兼容旧 `event_type` 字段。
- V2 Analyze 页面必须读取 `/api/v2/runs/:run_id/analysis` 中的 `pendingActions`；`WAITING_FOR_USER` 时调用 `/api/v2/runs/:run_id/messages` 提交补充或 `resumeMode=finalize`，接受返回的 `answeredActions` 并通过后续轮询刷新 pending actions，`WAITING_FOR_APPROVAL` 时调用 `/api/v2/actions/:action_id/decisions` 批准或拒绝并恢复 run。
- V2 Analyze 页面在 run `SUCCEEDED` 且存在 final answer 时必须提供 Case 保存区，用 final answer 预填标题、现象、根因、解决方案和 evidence refs，允许用户编辑后调用 `/api/v2/runs/:run_id/case` 写入 V2 Memory。
- V2 Analyze 页面的 artifact 下载必须由前端 `fetch` 携带 Authorization header 调用 `/api/v2/artifacts/:artifact_id`，不能依赖无法带 Bearer header 的裸链接。
- 状态展示使用 `QUEUED`、`RUNNING`、`WAITING_FOR_USER`、`WAITING_FOR_APPROVAL`、`SUCCEEDED`、`FAILED`。
- 执行阶段作为次级进度展示，不能由前端直接修改。
- `WAITING_FOR_USER` 按 pending action payload 中的 `questionId` 提交回答，重复提交使用由 run、action、resumeMode 和消息内容派生的稳定幂等 key。
- `WAITING_FOR_USER` 必须提供“没有更多信息，直接生成最终结果”入口，点击时调用 message API 并传 `resumeMode: "finalize"`；即使回答框为空也必须提交默认说明，使任务基于当前证据直接恢复到最终结果生成。
- `WAITING_FOR_APPROVAL` 展示动作类型、原因、目标范围和风险；拒绝时可填写原因；批准或拒绝请求使用由 run、action、decision 和原因派生的稳定幂等 key。
- 当前 WebUI 已在 Task execution 卡片内展示 pending prompt / pending approval，并通过 Server API 恢复任务。
- 时间线来自服务端事件摘要，不渲染隐藏思维链或未经清洗的 Provider 原始响应。
- Analyze 必须以 Session 为唯一历史入口。未选择 Session 时只显示新建入口；选择后展示 Session draft editor、uploads、active run、历史 runs 和 Evidence timeline。
- Session history 必须支持删除非运行中的 Session。删除前必须二次确认；删除成功后列表刷新，若删除的是当前选中 Session，则清空详情、运行记录、timeline 和 artifacts。删除只调用 `DELETE /api/sessions/:session_id`，不在前端尝试删除上传、任务或结果产物。
- `title/question/sourceUrl/instanceId/nodeId` 草稿输入 debounce PATCH 到 `/api/sessions/:session_id`，刷新页面后从 Server 恢复。
- `analysisLanguage` 作为 Session 草稿字段随 debounce PATCH 保存；新建 Session 使用当前 WebUI 语言，旧 Session 缺失该字段时按 `zh-CN` 展示和保存。
- `Session draft` 必须支持展开/收起；启动分析 run 后自动收起，收起态展示 title、question、source URL、metadata 绑定、upload/run 数量和 session 状态摘要。
- 上传仍使用 `/api/uploads*`，上传完成后调用 `/api/sessions/:session_id/uploads` 附加到当前 Session；上传是可选输入，用户只填写 `question` 也可以启动分析。若用户已在 Session draft 中选择文件但尚未点击单独上传按钮，点击 `Start analysis` 必须先上传并附加这些待处理文件，再调用 `/api/sessions/:session_id/tasks` 创建 run。
- Session draft 可选择 Diagnostic Skills，选择结果 debounce PATCH 到 `skillIds`；创建 run 后 artifacts 展示 `system_context.json` 中的 Diagnostic Skills 和 Metadata Context 摘要。
- System Context / Skills 页必须在 Refresh 旁提供 Import 入口，支持填写 `skillId`、`name`、`description`，选择 `.md/.markdown` 文件或手动粘贴 Markdown，并调用 `POST /api/skills/imports`。
- 选择 Markdown 文件时必须用 `File.text()` 读取内容；如果文件含 Codex frontmatter，则预填 `name` 和 `description`，正文编辑框使用去掉 frontmatter 后的 Markdown。
- Skill 导入成功后必须刷新 Skills 列表、选中新 Skill、关闭导入表单并展示成功状态；失败时在页面状态栏展示 Server 返回的错误。
- `Start analysis` 调用 `/api/sessions/:session_id/tasks`，每次创建新的 task run；同一 Session 可以多次运行。
- `Start analysis` 创建的新 task 必须继承 Session `analysisLanguage`。该设置只影响新 run 的 Claude Code 自然语言输出和 UI 固定文案，不翻译历史结果、Server/Claude 原始事件消息、错误、路径、JSON key 或 evidence refs。
- Runs 面板、收起态 timeline 和 Case 确认区优先展示 task `alias`，没有 alias 时使用状态/时间回退标题；不能把 `task_...` 作为主要显示名称。
- WebUI 选择 Session 时 best-effort 调用 Native Agent `PUT /workspace/current` 设置 Chrome 导入目标；失败只提示，不阻断 WebUI 上传。
- Evidence timeline 使用 `/api/sessions/:session_id/timeline`，合并 Session events 和 task analysis events。
- Evidence timeline 必须支持展开/收起；task 到达 `SUCCEEDED` 或 `FAILED` 后自动收起，收起态只展示最终结果摘要、失败摘要或当前 run 状态，用户可手动重新展开查看完整事件。
- `Task execution` 必须实时轮询 `/api/tasks/:task_id/analysis`，展示 revision、预算、最近事件、Claude callId/attempt、session outcome、MCP waiting request 和 evidence 摘要。
- 最终结果按 evidence ref 跳转到对应 artifact。
- 最终结果下方提供 Case 确认表单，允许用户在保存前修改 title、symptom、rootCause 和 solution。
- WebUI 通过 `GET /api/cases` 展示相似 Case，支持关键词搜索和禁用 Case，并展示 schema v2 的 `sourceType` 来源。
- Memory 页面是 V2 Memory 原生入口，默认直接调用 `/api/v2/cases*`，不再默认渲染旧 Rust `/api/cases*` 面板。
- V2 Memory 页面必须调用 `/api/v2/cases` 搜索和展示 Case，支持 `includeDisabled`，展示 score/search backend，并在后端返回时展示 FTS score 和 vector score 分量，便于验证本地 FTS5、keyword、vector 或 hybrid 召回路径；可调用 `PATCH /api/v2/cases/:case_id` 编辑 Case 字段和启用/禁用。
- V2 Memory 页面必须读取文本文件内容后调用 `/api/v2/cases/imports/preview`，展示 draft 和 validation errors；缺少必填字段时调用 `/api/v2/cases/imports/:import_id/messages` 提交补充信息，展示消息历史并刷新结构化 draft；确认时调用 `/api/v2/cases/imports/:import_id/confirm` 并允许用前端编辑后的字段覆盖 draft。
- 成功任务 artifacts 中存在 `caseContext` 时，WebUI 必须展示任务创建时召回的历史 Case，并说明其仅作分析参考。
- 成功任务 artifacts 中存在 `textInput` 时，WebUI 必须展示任务创建时固化的对话框输入，并支持 `session_text_input.json#question` evidence ref 跳转。
- 成功任务 artifacts 中存在 `analysisPackage`、`claudeMcpConfig`、`claudeSession`、`mcpCalls` 或 `agentResponse` 时，WebUI 必须展示 Claude Code session 面板，包含 analysis mode、permission profile、session id、runtime status、耗时、错误、structured output、MCP calls 和 artifact 路径。
- 最终结果中的 `case_context.json#cases/<index>` evidence ref 必须能跳转到对应历史 Case context 条目。
- 页面初始化从 `GET /api/sessions` 读取最近 Session，不以 localStorage 作为任务真源。
- 创建任务后每秒读取任务详情，终态停止；`SUCCEEDED` 再读取 artifacts，`FAILED` 展示失败阶段和消息。
- 创建任务提交用户问题，展示 `GENERATE_RESULT` 阶段，并在成功后读取结构化 LLM 结果。
- 创建任务可提交 `instanceId` / `nodeId`，历史任务详情展示 Server 解析值及 Metadata context artifact。`clusterId` 已从用户输入中移除，仅作为后端兼容字段存在。
- 成功任务 artifacts 展示 `toolResults`，包括工具名、状态、退出码、耗时、摘要、结构化 findings 和 stdout/stderr 路径。
- 结果中的 grep evidence ref 可跳转到当前页面对应 match。
- 历史成功任务可重新选择并读取 artifacts；上传进度和 Server 执行进度必须独立。
- 页面顶部提供 LLM debug 开关，读写 `/api/v2/debug/llm`。开关只影响 V2 Server 日志中的 LLM response content，不在页面展示 Provider 原始响应。
- Settings 页面必须提供 LLM 服务接口测试：读取当前 LLM 配置摘要，测试模型列表获取，发送简单 user message 并展示模型响应；任一请求失败时必须在页面展示完整异常文本。
- Settings 页面必须展示 Claude Code session runner：读取默认 analysis mode、permission profile、启用状态、执行模式和 dry-run 诊断结果；诊断只检查配置路径，不执行命令。
- Settings 页面必须展示 Domain Adapters：展示 `opengemini_influxdb` active，以及 `cassandra`、`rocksdb` skeleton。
- Settings 页面必须展示 Personal Claude Code 区块：只读 MCP HTTP URL、Authorization header 提示、Claude Code HTTP MCP 配置示例、Skills ZIP 下载和 Tools ZIP 下载。下载请求必须携带 API Key header；页面不能写入用户本地 Claude Code 配置、不能一键安装，也不能提供本地 bootstrap。
- Settings 页面是 V2 Settings 原生入口，默认直接调用 `/api/v2/settings*`、`/api/v2/debug/llm` 和 `/api/v2/exports/*`，不再默认渲染旧 Rust Settings 面板。
- V2 Settings 页面必须调用 `/api/v2/settings/llm`、`/api/v2/settings/llm/models` 和 `/api/v2/settings/llm/chat` 展示 V2 Agent provider 摘要、模型列表测试和消息测试，失败时展示完整错误响应。
- V2 Settings 页面必须调用 `/api/v2/settings/agent-backends` 和 `/api/v2/settings/agent-backends/:backend_id/test` 展示 V2 in-process Agent runtime 摘要、LangGraph runtime 节点列表和 dry-run 诊断。
- V2 Settings 页面必须调用 `/api/v2/settings/domain-adapters` 展示 `opengemini_influxdb` active，以及 `cassandra`、`rocksdb` skeleton；并调用 `/api/v2/debug/llm` 读取/切换 V2 response-content debug 开关。
- V2 Settings 页面必须展示 `/api/v2/mcp/readonly` HTTP MCP 配置示例，并通过带 Authorization header 的请求下载 `/api/v2/exports/skills.zip` 和 `/api/v2/exports/tools.zip`。

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
- Vite 开发服务默认把 `/api` 和 `/health` 代理到 Python V2
  `http://127.0.0.1:50993`，并允许通过
  `VITE_LOGAGENT_API_TARGET` 临时覆盖到 Rust V1 或其它后端。
- Python V2 Server 使用 `LOGAGENT_V2_WEBUI_DIR` 或默认仓库
  `webui/out` 托管静态页面；Rust Server 仍可使用
  `ServeDir("webui/out")` 托管同一构建产物。
- `scripts/build-webui.sh` 要求 `LOGAGENT_WORK_DIR`，构建后把静态产物同步到 `$LOGAGENT_WORK_DIR/webui/out`；`scripts/server-service.sh` 从该工作目录启动 Server。
- API Key 保存在本机 localStorage。
- LLM debug 开关不持久化在浏览器侧，页面刷新时以 Server 返回值为准。
- Raw JSON 来自 Server 返回的原始快照，不在浏览器执行任何内容；Raw JSON UI 必须按需展开，不能在初始 render 时全量 stringify 大对象。

## 验收

- `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/` 返回 Vite 构建页面。
- 开发态 `npm run dev` 默认代理到 V2 `50993`；设置
  `VITE_LOGAGENT_API_TARGET` 后必须代理到指定后端。
- Metadata 页面展示已导入 Instance 列表，并按 InstanceID 读取已存快照。
- Metadata 页面能用 Raw JSON 刷新已导入 openGemini Instance，并自动刷新列表和右侧快照。
- Metadata 页面能删除单条已导入 Instance，删除当前选中项后右侧视图清空。
- 重复导入同一 InstanceID 后，列表、快照和下游 task context 不展示旧节点残留。
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
- 点击 `新建 Session` 后会成功调用 `POST /api/sessions` 并选中新 Session；点击 Session 列表删除图标并确认后，非运行中 Session 会从历史列表移除，当前选中项被删除时详情区清空。
- 点击 `Start analysis` 后 Session draft 自动收起；运行中的 timeline 可展开查看事件，任务成功或失败后 timeline 自动收起并展示结果摘要或错误摘要。
- 成功任务轮询到 alias 后，Runs 列表和详情标题展示 alias 而不是裸 task ID。
- 顶部栏展示 `LogAgent Analysis Workbench`，不再展示 `LogAgent Metadata Console`。
- mock Claude CLI 任务能展示 Claude Code structured outcome，CLI 失败能展示 `PLAN_ANALYSIS` 错误。
- 运行中任务能实时展示 analysis loop 事件摘要和预算计数。
- LLM debug 开关能通过 API 开启/关闭 Server 侧 response content 日志。
- 带 Metadata 的任务能展示产品、版本、环境、节点状态、数据库和 PT 摘要。
- 带 Tool Runner 产物的任务能展示 tool result；存在 `findings` 时展示 severity、file、line 和 message。
- 带 Claude Code session artifacts 的任务能展示 `analysis_package.json`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl`、`agent_response.json` 路径、真实 runtime 状态、usage/cost、耗时、MCP calls 和错误。
- mock Claude CLI 任务能完成追问、审批、恢复和最终结果展示。
- 成功任务能保存为 Case；保存后相似 Case 列表能召回该 Case，禁用后默认列表不再展示。
- 新任务召回到历史 Case 时，成功任务详情能展示 `caseContext`。
- Memory 背后的 Case schema v2 响应中的 `taskId` 和 `sourceResultPath` 可为空，前端不能假设所有 Case 都绑定任务。
- 顶部导航能进入 `Memory` 页面；粘贴文本或上传 UTF-8 文本类文件后可生成结构化草稿，缺失必填字段时能提交补充回答，确认保存后可在列表中搜索、选择、编辑并切换启用状态。
- 顶部导航能进入 `Tools` 页面；工具目录来自 `GET /api/tools`，工具运行来自 `/api/tools/runs`，不能混入 Log analysis 任务列表。
- Tools / Tool plugins 页面是 V2 Tools 原生入口，默认直接调用 `/api/v2/tools` 和 `/api/v2/mcp/task/:run_id`，不再默认渲染旧 Rust `/api/tools*` 独立上传型工具运行面板。
- V2 Tools 页面必须调用 `/api/v2/tools` 展示 V2 工具目录，展示 enabled、backend、source、tags、readOnly、editable、exportable、manualOnly、runnable、文件数量约束、accepted suffixes、output views、match、params template 和 params schema，并支持通过带 Authorization header 的请求下载 `/api/v2/exports/tools.zip`。
- V2 Tools 页面的工具运行必须是 run-scoped：用户填写 V2 `run_id` 后，前端调用 `/api/v2/mcp/task/:run_id` 的 JSON-RPC `tools/call`。配置工具使用 `logagent.run_domain_tool`，`logagent.fetch` 使用 `logagent.fetch`；响应直接展示 MCP result/error。
- V2 Tools 页面选择工具时必须用 descriptor 的 `paramsTemplate` 预填 Params JSON，避免用户手工猜测内置工具和 submodule analyzer 的参数结构。
- V2 Tools 页面不提供旧 Rust Tool Runner 的独立上传型 `/api/tools/:tool_id/runs` 兼容入口；V2 的工具产物应挂在对应 run 的 evidence/artifacts 上。
- Tools 页面必须展示 descriptor 的 `source/tags/readOnly/editable/exportable/runnable` 信息，用 tag 区分 configured 和 built-in；built-in 工具只读、不可编辑、不可导出，但 runnable 时可通过 JSON 参数模板手动运行。
- Tools 页面支持所有 `/api/tools` catalog 中的 runnable 工具：按 `paramsTemplate` 预填 JSON textarea，用户手工修改后创建 `tool_run` task、轮询状态并展示结果；`logagent.preprocess_log_package` 作为 built-in runnable tool 支持批量 `.tar.gz` 日志包上传并展示 JSON 预处理摘要，`logagent.huawei_cloud_package_sync` 启用时要求上传一个包并填写 `updateSql` / `querySql`，`pprof_analyzer` 保留 top 函数表和 artifact 路径专用展示，其他工具展示 JSON result。
- Tools 页面必须提供 `Tool plugins` / `Fetch` / `Executors` 子页。Fetch 子页来自 `/api/fetch/imports/preview`、`/api/fetch/endpoints`、`/api/fetch/runs` 和 `/api/tools/runs/:task_id/result`，支持 DevTools bash cURL 粘贴、脱敏预览、endpoint 保存、启停/删除、手动运行、状态轮询和响应 artifact 展示。Executors 子页来自 `/api/executors`、`/api/executor-command-templates` 和 `/api/executor-runs`，支持执行机新增/编辑/禁用、选择白名单命令模板、创建 remote command run、轮询状态并展示 stdout/stderr preview 和 result/stdout/stderr artifact 路径。
- Tools / Fetch 页面是 V2 Fetch 原生入口，默认直接调用 `/api/v2/fetch*`，不再默认渲染旧 Rust `/api/fetch*` 面板。
- V2 Fetch 页面必须调用 `/api/v2/fetch/imports/preview` 和 `/api/v2/fetch/imports` 支持 DevTools bash cURL preview/import，展示 V2 返回的脱敏 endpoint 和 detected sensitive fields。
- V2 Fetch 页面必须调用 `/api/v2/fetch/endpoints` 列表、`PATCH /api/v2/fetch/endpoints/:endpoint_id` 启停、`DELETE /api/v2/fetch/endpoints/:endpoint_id` 删除 endpoint。
- V2 Fetch 页面的运行必须是 run-scoped：用户填写 V2 `run_id` 后，前端调用 `/api/v2/runs/:run_id/fetch/:endpoint_id`，结果展示 V2 result/evidence/artifact，并由后端写入对应 run。运行区必须提供 JSON override 输入，支持向后端传递 `variables`、`headers` 和 `body`，用于验证 task MCP `logagent.fetch` 的同等参数路径。
- Fetch 子页不能显示 Authorization、Cookie、token/api_key/secret/password/session 等敏感原值；预览、endpoint 详情和响应 JSON 中应展示 `<redacted>` 或已脱敏 URL/header/query。
- Tools / Executors 页面是 V2 Executors 原生入口，默认直接调用 `/api/v2/executors*` 和 `/api/v2/executor-runs*`，不再默认渲染旧 Rust `/api/executors*` 面板。
- V2 Executors 页面必须调用 `/api/v2/executors` 新增/列表 executor，调用 `PATCH /api/v2/executors/:executor_id` 编辑 executor，调用 `DELETE /api/v2/executors/:executor_id` 禁用 executor。
- V2 Executors 页面必须调用 `/api/v2/executor-command-templates` 展示白名单命令模板，包含模板 description、argv 和 timeout；调用 `/api/v2/executor-runs` 创建和列出 remote command run，轮询 `/api/v2/executor-runs/:run_id` 并展示 status、phase、attempts、executor/command IDs、created/updated timestamps；成功后读取 `/api/v2/executor-runs/:run_id/result` 展示 stdout/stderr preview、result/stdout/stderr path、SSH argv preview 和 started/completed timestamps。Executor 列表和详情必须展示 last check 状态、时间和消息。
- Executors 子页不能提供自由 shell 命令输入；首版只能运行 Server 返回的白名单模板，例如 `smoke_ls_root`。
- 顶部导航能进入 `Settings` 页面；能调用 `/api/settings/llm`、`/api/settings/llm/models` 和 `/api/settings/llm/chat`，并展示成功响应或完整异常。
- Settings 页面能调用 `/api/settings/agent-backends`、`/api/settings/agent-backends/:backend_id/test` 和 `/api/settings/domain-adapters`，并展示成功响应或完整异常。
- Settings 页面能展示 `/api/mcp/readonly` URL 和 header 示例，并能通过认证请求下载 `/api/exports/skills.zip` 和 `/api/exports/tools.zip`。
- Settings 顶部 V2 页面能调用 `/api/v2/settings/llm`、`/api/v2/settings/llm/models`、`/api/v2/settings/llm/chat`、`/api/v2/settings/agent-backends`、`/api/v2/settings/agent-backends/:backend_id/test`、`/api/v2/settings/domain-adapters` 和 `/api/v2/debug/llm`，展示 Agent runtime 的 LangGraph 节点列表，并展示成功响应或完整异常。
- Settings 顶部 V2 页面能展示 `/api/v2/mcp/readonly` 配置示例，并通过认证请求下载 `/api/v2/exports/skills.zip` 和 `/api/v2/exports/tools.zip`。
- 顶部导航能进入 `System Context` 页面；该页是 V2 System Context + Metadata 原生入口，默认直接调用 `/api/v2/skills*` 和 `/api/v2/metadata*`，不再默认渲染旧 Rust Skills/Metadata 面板。
- V2 System Context 页面必须调用 `/api/v2/skills`、`/api/v2/skills/:skill_id` 展示 V2 Skill 列表/详情，支持从 `.md/.markdown` 文件或 Markdown 文本调用 `/api/v2/skills/imports` 导入新 Skill。
- V2 System Context 页面必须调用 `/api/v2/skills/preview` 预览显式选择的 Skill resources，并通过带 Authorization header 的请求下载 `/api/v2/exports/skills.zip`。
- V2 System Context 页面必须调用 `/api/v2/metadata/instances` 展示 V2 Metadata instance 摘要。
- V2 Metadata 页面必须调用 `/api/v2/metadata/imports/preview` 和 `/api/v2/metadata/imports/fetch/preview` 支持 JSON/YAML/openGemini 内容或 URL 的导入预览，并调用 `/api/v2/metadata/imports/:import_id/confirm` 确认写入。
- V2 Metadata 页面必须支持直接调用 `/api/v2/metadata/imports` 和 `/api/v2/metadata/imports/fetch` 写入实例，调用 `/api/v2/metadata/imports` 列出导入历史，调用 `/api/v2/metadata/instances`、`/api/v2/metadata/instances/:instance_id/snapshot`、`POST /api/v2/metadata/instances/:instance_id/refresh` 和 `DELETE /api/v2/metadata/instances/:instance_id` 管理实例、用已保存 raw JSON 重新归一化快照，并查看 snapshot。
- System Context / Skills 页面能从 `.md/.markdown` 文件和手动 Markdown 文本导入 Diagnostic Skill，成功后列表刷新并选中新 Skill，失败时展示错误。
- Analyze 选择 Skill 后刷新能恢复选择，创建 run 后能在 artifacts 中看到固化的 Diagnostic Skills 和 Metadata Context snapshot。
