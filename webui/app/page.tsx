"use client";

import { ChangeEvent, useEffect, useMemo, useState } from "react";

const STORAGE_KEY = "logagent.webui.tasks";
const API_KEY_STORAGE = "logagent.webui.apiKey";
const CHUNK_BYTES = 512 * 1024;

type View = "import" | "tasks" | "evidence" | "metadata";

type UploadResponse = {
  uploadId: string;
  filename: string;
  size: number;
};

type TaskResponse = {
  taskId: string;
};

type SavedTask = {
  taskId: string;
  filename: string;
  size: number;
  createdAt: string;
};

type ArtifactResponse = {
  manifest?: {
    files?: Array<{
      path: string;
      size: number;
    }>;
  };
  grepResults?: {
    matches?: Array<{
      file: string;
      line: number;
      keyword: string;
      text: string;
    }>;
  };
};

type MetadataPreview = {
  importId: string;
  templateType: string;
  summary: {
    instances: number;
    clusters: number;
    nodes: number;
    databases?: number;
    partitionViews?: number;
    warnings: number;
    errors: number;
  };
  changes?: Array<{
    kind: string;
    id: string;
    action: string;
    message: string;
  }>;
  warnings?: string[];
  errors?: string[];
};

type NodeMetadata = {
  nodeId: string;
  role?: string | null;
  status?: string | null;
  host?: string | null;
};

type PartitionViewMetadata = {
  database: string;
  ptId: number;
  ownerNodeId?: number | null;
  status?: number | null;
  statusText?: string | null;
  version?: number | null;
  replicaGroupId?: number | null;
};

type DatabaseMetadata = {
  name: string;
  defaultRetentionPolicy?: string | null;
  replicaN?: number | null;
  retentionPolicies?: RetentionPolicyMetadata[];
};

type RetentionPolicyMetadata = {
  name: string;
  replicaN?: number | null;
  shardGroupDuration?: number | null;
  measurements?: MeasurementMetadata[];
  shardGroups?: ShardGroupMetadata[];
};

type MeasurementMetadata = {
  name: string;
  shardKeyType?: string | null;
  schema?: Array<{
    name: string;
    typ?: number | null;
  }>;
};

type ShardGroupMetadata = {
  id: number;
  startTime?: string | null;
  endTime?: string | null;
  shardIds?: number[];
  owners?: number[];
};

type ClusterMetadata = {
  clusterId: string;
  name?: string | null;
  product?: string | null;
  databases?: DatabaseMetadata[];
  partitionViews?: PartitionViewMetadata[];
  nodes?: NodeMetadata[];
};

const defaultMetadataTemplate = `{
  "ClusterID": 6735497445922383781,
  "MetaNodes": [
    {
      "ID": 1,
      "Host": "127.0.0.1:8091",
      "RPCAddr": "127.0.0.1:8092",
      "TCPHost": "127.0.0.1:8088",
      "Status": 0
    }
  ],
  "DataNodes": [
    {
      "ID": 2,
      "Host": "127.0.0.1:8400",
      "TCPHost": "127.0.0.1:8401",
      "Status": 1,
      "Az": ""
    }
  ],
  "SqlNodes": [
    {
      "ID": 3,
      "TCPHost": ":8086",
      "Status": 1
    }
  ],
  "Databases": {
    "mydb": { "Name": "mydb" }
  }
}`;

export default function Home() {
  const [activeView, setActiveView] = useState<View>("import");
  const [apiKey, setApiKey] = useState("");
  const [health, setHealth] = useState({ ok: false, text: "未检查" });
  const [sourceUrl, setSourceUrl] = useState("");
  const [files, setFiles] = useState<File[]>([]);
  const [progress, setProgress] = useState(0);
  const [statusLog, setStatusLog] = useState<string[]>([]);
  const [tasks, setTasks] = useState<SavedTask[]>([]);
  const [activeTaskId, setActiveTaskId] = useState("");
  const [artifacts, setArtifacts] = useState<ArtifactResponse | null>(null);
  const [metadataInstanceId, setMetadataInstanceId] = useState("");
  const [metadataClusterId, setMetadataClusterId] = useState("6735497445922383781");
  const [metadataResult, setMetadataResult] = useState<unknown>(null);
  const [metadataTemplateType, setMetadataTemplateType] = useState("opengemini");
  const [metadataFilename, setMetadataFilename] = useState("opengemini-getdata.json");
  const [metadataFetchUrl, setMetadataFetchUrl] = useState("http://127.0.0.1:8091/getdata");
  const [metadataTemplate, setMetadataTemplate] = useState(defaultMetadataTemplate);
  const [metadataImportId, setMetadataImportId] = useState("");
  const [metadataImportResult, setMetadataImportResult] = useState<unknown>(null);

  useEffect(() => {
    setApiKey(localStorage.getItem(API_KEY_STORAGE) || "");
    setTasks(loadSavedTasks());
    void checkHealth();
  }, []);

  useEffect(() => {
    localStorage.setItem(API_KEY_STORAGE, apiKey.trim());
  }, [apiKey]);

  const manifestFiles = artifacts?.manifest?.files || [];
  const grepMatches = artifacts?.grepResults?.matches || [];

  async function checkHealth() {
    try {
      const body = await fetchJson<{ status?: string }>("/health");
      setHealth({ ok: true, text: body.status || "ok" });
    } catch (err) {
      setHealth({ ok: false, text: errorMessage(err) });
    }
  }

  async function uploadAndRun() {
    if (!files.length) {
      log("请选择文件");
      return;
    }
    if (!apiKey.trim()) {
      log("请填写 API Key");
      return;
    }

    setProgress(0);
    const totalSize = files.reduce((sum, file) => sum + file.size, 0);
    const progressTotal = Math.max(totalSize, 1);
    let uploadedSize = 0;
    log(`开始上传 ${files.length} 个文件 (${formatBytes(totalSize)})`);

    try {
      const uploads: UploadResponse[] = [];
      for (const file of files) {
        log(`上传 ${file.name} (${formatBytes(file.size)})`);
        const upload = await uploadFile(file, (fileProgress) => {
          const current = uploadedSize + Math.round(file.size * fileProgress);
          setProgress(Math.round((current / progressTotal) * 100));
        });
        uploadedSize += file.size;
        setProgress(Math.round((uploadedSize / progressTotal) * 100));
        uploads.push(upload);
        log(`上传完成: ${upload.uploadId}`);
      }
      setProgress(100);
      const task = await createTask(uploads.map((upload) => upload.uploadId));
      log(`任务完成: ${task.taskId}`);
      const savedTask = {
        taskId: task.taskId,
        filename: uploads.map((upload) => upload.filename).join(", "),
        size: uploads.reduce((sum, upload) => sum + upload.size, 0),
        createdAt: new Date().toISOString()
      };
      saveTask(savedTask);
      setActiveTaskId(task.taskId);
      await loadArtifacts(task.taskId);
      setActiveView("evidence");
    } catch (err) {
      log(`失败: ${errorMessage(err)}`);
    }
  }

  async function uploadFile(file: File, onProgress: (value: number) => void) {
    if (file.size <= CHUNK_BYTES) {
      const form = new FormData();
      form.append("filename", file.name);
      form.append("file", file, file.name);
      const upload = await fetchJson<UploadResponse>("/api/uploads", {
        method: "POST",
        headers: authHeaders(apiKey),
        body: form
      });
      onProgress(1);
      return upload;
    }

    const init = await fetchJson<UploadResponse>("/api/uploads/init", {
      method: "POST",
      headers: jsonHeaders(apiKey),
      body: JSON.stringify({ filename: file.name, size: file.size })
    });

    let offset = 0;
    while (offset < file.size) {
      const next = Math.min(offset + CHUNK_BYTES, file.size);
      const chunk = file.slice(offset, next);
      await fetchJson(`/api/uploads/${encodeURIComponent(init.uploadId)}/chunks?offset=${offset}`, {
        method: "POST",
        headers: authHeaders(apiKey),
        body: chunk
      });
      offset = next;
      onProgress(offset / file.size);
    }

    return fetchJson<UploadResponse>(`/api/uploads/${encodeURIComponent(init.uploadId)}/complete`, {
      method: "POST",
      headers: authHeaders(apiKey)
    });
  }

  async function createTask(uploadIds: string[]) {
    return fetchJson<TaskResponse>("/api/tasks", {
      method: "POST",
      headers: jsonHeaders(apiKey),
      body: JSON.stringify({
        uploadIds,
        sourceUrl: sourceUrl.trim() || null
      })
    });
  }

  async function loadArtifacts(taskId: string) {
    const body = await fetchJson<ArtifactResponse>(`/api/tasks/${encodeURIComponent(taskId)}/artifacts`, {
      headers: authHeaders(apiKey)
    });
    setActiveTaskId(taskId);
    setArtifacts(body);
    setActiveView("evidence");
  }

  async function queryInstance() {
    if (!metadataInstanceId.trim()) {
      setMetadataResult("请输入 Instance ID");
      return;
    }
    try {
      const body = await fetchJson(`/api/metadata/instances/${encodeURIComponent(metadataInstanceId.trim())}`, {
        headers: authHeaders(apiKey)
      });
      setMetadataResult(body);
    } catch (err) {
      setMetadataResult(`查询失败: ${errorMessage(err)}`);
    }
  }

  async function queryCluster() {
    if (!metadataClusterId.trim()) {
      setMetadataResult("请输入 Cluster ID");
      return;
    }
    try {
      const cluster = await fetchJson<ClusterMetadata>(
        `/api/metadata/clusters/${encodeURIComponent(metadataClusterId.trim())}`,
        {
          headers: authHeaders(apiKey)
        }
      );
      const nodes = await fetchJson<{ nodes: NodeMetadata[] }>(
        `/api/metadata/clusters/${encodeURIComponent(metadataClusterId.trim())}/nodes`,
        {
          headers: authHeaders(apiKey)
        }
      );
      setMetadataResult({ ...cluster, nodes: nodes.nodes });
    } catch (err) {
      setMetadataResult(`查询失败: ${errorMessage(err)}`);
    }
  }

  async function fetchMetadataImport() {
    if (!apiKey.trim()) {
      setMetadataImportResult("请填写 API Key");
      return;
    }
    if (!metadataFetchUrl.trim()) {
      setMetadataImportResult("请输入真实元数据 URL");
      return;
    }
    try {
      const preview = await fetchJson<MetadataPreview>("/api/metadata/imports/fetch", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          url: metadataFetchUrl.trim(),
          templateType: metadataTemplateType.trim() || "opengemini",
          filename: metadataFilename.trim() || metadataFetchUrl.trim()
        })
      });
      setMetadataImportId(preview.importId);
      setMetadataImportResult(preview);
    } catch (err) {
      setMetadataImportResult(`拉取失败: ${errorMessage(err)}`);
    }
  }

  async function previewMetadataImport() {
    if (!apiKey.trim()) {
      setMetadataImportResult("请填写 API Key");
      return;
    }
    try {
      const preview = await fetchJson<MetadataPreview>("/api/metadata/imports", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          templateType: metadataTemplateType.trim(),
          filename: metadataFilename.trim() || null,
          content: metadataTemplate
        })
      });
      setMetadataImportId(preview.importId);
      setMetadataImportResult(preview);
    } catch (err) {
      setMetadataImportResult(`预览失败: ${errorMessage(err)}`);
    }
  }

  async function confirmMetadataImport() {
    if (!metadataImportId) {
      setMetadataImportResult("请先预览导入");
      return;
    }
    try {
      const response = await fetchJson(`/api/metadata/imports/${encodeURIComponent(metadataImportId)}/confirm`, {
        method: "POST",
        headers: authHeaders(apiKey)
      });
      setMetadataImportResult(response);
    } catch (err) {
      setMetadataImportResult(`确认失败: ${errorMessage(err)}`);
    }
  }

  function onFilesChange(event: ChangeEvent<HTMLInputElement>) {
    setFiles(Array.from(event.target.files || []));
  }

  function log(message: string) {
    const now = new Date().toLocaleTimeString();
    setStatusLog((items) => [`[${now}] ${message}`, ...items].slice(0, 80));
  }

  function saveTask(task: SavedTask) {
    const nextTasks = [task, ...tasks.filter((item) => item.taskId !== task.taskId)].slice(0, 50);
    setTasks(nextTasks);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(nextTasks));
  }

  function clearTasks() {
    setTasks([]);
    localStorage.setItem(STORAGE_KEY, "[]");
  }

  const rawArtifacts = useMemo(() => JSON.stringify(artifacts, null, 2), [artifacts]);

  return (
    <main className="grid min-h-screen bg-shell text-ink md:grid-cols-[260px_minmax(0,1fr)]">
      <aside className="flex flex-col gap-6 border-r border-line bg-[#101828] p-5 text-white">
        <div className="flex items-center gap-3">
          <div className="grid h-11 w-11 place-items-center rounded-lg border border-white/30 bg-accent font-bold">LA</div>
          <div>
            <h1 className="text-lg font-semibold">LogAgent</h1>
            <p className="text-sm text-slate-300">Evidence workspace</p>
          </div>
        </div>
        <nav className="grid gap-2">
          {(["import", "tasks", "evidence", "metadata"] as View[]).map((view) => (
            <button
              key={view}
              type="button"
              className={`rounded-lg px-3 py-3 text-left text-sm font-medium ${
                activeView === view ? "bg-white/10 text-white" : "text-slate-300 hover:bg-white/10 hover:text-white"
              }`}
              onClick={() => setActiveView(view)}
            >
              {viewLabel(view)}
            </button>
          ))}
        </nav>
        <div className="mt-auto flex items-center gap-2 text-sm text-slate-300">
          <span className={`h-2.5 w-2.5 rounded-full ${health.ok ? "bg-accent" : "bg-red-600"}`} />
          <span>{health.text}</span>
        </div>
      </aside>

      <section className="grid content-start gap-5 p-5">
        <header className="flex min-h-20 flex-col gap-4 rounded-lg border border-line bg-white p-5 shadow-panel lg:flex-row lg:items-end lg:justify-between">
          <div>
            <h2 className="text-2xl font-semibold">故障日志分析</h2>
            <p className="text-sm text-muted">连接当前 Server</p>
          </div>
          <div className="grid gap-3 sm:grid-cols-[minmax(220px,340px)_auto] sm:items-end">
            <label className="grid gap-1.5">
              <span className="text-sm font-semibold text-slate-700">API Key</span>
              <input
                className={inputClass}
                type="password"
                autoComplete="off"
                placeholder="dev-token"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
              />
            </label>
            <button className={secondaryButtonClass} type="button" onClick={() => void checkHealth()}>
              检查健康
            </button>
          </div>
        </header>

        {activeView === "import" && (
          <Panel title="导入日志" subtitle="选择本机文件，上传到 Server 并创建分析任务">
            <div className="grid gap-4 lg:grid-cols-2">
              <label className="grid gap-1.5">
                <span className="text-sm font-semibold text-slate-700">来源 URL</span>
                <input
                  className={inputClass}
                  type="url"
                  placeholder="https://logs.example.com/download/..."
                  value={sourceUrl}
                  onChange={(event) => setSourceUrl(event.target.value)}
                />
              </label>
              <label className="grid gap-1.5">
                <span className="text-sm font-semibold text-slate-700">日志文件</span>
                <input className={inputClass} type="file" multiple onChange={onFilesChange} />
              </label>
            </div>
            <div className="mt-4 flex flex-wrap gap-3">
              <button className={primaryButtonClass} type="button" onClick={() => void uploadAndRun()}>
                上传并创建任务
              </button>
            </div>
            <div className="mt-4 h-2 overflow-hidden rounded-full bg-slate-200">
              <div className="h-full bg-accent transition-all" style={{ width: `${progress}%` }} />
            </div>
            <pre className="mt-4 max-h-64 overflow-auto rounded-lg border border-line bg-[#0b1220] p-3 text-sm text-slate-100">
              {statusLog.join("\n") || "暂无日志"}
            </pre>
          </Panel>
        )}

        {activeView === "tasks" && (
          <Panel
            title="最近任务"
            subtitle="本地浏览器保存的任务入口"
            action={
              <button className={secondaryButtonClass} type="button" onClick={clearTasks}>
                清空
              </button>
            }
          >
            <div className="grid gap-3">
              {tasks.length ? (
                tasks.map((task) => (
                  <div key={task.taskId} className="grid gap-3 rounded-lg border border-line p-3 lg:grid-cols-[minmax(160px,1.2fr)_minmax(120px,1fr)_92px] lg:items-center">
                    <div className="min-w-0">
                      <strong className="block truncate">{task.taskId}</strong>
                      <span className="block truncate text-sm text-muted">{task.filename}</span>
                    </div>
                    <span className="text-sm text-muted">{formatBytes(task.size)} · {new Date(task.createdAt).toLocaleString()}</span>
                    <button className={secondaryButtonClass} type="button" onClick={() => void loadArtifacts(task.taskId)}>
                      查看
                    </button>
                  </div>
                ))
              ) : (
                <Empty text="暂无任务" />
              )}
            </div>
          </Panel>
        )}

        {activeView === "evidence" && (
          <Panel
            title="证据链"
            subtitle={activeTaskId ? `当前任务 ${activeTaskId}` : "选择任务后查看产物"}
            action={
              <button
                className={secondaryButtonClass}
                type="button"
                onClick={() => activeTaskId ? void loadArtifacts(activeTaskId) : log("请选择一个任务")}
              >
                刷新产物
              </button>
            }
          >
            <div className="grid gap-4 xl:grid-cols-2">
              <EvidenceBlock title="文件清单">
                {manifestFiles.length ? (
                  manifestFiles.map((file) => (
                    <DataRow key={file.path} title={file.path} subtitle={formatBytes(file.size)} />
                  ))
                ) : (
                  <Empty text="暂无数据" />
                )}
              </EvidenceBlock>
              <EvidenceBlock title="grep 命中">
                {grepMatches.length ? (
                  grepMatches.map((match, index) => (
                    <DataRow
                      key={`${match.file}:${match.line}:${index}`}
                      title={`${match.file}:${match.line}`}
                      subtitle={match.keyword}
                      detail={match.text}
                    />
                  ))
                ) : (
                  <Empty text="暂无命中" />
                )}
              </EvidenceBlock>
            </div>
            <details className="mt-4">
              <summary className="cursor-pointer font-semibold">原始 JSON</summary>
              <pre className="mt-3 max-h-96 overflow-auto rounded-lg border border-line bg-[#0b1220] p-3 text-sm text-slate-100">
                {rawArtifacts || "{}"}
              </pre>
            </details>
          </Panel>
        )}

        {activeView === "metadata" && (
          <Panel title="Metadata" subtitle="实例、集群和 openGemini 元数据导入">
            <div className="grid gap-5 xl:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
              <section>
                <h4 className="mb-3 font-semibold">查询</h4>
                <div className="grid gap-3 lg:grid-cols-2">
                  <label className="grid gap-1.5">
                    <span className="text-sm font-semibold text-slate-700">Instance ID</span>
                    <input className={inputClass} value={metadataInstanceId} onChange={(event) => setMetadataInstanceId(event.target.value)} placeholder="i-123" />
                  </label>
                  <label className="grid gap-1.5">
                    <span className="text-sm font-semibold text-slate-700">Cluster ID</span>
                    <input className={inputClass} value={metadataClusterId} onChange={(event) => setMetadataClusterId(event.target.value)} placeholder="6735497445922383781" />
                  </label>
                </div>
                <div className="mt-3 flex flex-wrap gap-3">
                  <button className={secondaryButtonClass} type="button" onClick={() => void queryInstance()}>
                    查询实例
                  </button>
                  <button className={secondaryButtonClass} type="button" onClick={() => void queryCluster()}>
                    查询集群
                  </button>
                </div>
                <div className="mt-4">
                  <MetadataResult value={metadataResult} />
                </div>
              </section>

              <section>
                <h4 className="mb-3 font-semibold">导入</h4>
                <div className="grid gap-3 lg:grid-cols-2">
                  <label className="grid gap-1.5">
                    <span className="text-sm font-semibold text-slate-700">模板类型</span>
                    <input className={inputClass} value={metadataTemplateType} onChange={(event) => setMetadataTemplateType(event.target.value)} />
                  </label>
                  <label className="grid gap-1.5">
                    <span className="text-sm font-semibold text-slate-700">文件名</span>
                    <input className={inputClass} value={metadataFilename} onChange={(event) => setMetadataFilename(event.target.value)} />
                  </label>
                  <label className="grid gap-1.5 lg:col-span-2">
                    <span className="text-sm font-semibold text-slate-700">真实元数据 URL</span>
                    <input className={inputClass} type="url" value={metadataFetchUrl} onChange={(event) => setMetadataFetchUrl(event.target.value)} />
                  </label>
                  <label className="grid gap-1.5 lg:col-span-2">
                    <span className="text-sm font-semibold text-slate-700">模板内容</span>
                    <textarea className={`${inputClass} min-h-64 resize-y`} value={metadataTemplate} onChange={(event) => setMetadataTemplate(event.target.value)} />
                  </label>
                </div>
                <div className="mt-3 flex flex-wrap gap-3">
                  <button className={secondaryButtonClass} type="button" onClick={() => void fetchMetadataImport()}>
                    拉取并预览
                  </button>
                  <button className={primaryButtonClass} type="button" onClick={() => void previewMetadataImport()}>
                    预览导入
                  </button>
                  <button className={secondaryButtonClass} type="button" onClick={() => void confirmMetadataImport()}>
                    确认导入
                  </button>
                </div>
                <div className="mt-4">
                  <MetadataImportResult value={metadataImportResult} />
                </div>
              </section>
            </div>
          </Panel>
        )}
      </section>
    </main>
  );
}

function Panel({
  title,
  subtitle,
  action,
  children
}: {
  title: string;
  subtitle: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-line bg-white p-5 shadow-panel">
      <div className="mb-5 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h3 className="text-lg font-semibold">{title}</h3>
          <p className="text-sm text-muted">{subtitle}</p>
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function EvidenceBlock({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grid content-start gap-3">
      <h4 className="font-semibold">{title}</h4>
      <div className="grid gap-2">{children}</div>
    </section>
  );
}

function DataRow({ title, subtitle, detail }: { title: string; subtitle?: string; detail?: string }) {
  return (
    <div className="grid gap-1 rounded-lg border border-line p-3">
      <code className="break-words text-sm">{title}</code>
      {subtitle && <small className="text-muted">{subtitle}</small>}
      {detail && <div className="text-sm">{detail}</div>}
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return <div className="rounded-lg border border-dashed border-line p-5 text-sm text-muted">{text}</div>;
}

function MetadataResult({ value }: { value: unknown }) {
  if (!value) {
    return <Empty text="暂无数据" />;
  }
  if (typeof value === "string") {
    return <DataRow title={value} />;
  }
  if (isClusterMetadata(value)) {
    return <ClusterView cluster={value} />;
  }
  return <JsonBlock value={value} />;
}

function ClusterView({ cluster }: { cluster: ClusterMetadata }) {
  const nodes = cluster.nodes || [];
  const partitionViews = cluster.partitionViews || [];
  const databases = cluster.databases || [];
  return (
    <div className="grid gap-4">
      <section className="grid gap-2">
        <h4 className="font-semibold">Cluster</h4>
        <div className="grid gap-2 lg:grid-cols-5">
          <KeyValue label="clusterId" value={cluster.clusterId} />
          <KeyValue label="name" value={cluster.name || "-"} />
          <KeyValue label="product" value={cluster.product || "-"} />
          <KeyValue label="databaseCount" value={databases.length} />
          <KeyValue label="partitionViewCount" value={partitionViews.length} />
        </div>
      </section>
      <section className="grid gap-2">
        <h4 className="font-semibold">Nodes</h4>
        {nodes.length ? nodes.map((node) => (
          <DataRow key={node.nodeId} title={node.nodeId} subtitle={`${node.role || "-"} · ${node.status || "-"} · ${node.host || "-"}`} />
        )) : <Empty text="暂无节点" />}
      </section>
      <section className="grid gap-2">
        <h4 className="font-semibold">PtView</h4>
        {partitionViews.length ? partitionViews.map((partition) => (
          <DataRow
            key={`${partition.database}:${partition.ptId}`}
            title={`${partition.database} / pt-${partition.ptId}`}
            subtitle={`owner data-${partition.ownerNodeId ?? "-"} · ${partition.statusText || partition.status || "-"} · ver ${partition.version ?? "-"} · rg ${partition.replicaGroupId ?? "-"}`}
          />
        )) : <Empty text="暂无 PtView" />}
      </section>
      <section className="grid gap-2">
        <h4 className="font-semibold">Databases</h4>
        {databases.length ? databases.map((database) => <DatabaseView key={database.name} database={database} />) : <Empty text="暂无 Database" />}
      </section>
      <details open>
        <summary className="cursor-pointer font-semibold">原始查询结果</summary>
        <JsonBlock value={cluster} />
      </details>
    </div>
  );
}

function DatabaseView({ database }: { database: DatabaseMetadata }) {
  const policies = database.retentionPolicies || [];
  return (
    <div className="grid gap-3 rounded-lg border border-line p-3">
      <div className="grid gap-1">
        <strong>{database.name}</strong>
        <small className="text-muted">default RP: {database.defaultRetentionPolicy || "-"} · replicaN: {database.replicaN ?? "-"}</small>
      </div>
      {policies.length ? policies.map((policy) => <RetentionPolicyView key={policy.name} policy={policy} />) : <Empty text="暂无保留策略" />}
    </div>
  );
}

function RetentionPolicyView({ policy }: { policy: RetentionPolicyMetadata }) {
  const measurements = policy.measurements || [];
  const shardGroups = policy.shardGroups || [];
  return (
    <div className="grid gap-3 rounded-lg border border-line p-3">
      <div className="grid gap-1">
        <strong>RP {policy.name}</strong>
        <small className="text-muted">replicaN {policy.replicaN ?? "-"} · shardGroupDuration {policy.shardGroupDuration ?? "-"}</small>
      </div>
      <div className="grid gap-3 lg:grid-cols-2">
        <section className="grid content-start gap-2">
          <small className="font-semibold text-muted">Measurements</small>
          {measurements.length ? measurements.map((measurement) => (
            <DataRow
              key={measurement.name}
              title={measurement.name}
              subtitle={`${measurement.shardKeyType || "-"} · fields: ${formatSchema(measurement)}`}
            />
          )) : <Empty text="暂无 Measurement" />}
        </section>
        <section className="grid content-start gap-2">
          <small className="font-semibold text-muted">ShardGroups</small>
          {shardGroups.length ? shardGroups.map((group) => (
            <DataRow
              key={group.id}
              title={`sg-${group.id}`}
              subtitle={`${group.startTime || "-"} ~ ${group.endTime || "-"} · shards: ${(group.shardIds || []).join(",") || "-"} · owners: ${(group.owners || []).join(",") || "-"}`}
            />
          )) : <Empty text="暂无 ShardGroup" />}
        </section>
      </div>
    </div>
  );
}

function MetadataImportResult({ value }: { value: unknown }) {
  if (!value) {
    return <Empty text="暂无预览" />;
  }
  if (typeof value === "string") {
    return <DataRow title={value} />;
  }
  if (isMetadataPreview(value)) {
    return (
      <div className="grid gap-3">
        <div className="grid gap-2 lg:grid-cols-5">
          <KeyValue label="importId" value={value.importId} />
          <KeyValue label="instances" value={value.summary.instances} />
          <KeyValue label="clusters" value={value.summary.clusters} />
          <KeyValue label="databases" value={value.summary.databases || 0} />
          <KeyValue label="partitionViews" value={value.summary.partitionViews || 0} />
        </div>
        <section className="grid gap-2">
          <h4 className="font-semibold">Changes</h4>
          {value.changes?.length ? value.changes.map((change) => (
            <DataRow key={`${change.kind}:${change.id}`} title={`${change.kind} ${change.id} ${change.action}`} detail={change.message} />
          )) : <Empty text="暂无变更" />}
        </section>
        {!!value.warnings?.length && (
          <section className="grid gap-2">
            <h4 className="font-semibold">Warnings</h4>
            {value.warnings.map((warning) => <DataRow key={warning} title={warning} />)}
          </section>
        )}
        {!!value.errors?.length && (
          <section className="grid gap-2">
            <h4 className="font-semibold">Errors</h4>
            {value.errors.map((error) => <DataRow key={error} title={error} />)}
          </section>
        )}
        <JsonBlock value={value} />
      </div>
    );
  }
  return <JsonBlock value={value} />;
}

function KeyValue({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="grid gap-1 rounded-lg border border-line p-3">
      <small className="text-muted">{label}</small>
      <strong className="break-words">{value}</strong>
    </div>
  );
}

function JsonBlock({ value }: { value: unknown }) {
  return (
    <pre className="max-h-96 overflow-auto rounded-lg border border-line bg-slate-50 p-3 text-sm">
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}

async function fetchJson<T>(url: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(url, options);
  const text = await response.text();
  const body = text ? JSON.parse(text) : {};
  if (!response.ok) {
    throw new Error(body.error || `HTTP ${response.status}`);
  }
  return body as T;
}

function authHeaders(apiKey: string): HeadersInit {
  return {
    Authorization: `Bearer ${apiKey.trim()}`
  };
}

function jsonHeaders(apiKey: string): HeadersInit {
  return {
    ...authHeaders(apiKey),
    "Content-Type": "application/json"
  };
}

function loadSavedTasks() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || "[]") as SavedTask[];
  } catch {
    return [];
  }
}

function formatBytes(value: number) {
  if (!Number.isFinite(value)) {
    return "-";
  }
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatSchema(measurement: MeasurementMetadata) {
  const schema = measurement.schema || [];
  return schema.map((field) => `${field.name}:${field.typ ?? "-"}`).join(", ") || "-";
}

function viewLabel(view: View) {
  switch (view) {
    case "import":
      return "导入";
    case "tasks":
      return "任务";
    case "evidence":
      return "证据";
    case "metadata":
      return "Metadata";
  }
}

function errorMessage(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}

function isClusterMetadata(value: unknown): value is ClusterMetadata {
  return typeof value === "object" && value !== null && "clusterId" in value;
}

function isMetadataPreview(value: unknown): value is MetadataPreview {
  return typeof value === "object" && value !== null && "importId" in value && "summary" in value;
}

const inputClass = "min-h-11 w-full rounded-lg border border-slate-300 px-3 py-2 text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/15";
const primaryButtonClass = "min-h-10 rounded-lg border border-accent bg-accent px-4 py-2 font-semibold text-white hover:bg-accent-dark";
const secondaryButtonClass = "min-h-10 rounded-lg border border-slate-300 bg-white px-4 py-2 font-semibold text-slate-700 hover:bg-slate-50";
