import { FileArchive, UploadCloud } from "lucide-react";
import { useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";

const CHUNK_BYTES = 512 * 1024;

type UploadResponse = { uploadId: string; filename: string; size: number };
type Artifacts = {
  taskId?: string;
  manifest?: { files?: Array<{ path: string; size: number }> };
  grepResults?: { matches?: Array<{ file: string; line: number; keyword: string; text: string }> };
};

export function OperationsView({ apiKey }: { apiKey: string }) {
  const [files, setFiles] = useState<File[]>([]);
  const [sourceUrl, setSourceUrl] = useState("");
  const [status, setStatus] = useState("等待上传");
  const [progress, setProgress] = useState(0);
  const [artifacts, setArtifacts] = useState<Artifacts | null>(null);

  async function run() {
    if (!files.length || !apiKey.trim()) {
      setStatus(!files.length ? "请选择日志文件" : "请填写 API Key");
      return;
    }
    try {
      const uploads: UploadResponse[] = [];
      for (let index = 0; index < files.length; index += 1) {
        setStatus(`上传 ${files[index].name}`);
        uploads.push(await uploadFile(files[index], apiKey, (value) => setProgress(Math.round(((index + value) / files.length) * 100))));
      }
      setStatus("创建分析任务");
      const task = await fetchJson<{ taskId: string }>("/api/tasks", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ uploadIds: uploads.map((upload) => upload.uploadId), sourceUrl: sourceUrl || null })
      });
      const result = await fetchJson<Artifacts>(`/api/tasks/${encodeURIComponent(task.taskId)}/artifacts`, { headers: authHeaders(apiKey) });
      setArtifacts(result);
      setStatus(`任务完成：${task.taskId}`);
      setProgress(100);
    } catch (reason) {
      setStatus(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader><CardTitle>Log import and evidence</CardTitle><CardDescription>保留原有多文件、分片上传和任务证据查看能力</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          <Input value={sourceUrl} onChange={(event) => setSourceUrl(event.target.value)} placeholder="Source URL (optional)" />
          <label className="flex min-h-36 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
            <UploadCloud className="mb-2 h-7 w-7" />
            {files.length ? `${files.length} file(s): ${files.map((file) => file.name).join(", ")}` : "选择 .log / .txt / .zip / .tar.gz / .tgz / .tar"}
            <input className="hidden" type="file" multiple onChange={(event) => setFiles(Array.from(event.target.files ?? []))} />
          </label>
          <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${progress}%` }} /></div>
          <div className="flex items-center justify-between gap-3"><span className="text-sm text-muted-foreground">{status}</span><Button onClick={() => void run()}>上传并分析</Button></div>
        </CardContent>
      </Card>
      {artifacts ? (
        <div className="grid gap-5 xl:grid-cols-2">
          <Evidence title="Manifest" count={artifacts.manifest?.files?.length ?? 0}>
            {(artifacts.manifest?.files ?? []).map((file) => <DataLine key={file.path} title={file.path} detail={`${file.size.toLocaleString()} bytes`} />)}
          </Evidence>
          <Evidence title="Grep matches" count={artifacts.grepResults?.matches?.length ?? 0}>
            {(artifacts.grepResults?.matches ?? []).map((match, index) => <DataLine key={`${match.file}:${match.line}:${index}`} title={`${match.file}:${match.line}`} detail={`${match.keyword} · ${match.text}`} />)}
          </Evidence>
        </div>
      ) : <EmptyState>任务完成后在这里查看 manifest 和 grep evidence。</EmptyState>}
    </div>
  );
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
