# WebUI

## 当前实现

WebUI 使用 React 18、Vite、TypeScript、Tailwind CSS、shadcn/ui 组合组件和 React Flow。`npm run build` 输出到 `webui/out`，由 Rust Server 静态托管。

当前页面：

- 顶部栏使用 `LogAgent Analysis Workbench` 作为全局产品名，覆盖证据、Memory、System Context、Metadata 和工具工作流，不再只强调 Metadata。

- 顶部导航默认进入 `Log Analysis`，可见顺序固定为 `Log Analysis`、`Memory`、`System Context`、`Tools`；Metadata 不再是顶层 tab，仍在 System Context 的 Metadata tab 中可用。
- `Log Analysis`：Session-first 工作流。用户先创建或选择 Session，草稿自动保存，可以只填写问题直接分析，也可以多文件/分片上传完成后附加到 Session，再显式创建一次分析 run；同一 Session 可保留多次 run。
- `Memory`：Case 兼容管理页，支持文本/文本文件导入、LLM 结构化整理、缺失信息追问、确认保存、搜索、详情编辑、证据引用维护和启用/禁用。
- `System Context`：管理 Prompt Pack、产品架构、Mermaid 架构图、Runbook、知识说明和 Metadata adapter；其中 Metadata tab 复用现有 openGemini 拓扑页面。
- `Tools`：工具目录、手动工具运行、执行状态轮询和结果展示；首版支持 `pprof_analyzer`。
- `Log Analysis` 从 Server 加载持久化 Session history，选择 Session 后展示草稿、optional uploads、active run 和历史 runs；活动 run 每秒轮询，成功后读取 artifacts，失败时展示阶段和错误。
- `Log Analysis` Session draft 可选择 System Context 资源；创建 run 后展示本次固化的 `system_context.json` 摘要。
- 成功 run 优先展示 Server 持久化的 task alias；未完成或旧任务没有 alias 时使用状态/时间生成可读标题，避免把 `task_...` 作为主要列表名称。
- `Session draft` 和统一 Evidence Timeline 支持展开/收起；启动分析 run 后草稿自动收起，task 运行完成后 timeline 自动收起并只展示最终结果或失败摘要。
- WebUI 选择 Session 时会 best-effort 调用本机 Native Agent `PUT http://127.0.0.1:17321/workspace/current` 设置活动 Session；失败只提示本地 Agent 未连接，不影响 WebUI 上传。
- Session 内新增 unified Evidence Timeline，合并 session events 和 task `analysis_events.jsonl`，显示 upload、Metadata、Case recall、grep、tool output、LLM call、model decision、用户追问/审批和 final result。
- `Task execution` 读取 `/api/tasks/:task_id/analysis`，实时展示 Analysis loop revision、预算、事件摘要、LLM callId/attempt/schema retry、model decision、action 和 evidence。
- 成功任务展示 `session_text_input.json` 中的 Session 对话框输入，最终结果引用 `session_text_input.json#question` 时可滚动定位到该输入。
- `Task execution` 在 `WAITING_FOR_USER` 展示待补充问题并提交回答，在 `WAITING_FOR_APPROVAL` 展示待审批 action、risk、input，并支持批准或拒绝后继续任务。
- 用户可填写分析问题；任务成功后展示单次 LLM 生成的摘要、症状、可能根因、检查项、修复建议、缺失信息和置信度。
- 成功任务支持编辑标题、现象、根因和解决方案后人工确认保存为 Case；同页可搜索相似 Case 并禁用不再召回的 Case。
- 成功任务展示任务创建时固化的 `caseContext`，区分历史 Case 参考和实时 Case 搜索结果；Case 列表已适配 schema v2 并展示 `task` / `manual` 来源。
- 顶部 `Memory` 页面通过 Case import 草稿创建 `manual` Case：用户粘贴大段文字或上传 UTF-8 文本类文件，LLM 整理为结构化草稿，缺少标题、现象、根因或解决方案时以对话方式补充；确认前仍可编辑产品、版本、环境、InstanceID、NodeID、标题、现象、根因、解决方案和 evidence refs。
- 页面顶部提供 `LLM debug` 开关，调用 Server runtime debug API 控制 LLM response content 是否打印到 Server 日志。
- 创建任务时可填写 `instanceId` / `nodeId`，任务详情展示 Server 解析后的关联 ID；`clusterId` 不再作为用户输入。
- 成功任务展示创建时固化的 Metadata 产品、版本、环境、节点状态、节点/数据库/PT 摘要。
- 成功任务展示 Tool Runner 产物，包括工具名、状态、退出码、耗时、摘要、结构化 findings 和 stdout/stderr 路径。
- Tools 页面复用上传和 Server task 轮询，`pprof_analyzer` 可上传 `.pprof` / `.prof` / `.profile` / `.pb.gz`，展示 profile type、total、top 函数表和 top/tree/raw/stderr artifact 路径。
- 根因 evidence ref 可滚动定位到对应 grep match。
- 上传进度与后台 run 执行状态分别显示；刷新页面从 Server Session 恢复，不依赖浏览器任务 localStorage。

规划中的 Analysis 任务详情增强：

- 展示已确认事实、候选假设、信息缺口和更细粒度预算。
- 展示最终结果及日志、工具、代码、环境和 Case 证据跳转。
- 不展示模型隐藏思维链，只展示可审计的决策摘要。

Metadata 能力：

- 手工输入 InstanceID 后从 `http://127.0.0.1:8091/getdata` 实时只读加载。
- InstanceID 旁支持输入可选备注名，实时加载和导入预览会随请求提交。
- 预览并确认写入 Server Metadata Store。
- 展示已导入 Instance 列表和备注名；列表备注单行省略，避免长文本撑开布局，并按 InstanceID 读取已经持久化的快照。
- Overview：InstanceID、备注名、sourceClusterId、Term、Index、节点/DB/PT/Shard 数量、功能开关和全部 MaxID。
- Nodes：MetaNode、DataNode、SqlNode 完整地址、状态、连接和 AZ 字段。
- Partitions：Database、PtId、Owner DataNode、Status、Ver、RGID。
- Topology：DataNode 大容器分栏，内部按 `Database -> DBPT` 分组展示 ShardGroup/Shard 和 IndexGroup/Index。
- Topology 支持 Database、DataNode、时间范围、仅异常、Shard/Index 显隐筛选。
- 点击拓扑实体时在右侧展示完整字段和关联对象。
- 缺失 DataNode 或缺失 PT 使用红色虚拟容器/lane 展示，不会从拓扑中消失。
- Databases：RP duration、ShardGroup、Shard、IndexGroup 和 Index 明细。
- Schemas：通过 MstVersions 展示逻辑表和物理表映射及 Schema。
- Diagnostics：检查节点离线、连接状态、PT/Shard owner、默认 RP、ShardGroup、Schema 和 Index 引用。
- Raw JSON：保留并筛选原始 `/getdata` JSON。

System Context 能力：

- 列出 Server System Context resources 和 Metadata adapter resources。
- 创建 Prompt Pack、Architecture Doc、Runbook、Glossary、Tool Capability 和 Knowledge Note。
- 新增 draft 或 active version，激活历史 version。
- Architecture 页面使用 Mermaid 文本作为架构图源，提供源码预览。
- Prompt Preview 调用 `/api/system-context/preview` 展示将注入的背景资源。

重要语义：

```text
Shard.Owners / Index.Owners = PT ID
DataNode -> Database/PT -> ShardGroup -> Shard -> IndexGroup -> Index
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
    OperationsView.tsx
    ToolsView.tsx
    upload.ts
    styles.css
  index.html
  vite.config.ts
  tailwind.config.ts
  out/
```

## 本地运行

```bash
cd webui
npm install
npm run dev
```

Vite 开发服务会把 `/api` 和 `/health` 代理到 `http://127.0.0.1:50992`。

生产构建和 Server 托管：

```bash
cd webui
npm run build
cd ..
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/server-test.yaml
```

访问：

```text
http://127.0.0.1:50992/
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
- `GET /api/metadata/instances/:instance_id/snapshot`
- `POST /api/metadata/snapshots/fetch`
- `GET /api/metadata/clusters/:cluster_id`
- `GET /api/metadata/clusters/:cluster_id/nodes`
- `POST /api/metadata/imports/fetch`
- `POST /api/metadata/imports/:import_id/confirm`

System Context：

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
- `GET /api/debug/llm`
- `PUT /api/debug/llm`
- `POST /api/tasks/:task_id/messages`
- `POST /api/tasks/:task_id/actions/:action_id/decision`
- `POST /api/tasks/:task_id/case`
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
