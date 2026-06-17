import { Download, FileArchive, Play, RefreshCw, UploadCloud, Wrench } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  callV2TaskTool,
  createV2ToolRun,
  createV2Workspace,
  downloadV2Artifact,
  downloadV2ToolsZip,
  getV2ToolRun,
  getV2ToolRunArtifacts,
  getV2ToolRunResult,
  listV2ToolRuns,
  listV2Tools,
  uploadV2Files,
  type V2ToolDescriptor,
  type V2ToolRun,
  type V2ToolRunArtifacts
} from "./v2-api";

export function V2ToolsBridge({ apiKey }: { apiKey: string }) {
  const [tools, setTools] = useState<V2ToolDescriptor[]>([]);
  const [selectedToolId, setSelectedToolId] = useState("");
  const [runId, setRunId] = useState("");
  const [manualWorkspaceId, setManualWorkspaceId] = useState("");
  const [manualFiles, setManualFiles] = useState<File[]>([]);
  const [manualRuns, setManualRuns] = useState<V2ToolRun[]>([]);
  const [selectedManualRunId, setSelectedManualRunId] = useState("");
  const [manualResultText, setManualResultText] = useState("");
  const [manualArtifacts, setManualArtifacts] = useState<V2ToolRunArtifacts | null>(null);
  const [manualUploadProgress, setManualUploadProgress] = useState(0);
  const [paramsText, setParamsText] = useState("{}");
  const [resultText, setResultText] = useState("");
  const [status, setStatus] = useState("V2 tools waiting to load");
  const [loading, setLoading] = useState(false);

  const selectedTool = useMemo(() => tools.find((tool) => tool.toolId === selectedToolId) ?? tools[0] ?? null, [selectedToolId, tools]);
  const selectedManualRun = useMemo(() => manualRuns.find((run) => run.id === selectedManualRunId) ?? null, [manualRuns, selectedManualRunId]);

  const refreshTools = useCallback(async () => {
    if (!apiKey.trim()) {
      setTools([]);
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const response = await listV2Tools(apiKey);
      setTools(response.tools);
      if (!response.tools.some((tool) => tool.toolId === selectedToolId) && response.tools.length) {
        setSelectedToolId(response.tools[0].toolId);
      }
      setStatus(`V2 loaded ${response.tools.length} tools`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, selectedToolId]);

  const loadManualRun = useCallback(async (targetRunId: string) => {
    if (!apiKey.trim()) return null;
    const run = await getV2ToolRun(apiKey, targetRunId);
    setManualRuns((current) => upsertRun(current, run));
    const artifacts = await getV2ToolRunArtifacts(apiKey, targetRunId);
    setManualArtifacts(artifacts);
    if (run.status === "succeeded") {
      const result = await getV2ToolRunResult(apiKey, targetRunId);
      setManualResultText(JSON.stringify(result.result, null, 2));
    } else {
      setManualResultText(JSON.stringify(run, null, 2));
    }
    return run;
  }, [apiKey]);

  const refreshManualRuns = useCallback(async () => {
    if (!apiKey.trim()) {
      setManualRuns([]);
      setManualArtifacts(null);
      return;
    }
    const response = await listV2ToolRuns(apiKey, {
      toolId: selectedTool?.toolId,
      workspaceId: manualWorkspaceId.trim() || undefined,
      limit: 20
    });
    setManualRuns(response.runs);
    if (selectedManualRunId) {
      const current = response.runs.find((run) => run.id === selectedManualRunId);
      if (current) {
        await loadManualRun(current.id);
      } else {
        setManualArtifacts(null);
        setManualResultText("");
      }
    }
  }, [apiKey, loadManualRun, manualWorkspaceId, selectedManualRunId, selectedTool?.toolId]);

  useEffect(() => {
    void refreshTools();
  }, [refreshTools]);

  useEffect(() => {
    void refreshManualRuns().catch(() => undefined);
  }, [refreshManualRuns]);

  useEffect(() => {
    if (!selectedManualRunId || !selectedManualRun || isTerminalToolRun(selectedManualRun.status)) return;
    const timer = window.setInterval(() => {
      void loadManualRun(selectedManualRunId).catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [loadManualRun, selectedManualRun, selectedManualRunId]);

  useEffect(() => {
    setParamsText(JSON.stringify(selectedTool?.paramsTemplate ?? {}, null, 2));
    setResultText("");
    setManualResultText("");
    setManualArtifacts(null);
    setManualFiles([]);
    setSelectedManualRunId("");
    setManualUploadProgress(0);
  }, [selectedTool]);

  async function runTool() {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    if (!selectedTool) {
      setStatus("Select a V2 tool");
      return;
    }
    if (!runId.trim()) {
      setStatus("V2 tool execution requires a run id");
      return;
    }
    if (!selectedTool.enabled || !selectedTool.runnable) {
      setStatus(`${selectedTool.displayName} is not runnable`);
      return;
    }
    let params: unknown;
    try {
      params = JSON.parse(paramsText);
    } catch (reason) {
      setStatus(`Invalid JSON params: ${errorMessage(reason)}`);
      return;
    }
    if (!isJsonObject(params)) {
      setStatus("Params must be a JSON object");
      return;
    }
    setLoading(true);
    try {
      const toolName = selectedTool.toolId === "logagent.fetch" ? "logagent.fetch" : "logagent.run_domain_tool";
      const args = selectedTool.toolId === "logagent.fetch" ? params : { toolId: selectedTool.toolId, params };
      const response = await callV2TaskTool(apiKey, runId.trim(), toolName, args);
      if (response.error) {
        setResultText(JSON.stringify(response.error, null, 2));
        setStatus(response.error.message);
      } else {
        setResultText(JSON.stringify(response.result, null, 2));
        setStatus(`V2 task MCP called ${toolName}`);
      }
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function runManualTool() {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    if (!selectedTool) {
      setStatus("Select a V2 tool");
      return;
    }
    if (!selectedTool.enabled || !selectedTool.runnable) {
      setStatus(`${selectedTool.displayName} is not runnable`);
      return;
    }
    const minFiles = toolMinFiles(selectedTool);
    const maxFiles = toolMaxFiles(selectedTool);
    if (manualFiles.length < minFiles || manualFiles.length > maxFiles) {
      setStatus(`Choose ${minFiles}..${maxFiles} file(s) for manual tool_run`);
      return;
    }
    let params: unknown;
    try {
      params = JSON.parse(paramsText);
    } catch (reason) {
      setStatus(`Invalid JSON params: ${errorMessage(reason)}`);
      return;
    }
    if (!isJsonObject(params)) {
      setStatus("Params must be a JSON object");
      return;
    }
    setLoading(true);
    setManualResultText("");
    setManualArtifacts(null);
    try {
      let workspaceId = manualWorkspaceId.trim();
      if (!workspaceId) {
        setStatus("Creating manual tool workspace");
        const workspace = await createV2Workspace(apiKey, {
          question: `Manual tool run: ${selectedTool.toolId}`,
          mode: "diagnose",
          language: "zh-CN"
        });
        workspaceId = workspace.id;
        setManualWorkspaceId(workspaceId);
      }
      setManualUploadProgress(manualFiles.length ? 0 : 100);
      const uploads = manualFiles.length
        ? await uploadV2Files(apiKey, workspaceId, manualFiles, setManualUploadProgress)
        : [];
      setStatus(`Creating tool_run for ${selectedTool.toolId}`);
      const run = await createV2ToolRun(apiKey, selectedTool.toolId, {
        workspaceId,
        uploadIds: uploads.map((upload) => upload.id),
        params
      });
      setSelectedManualRunId(run.id);
      setManualRuns((current) => upsertRun(current, run));
      setManualResultText(JSON.stringify(run, null, 2));
      setStatus(`Created V2 tool_run ${run.id}`);
      await loadManualRun(run.id);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function selectManualRun(runId: string) {
    setSelectedManualRunId(runId);
    setManualResultText("");
    setManualArtifacts(null);
    setLoading(true);
    try {
      await loadManualRun(runId);
      setStatus(`Loaded V2 tool_run ${runId}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function downloadManualArtifact(artifactId: string, relativePath: string) {
    try {
      await downloadV2Artifact(apiKey, artifactId, filenameFromPath(relativePath));
      setStatus(`Downloaded artifact ${relativePath}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    }
  }

  async function downloadTools() {
    setLoading(true);
    try {
      await downloadV2ToolsZip(apiKey);
      setStatus("Downloaded V2 tools.zip");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Wrench className="h-5 w-5 text-primary" />
              <CardTitle>V2 Tools Workbench</CardTitle>
            </div>
            <CardDescription>V2 tool catalog and run-scoped task MCP execution</CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refreshTools()}>
              <RefreshCw className="mr-2 h-4 w-4" />刷新
            </Button>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void downloadTools()}>
              <Download className="mr-2 h-4 w-4" />tools.zip
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)_460px]">
          <div className="rounded-lg border border-border p-3">
            <h3 className="mb-3 text-sm font-semibold">V2 catalog</h3>
            <div className="max-h-[420px] space-y-2 overflow-auto">
              {tools.length ? tools.map((tool) => (
                <button className={`w-full rounded-lg border p-3 text-left ${selectedTool?.toolId === tool.toolId ? "border-primary bg-slate-50" : "border-border"}`} key={tool.toolId} onClick={() => setSelectedToolId(tool.toolId)}>
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium">{tool.displayName}</p>
                      <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{tool.toolId}</p>
                    </div>
                    <Badge variant={tool.enabled ? "success" : "destructive"}>{tool.enabled ? "enabled" : "disabled"}</Badge>
                  </div>
                  <div className="mt-2 flex flex-wrap gap-1">
                    <Badge variant="secondary">{tool.backend}</Badge>
                    <Badge variant="outline">{tool.source ?? "configured"}</Badge>
                    {tool.runnable ? <Badge variant="success">runnable</Badge> : <Badge variant="secondary">not runnable</Badge>}
                    {tool.exportable ? <Badge variant="outline">exportable</Badge> : null}
                    {tool.manualOnly ? <Badge variant="outline">manual only</Badge> : null}
                  </div>
                  {tool.tags?.length ? <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{tool.tags.join(", ")}</p> : null}
                </button>
              )) : <EmptyState>No V2 tools.</EmptyState>}
            </div>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            {selectedTool ? (
              <>
                <div>
                  <h3 className="text-sm font-semibold">{selectedTool.displayName}</h3>
                  <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{selectedTool.toolId}</p>
                </div>
                <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
                  <Metric label="backend" value={selectedTool.backend} />
                  <Metric label="source" value={selectedTool.source ?? "configured"} />
                  <Metric label="readOnly" value={String(selectedTool.readOnly)} />
                  <Metric label="editable" value={String(Boolean(selectedTool.editable))} />
                  <Metric label="exportable" value={String(Boolean(selectedTool.exportable))} />
                  <Metric label="manualOnly" value={String(Boolean(selectedTool.manualOnly))} />
                  <Metric label="file range" value={`${selectedTool.minFiles ?? "-"}..${selectedTool.maxFiles ?? selectedTool.maxInputFiles ?? "-"}`} />
                  <Metric label="maxInputFiles" value={String(selectedTool.maxInputFiles ?? "-")} />
                  <Metric label="allowedHosts" value={(selectedTool.allowedHosts ?? []).join(", ") || "-"} />
                  <Metric label="acceptedSuffixes" value={(selectedTool.acceptedSuffixes ?? []).join(", ") || "-"} />
                  <Metric label="outputViews" value={(selectedTool.outputViews ?? []).join(", ") || "-"} />
                </div>
                <div className="grid gap-4 lg:grid-cols-2">
                  <JsonBlock title="paramsTemplate" value={selectedTool.paramsTemplate ?? {}} />
                  <JsonBlock title="match" value={selectedTool.match ?? {}} />
                </div>
                <div>
                  <JsonBlock title="paramsSchema" value={selectedTool.paramsSchema ?? {}} />
                </div>
              </>
            ) : <EmptyState>Select a V2 tool.</EmptyState>}
          </div>

          <div className="space-y-4">
            <div className="space-y-4 rounded-lg border border-border p-4">
              <div>
                <h3 className="text-sm font-semibold">Run-scoped execution</h3>
                <p className="mt-1 text-xs text-muted-foreground">Configured tools run through `logagent.run_domain_tool`; `logagent.fetch` expects an `endpointId` param.</p>
              </div>
              <Input value={runId} onChange={(event) => setRunId(event.target.value)} placeholder="V2 run id, e.g. run_..." />
              <div className="space-y-2">
                <p className="text-xs text-muted-foreground">Params JSON</p>
                <textarea className="min-h-32 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" spellCheck={false} value={paramsText} onChange={(event) => setParamsText(event.target.value)} />
              </div>
              <div className="flex flex-wrap items-center justify-between gap-3">
                <span className="text-xs text-muted-foreground">{status}</span>
                <Button disabled={loading || !selectedTool || !runId.trim()} onClick={() => void runTool()}><Play className="mr-2 h-4 w-4" />Run via task MCP</Button>
              </div>
              {resultText ? <pre className="max-h-80 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{resultText}</pre> : null}
            </div>

            <div className="space-y-4 rounded-lg border border-border p-4">
              <div>
                <h3 className="text-sm font-semibold">Manual tool_run</h3>
                <p className="mt-1 text-xs text-muted-foreground">Upload files to a V2 Workspace and queue `/api/v2/tools/:tool_id/runs`.</p>
              </div>
              <Input value={manualWorkspaceId} onChange={(event) => setManualWorkspaceId(event.target.value)} placeholder="Workspace id; blank creates one" />
              {selectedTool && toolMaxFiles(selectedTool) > 0 ? (
                <label className="flex min-h-24 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 px-4 text-center text-sm text-muted-foreground">
                  <UploadCloud className="mb-2 h-6 w-6" />
                  {manualFiles.length ? manualFiles.map((file) => file.name).join(", ") : `Choose ${toolMinFiles(selectedTool)}..${toolMaxFiles(selectedTool)} file(s)`}
                  <input
                    accept={fileAccept(selectedTool)}
                    className="hidden"
                    multiple={toolMaxFiles(selectedTool) > 1}
                    type="file"
                    onChange={(event) => setManualFiles(Array.from(event.target.files ?? []).slice(0, toolMaxFiles(selectedTool)))}
                  />
                </label>
              ) : (
                <div className="rounded-lg border border-border p-3 text-sm text-muted-foreground">This tool does not require uploaded files.</div>
              )}
              <div>
                <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>Upload</span><span>{manualUploadProgress}%</span></div>
                <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${manualUploadProgress}%` }} /></div>
              </div>
              <div className="flex flex-wrap items-center justify-between gap-3">
                <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refreshManualRuns()}>
                  <RefreshCw className="mr-2 h-4 w-4" />Runs
                </Button>
                <Button disabled={loading || !selectedTool || !selectedTool.runnable} onClick={() => void runManualTool()}>
                  <Play className="mr-2 h-4 w-4" />Create tool_run
                </Button>
              </div>
              {manualRuns.length ? (
                <div className="max-h-44 space-y-2 overflow-auto">
                  {manualRuns.map((run) => (
                    <button className={`w-full rounded-md border p-2 text-left ${selectedManualRun?.id === run.id ? "border-primary bg-slate-50" : "border-border"}`} key={run.id} onClick={() => void selectManualRun(run.id)}>
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-xs"><FileArchive className="mr-1 inline h-3.5 w-3.5 text-slate-400" />{run.id}</span>
                        <Badge variant={runStatusVariant(run.status)}>{run.status}</Badge>
                      </div>
                      <p className="mt-1 text-xs text-muted-foreground">{run.phase} · {new Date(run.created_at).toLocaleString()}</p>
                    </button>
                  ))}
                </div>
              ) : <EmptyState>No manual tool runs.</EmptyState>}
              {selectedManualRun ? (
                <div className="grid gap-2 rounded-lg border border-border p-3 text-xs sm:grid-cols-2">
                  <div>
                    <p className="text-muted-foreground">Selected run</p>
                    <p className="mt-1 break-all font-mono">{selectedManualRun.id}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Status</p>
                    <div className="mt-1"><Badge variant={runStatusVariant(selectedManualRun.status)}>{selectedManualRun.status}</Badge></div>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Phase</p>
                    <p className="mt-1 break-all">{selectedManualRun.phase}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Artifacts</p>
                    <p className="mt-1">{artifactCount(manualArtifacts)}</p>
                  </div>
                </div>
              ) : null}
              {manualArtifacts ? (
                <ToolRunArtifactList artifacts={manualArtifacts} onDownload={(artifactId, relativePath) => void downloadManualArtifact(artifactId, relativePath)} />
              ) : null}
              {manualResultText ? <pre className="max-h-80 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{manualResultText}</pre> : null}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value}</p></div>;
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return (
    <div>
      <p className="mb-2 text-xs text-muted-foreground">{title}</p>
      <pre className="max-h-52 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{JSON.stringify(value, null, 2)}</pre>
    </div>
  );
}

function ToolRunArtifactList({ artifacts, onDownload }: { artifacts: V2ToolRunArtifacts; onDownload: (artifactId: string, relativePath: string) => void }) {
  type ToolRunArtifactItem = {
    id: string;
    kind: string;
    summary: string;
    relativePath: string;
    logicalPath?: string;
    sizeBytes: number;
    contentType: string;
  };
  const items: ToolRunArtifactItem[] = [
    ...artifacts.uploads.map((upload) => ({
      id: upload.artifact_id,
      kind: "upload",
      summary: upload.filename,
      relativePath: upload.relative_path,
      sizeBytes: upload.size_bytes,
      contentType: upload.content_type
    })),
    ...artifacts.evidenceArtifacts.map((artifact) => ({
      id: artifact.artifact_id,
      kind: artifact.evidence_kind,
      summary: artifact.evidence_summary,
      relativePath: artifact.relative_path,
      sizeBytes: artifact.size_bytes,
      contentType: artifact.content_type
    })),
    ...(artifacts.supportArtifacts ?? []).map((artifact) => ({
      id: artifact.artifact_id,
      kind: artifact.source_evidence_kind ?? "support",
      summary: artifact.role ?? artifact.logical_path,
      relativePath: artifact.relative_path,
      logicalPath: artifact.logical_path,
      sizeBytes: artifact.size_bytes,
      contentType: artifact.content_type
    }))
  ];
  return (
    <div className="rounded-lg border border-border p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-sm font-semibold">Artifacts</p>
        <Badge variant="secondary">{items.length}</Badge>
      </div>
      {items.length ? (
        <div className="max-h-56 space-y-2 overflow-auto">
          {items.map((item) => (
            <div className="rounded-md border border-border p-2" key={`${item.kind}:${item.id}:${item.relativePath}`}>
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="truncate text-xs font-medium"><FileArchive className="mr-1 inline h-3.5 w-3.5 text-slate-400" />{item.kind}</p>
                  <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.summary}</p>
                </div>
                <Button className="h-8 w-8 shrink-0 px-0" variant="outline" title="Download artifact" aria-label="Download artifact" onClick={() => onDownload(item.id, item.relativePath)}>
                  <Download className="h-4 w-4" />
                </Button>
              </div>
              <p className="mt-2 break-all font-mono text-[11px] text-muted-foreground">{item.logicalPath ?? item.relativePath}</p>
              {item.logicalPath ? <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">{item.relativePath}</p> : null}
              <p className="mt-1 text-[11px] text-muted-foreground">{item.contentType} · {item.sizeBytes.toLocaleString()} bytes</p>
            </div>
          ))}
        </div>
      ) : <EmptyState>No artifacts for this run.</EmptyState>}
    </div>
  );
}

function toolMinFiles(tool: V2ToolDescriptor) {
  return tool.minFiles ?? 0;
}

function toolMaxFiles(tool: V2ToolDescriptor) {
  return tool.maxFiles ?? tool.maxInputFiles ?? 0;
}

function fileAccept(tool: V2ToolDescriptor) {
  const suffixes = (tool.acceptedSuffixes ?? []).filter((suffix) => suffix && suffix !== "*");
  return suffixes.join(",");
}

function isJsonObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isTerminalToolRun(status: V2ToolRun["status"]) {
  return status === "succeeded" || status === "failed";
}

function runStatusVariant(status: V2ToolRun["status"]) {
  if (status === "succeeded") return "success";
  if (status === "failed") return "destructive";
  if (status.startsWith("waiting")) return "warning";
  return "secondary";
}

function artifactCount(artifacts: V2ToolRunArtifacts | null) {
  if (!artifacts) return 0;
  return artifacts.uploads.length + artifacts.evidenceArtifacts.length + (artifacts.supportArtifacts?.length ?? 0);
}

function upsertRun(runs: V2ToolRun[], run: V2ToolRun) {
  return [run, ...runs.filter((item) => item.id !== run.id)];
}

function filenameFromPath(path: string) {
  const value = path.split("/").filter(Boolean).pop();
  return value || "artifact";
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
