# WebUI

## 当前实现

WebUI 使用 React 18、Vite、TypeScript、Tailwind CSS、shadcn/ui 组合组件和 React Flow。`npm run build` 输出到 `webui/out`，由 Rust Server 静态托管。

当前页面：

- `Metadata`：openGemini 元数据总览和诊断。
- `Log analysis`：多文件/分片上传、创建任务、查看 manifest 和 grep evidence。

Metadata 能力：

- 从 `http://127.0.0.1:8091/getdata` 实时只读加载。
- 预览并确认写入 Server Metadata Store。
- 读取已经持久化的 cluster。
- Overview：ClusterID、Term、Index、节点/DB/PT/Shard 数量、功能开关和全部 MaxID。
- Nodes：MetaNode、DataNode、SqlNode 完整地址、状态、连接和 AZ 字段。
- Partitions：Database、PtId、Owner DataNode、Status、Ver、RGID。
- Topology：React Flow 展示 `DataNode -> Database/PT -> ShardGroup -> Shard -> IndexGroup -> Index`。
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
    OperationsView.tsx
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
- `GET /api/tasks/:task_id/artifacts`
