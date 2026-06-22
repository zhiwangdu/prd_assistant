import { Download, Play, Plus, RefreshCw, Save, Server, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { errorMessage } from "./errors";
import { startPolling } from "./polling";
import {
  createV2Executor,
  createV2ExecutorRun,
  disableV2Executor,
  downloadV2ExecutorRunFile,
  getV2ExecutorRun,
  getV2ExecutorRunResult,
  listV2ExecutorCommandTemplates,
  listV2ExecutorRuns,
  listV2Executors,
  updateV2Executor,
  type V2RemoteCommandTemplate,
  type V2RemoteExecutorRecord,
  type V2RemoteRunRecord,
  type V2RemoteRunResult,
  type V2RemoteRunStatus,
  type V2RemoteRunSummary
} from "./v2-api";

const emptyForm = {
  name: "",
  host: "",
  port: "22",
  user: "root",
  tags: "",
  notes: "",
  enabled: true
};

export function V2ExecutorsBridge({ apiKey }: { apiKey: string }) {
  const [executors, setExecutors] = useState<V2RemoteExecutorRecord[]>([]);
  const [commands, setCommands] = useState<V2RemoteCommandTemplate[]>([]);
  const [remoteEnabled, setRemoteEnabled] = useState(false);
  const [runs, setRuns] = useState<V2RemoteRunSummary[]>([]);
  const [selectedExecutorId, setSelectedExecutorId] = useState("");
  const [selectedCommandId, setSelectedCommandId] = useState("smoke_ls_root");
  const [selectedRun, setSelectedRun] = useState<V2RemoteRunRecord | null>(null);
  const [result, setResult] = useState<V2RemoteRunResult | null>(null);
  const [form, setForm] = useState(emptyForm);
  const [status, setStatus] = useState("V2 Executors ready");
  const [loading, setLoading] = useState(false);
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
    const response = await listV2Executors(apiKey);
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
    const response = await listV2ExecutorCommandTemplates(apiKey);
    setCommands(response.commands);
    setRemoteEnabled(response.enabled);
    if (!response.commands.some((command) => command.commandId === selectedCommandId) && response.commands.length) {
      setSelectedCommandId(response.commands[0].commandId);
    }
  }, [apiKey, selectedCommandId]);

  const refreshRuns = useCallback(async () => {
    if (!apiKey.trim()) {
      setRuns([]);
      return;
    }
    const response = await listV2ExecutorRuns(apiKey, { executorId: selectedExecutorId || undefined, limit: 30 });
    setRuns(response.runs);
  }, [apiKey, selectedExecutorId]);

  const selectRun = useCallback(async (runId: string) => {
    const run = await getV2ExecutorRun(apiKey, runId);
    setSelectedRun(run);
    if (run.status === "SUCCEEDED") {
      setResult(await getV2ExecutorRunResult(apiKey, runId));
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
    return startPolling(() => {
      void refreshRuns().catch(() => undefined);
      if (selectedRun && !isTerminal(selectedRun.status)) {
        void selectRun(selectedRun.taskId).catch((reason) => setStatus(errorMessage(reason)));
      }
    }, 1000);
  }, [apiKey, refreshRuns, selectedRun, selectRun]);

  function selectExecutor(executor: V2RemoteExecutorRecord) {
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
    setLoading(true);
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
      const executor = selectedExecutorId
        ? await updateV2Executor(apiKey, selectedExecutorId, payload)
        : await createV2Executor(apiKey, payload);
      setStatus(selectedExecutorId ? "V2 executor updated" : "V2 executor created");
      await refreshExecutors();
      selectExecutor(executor);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function disableExecutor() {
    if (!selectedExecutorId) return;
    setLoading(true);
    try {
      const executor = await disableV2Executor(apiKey, selectedExecutorId);
      setStatus("V2 executor disabled");
      await refreshExecutors();
      selectExecutor(executor);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function runCommand() {
    if (!selectedExecutor || !selectedCommand) {
      setStatus("Select V2 executor and command");
      return;
    }
    setRunning(true);
    setResult(null);
    try {
      const run = await createV2ExecutorRun(apiKey, {
        executorId: selectedExecutor.executorId,
        commandId: selectedCommand.commandId,
        idempotencyKey: `webui-v2-${selectedExecutor.executorId}-${selectedCommand.commandId}-${Date.now()}`
      });
      setStatus(`Created V2 ${run.taskId}`);
      await refreshRuns();
      await selectRun(run.taskId);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setRunning(false);
    }
  }

  async function downloadRunFile(fileName: "result" | "stdout" | "stderr") {
    if (!selectedRun) return;
    try {
      await downloadV2ExecutorRunFile(apiKey, selectedRun.taskId, fileName);
      setStatus(`Downloaded V2 remote run ${fileName}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Server className="h-5 w-5 text-primary" />
              <CardTitle>V2 Executors Workbench</CardTitle>
            </div>
            <CardDescription>Python V2 Remote Executor CRUD, whitelisted SSH command runs, and persisted stdout/stderr/result inspection</CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <Badge variant={remoteEnabled ? "success" : "destructive"}>{remoteEnabled ? "enabled" : "disabled"}</Badge>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => { void refreshExecutors(); void refreshCommands(); void refreshRuns(); }}><RefreshCw className="mr-2 h-4 w-4" />刷新</Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid gap-5 xl:grid-cols-[320px_minmax(0,1fr)]">
          <div className="rounded-lg border border-border p-3">
            <Button className="mb-3 w-full justify-start" variant="outline" onClick={newExecutor}><Plus className="mr-2 h-4 w-4" />New V2 executor</Button>
            <div className="max-h-[420px] space-y-2 overflow-auto">
              {executors.length ? executors.map((executor) => (
                <button className={`w-full rounded-lg border p-3 text-left ${selectedExecutorId === executor.executorId ? "border-primary bg-slate-50" : "border-border"}`} key={executor.executorId} onClick={() => selectExecutor(executor)}>
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="text-sm font-medium">{executor.name}</p>
                      <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{executor.user}@{executor.host}:{executor.port}</p>
                    </div>
                    <Badge variant={executor.enabled ? "success" : "destructive"}>{executor.enabled ? "enabled" : "disabled"}</Badge>
                  </div>
                  {executor.tags.length ? <p className="mt-2 text-xs text-muted-foreground">{executor.tags.join(", ")}</p> : null}
                  <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <LastCheckBadge executor={executor} />
                    <span>{executor.lastCheck ? formatDate(executor.lastCheck.checkedAt) : "No last check"}</span>
                  </div>
                </button>
              )) : <EmptyState>No V2 executors yet.</EmptyState>}
            </div>
          </div>

          <div className="space-y-5">
            <div className="rounded-lg border border-border p-4">
              <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h3 className="text-sm font-semibold">{selectedExecutor ? selectedExecutor.name : "V2 executor details"}</h3>
                  <p className="text-xs text-muted-foreground">{selectedExecutor ? `${selectedExecutor.user}@${selectedExecutor.host}:${selectedExecutor.port}` : "Create or select an executor"}</p>
                </div>
                {selectedExecutor ? <Badge variant={selectedExecutor.enabled ? "success" : "destructive"}>{selectedExecutor.enabled ? "ready" : "disabled"}</Badge> : null}
              </div>
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
              <textarea className="mt-3 min-h-20 w-full rounded-md border border-input bg-white px-3 py-2 text-sm shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-ring" value={form.notes} onChange={(event) => setForm({ ...form, notes: event.target.value })} placeholder="Notes" />
              {selectedExecutor ? (
                <div className="mt-3 space-y-3">
                  <div className="grid gap-3 md:grid-cols-3">
                    <Metric label="Executor ID" value={selectedExecutor.executorId} />
                    <Metric label="Created" value={formatDate(selectedExecutor.createdAt)} />
                    <Metric label="Updated" value={formatDate(selectedExecutor.updatedAt)} />
                  </div>
                  <LastCheckDetails executor={selectedExecutor} />
                </div>
              ) : null}
              <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
                <span className="text-sm text-muted-foreground">{status}</span>
                <div className="flex flex-wrap gap-2">
                  {selectedExecutorId ? <Button variant="outline" disabled={loading} onClick={() => void disableExecutor()}><Trash2 className="mr-2 h-4 w-4" />Disable</Button> : null}
                  <Button disabled={loading} onClick={() => void saveExecutor()}><Save className="mr-2 h-4 w-4" />{selectedExecutorId ? "Save" : "Create"}</Button>
                </div>
              </div>
            </div>

            <div className="grid gap-5 xl:grid-cols-[320px_minmax(0,1fr)]">
              <div className="rounded-lg border border-border p-4">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <h3 className="text-sm font-semibold">V2 command templates</h3>
                  <Button className="h-8 px-3" variant="outline" onClick={() => void refreshCommands()}><RefreshCw className="h-4 w-4" /></Button>
                </div>
                <div className="space-y-2">
                  {commands.length ? commands.map((command) => (
                    <button className={`w-full rounded-lg border p-3 text-left ${selectedCommandId === command.commandId ? "border-primary bg-slate-50" : "border-border"}`} key={command.commandId} onClick={() => setSelectedCommandId(command.commandId)}>
                      <div className="flex items-start justify-between gap-2">
                        <p className="text-sm font-medium">{command.displayName}</p>
                        <Badge variant={command.enabled ? "success" : "destructive"}>{command.enabled ? "enabled" : "disabled"}</Badge>
                      </div>
                      <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{command.description}</p>
                      <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{command.argv.join(" ")}</p>
                      <p className="mt-1 text-xs text-muted-foreground">timeout {command.timeoutSeconds ?? "-"}s · {command.commandId}</p>
                    </button>
                  )) : <EmptyState>No V2 command templates configured.</EmptyState>}
                </div>
                <Button className="mt-3 w-full" disabled={running || !remoteEnabled || !selectedExecutor?.enabled || !selectedCommand?.enabled} onClick={() => void runCommand()}><Play className="mr-2 h-4 w-4" />Run V2 command</Button>
              </div>

              <div className="rounded-lg border border-border p-4">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <h3 className="text-sm font-semibold">V2 remote runs</h3>
                  <Button className="h-8 px-3" variant="outline" onClick={() => void refreshRuns()}><RefreshCw className="h-4 w-4" /></Button>
                </div>
                <div className="max-h-[360px] space-y-2 overflow-auto">
                  {runs.length ? runs.map((run) => (
                    <button className={`w-full rounded-lg border p-3 text-left ${selectedRun?.taskId === run.taskId ? "border-primary bg-slate-50" : "border-border"}`} key={run.taskId} onClick={() => void selectRun(run.taskId)}>
                      <div className="flex items-center justify-between gap-2"><span className="font-mono text-xs">{run.taskId}</span><RunStatusBadge status={run.status} /></div>
                      <p className="mt-1 text-xs text-muted-foreground">{run.alias ?? run.phase ?? "Remote command"} · {new Date(run.createdAt).toLocaleString()}</p>
                      <p className="mt-1 text-xs text-muted-foreground">{run.phase ?? "queued"}</p>
                    </button>
                  )) : <EmptyState>No V2 remote runs yet.</EmptyState>}
                </div>
              </div>
            </div>

            <div className="rounded-lg border border-border p-4">
              <h3 className="mb-2 text-sm font-semibold">V2 run result</h3>
              {selectedRun ? (
                <div className="space-y-4">
                  <div className="flex flex-wrap items-center gap-2"><RunStatusBadge status={selectedRun.status} /><span className="text-sm text-muted-foreground">{selectedRun.phase ?? "No active phase"}</span></div>
                  <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-6">
                    <Metric label="Run ID" value={selectedRun.taskId} />
                    <Metric label="Attempts" value={String(selectedRun.attempts ?? 0)} />
                    <Metric label="Executor" value={selectedRun.remoteExecutorId ?? "-"} />
                    <Metric label="Command" value={selectedRun.remoteCommandId ?? "-"} />
                    <Metric label="Created" value={formatDate(selectedRun.createdAt)} />
                    <Metric label="Updated" value={selectedRun.updatedAt ? formatDate(selectedRun.updatedAt) : "-"} />
                  </div>
                  {selectedRun.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedRun.error?.phase ? `${selectedRun.error.phase}: ` : ""}{selectedRun.error?.message ?? "Remote run failed"}</div> : null}
                  {selectedRun.status === "SUCCEEDED" && !result ? <Button onClick={() => void selectRun(selectedRun.taskId)}>Load V2 result</Button> : null}
                  {result ? <RemoteResultView result={result} onDownload={(fileName) => void downloadRunFile(fileName)} /> : null}
                </div>
              ) : <EmptyState>Select or create a V2 remote run.</EmptyState>}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function RemoteResultView({ result, onDownload }: { result: V2RemoteRunResult; onDownload: (fileName: "result" | "stdout" | "stderr") => void }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-4">
        <Metric label="Status" value={result.result.status} />
        <Metric label="Exit" value={result.result.exitCode == null ? "-" : String(result.result.exitCode)} />
        <Metric label="Duration" value={`${result.result.durationMs}ms`} />
        <Metric label="Executor" value={result.executorId} />
        <Metric label="Command ID" value={result.commandId} />
        <Metric label="Started" value={formatDate(result.result.startedAt)} />
        <Metric label="Completed" value={formatDate(result.result.completedAt)} />
        <Metric label="Command" value={result.result.commandArgv.join(" ")} />
      </div>
      {result.result.error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{result.result.error}</div> : null}
      {result.result.warnings.length ? <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">{result.result.warnings.join(" · ")}</div> : null}
      {result.result.sshArgvPreview?.length ? <OutputPreview title="ssh argv preview" value={result.result.sshArgvPreview.join(" ")} /> : null}
      <div className="grid gap-3 lg:grid-cols-2">
        <OutputPreview title="stdout" value={result.result.stdoutPreview} />
        <OutputPreview title="stderr" value={result.result.stderrPreview} />
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        <ArtifactPath label="Result" value={result.resultPath} onDownload={() => onDownload("result")} />
        <ArtifactPath label="Stdout" value={result.result.stdoutPath} onDownload={() => onDownload("stdout")} />
        <ArtifactPath label="Stderr" value={result.result.stderrPath} onDownload={() => onDownload("stderr")} />
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

function ArtifactPath({ label, value, onDownload }: { label: string; value: string; onDownload: () => void }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-xs text-muted-foreground"><Server className="h-4 w-4" />{label}</div>
          <p className="mt-1 break-all font-mono text-xs">{value}</p>
        </div>
        <Button className="h-8 w-8 shrink-0 px-0" variant="outline" title={`Download ${label}`} aria-label={`Download ${label}`} onClick={onDownload}>
          <Download className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}

function LastCheckBadge({ executor }: { executor: V2RemoteExecutorRecord }) {
  if (!executor.lastCheck) {
    return <Badge variant="secondary">unchecked</Badge>;
  }
  return <Badge variant={executor.lastCheck.status === "OK" ? "success" : "destructive"}>{executor.lastCheck.status}</Badge>;
}

function LastCheckDetails({ executor }: { executor: V2RemoteExecutorRecord }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="text-xs text-muted-foreground">Last check</p>
        <LastCheckBadge executor={executor} />
        <span className="text-xs text-muted-foreground">{executor.lastCheck ? formatDate(executor.lastCheck.checkedAt) : "-"}</span>
      </div>
      <p className="mt-1 break-words text-sm">{executor.lastCheck?.message ?? "No executor check has been recorded."}</p>
    </div>
  );
}

function RunStatusBadge({ status }: { status: V2RemoteRunStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{status}</Badge>;
}

function isTerminal(status: V2RemoteRunStatus) {
  return status === "SUCCEEDED" || status === "FAILED";
}

function parseTags(value: string) {
  return value.split(",").map((tag) => tag.trim()).filter(Boolean);
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}
