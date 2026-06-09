# WebUI

## 当前实现

WebUI 使用 React 18、Vite、TypeScript、Tailwind CSS、shadcn/ui 组合组件和 React Flow。`npm run build` 输出到 `webui/out`，由 Rust Server 静态托管。

当前页面：

- `Metadata`：按 InstanceID 管理 openGemini 元数据、已导入列表、拓扑总览和诊断。
- `Log analysis`：多文件/分片上传、创建任务、查看 manifest 和 grep evidence。
- `Tools`：工具目录、手动工具运行、执行状态轮询和结果展示；首版支持 `pprof_analyzer`。
- `Cases`：Case Store 管理页，支持搜索、手工录入、详情编辑、证据引用维护和启用/禁用。
- `Log analysis` 从 Server 加载持久化任务列表，展示状态/阶段/attempt，活动任务每秒轮询，成功后读取 artifacts，失败时展示阶段和错误。
- `Task execution` 读取 `/api/tasks/:task_id/analysis`，实时展示 Analysis loop revision、预算、事件摘要、LLM callId/attempt/schema retry、model decision、action 和 evidence。
- `Task execution` 在 `WAITING_FOR_USER` 展示待补充问题并提交回答，在 `WAITING_FOR_APPROVAL` 展示待审批 action、risk、input，并支持批准或拒绝后继续任务。
- 用户可填写分析问题；任务成功后展示单次 LLM 生成的摘要、症状、可能根因、检查项、修复建议、缺失信息和置信度。
- 成功任务支持编辑标题、现象、根因和解决方案后人工确认保存为 Case；同页可搜索相似 Case 并禁用不再召回的 Case。
- 成功任务展示任务创建时固化的 `caseContext`，区分历史 Case 参考和实时 Case 搜索结果；Case 列表已适配 schema v2 并展示 `task` / `manual` 来源。
- 顶部 `Cases` 页面可手工创建 `manual` Case，并编辑产品、版本、环境、InstanceID、NodeID、标题、现象、根因、解决方案和 evidence refs。
- 页面顶部提供 `LLM debug` 开关，调用 Server runtime debug API 控制 LLM response content 是否打印到 Server 日志。
- 创建任务时可填写 `instanceId` / `nodeId`，任务详情展示 Server 解析后的关联 ID；`clusterId` 不再作为用户输入。
- 成功任务展示创建时固化的 Metadata 产品、版本、环境、节点状态、节点/数据库/PT 摘要。
- 成功任务展示 Tool Runner 产物，包括工具名、状态、退出码、耗时、摘要、结构化 findings 和 stdout/stderr 路径。
- Tools 页面复用上传和 Server task 轮询，`pprof_analyzer` 可上传 `.pprof` / `.prof` / `.profile` / `.pb.gz`，展示 profile type、total、top 函数表和 top/tree/raw/stderr artifact 路径。
- 根因 evidence ref 可滚动定位到对应 grep match。
- 上传进度与后台任务执行状态分别显示；刷新页面不再依赖浏览器任务 localStorage。

规划中的 Analysis 任务详情增强：

- 展示已确认事实、候选假设、信息缺口和更细粒度预算。
- 展示最终结果及日志、工具、代码、环境和 Case 证据跳转。
- 不展示模型隐藏思维链，只展示可审计的决策摘要。

Metadata 能力：

- 手工输入 InstanceID 后从 `http://127.0.0.1:8091/getdata` 实时只读加载。
- 预览并确认写入 Server Metadata Store。
- 展示已导入 Instance 列表，并按 InstanceID 读取已经持久化的快照。
- Overview：InstanceID、sourceClusterId、Term、Index、节点/DB/PT/Shard 数量、功能开关和全部 MaxID。
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

日志分析：

- `POST /api/uploads`
- `POST /api/uploads/init`
- `POST /api/uploads/:upload_id/chunks`
- `POST /api/uploads/:upload_id/complete`
- `POST /api/tasks`
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
