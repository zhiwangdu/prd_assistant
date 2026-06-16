import { Play, Plus, RefreshCw, Save, Server, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { V2ExecutorsBridge } from "./V2ExecutorsBridge";

type RunStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";

type RemoteExecutorRecord = {
  executorId: string;
  name: string;
  host: string;
  port: number;
  user: string;
  tags: string[];
  enabled: boolean;
  notes?: string | null;
  lastCheck?: { checkedAt: string; status: "OK" | "FAILED"; message: string } | null;
  createdAt: string;
  updatedAt: string;
};

type RemoteCommandTemplate = {
  commandId: string;
  displayName: string;
  description: string;
  enabled: boolean;
  argv: string[];
  timeoutSeconds: number;
};

type RemoteRunSummary = {
  taskId: string;
  alias?: string | null;
  taskKind: "remote_command_run";
  status: RunStatus;
  phase?: string | null;
  createdAt: string;
};

type RemoteRunRecord = RemoteRunSummary & {
  attempts?: number;
  remoteExecutorId?: string | null;
  remoteCommandId?: string | null;
  error?: { phase?: string | null; message: string } | null;
};

type RemoteRunResultResponse = {
  taskId: string;
  executorId: string;
  commandId: string;
  resultPath: string;
  result: {
    status: "OK" | "FAILED" | "TIMED_OUT";
    exitCode?: number | null;
    durationMs: number;
    commandArgv: string[];
    stdoutPath: string;
    stderrPath: string;
    stdoutPreview: string;
    stderrPreview: string;
    warnings: string[];
    error?: string | null;
    startedAt: string;
    completedAt: string;
  };
};

const emptyForm = {
  name: "",
  host: "",
  port: "22",
  user: "root",
  tags: "",
  notes: "",
  enabled: true
};

export function ExecutorsView({ apiKey }: { apiKey: string }) {
  const [executors, setExecutors] = useState<RemoteExecutorRecord[]>([]);
  const [commands, setCommands] = useState<RemoteCommandTemplate[]>([]);
  const [runs, setRuns] = useState<RemoteRunSummary[]>([]);
  const [selectedExecutorId, setSelectedExecutorId] = useState("");
  const [selectedCommandId, setSelectedCommandId] = useState("smoke_ls_root");
  const [selectedRun, setSelectedRun] = useState<RemoteRunRecord | null>(null);
  const [result, setResult] = useState<RemoteRunResultResponse | null>(null);
  const [form, setForm] = useState(emptyForm);
  const [status, setStatus] = useState("Executors ready");
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState(false);

  const selectedExecutor = useMemo(
    () => executors.find((executor) => executor.executorId === selectedExecutorId) ?? null,
    [executors, selectedExecutorId]
  );
  const selectedCommand = useMemo(
    () => commands.find((command) => command.commandId === selectedCommandId) ?? commands[0] ?? null,
    [commands, selectedCommandId]
  );

  const refreshExecutors = useCallback(async () => {
    if (!apiKey.trim()) {
      setExecutors([]);
      return;
    }
    const response = await fetchJson<{ executors: RemoteExecutorRecord[] }>("/api/executors", { headers: authHeaders(apiKey) });
    setExecutors(response.executors);
    if (!selectedExecutorId && response.executors.length) {
      selectExecutor(response.executors[0]);
    }
  }, [apiKey, selectedExecutorId]);

  const refreshCommands = useCallback(async () => {
    if (!apiKey.trim()) {
      setCommands([]);
      return;
    }
    const response = await fetchJson<{ commands: RemoteCommandTemplate[] }>("/api/executor-command-templates", { headers: authHeaders(apiKey) });
    setCommands(response.commands);
    if (!response.commands.some((command) => command.commandId === selectedCommandId) && response.commands.length) {
      setSelectedCommandId(response.commands[0].commandId);
    }
  }, [apiKey, selectedCommandId]);

  const refreshRuns = useCallback(async () => {
    if (!apiKey.trim()) {
      setRuns([]);
      return;
    }
    const params = new URLSearchParams();
    params.set("limit", "30");
    if (selectedExecutorId) params.set("executorId", selectedExecutorId);
    const response = await fetchJson<{ runs: RemoteRunSummary[] }>(`/api/executor-runs?${params.toString()}`, { headers: authHeaders(apiKey) });
    setRuns(response.runs);
  }, [apiKey, selectedExecutorId]);

  const selectRun = useCallback(async (taskId: string) => {
    const run = await fetchJson<RemoteRunRecord>(`/api/executor-runs/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) });
    setSelectedRun(run);
    if (run.status === "SUCCEEDED") {
      const nextResult = await fetchJson<RemoteRunResultResponse>(`/api/executor-runs/${encodeURIComponent(taskId)}/result`, { headers: authHeaders(apiKey) });
      setResult(nextResult);
    } else {
      setResult(null);
    }
  }, [apiKey]);

  useEffect(() => {
    void refreshExecutors().catch((reason) => setStatus(errorMessage(reason)));
    void refreshCommands().catch((reason) => setStatus(errorMessage(reason)));
  }, [refreshCommands, refreshExecutors]);

  useEffect(() => {
    setSelectedRun(null);
    setResult(null);
    void refreshRuns().catch((reason) => setStatus(errorMessage(reason)));
  }, [refreshRuns]);

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

  function selectExecutor(executor: RemoteExecutorRecord) {
    setSelectedExecutorId(executor.executorId);
    setForm({
      name: executor.name,
      host: executor.host,
      port: String(executor.port),
      user: executor.user,
      tags: executor.tags.join(", "),
      notes: executor.notes ?? "",
      enabled: executor.enabled
    });
  }

  function newExecutor() {
    setSelectedExecutorId("");
    setForm(emptyForm);
    setSelectedRun(null);
    setResult(null);
  }

  async function saveExecutor() {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    setSaving(true);
    try {
      const payload = {
        name: form.name,
        host: form.host,
        port: Number(form.port) || 22,
        user: form.user,
        tags: parseTags(form.tags),
        notes: form.notes.trim() ? form.notes : null,
        enabled: form.enabled
      };
      const path = selectedExecutorId ? `/api/executors/${encodeURIComponent(selectedExecutorId)}` : "/api/executors";
      const executor = await fetchJson<RemoteExecutorRecord>(path, {
        method: selectedExecutorId ? "PATCH" : "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify(payload)
      });
      setStatus(selectedExecutorId ? "Executor updated" : "Executor created");
      await refreshExecutors();
      selectExecutor(executor);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }

  async function disableExecutor() {
    if (!selectedExecutorId) return;
    setSaving(true);
    try {
      const executor = await fetchJson<RemoteExecutorRecord>(`/api/executors/${encodeURIComponent(selectedExecutorId)}`, {
        method: "DELETE",
        headers: authHeaders(apiKey)
      });
      setStatus("Executor disabled");
      await refreshExecutors();
      selectExecutor(executor);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }

  async function runCommand() {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    if (!selectedExecutor) {
      setStatus("Select an executor");
      return;
    }
    if (!selectedExecutor.enabled) {
      setStatus("Executor is disabled");
      return;
    }
    if (!selectedCommand || !selectedCommand.enabled) {
      setStatus("Select an enabled command template");
      return;
    }
    setRunning(true);
    setResult(null);
    try {
      const run = await fetchJson<RemoteRunSummary>("/api/executor-runs", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          executorId: selectedExecutor.executorId,
          commandId: selectedCommand.commandId,
          idempotencyKey: `webui-${selectedExecutor.executorId}-${selectedCommand.commandId}-${Date.now()}`
        })
      });
      setStatus(`Created ${run.taskId}`);
      await refreshRuns();
      await selectRun(run.taskId);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div className="space-y-5">
      <V2ExecutorsBridge apiKey={apiKey} />

      <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>Executors</CardTitle>
                <CardDescription>Managed ECS SSH targets</CardDescription>
              </div>
              <Button className="h-8 px-3" variant="outline" onClick={() => void refreshExecutors()}><RefreshCw className="h-4 w-4" /></Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            <Button className="w-full justify-start" variant="outline" onClick={newExecutor}><Plus className="mr-2 h-4 w-4" />New executor</Button>
            {executors.length ? executors.map((executor) => (
              <button key={executor.executorId} className={`w-full rounded-lg border p-3 text-left ${selectedExecutorId === executor.executorId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => selectExecutor(executor)}>
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <p className="text-sm font-medium">{executor.name}</p>
                    <p className="mt-1 font-mono text-xs text-muted-foreground">{executor.user}@{executor.host}:{executor.port}</p>
                  </div>
                  <Badge variant={executor.enabled ? "success" : "destructive"}>{executor.enabled ? "enabled" : "disabled"}</Badge>
                </div>
                {executor.tags.length ? <p className="mt-2 text-xs text-muted-foreground">{executor.tags.join(", ")}</p> : null}
              </button>
            )) : <EmptyState>No executors yet.</EmptyState>}
          </CardContent>
        </Card>

        <div className="space-y-5">
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>{selectedExecutor ? selectedExecutor.name : "Executor details"}</CardTitle>
                <CardDescription>{selectedExecutor ? `${selectedExecutor.user}@${selectedExecutor.host}:${selectedExecutor.port}` : "Create or select an executor"}</CardDescription>
              </div>
              {selectedExecutor ? <Badge variant={selectedExecutor.enabled ? "success" : "destructive"}>{selectedExecutor.enabled ? "ready" : "disabled"}</Badge> : null}
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-3 md:grid-cols-2">
              <Input value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} placeholder="Name" />
              <Input value={form.host} onChange={(event) => setForm({ ...form, host: event.target.value })} placeholder="Host or IP" />
              <Input value={form.user} onChange={(event) => setForm({ ...form, user: event.target.value })} placeholder="SSH user" />
              <Input type="number" min={1} max={65535} value={form.port} onChange={(event) => setForm({ ...form, port: event.target.value })} placeholder="Port" />
              <Input value={form.tags} onChange={(event) => setForm({ ...form, tags: event.target.value })} placeholder="Tags, comma separated" />
              <label className="flex h-10 items-center gap-2 rounded-md border border-border px-3 text-sm text-muted-foreground">
                <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={form.enabled} onChange={(event) => setForm({ ...form, enabled: event.target.checked })} />
                Enabled
              </label>
            </div>
            <textarea className="min-h-20 w-full rounded-md border border-input bg-white px-3 py-2 text-sm shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-ring" value={form.notes} onChange={(event) => setForm({ ...form, notes: event.target.value })} placeholder="Notes" />
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span className="text-sm text-muted-foreground">{status}</span>
              <div className="flex flex-wrap gap-2">
                {selectedExecutorId ? <Button variant="outline" disabled={saving} onClick={() => void disableExecutor()}><Trash2 className="mr-2 h-4 w-4" />Disable</Button> : null}
                <Button disabled={saving} onClick={() => void saveExecutor()}><Save className="mr-2 h-4 w-4" />{selectedExecutorId ? "Save" : "Create"}</Button>
              </div>
            </div>
          </CardContent>
        </Card>

        <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between gap-3">
                <div>
                  <CardTitle>Command</CardTitle>
                  <CardDescription>Whitelisted SSH templates</CardDescription>
                </div>
                <Button className="h-8 px-3" variant="outline" onClick={() => void refreshCommands()}><RefreshCw className="h-4 w-4" /></Button>
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              {commands.length ? commands.map((command) => (
                <button key={command.commandId} className={`w-full rounded-lg border p-3 text-left ${selectedCommandId === command.commandId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => setSelectedCommandId(command.commandId)}>
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <p className="text-sm font-medium">{command.displayName}</p>
                      <p className="mt-1 font-mono text-xs text-muted-foreground">{command.argv.join(" ")}</p>
                    </div>
                    <Badge variant={command.enabled ? "success" : "destructive"}>{command.enabled ? "enabled" : "disabled"}</Badge>
                  </div>
                  {command.description ? <p className="mt-2 text-xs text-muted-foreground">{command.description}</p> : null}
                </button>
              )) : <EmptyState>No command templates configured.</EmptyState>}
              <Button className="w-full" disabled={running || !selectedExecutor?.enabled || !selectedCommand?.enabled} onClick={() => void runCommand()}><Play className="mr-2 h-4 w-4" />Run on selected executor</Button>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <div className="flex items-center justify-between gap-3">
                <div>
                  <CardTitle>Remote runs</CardTitle>
                  <CardDescription>{selectedExecutor ? `Filtered to ${selectedExecutor.name}` : "Recent remote command runs"}</CardDescription>
                </div>
                <Button className="h-8 px-3" variant="outline" onClick={() => void refreshRuns()}><RefreshCw className="h-4 w-4" /></Button>
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              {runs.length ? runs.map((run) => (
                <button key={run.taskId} className={`w-full rounded-lg border p-3 text-left ${selectedRun?.taskId === run.taskId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => void selectRun(run.taskId)}>
                  <div className="flex items-center justify-between gap-2"><span className="font-mono text-xs">{run.taskId}</span><RunStatusBadge status={run.status} /></div>
                  <p className="mt-1 text-xs text-muted-foreground">{run.alias ?? run.phase ?? "Remote command"} · {new Date(run.createdAt).toLocaleString()}</p>
                </button>
              )) : <EmptyState>No remote runs yet.</EmptyState>}
            </CardContent>
          </Card>
        </div>

        <Card>
          <CardHeader>
            <CardTitle>Run result</CardTitle>
            <CardDescription>{selectedRun ? `${selectedRun.taskId} · attempt ${selectedRun.attempts ?? 0}` : "Select or create a remote run"}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {selectedRun ? (
              <>
                <div className="flex flex-wrap items-center gap-2"><RunStatusBadge status={selectedRun.status} /><span className="text-sm text-muted-foreground">{selectedRun.phase ?? "No active phase"}</span></div>
                {selectedRun.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedRun.error?.phase ? `${selectedRun.error.phase}: ` : ""}{selectedRun.error?.message ?? "Remote run failed"}</div> : null}
                {!isTerminal(selectedRun.status) ? <p className="text-sm text-muted-foreground">Server is executing the SSH command in the background.</p> : null}
                {selectedRun.status === "SUCCEEDED" && !result ? <Button onClick={() => void selectRun(selectedRun.taskId)}>Load result</Button> : null}
                {result ? <RemoteResultView result={result} /> : null}
              </>
            ) : <EmptyState>Select a run to inspect stdout, stderr, and artifacts.</EmptyState>}
          </CardContent>
        </Card>
        </div>
      </div>
    </div>
  );
}

function RemoteResultView({ result }: { result: RemoteRunResultResponse }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-4">
        <Metric label="Status" value={result.result.status} />
        <Metric label="Exit" value={result.result.exitCode == null ? "-" : String(result.result.exitCode)} />
        <Metric label="Duration" value={`${result.result.durationMs}ms`} />
        <Metric label="Command" value={result.result.commandArgv.join(" ")} />
      </div>
      {result.result.error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{result.result.error}</div> : null}
      {result.result.warnings.length ? <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">{result.result.warnings.join(" · ")}</div> : null}
      <div className="grid gap-3 lg:grid-cols-2">
        <OutputPreview title="stdout" value={result.result.stdoutPreview} />
        <OutputPreview title="stderr" value={result.result.stderrPreview} />
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        <ArtifactPath label="Result" value={result.resultPath} />
        <ArtifactPath label="Stdout" value={result.result.stdoutPath} />
        <ArtifactPath label="Stderr" value={result.result.stderrPath} />
      </div>
    </div>
  );
}

function OutputPreview({ title, value }: { title: string; value: string }) {
  return (
    <div className="rounded-lg border border-border">
      <div className="border-b border-border bg-slate-50 px-3 py-2 text-xs font-medium text-muted-foreground">{title}</div>
      <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words p-3 font-mono text-xs">{value || "-"}</pre>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm font-medium">{value}</p></div>;
}

function ArtifactPath({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-xs text-muted-foreground"><Server className="h-4 w-4" />{label}</div><p className="mt-1 break-all font-mono text-xs">{value}</p></div>;
}

function RunStatusBadge({ status }: { status: RunStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{status}</Badge>;
}

function isTerminal(status: RunStatus) {
  return status === "SUCCEEDED" || status === "FAILED";
}

function parseTags(value: string) {
  return value.split(",").map((tag) => tag.trim()).filter(Boolean);
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
