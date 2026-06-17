# WebUI

## 当前实现

WebUI 使用 React 18、Vite、TypeScript、Tailwind CSS 和 shadcn/ui 组合组件。`npm run build` 输出到 `webui/out`，由 Rust Server 静态托管。

当前页面：

- 顶部栏使用 `LogAgent Analysis Workbench` 作为全局产品名，覆盖证据、Memory、Skill-backed System Context、Metadata 和工具工作流，不再只强调 Metadata。

- 顶部导航默认进入 `Analyze`，可见顺序固定为 `Analyze`、`Memory`、`System Context`、`Tools`、`Settings`；Metadata 不再是顶层 tab，作为 System Context 页面内的 V2 Metadata 工作台区块提供。
- `Analyze`：已切换为 Python V2 原生入口，直接调用 `/api/v2/*`。可新建 Workspace、选择历史 Workspace、回填并保存 Workspace 问题/模式、软删除历史 Workspace、上传小文件或分片上传大文件、创建 Run、轮询 `/api/v2/runs/:run_id/analysis`，展示 V2 run 状态、evidence、timeline、pending actions、resources、最终结果和 artifacts；运行资源区会展开 `analysis_state.json` 中的 LangGraph runtime、Agent request/response 审计、`claude_mcp_config.json`、`claude_session.json` 和 `mcp_calls.jsonl` 摘要；当 V2 run 等待用户或审批时，可提交补充、请求基于当前证据收尾、批准或拒绝 action，补充消息会带上 pending action 的 `questionId`，补充和审批都会生成稳定 `idempotencyKey`，`collect_environment` 审批会从已配置 Remote Executor 和命令模板中选择目标并随 decision `input` 提交；返回的 `answeredActions` 会被后续轮询反映到 pending actions；成功 run 可编辑并保存为 V2 task Case，并通过带 Authorization header 的下载按钮读取 `/api/v2/artifacts/:artifact_id`。旧 Rust Session-first Analyze 页面不再默认渲染。
- V2 Analyze artifacts 列表会合并 uploads、evidence artifacts 和 V2
  `supportArtifacts`。support artifacts 用 Rust/V1 逻辑路径展示，并可通过同一
  `/api/v2/artifacts/:artifact_id` 下载，覆盖 Tool Runner stdout/stderr、
  Fetch response body 和 pprof top/tree/raw/stderr/SVG 等结果支持文件。
- V2 Analyze timeline 使用 Python V2 timeline event 的 `kind` 字段作为事件标签，并兼容旧 `event_type` 字段。
- `Analyze` 固定 UI 文案、状态、阶段、置信度和常见 timeline event 默认优先使用简体中文展示，保留 `Session`、`Case`、`Claude Code`、`MCP`、`Metadata`、`Tool Runner`、`grep`、`artifact`、`evidence ref` 等无法准确替代的专业名词。顶部语言选择支持 `简体中文` / `English` 切换；该选择会写入浏览器本地配置，并同步到当前 Session 的 `analysisLanguage`，新创建的 run 会要求 Claude Code 按该语言输出自然语言字段。
- `Memory`：已切换为 V2 Memory 工作台，直接调用 Python V2 `/api/v2/cases*`：支持 V2 Case 搜索、include disabled、文本/文件读取后 import preview、缺失字段补充消息、导入历史/草稿恢复、编辑结构化 draft、confirm 写入、Case 详情编辑和启用/禁用；搜索结果和详情会展示 `searchBackend`、总分、FTS 分数和 vector 分数，便于验证本地 FTS5 / keyword / vector / hybrid 召回路径。旧 Rust Memory 页面不再默认渲染。
- `System Context`：已切换为 V2 System Context + V2 Metadata 工作台，直接调用 Python V2 `/api/v2/skills*`、`/api/v2/system-context/*`、`/api/v2/metadata/instances` 和 `/api/v2/metadata/*`：支持 V2 Skill 列表/详情、Markdown Skill import、显式 Skill selection preview、`skills.zip` 下载、V1-compatible System Context resource 列表/创建/编辑、版本追加/编辑/激活、资源+Metadata adapter prompt preview、JSON/YAML/openGemini 内容或 URL 的导入预览、确认、直接导入、导入历史、实例列表、用已保存 raw JSON 刷新实例、实例删除和 snapshot JSON 查看。旧 Rust Skills/Metadata 页面不再默认渲染。
- `Tools / Tool plugins`：已切换为 V2 Tools 工作台，调用 Python V2 `/api/v2/tools` 展示工具目录，展示 `source`、`tags`、`readOnly`、`editable`、`exportable`、`manualOnly`、文件数量约束、`acceptedSuffixes`、`outputViews`、`match`、`paramsTemplate` 和 `paramsSchema`，支持下载 `/api/v2/exports/tools.zip`。页面同时支持两种 V2 原生运行方式：输入 V2 `run_id` 后按选中工具的 `paramsTemplate` 预填 params，通过 `/api/v2/mcp/task/:run_id` 调用 task MCP；或在 Manual `tool_run` 区复用/自动创建 Workspace、上传匹配文件并调用 `/api/v2/tools/:tool_id/runs`，随后刷新 `/api/v2/tools/runs`、轮询选中的非终态 run、读取 `/api/v2/tools/runs/:run_id/result` 和 `/api/v2/tools/runs/:run_id/artifacts`，并通过带 Authorization header 的 `/api/v2/artifacts/:artifact_id` 下载 upload、evidence 和 support artifacts。旧 Rust 独立上传型 Tool Runner 页面不再默认渲染。
- `Tools / Fetch`：已切换为 V2 Fetch 工作台，调用 Python V2 `/api/v2/fetch/*`：支持 cURL preview/import、endpoint 列表、启停/删除、敏感字段脱敏提示，并允许输入 V2 `run_id` 和 run override JSON（`variables`、`headers`、`body`）后调用 `/api/v2/runs/:run_id/fetch/:endpoint_id` 将 Fetch 结果写入对应 run 的 evidence/artifact；也支持不绑定已有分析 run 的 standalone Fetch `tool_run`，可选复用 Workspace 或自动创建隔离 Workspace，调用 `/api/v2/fetch/endpoints/:endpoint_id/runs` 创建、`/api/v2/fetch/runs` 刷新历史、选择非终态 run 后轮询并读取结果。结果面板展示 result artifact、response body artifact、evidence 和 body preview，并通过带 Authorization header 的 artifact API 下载 `result.json` 与 `response_body.bin`。旧 Rust Fetch 页面不再默认渲染。
- `Tools / Executors`：已切换为 V2 Executors 工作台，调用 Python V2 `/api/v2/executors*` 和 `/api/v2/executor-runs*`：支持 V2 executor 新增/编辑/禁用、白名单命令模板选择、DB-backed remote command run 创建/轮询、stdout/stderr/result 路径和预览展示，并通过受保护的 `/api/v2/executor-runs/:run_id/files/result|stdout|stderr` 下载三类结果文件；同时显示 executor last check、模板 description/timeout、run attempts、executor/command IDs、created/updated timestamps、SSH argv preview 和 started/completed timestamps。旧 Rust Executors 页面不再默认渲染。
- `Settings`：已切换为 V2 Settings 工作台，调用 Python V2 `/api/v2/settings/*` 和 `/api/v2/debug/llm`：支持 V2 Agent provider 摘要、模型列表和消息测试、Agent backend dry-run、LangGraph runtime 节点展示、Domain Adapter 摘要、response-content debug 开关、V2 只读 MCP 配置示例，以及 `skills.zip` / `tools.zip` 下载。旧 Rust Settings 页面不再默认渲染。
- 默认 V2 页面用户可见标题使用 `Workbench` / `Console`，不再把页面称为临时 `Bridge`；源码文件名仍保留 `V2*Bridge.tsx`，避免一次性重命名造成无关 churn。
- `Settings / Personal Claude Code` 展示只读 MCP HTTP URL、Authorization header 提示、Claude Code HTTP MCP 配置示例，并通过带 API Key header 的下载按钮获取 `skills.zip` 和 `tools.zip`；不提供一键安装、本地 bootstrap 或个人 Claude Code 配置写入。
- `Analyze` 从 Server 加载持久化 Session history，支持新建和删除非运行中 Session；选择 Session 后展示草稿、optional uploads、active run 和历史 runs；活动 run 每秒轮询，成功后读取 artifacts，失败时展示阶段和错误。删除 Session 前会二次确认，删除后只清理 Session 历史项，关联上传、任务和结果产物由 Server 保留。
- `Analyze` Session draft 可选择 Diagnostic Skills；创建 run 后展示本次固化的 `system_context.json` 中的 Diagnostic Skills 和 Metadata Context 摘要。
- `Analyze` 的语言切换只翻译前端固定文案和协议值展示，并影响新 run 的 Claude prompt；旧结果、Server/Claude 原始事件消息、错误详情、路径、JSON key 和 evidence refs 保持原样。
- 成功 run 优先展示 Server 持久化的 task alias；未完成或旧任务没有 alias 时使用状态/时间生成可读标题，避免把 `task_...` 作为主要列表名称。
- `Session draft` 和统一 Evidence Timeline 支持展开/收起；启动分析 run 后草稿自动收起，task 运行完成后 timeline 自动收起并只展示最终结果或失败摘要。
- WebUI 选择 Session 时会 best-effort 调用本机 Native Agent `PUT http://127.0.0.1:17321/workspace/current` 设置活动 Session；失败只提示本地 Agent 未连接，不影响 WebUI 上传。
- Session 内新增 unified Evidence Timeline，合并 session events 和 task `analysis_events.jsonl`，显示 upload、Metadata、Case recall、grep、tool output、Claude Code session、MCP waiting request、用户追问/审批和 final result。
- `Task execution` 读取 `/api/tasks/:task_id/analysis`，实时展示 Analysis revision、预算、事件摘要、Claude callId/attempt、session outcome 和 evidence。
- 成功任务展示 `session_text_input.json` 中的 Session 对话框输入，最终结果引用 `session_text_input.json#question` 时可滚动定位到该输入。
- `Task execution` 在 `WAITING_FOR_USER` 展示待补充问题并提交回答，也提供“没有更多信息，直接生成最终结果”按钮；该按钮会以 `resumeMode: "finalize"` 提交，即使回答框为空也会请求基于当前证据生成最终结果。在 `WAITING_FOR_APPROVAL` 展示待审批 action、risk、input，并支持批准或拒绝后继续任务；V2 `collect_environment` 审批会显示 Remote Executor 和白名单命令模板选择，留空则走兼容 MOCK 采集。
- 用户可填写分析问题；任务成功后展示单次 LLM 生成的摘要、症状、可能根因、检查项、修复建议、缺失信息和置信度。
- 成功任务支持编辑标题、现象、根因和解决方案后人工确认保存为 Case；同页可搜索相似 Case 并禁用不再召回的 Case。
- 成功任务展示任务创建时固化的 `caseContext`，区分历史 Case 参考和实时 Case 搜索结果；Case 列表已适配 schema v2 并展示 `task` / `manual` 来源。
- 顶部 `Memory` 页面通过 Case import 草稿创建 `manual` Case：用户粘贴大段文字或上传 UTF-8 文本类文件，LLM 整理为结构化草稿，缺少标题、现象、根因或解决方案时以对话方式补充；页面会展示最近 V2 import history，可点击恢复未确认或已确认 draft、validation errors 和补充消息；确认前仍可编辑产品、版本、环境、InstanceID、NodeID、标题、现象、根因、解决方案和 evidence refs。
- 页面顶部提供 `LLM debug` 开关，调用 V2 `/api/v2/debug/llm` 控制 LLM response content 是否打印到 Server 日志。
- 创建任务时可填写 `instanceId` / `nodeId`，任务详情展示 Server 解析后的关联 ID；`clusterId` 不再作为用户输入。
- 成功任务展示创建时固化的 Metadata 产品、版本、环境、节点状态、节点/数据库/PT 摘要。
- 成功任务展示 Claude Code session 面板，包括 `analysis_package.json`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl`、`agent_response.json` 路径、analysis mode、permission profile、session id、runtime status、usage/cost、耗时、MCP calls 和错误。
- 成功任务展示 Tool Runner 产物，包括工具名、状态、退出码、耗时、摘要、结构化 findings 和 stdout/stderr 路径。
- Tools / Tool plugins 页面复用上传和 Server task 轮询，按 `/api/tools` descriptor 的 `source/tags/readOnly/exportable/runnable` 展示 configured 与 built-in 工具差异；所有 runnable 工具都按 `paramsTemplate` 预填 JSON 参数并允许手工修改后运行，metadata built-ins 不需要上传，configured command tools 上传匹配文件，`logagent.preprocess_log_package` 接受多个 `.tar.gz` 日志包，`logagent.huawei_cloud_package_sync` 上传一个包并展示 JSON result，`pprof_analyzer` 展示 profile type、total、top 函数表和 top/tree/raw/stderr artifact 路径，其他工具展示 JSON result。
- Tools / Fetch 页面调用 `/api/fetch/*` 管理 Server 内置 Fetch endpoint。预览、endpoint 详情和运行结果都只展示脱敏 request/response；运行结果读取 `/api/tools/runs/:task_id/result` 中的 `tool=logagent.fetch` artifact。
- Tools / Executors 页面调用 `/api/executors` 管理执行机，调用 `/api/executor-command-templates` 读取白名单命令模板，并通过 `/api/executor-runs` 发起和轮询 `remote_command_run`。首版使用内置 `smoke_ls_root` 模板做低风险 SSH smoke，不允许输入自由 shell 命令。
- 根因 evidence ref 可滚动定位到对应 grep match。
- 上传进度与后台 run 执行状态分别显示；刷新页面从 Server Session 恢复，不依赖浏览器任务 localStorage。

规划中的 Analysis 任务详情增强：

- 展示已确认事实、候选假设、信息缺口和更细粒度预算。
- 展示最终结果及日志、工具、代码、环境和 Case 证据跳转。
- 不展示模型隐藏思维链，只展示可审计的决策摘要。

Metadata 能力：

- 手工输入 InstanceID 后从 `http://127.0.0.1:8091/getdata` 实时只读加载。
- InstanceID 旁支持输入可选备注名，实时加载和导入预览会随请求提交。
- 导入区支持三种方式：实时加载 openGemini `/getdata` URL、上传 `.json` 元数据文件、手动粘贴 JSON 文本。
- JSON 文件和手动 JSON 文本通过 `/api/metadata/imports` 生成导入预览；完整 Metadata JSON 模板可包含多个 Instance，openGemini `/getdata` JSON 仍需填写 InstanceID。
- 预览并确认写入 Server Metadata Store。
- 展示已导入 Instance 列表和备注名；列表备注单行省略，并支持向左收缩/展开，避免长文本撑开布局，并按 InstanceID 读取已经持久化的快照。
- 已导入 Instance 列表支持删除单条 metadata；导入区支持用已存 openGemini Raw JSON 手动刷新当前 Instance，刷新后重新读取列表和右侧快照。
- 重复确认导入相同 InstanceID 时，Server 按新快照覆盖旧快照，不保留旧节点残留。
- Overview：InstanceID、备注名、sourceClusterId、Term、Index、节点/DB/PT/Shard 数量、功能开关和全部 MaxID。
- Nodes：MetaNode、DataNode、SqlNode 完整地址、状态、连接和 AZ 字段；MetaNode 状态固定显示 none，Data/SQL 节点按 none/alive/leaving/left/failed 映射。
- Partitions：Database、PtId、Owner DataNode、Status、Ver、RGID。
- Metadata Explorer：合并原 Topology 和 Databases，提供 `Node / DBPT / Shards` 与 `DB / RP / Shards / Indexes` 两个视角。
- `Node / DBPT / Shards` 视角按 `Database -> DataNode -> DBPT -> Shards` 级联展开，Shard 行展示 time range、IndexID 和 Index 状态信息。
- Explorer 支持 Database、DataNode、时间范围、仅异常、Shard 行和 Index 信息显隐筛选，不再渲染 Graph；点击 DBPT 时在右侧展示聚合指标、异常和时间范围。
- 缺失 DataNode 或缺失 PT 使用红色虚拟容器/lane 展示，不会从拓扑中消失。
- `DB / RP / Shards / Indexes` 视角按 `Database -> RP -> ShardGroup/IndexGroup -> Shard/Index` 级联展开。
- Schemas：默认选择第一个非 `_internal` DB 及其第一个 RP，RP 选项跟随 DB 联动，Measurement/field 搜索用于缩小结果，field type 按 openGemini 码位展示为 `0 Unknown`、`1 Integer`、`2 Unsigned`、`3 Float`、`4 String`、`5 Boolean`、`6 Tag`、`7 Unknown`。
- Metadata 明细表使用局部滚动和固定表头，浏览大量节点、分片、索引或字段时保留字段含义。
- Diagnostics：检查节点离线、连接状态、PT/Shard owner、默认 RP、ShardGroup、Schema 和 Index 引用。
- Raw JSON：按需展开原始 `/getdata` JSON，不在进入页面时全量 stringify 或渲染全部节点。

System Context 能力：

- 列出 Server Skill Registry 中的 Diagnostic Skills。
- 查看 Skill displayName、description、revision、匹配字段、reference 摘要和 SKILL.md 注入片段。
- Refresh 旁的 Import 表单可填写 `skillId`、`name`、`description`，选择 `.md/.markdown` 文件或直接粘贴 Markdown；文件存在 Codex frontmatter 时会预填 name/description 并把正文写入编辑框。
- 导入成功后自动刷新 Skills 列表、选中新 Skill，并可在 Analyze Session draft 中显式选择。
- V2 Compatibility resources 区块调用 `/api/v2/system-context/resources` 管理 V1 风格资源：可创建 prompt pack / architecture doc / runbook / glossary / tool capability / knowledge note / diagnostic skill，编辑标题、scope、tags、产品/版本/环境和启用状态，追加新版本、编辑已有版本内容/prompt policy，并选择是否立即激活。
- Resource preview 调用 `/api/v2/system-context/preview`，可显式包含资源或只读 `meta_<instanceId>` Metadata adapter，并展示后端生成的 prompt preview。
- System Context 页面内的 V2 Metadata 工作台继续提供 Metadata 导入、实例管理、snapshot 查看和 raw JSON 刷新能力。
- 旧 Rust `/api/system-context/resources` 默认只作为 Metadata adapter 列表入口；V2 兼容资源通过 `/api/v2/system-context/*` 管理，仍不作为新分析运行的默认知识主路径。

重要语义：

```text
Shard.Owners / Index.Owners = PT ID
Metadata Explorer: Database -> DataNode -> DBPT -> Shards / Database -> RP -> ShardGroup/IndexGroup
```

## 文件结构

```text
webui/
  src/
    components/
    metadata/
      api.ts
      diagnostics.ts
      MetadataDashboard.tsx
      topology.tsx
      types.ts
      view-model.ts
    App.tsx
    CasesView.tsx
    ExecutorsView.tsx
    OperationsView.tsx
    ToolsView.tsx
    V2AnalyzeBridge.tsx
    V2ExecutorsBridge.tsx
    V2FetchBridge.tsx
    V2MemoryBridge.tsx
    V2MetadataBridge.tsx
    V2SettingsBridge.tsx
    V2SystemContextBridge.tsx
    V2ToolsBridge.tsx
    upload.ts
    v2-api.ts
    styles.css
  index.html
  vite.config.ts
  tailwind.config.ts
  out/
```

## 本地运行

先启动默认 V2 后端：

```bash
cd server-v2
python3 -m venv .venv
. .venv/bin/activate
pip install -e ".[dev]"
export LOGAGENT_V2_API_KEY=dev-token
python -m logagent_v2 init-db
python -m logagent_v2 server
```

再启动 WebUI 开发服务：

```bash
cd webui
npm install
npm run dev
```

Vite 开发服务默认把 `/api` 和 `/health` 代理到 V2
`http://127.0.0.1:50993`。如需临时连接 Rust V1 或其它后端，可设置：

```bash
VITE_LOGAGENT_API_TARGET=http://127.0.0.1:50992 npm run dev
```

生产构建和 V2 Server 托管：

```bash
cd webui
npm run build
cd ../server-v2
export LOGAGENT_V2_API_KEY=dev-token
python -m logagent_v2 init-db
python -m logagent_v2 server
```

访问：

```text
http://127.0.0.1:50993/
API Key: dev-token
```

使用工作目录脚本构建并交给 Server 托管：

```bash
export LOGAGENT_WORK_DIR=/tmp/logagent-runtime
./scripts/init-workdir.sh
./scripts/build-webui.sh
```

`build-webui.sh` 会运行 `npm --prefix webui run build`，并把 `webui/out` 同步到 `$LOGAGENT_WORK_DIR/webui/out`。`server-service.sh` 会从 `$LOGAGENT_WORK_DIR` 作为当前目录启动 Server，因此静态托管路径仍是相对的 `webui/out`。

## 验证

```bash
npm run lint
npm run typecheck
npm run build
```

## 接口

受保护接口使用 `Authorization: Bearer <api-key>`。

Metadata：

- `GET /api/metadata/instances`
- `DELETE /api/metadata/instances/:instance_id`
- `GET /api/metadata/instances/:instance_id/snapshot`
- `POST /api/metadata/instances/:instance_id/refresh`
- `POST /api/metadata/snapshots/fetch`
- `GET /api/metadata/clusters/:cluster_id`
- `GET /api/metadata/clusters/:cluster_id/nodes`
- `POST /api/metadata/imports/fetch`
- `POST /api/metadata/imports/:import_id/confirm`

System Context：

- `GET /api/skills`
- `GET /api/skills/:skill_id`
- `POST /api/skills/imports`
- `POST /api/skills/preview`
- `GET /api/system-context/resources`
- `POST /api/system-context/resources`
- `GET /api/system-context/resources/:context_id`
- `PATCH /api/system-context/resources/:context_id`
- `POST /api/system-context/resources/:context_id/versions`
- `PATCH /api/system-context/resources/:context_id/versions/:version_id`
- `POST /api/system-context/resources/:context_id/versions/:version_id/activate`
- `POST /api/system-context/preview`

日志分析：

- `POST /api/uploads`
- `POST /api/uploads/init`
- `POST /api/uploads/:upload_id/chunks`
- `POST /api/uploads/:upload_id/complete`
- `POST /api/sessions`
- `GET /api/sessions`
- `GET /api/sessions/:session_id`
- `PATCH /api/sessions/:session_id`
- `POST /api/sessions/:session_id/uploads`
- `DELETE /api/sessions/:session_id/uploads/:upload_id`
- `POST /api/sessions/:session_id/tasks`
- `GET /api/sessions/:session_id/timeline`
- `POST /api/tasks`（兼容入口，必须携带 sessionId）
- `GET /api/tasks`
- `GET /api/tasks/:task_id`
- `GET /api/tasks/:task_id/artifacts`
- `GET /api/tasks/:task_id/result`
- `GET /api/tasks/:task_id/analysis`
- `POST /api/tasks/:task_id/messages`
- `POST /api/tasks/:task_id/actions/:action_id/decision`
- `POST /api/tasks/:task_id/case`

V2 APIs：

- `POST /api/v2/workspaces`
- `GET /api/v2/workspaces`
- `GET /api/v2/workspaces/:workspace_id`
- `PATCH /api/v2/workspaces/:workspace_id`
- `DELETE /api/v2/workspaces/:workspace_id`
- `GET /api/v2/workspaces/:workspace_id/uploads`
- `GET /api/v2/workspaces/:workspace_id/runs`
- `POST /api/v2/workspaces/:workspace_id/uploads`
- `POST /api/v2/workspaces/:workspace_id/uploads/init`
- `POST /api/v2/uploads/:session_id/chunks`
- `POST /api/v2/uploads/:session_id/complete`
- `POST /api/v2/workspaces/:workspace_id/runs`
- `GET /api/v2/runs/:run_id/analysis`
- `POST /api/v2/runs/:run_id/messages`
- `POST /api/v2/actions/:action_id/decisions`
- `GET /api/v2/artifacts/:artifact_id`
- `GET /api/v2/debug/llm`
- `PUT /api/v2/debug/llm`
- `GET /api/v2/tools`
- `GET /api/v2/exports/tools.zip`
- `POST /api/v2/mcp/task/:run_id`
- `POST /api/v2/mcp/readonly`
- `GET /api/v2/debug/llm`
- `PUT /api/v2/debug/llm`
- `GET /api/v2/settings/llm`
- `GET /api/v2/settings/llm/models`
- `POST /api/v2/settings/llm/chat`
- `GET /api/v2/settings/agent-backends`
- `POST /api/v2/settings/agent-backends/:backend_id/test`
- `GET /api/v2/settings/domain-adapters`
- `GET /api/v2/fetch/endpoints`
- `POST /api/v2/fetch/imports/preview`
- `POST /api/v2/fetch/imports`
- `PATCH /api/v2/fetch/endpoints/:endpoint_id`
- `DELETE /api/v2/fetch/endpoints/:endpoint_id`
- `POST /api/v2/runs/:run_id/fetch/:endpoint_id`
- `GET /api/v2/executors`
- `POST /api/v2/executors`
- `PATCH /api/v2/executors/:executor_id`
- `DELETE /api/v2/executors/:executor_id`
- `GET /api/v2/executor-command-templates`
- `GET /api/v2/executor-runs`
- `POST /api/v2/executor-runs`
- `GET /api/v2/executor-runs/:run_id`
- `GET /api/v2/executor-runs/:run_id/result`
- `GET /api/v2/executor-runs/:run_id/files/:file_name`
- `GET /api/v2/skills`
- `GET /api/v2/skills/:skill_id`
- `POST /api/v2/skills/imports`
- `POST /api/v2/skills/preview`
- `GET /api/v2/exports/skills.zip`
- `GET /api/v2/system-context/resources`
- `POST /api/v2/system-context/resources`
- `GET /api/v2/system-context/resources/:context_id`
- `PATCH /api/v2/system-context/resources/:context_id`
- `POST /api/v2/system-context/resources/:context_id/versions`
- `PATCH /api/v2/system-context/resources/:context_id/versions/:version_id`
- `POST /api/v2/system-context/resources/:context_id/versions/:version_id/activate`
- `POST /api/v2/system-context/preview`
- `GET /api/v2/metadata/instances`
- `GET /api/v2/metadata/instances/:instance_id/snapshot`
- `POST /api/v2/metadata/instances/:instance_id/refresh`
- `DELETE /api/v2/metadata/instances/:instance_id`
- `GET /api/v2/metadata/imports`
- `POST /api/v2/metadata/imports/preview`
- `POST /api/v2/metadata/imports/fetch/preview`
- `POST /api/v2/metadata/imports/:import_id/confirm`
- `POST /api/v2/metadata/imports`
- `POST /api/v2/metadata/imports/fetch`
- `GET /api/v2/cases`
- `GET /api/v2/cases/:case_id`
- `PATCH /api/v2/cases/:case_id`
- `GET /api/v2/cases/imports`
- `POST /api/v2/cases/imports/preview`
- `POST /api/v2/cases/imports/:import_id/messages`
- `POST /api/v2/cases/imports/:import_id/confirm`

Fetch：

- `POST /api/fetch/imports/preview`
- `GET /api/fetch/endpoints`
- `POST /api/fetch/endpoints`
- `GET /api/fetch/endpoints/:fetch_id`
- `PATCH /api/fetch/endpoints/:fetch_id`
- `DELETE /api/fetch/endpoints/:fetch_id`
- `POST /api/fetch/endpoints/:fetch_id/runs`
- `GET /api/fetch/runs`
- `POST /api/cases`
- `POST /api/cases/imports`
- `GET /api/cases/imports/:draft_id`
- `PATCH /api/cases/imports/:draft_id`
- `POST /api/cases/imports/:draft_id/messages`
- `POST /api/cases/imports/:draft_id/confirm`
- `GET /api/cases`
- `GET /api/cases/:case_id`
- `PATCH /api/cases/:case_id`

Tools：

- `GET /api/tools`
- `GET /api/tools/:tool_id`
- `POST /api/tools/:tool_id/runs`
- `GET /api/tools/runs`
- `GET /api/tools/runs/:task_id`
- `GET /api/tools/runs/:task_id/result`
- `GET /api/tools/runs/:task_id/artifacts`
- `GET /api/executors`
- `POST /api/executors`
- `GET /api/executors/:executor_id`
- `PATCH /api/executors/:executor_id`
- `DELETE /api/executors/:executor_id`
- `GET /api/executor-command-templates`
- `GET /api/executor-runs`
- `POST /api/executor-runs`
- `GET /api/executor-runs/:task_id`
- `GET /api/executor-runs/:task_id/result`

Settings：

- `GET /api/settings/llm`
- `GET /api/settings/llm/models`
- `POST /api/settings/llm/chat`
- `GET /api/settings/agent-backends`
- `POST /api/settings/agent-backends/:backend_id/test`
- `GET /api/settings/domain-adapters`
- `POST /api/mcp/readonly`
- `GET /api/exports/skills.zip`
- `GET /api/exports/tools.zip`
