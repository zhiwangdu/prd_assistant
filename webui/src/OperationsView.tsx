import { FileArchive, RefreshCw, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";

const CHUNK_BYTES = 512 * 1024;

type UploadResponse = { uploadId: string; filename: string; size: number };
type TaskStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";
type TaskSummary = {
  taskId: string;
  url: string;
  status: TaskStatus;
  phase?: "EXTRACT" | "SEARCH_LOGS" | null;
  createdAt: string;
};
type TaskRecord = TaskSummary & {
  attempts: number;
  error?: { phase?: string | null; message: string } | null;
};
type Artifacts = {
  taskId?: string;
  manifest?: { files?: Array<{ path: string; size: number }> };
  grepResults?: { matches?: Array<{ file: string; line: number; keyword: string; text: string }> };
};

export function OperationsView({ apiKey }: { apiKey: string }) {
  const [files, setFiles] = useState<File[]>([]);
  const [sourceUrl, setSourceUrl] = useState("");
  const [uploadStatus, setUploadStatus] = useState("等待上传");
  const [uploadProgress, setUploadProgress] = useState(0);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [selectedTask, setSelectedTask] = useState<TaskRecord | null>(null);
  const [artifacts, setArtifacts] = useState<Artifacts | null>(null);
  const [loading, setLoading] = useState(false);

  const refreshTasks = useCallback(async () => {
    if (!apiKey.trim()) {
      setTasks([]);
      return;
    }
    const result = await fetchJson<{ tasks: TaskSummary[] }>("/api/tasks", { headers: authHeaders(apiKey) });
    setTasks(result.tasks);
    setSelectedTask((current) => {
      if (current || !result.tasks.length) return current;
      return result.tasks[0] as TaskRecord;
    });
  }, [apiKey]);

  const selectTask = useCallback(async (taskId: string) => {
    const task = await fetchJson<TaskRecord>(`/api/tasks/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) });
    setSelectedTask(task);
    if (task.status === "SUCCEEDED") {
      setArtifacts(await fetchJson<Artifacts>(`/api/tasks/${encodeURIComponent(taskId)}/artifacts`, { headers: authHeaders(apiKey) }));
    } else {
      setArtifacts(null);
    }
  }, [apiKey]);

  useEffect(() => {
    setSelectedTask(null);
    setArtifacts(null);
    void refreshTasks().catch((reason) => setUploadStatus(errorMessage(reason)));
  }, [refreshTasks]);

  useEffect(() => {
    if (!apiKey.trim()) return;
    const timer = window.setInterval(() => {
      void refreshTasks().catch(() => undefined);
      if (selectedTask && !isTerminal(selectedTask.status)) {
        void selectTask(selectedTask.taskId).catch((reason) => setUploadStatus(errorMessage(reason)));
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [apiKey, refreshTasks, selectTask, selectedTask]);

  async function run() {
    if (!files.length || !apiKey.trim()) {
      setUploadStatus(!files.length ? "请选择日志文件" : "请填写 API Key");
      return;
    }
    setLoading(true);
    setArtifacts(null);
    try {
      const uploads: UploadResponse[] = [];
      for (let index = 0; index < files.length; index += 1) {
        setUploadStatus(`上传 ${files[index].name}`);
        uploads.push(await uploadFile(files[index], apiKey, (value) => setUploadProgress(Math.round(((index + value) / files.length) * 100))));
      }
      setUploadStatus("上传完成，创建分析任务");
      const task = await fetchJson<TaskSummary>("/api/tasks", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ uploadIds: uploads.map((upload) => upload.uploadId), sourceUrl: sourceUrl || null })
      });
      setUploadProgress(100);
      setUploadStatus(`已创建任务 ${task.taskId}`);
      await refreshTasks();
      await selectTask(task.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader><CardTitle>Log import and evidence</CardTitle><CardDescription>上传进度与 Server 后台任务执行状态独立展示</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          <Input value={sourceUrl} onChange={(event) => setSourceUrl(event.target.value)} placeholder="Source URL (optional)" />
          <label className="flex min-h-36 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
            <UploadCloud className="mb-2 h-7 w-7" />
            {files.length ? `${files.length} file(s): ${files.map((file) => file.name).join(", ")}` : "选择 .log / .txt / .zip / .tar.gz / .tgz / .tar"}
            <input className="hidden" type="file" multiple onChange={(event) => setFiles(Array.from(event.target.files ?? []))} />
          </label>
          <div>
            <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>Upload</span><span>{uploadProgress}%</span></div>
            <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
          </div>
          <div className="flex items-center justify-between gap-3"><span className="text-sm text-muted-foreground">{uploadStatus}</span><Button disabled={loading} onClick={() => void run()}>{loading ? "上传中" : "上传并分析"}</Button></div>
        </CardContent>
      </Card>

      <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
        <Card>
          <CardHeader><div className="flex items-center justify-between"><CardTitle>Server tasks</CardTitle><Button className="h-8 px-3" variant="outline" onClick={() => void refreshTasks()}><RefreshCw className="h-4 w-4" /></Button></div><CardDescription>刷新页面后仍可查看历史和运行中任务</CardDescription></CardHeader>
          <CardContent className="space-y-2">
            {tasks.length ? tasks.map((task) => (
              <button key={task.taskId} className={`w-full rounded-lg border p-3 text-left ${selectedTask?.taskId === task.taskId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => void selectTask(task.taskId)}>
                <div className="flex items-center justify-between gap-2"><span className="font-mono text-xs">{task.taskId}</span><StatusBadge status={task.status} /></div>
                <p className="mt-1 text-xs text-muted-foreground">{task.phase ?? "No active phase"} · {new Date(task.createdAt).toLocaleString()}</p>
              </button>
            )) : <EmptyState>Server 暂无持久化任务。</EmptyState>}
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>Task execution</CardTitle><CardDescription>{selectedTask ? `${selectedTask.taskId} · attempt ${selectedTask.attempts ?? 0}` : "选择一个任务查看执行状态"}</CardDescription></CardHeader>
          <CardContent>
            {selectedTask ? (
              <div className="space-y-3">
                <div className="flex items-center gap-2"><StatusBadge status={selectedTask.status} /><span className="text-sm text-muted-foreground">{selectedTask.phase ?? "No active phase"}</span></div>
                {selectedTask.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedTask.error?.phase ? `${selectedTask.error.phase}: ` : ""}{selectedTask.error?.message ?? "Task failed"}</div> : null}
                {!isTerminal(selectedTask.status) ? <p className="text-sm text-muted-foreground">任务由 Server 后台执行，每秒自动刷新。</p> : null}
                {selectedTask.status === "SUCCEEDED" && !artifacts ? <Button onClick={() => void selectTask(selectedTask.taskId)}>加载 artifacts</Button> : null}
              </div>
            ) : <EmptyState>选择或创建任务后查看执行状态。</EmptyState>}
          </CardContent>
        </Card>
      </div>

      {artifacts ? (
        <div className="grid gap-5 xl:grid-cols-2">
          <Evidence title="Manifest" count={artifacts.manifest?.files?.length ?? 0}>
            {(artifacts.manifest?.files ?? []).map((file) => <DataLine key={file.path} title={file.path} detail={`${file.size.toLocaleString()} bytes`} />)}
          </Evidence>
          <Evidence title="Grep matches" count={artifacts.grepResults?.matches?.length ?? 0}>
            {(artifacts.grepResults?.matches ?? []).map((match, index) => <DataLine key={`${match.file}:${match.line}:${index}`} title={`${match.file}:${match.line}`} detail={`${match.keyword} · ${match.text}`} />)}
          </Evidence>
        </div>
      ) : <EmptyState>成功任务的 manifest 和 grep evidence 会显示在这里。</EmptyState>}
    </div>
  );
}

function StatusBadge({ status }: { status: TaskStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{status}</Badge>;
}

function isTerminal(status: TaskStatus) {
  return status === "SUCCEEDED" || status === "FAILED";
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}

function Evidence({ title, count, children }: { title: string; count: number; children: React.ReactNode }) {
  return <Card><CardHeader><div className="flex items-center justify-between"><CardTitle>{title}</CardTitle><Badge variant="secondary">{count}</Badge></div></CardHeader><CardContent className="space-y-2">{count ? children : <EmptyState>暂无数据</EmptyState>}</CardContent></Card>;
}

function DataLine({ title, detail }: { title: string; detail: string }) {
  return <div className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-sm font-medium"><FileArchive className="h-4 w-4 text-slate-400" />{title}</div><p className="mt-1 break-words text-xs text-muted-foreground">{detail}</p></div>;
}

async function uploadFile(file: File, apiKey: string, onProgress: (value: number) => void) {
  if (file.size <= CHUNK_BYTES) {
    const form = new FormData();
    form.append("filename", file.name);
    form.append("file", file, file.name);
    const result = await fetchJson<UploadResponse>("/api/uploads", { method: "POST", headers: authHeaders(apiKey), body: form });
    onProgress(1);
    return result;
  }
  const upload = await fetchJson<UploadResponse>("/api/uploads/init", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ filename: file.name, size: file.size })
  });
  for (let offset = 0; offset < file.size; offset += CHUNK_BYTES) {
    const next = Math.min(offset + CHUNK_BYTES, file.size);
    await fetchJson(`/api/uploads/${encodeURIComponent(upload.uploadId)}/chunks?offset=${offset}`, {
      method: "POST",
      headers: authHeaders(apiKey),
      body: file.slice(offset, next)
    });
    onProgress(next / file.size);
  }
  return fetchJson<UploadResponse>(`/api/uploads/${encodeURIComponent(upload.uploadId)}/complete`, { method: "POST", headers: authHeaders(apiKey) });
}
