import { FileArchive, Play, RefreshCw, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { uploadFile } from "./upload";

type ToolDescriptor = {
  toolId: string;
  displayName: string;
  description: string;
  enabled: boolean;
  backend: string;
  acceptedSuffixes: string[];
  minFiles: number;
  maxFiles: number;
  outputViews: string[];
};

type ToolRunStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";
type ToolRunSummary = {
  taskId: string;
  taskKind: "tool_run" | "log_analysis";
  status: ToolRunStatus;
  phase?: string | null;
  createdAt: string;
};
type ToolRunRecord = ToolRunSummary & {
  attempts?: number;
  toolId?: string | null;
  toolParams?: Record<string, unknown>;
  error?: { phase?: string | null; message: string } | null;
};
type ToolRunResultResponse = {
  taskId: string;
  toolId: string;
  resultPath: string;
  result: PprofResult;
};
type PprofResult = {
  schemaVersion: number;
  toolId: string;
  actionId: string;
  status: "OK" | "FAILED" | "TIMED_OUT";
  profileType: string;
  sampleIndex: string;
  total?: string | null;
  top: PprofTopEntry[];
  artifacts: {
    topTextPath: string;
    treeTextPath: string;
    rawTextPath: string;
    svgPath?: string | null;
    stderrPath: string;
  };
  warnings: string[];
  error?: string | null;
  durationMs: number;
  createdAt: string;
};
type PprofTopEntry = {
  rank: number;
  flat: string;
  flatPercent?: number | null;
  sumPercent?: number | null;
  cum: string;
  cumPercent?: number | null;
  function: string;
};

export function ToolsView({ apiKey }: { apiKey: string }) {
  const [tools, setTools] = useState<ToolDescriptor[]>([]);
  const [selectedToolId, setSelectedToolId] = useState("pprof_analyzer");
  const [runs, setRuns] = useState<ToolRunSummary[]>([]);
  const [selectedRun, setSelectedRun] = useState<ToolRunRecord | null>(null);
  const [result, setResult] = useState<ToolRunResultResponse | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [sampleIndex, setSampleIndex] = useState("samples");
  const [nodeCount, setNodeCount] = useState(50);
  const [generateSvg, setGenerateSvg] = useState(false);
  const [status, setStatus] = useState("Tools ready");
  const [uploadProgress, setUploadProgress] = useState(0);
  const [loading, setLoading] = useState(false);

  const selectedTool = tools.find((tool) => tool.toolId === selectedToolId) ?? tools[0] ?? null;

  const refreshTools = useCallback(async () => {
    if (!apiKey.trim()) {
      setTools([]);
      return;
    }
    const response = await fetchJson<{ tools: ToolDescriptor[] }>("/api/tools", { headers: authHeaders(apiKey) });
    setTools(response.tools);
    if (!response.tools.some((tool) => tool.toolId === selectedToolId) && response.tools.length) {
      setSelectedToolId(response.tools[0].toolId);
    }
  }, [apiKey, selectedToolId]);

  const refreshRuns = useCallback(async () => {
    if (!apiKey.trim()) {
      setRuns([]);
      return;
    }
    const params = new URLSearchParams();
    params.set("limit", "30");
    if (selectedToolId) params.set("toolId", selectedToolId);
    const response = await fetchJson<{ runs: ToolRunSummary[] }>(`/api/tools/runs?${params.toString()}`, { headers: authHeaders(apiKey) });
    setRuns(response.runs);
  }, [apiKey, selectedToolId]);

  const selectRun = useCallback(async (taskId: string) => {
    const run = await fetchJson<ToolRunRecord>(`/api/tools/runs/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) });
    setSelectedRun(run);
    if (run.status === "SUCCEEDED") {
      const nextResult = await fetchJson<ToolRunResultResponse>(`/api/tools/runs/${encodeURIComponent(taskId)}/result`, { headers: authHeaders(apiKey) });
      setResult(nextResult);
    } else {
      setResult(null);
    }
  }, [apiKey]);

  useEffect(() => {
    setSelectedRun(null);
    setResult(null);
    void refreshTools().catch((reason) => setStatus(errorMessage(reason)));
    void refreshRuns().catch((reason) => setStatus(errorMessage(reason)));
  }, [refreshRuns, refreshTools]);

  useEffect(() => {
    if (!apiKey.trim()) return;
    const timer = window.setInterval(() => {
      void refreshRuns().catch(() => undefined);
      if (selectedRun && !isTerminal(selectedRun.status)) {
        void selectRun(selectedRun.taskId).catch((reason) => setStatus(errorMessage(reason)));
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [apiKey, refreshRuns, selectedRun, selectRun]);

  async function runTool() {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    if (!selectedTool) {
      setStatus("No tool selected");
      return;
    }
    if (!selectedTool.enabled) {
      setStatus(`${selectedTool.displayName} is disabled in server config`);
      return;
    }
    if (!file) {
      setStatus("Choose a pprof file");
      return;
    }
    setLoading(true);
    setUploadProgress(0);
    setResult(null);
    try {
      setStatus(`Uploading ${file.name}`);
      const upload = await uploadFile(file, apiKey, (value) => setUploadProgress(Math.round(value * 100)));
      setStatus("Upload complete, starting tool run");
      const run = await fetchJson<ToolRunSummary>(`/api/tools/${encodeURIComponent(selectedTool.toolId)}/runs`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          uploadIds: [upload.uploadId],
          params: {
            sampleIndex,
            nodeCount,
            generateSvg
          },
          idempotencyKey: `webui-${selectedTool.toolId}-${Date.now()}`
        })
      });
      setStatus(`Created ${run.taskId}`);
      await refreshRuns();
      await selectRun(run.taskId);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="grid gap-5 xl:grid-cols-[340px_1fr]">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <div>
              <CardTitle>Tool catalog</CardTitle>
              <CardDescription>Configured plugins exposed by the Rust Server</CardDescription>
            </div>
            <Button className="h-8 px-3" variant="outline" onClick={() => void refreshTools()}><RefreshCw className="h-4 w-4" /></Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {tools.length ? tools.map((tool) => (
            <button key={tool.toolId} className={`w-full rounded-lg border p-3 text-left ${selectedToolId === tool.toolId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => setSelectedToolId(tool.toolId)}>
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">{tool.displayName}</p>
                  <p className="mt-1 text-xs text-muted-foreground">{tool.toolId} · {tool.backend}</p>
                </div>
                <Badge variant={tool.enabled ? "success" : "destructive"}>{tool.enabled ? "enabled" : "disabled"}</Badge>
              </div>
              <p className="mt-2 text-xs text-muted-foreground">{tool.description}</p>
            </button>
          )) : <EmptyState>No tools loaded.</EmptyState>}
        </CardContent>
      </Card>

      <div className="space-y-5">
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>{selectedTool?.displayName ?? "Tools"}</CardTitle>
                <CardDescription>{selectedTool ? selectedTool.acceptedSuffixes.join(", ") : "Select a tool to run"}</CardDescription>
              </div>
              {selectedTool ? <Badge variant={selectedTool.enabled ? "success" : "destructive"}>{selectedTool.enabled ? "ready" : "disabled"}</Badge> : null}
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <label className="flex min-h-32 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
              <UploadCloud className="mb-2 h-7 w-7" />
              {file ? file.name : "Choose a Go pprof profile"}
              <input className="hidden" type="file" onChange={(event) => setFile(event.target.files?.[0] ?? null)} />
            </label>
            <div className="grid gap-3 md:grid-cols-[1fr_160px_auto] md:items-center">
              <Input value={sampleIndex} onChange={(event) => setSampleIndex(event.target.value)} placeholder="sample index" />
              <Input type="number" min={1} max={200} value={nodeCount} onChange={(event) => setNodeCount(Number(event.target.value) || 50)} />
              <label className="flex h-10 items-center gap-2 rounded-md border border-border px-3 text-sm text-muted-foreground">
                <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={generateSvg} onChange={(event) => setGenerateSvg(event.target.checked)} />
                SVG
              </label>
            </div>
            <div>
              <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>Upload</span><span>{uploadProgress}%</span></div>
              <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span className="text-sm text-muted-foreground">{status}</span>
              <Button disabled={loading || !selectedTool?.enabled} onClick={() => void runTool()}><Play className="mr-2 h-4 w-4" />Run tool</Button>
            </div>
          </CardContent>
        </Card>

        <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between gap-3">
                <CardTitle>Tool runs</CardTitle>
                <Button className="h-8 px-3" variant="outline" onClick={() => void refreshRuns()}><RefreshCw className="h-4 w-4" /></Button>
              </div>
            </CardHeader>
            <CardContent className="space-y-2">
              {runs.length ? runs.map((run) => (
                <button key={run.taskId} className={`w-full rounded-lg border p-3 text-left ${selectedRun?.taskId === run.taskId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => void selectRun(run.taskId)}>
                  <div className="flex items-center justify-between gap-2"><span className="font-mono text-xs">{run.taskId}</span><RunStatusBadge status={run.status} /></div>
                  <p className="mt-1 text-xs text-muted-foreground">{run.phase ?? "No active phase"} · {new Date(run.createdAt).toLocaleString()}</p>
                </button>
              )) : <EmptyState>No tool runs yet.</EmptyState>}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Run status</CardTitle>
              <CardDescription>{selectedRun ? `${selectedRun.taskId} · attempt ${selectedRun.attempts ?? 0}` : "Select a run"}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {selectedRun ? (
                <>
                  <div className="flex flex-wrap items-center gap-2"><RunStatusBadge status={selectedRun.status} /><span className="text-sm text-muted-foreground">{selectedRun.phase ?? "No active phase"}</span></div>
                  {selectedRun.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedRun.error?.phase ? `${selectedRun.error.phase}: ` : ""}{selectedRun.error?.message ?? "Tool run failed"}</div> : null}
                  {!isTerminal(selectedRun.status) ? <p className="text-sm text-muted-foreground">Server is running the tool in the background.</p> : null}
                  {selectedRun.status === "SUCCEEDED" && !result ? <Button onClick={() => void selectRun(selectedRun.taskId)}>Load result</Button> : null}
                  {result ? <PprofResultView result={result.result} resultPath={result.resultPath} /> : null}
                </>
              ) : <EmptyState>Select or create a run to inspect status and artifacts.</EmptyState>}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

function PprofResultView({ result, resultPath }: { result: PprofResult; resultPath: string }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-4">
        <Metric label="Status" value={result.status} />
        <Metric label="Profile" value={result.profileType || "unknown"} />
        <Metric label="Total" value={result.total ?? "-"} />
        <Metric label="Duration" value={`${result.durationMs}ms`} />
      </div>
      {result.error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{result.error}</div> : null}
      {result.warnings.length ? <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">{result.warnings.join(" · ")}</div> : null}
      <div className="max-h-[560px] overflow-auto rounded-lg border border-border">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 z-10 bg-slate-50 text-xs text-muted-foreground shadow-[0_1px_0_hsl(var(--border))]">
            <tr>
              <th className="px-3 py-2">#</th>
              <th className="px-3 py-2">Flat</th>
              <th className="px-3 py-2">Flat %</th>
              <th className="px-3 py-2">Cum</th>
              <th className="px-3 py-2">Cum %</th>
              <th className="px-3 py-2">Function</th>
            </tr>
          </thead>
          <tbody>
            {result.top.length ? result.top.map((entry) => (
              <tr className="border-t border-border" key={`${entry.rank}:${entry.function}`}>
                <td className="px-3 py-2 text-muted-foreground">{entry.rank}</td>
                <td className="px-3 py-2 font-mono text-xs">{entry.flat}</td>
                <td className="px-3 py-2">{formatPercent(entry.flatPercent)}</td>
                <td className="px-3 py-2 font-mono text-xs">{entry.cum}</td>
                <td className="px-3 py-2">{formatPercent(entry.cumPercent)}</td>
                <td className="px-3 py-2 font-mono text-xs">{entry.function}</td>
              </tr>
            )) : (
              <tr><td className="px-3 py-8 text-center text-sm text-muted-foreground" colSpan={6}>No parsed top entries. Check raw artifacts.</td></tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="grid gap-2 md:grid-cols-2">
        <ArtifactPath label="Result" value={resultPath} />
        <ArtifactPath label="Top text" value={result.artifacts.topTextPath} />
        <ArtifactPath label="Tree text" value={result.artifacts.treeTextPath} />
        <ArtifactPath label="Raw text" value={result.artifacts.rawTextPath} />
        <ArtifactPath label="Stderr" value={result.artifacts.stderrPath} />
        {result.artifacts.svgPath ? <ArtifactPath label="SVG" value={result.artifacts.svgPath} /> : null}
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm font-medium">{value}</p></div>;
}

function ArtifactPath({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-xs text-muted-foreground"><FileArchive className="h-4 w-4" />{label}</div><p className="mt-1 break-all font-mono text-xs">{value}</p></div>;
}

function RunStatusBadge({ status }: { status: ToolRunStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{status}</Badge>;
}

function isTerminal(status: ToolRunStatus) {
  return status === "SUCCEEDED" || status === "FAILED";
}

function formatPercent(value?: number | null) {
  return typeof value === "number" ? `${value.toFixed(2)}%` : "-";
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
