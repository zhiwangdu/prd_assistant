import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState } from "./components/ui";
import { valueOrDash } from "./lib/utils";
import { authHeaders, fetchJson } from "./metadata/api";

type RunStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED" | "CANCELLED";

type RunSummary = {
  taskId: string;
  taskKind?: string;
  status: RunStatus;
  phase?: string | null;
  toolId?: string | null;
  createdAt?: string;
};

type RunRecord = RunSummary & {
  attempts?: number;
  toolParams?: Record<string, unknown>;
  error?: { phase?: string | null; message: string } | null;
  updatedAt?: string;
};

type RunResult = {
  taskId: string;
  toolId: string;
  resultPath: string;
  result: unknown;
};

const TERMINAL: RunStatus[] = ["SUCCEEDED", "FAILED", "CANCELLED"];

function statusVariant(status: RunStatus) {
  switch (status) {
    case "SUCCEEDED":
      return "success" as const;
    case "FAILED":
    case "CANCELLED":
      return "destructive" as const;
    case "RUNNING":
      return "default" as const;
    case "QUEUED":
      return "secondary" as const;
    default:
      return "warning" as const;
  }
}

export function RunsView({ apiKey }: { apiKey: string }) {
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [record, setRecord] = useState<RunRecord | null>(null);
  const [result, setResult] = useState<RunResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const refreshRuns = useCallback(async () => {
    if (!apiKey.trim()) return;
    try {
      const response = await fetchJson<{ runs: RunSummary[] }>("/api/tools/runs?limit=100", { headers: authHeaders(apiKey) });
      setRuns(response.runs ?? []);
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [apiKey]);

  useEffect(() => {
    void refreshRuns();
  }, [refreshRuns]);

  // Poll while any run is still non-terminal.
  useEffect(() => {
    if (!apiKey.trim() || runs.length === 0) return;
    if (!runs.some((run) => !TERMINAL.includes(run.status))) return;
    const timer = setInterval(() => void refreshRuns(), 2500);
    return () => clearInterval(timer);
  }, [apiKey, runs, refreshRuns]);

  const selectRun = useCallback(async (taskId: string) => {
    setSelectedId(taskId);
    setRecord(null);
    setResult(null);
    if (!apiKey.trim() || !taskId) return;
    setLoading(true);
    try {
      const detail = await fetchJson<RunRecord>(`/api/tools/runs/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) });
      setRecord(detail);
      if (detail.status === "SUCCEEDED") {
        try {
          const runResult = await fetchJson<RunResult>(`/api/tools/runs/${encodeURIComponent(taskId)}/result`, { headers: authHeaders(apiKey) });
          setResult(runResult);
        } catch {
          setResult(null);
        }
      }
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey]);

  useEffect(() => {
    if (selectedId) void selectRun(selectedId);
  }, [selectedId, selectRun]);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Runs</h2>
          <p className="text-sm text-muted-foreground">Tool, fetch, executor, and preprocess run history.</p>
        </div>
        <Button className="h-8 px-3" variant="outline" onClick={() => void refreshRuns()}><RefreshCw className="h-4 w-4" /></Button>
      </div>
      {error ? <p className="text-sm text-red-600">{error}</p> : null}
      <div className="grid gap-4 lg:grid-cols-[360px_1fr]">
        <Card>
          <CardHeader>
            <CardTitle>Recent runs</CardTitle>
            <CardDescription>{runs.length} run(s)</CardDescription>
          </CardHeader>
          <CardContent className="space-y-1">
            {runs.length === 0 ? <EmptyState>No runs yet.</EmptyState> : runs.map((run) => (
              <button key={run.taskId} className={`w-full rounded-md border px-3 py-2 text-left text-sm ${selectedId === run.taskId ? "border-primary bg-slate-50" : "border-transparent hover:bg-slate-50"}`} onClick={() => void selectRun(run.taskId)}>
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate font-mono text-xs">{run.taskId}</span>
                  <Badge variant={statusVariant(run.status)}>{run.status}</Badge>
                </div>
                <div className="mt-0.5 truncate text-xs text-muted-foreground">{run.toolId ?? run.taskKind ?? "-"}</div>
              </button>
            ))}
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>Detail</CardTitle></CardHeader>
          <CardContent>
            {!record ? <EmptyState>Select a run to inspect.</EmptyState> : (
              <div className="space-y-4">
                <dl className="grid grid-cols-[120px_1fr] gap-2 text-sm">
                  <dt className="text-muted-foreground">Task ID</dt><dd className="font-mono text-xs">{record.taskId}</dd>
                  <dt className="text-muted-foreground">Tool</dt><dd>{valueOrDash(record.toolId)}</dd>
                  <dt className="text-muted-foreground">Status</dt><dd><Badge variant={statusVariant(record.status)}>{record.status}</Badge></dd>
                  <dt className="text-muted-foreground">Phase</dt><dd>{valueOrDash(record.phase ?? undefined)}</dd>
                  <dt className="text-muted-foreground">Attempts</dt><dd>{valueOrDash(record.attempts)}</dd>
                  <dt className="text-muted-foreground">Created</dt><dd className="text-xs">{valueOrDash(record.createdAt)}</dd>
                </dl>
                {record.error ? (
                  <div className="rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700">
                    <p className="font-medium">{record.error.phase ? `Phase: ${record.error.phase}` : "Error"}</p>
                    <p className="mt-1 break-words">{record.error.message}</p>
                  </div>
                ) : null}
                {record.toolParams && Object.keys(record.toolParams).length > 0 ? (
                  <div>
                    <p className="mb-1 text-xs font-medium text-muted-foreground">Params</p>
                    <pre className="overflow-auto rounded-md bg-slate-900 p-3 text-xs text-slate-100">{JSON.stringify(record.toolParams, null, 2)}</pre>
                  </div>
                ) : null}
                {result ? (
                  <div>
                    <p className="mb-1 text-xs font-medium text-muted-foreground">Result ({result.resultPath})</p>
                    <pre className="max-h-96 overflow-auto rounded-md bg-slate-900 p-3 text-xs text-slate-100">{JSON.stringify(result.result, null, 2)}</pre>
                  </div>
                ) : null}
                {loading ? <p className="text-xs text-muted-foreground">Loading…</p> : null}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
