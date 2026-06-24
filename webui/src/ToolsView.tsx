import { FileArchive, Play, RefreshCw, Search, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { toolsCopy, type ToolsCopy, type UiLanguage } from "./i18n";
import { uploadFile } from "./upload";

type ToolDescriptor = {
  toolId: string;
  displayName: string;
  description: string;
  enabled: boolean;
  source: "built_in" | "configured";
  readOnly: boolean;
  editable: boolean;
  exportable: boolean;
  runnable: boolean;
  tags: string[];
  backend: string;
  acceptedSuffixes: string[];
  minFiles: number;
  maxFiles: number;
  paramsSchema?: Record<string, unknown>;
  paramsTemplate?: Record<string, unknown>;
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
  result: unknown;
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

// Derived functional category for grouping the tool catalog. Tags are noisy
// ("configured"/"manual-run"/"tool-runner"/"external" always clump), so we
// derive a clean category from tags + toolId + backend instead of grouping by
// raw tags.
const CATEGORY_ORDER = ["analyzers", "metadata", "fetch", "sync", "gemini", "other"] as const;
type Category = (typeof CATEGORY_ORDER)[number];

function categoryOf(tool: ToolDescriptor): Category {
  const tags = tool.tags.map((tag) => tag.toLowerCase());
  const id = tool.toolId.toLowerCase();
  if (tags.includes("metadata")) return "metadata";
  if (tags.includes("fetch") || tool.backend === "fetch") return "fetch";
  if (tags.includes("gemini-db") || tool.backend === "gemini_db_influx") return "gemini";
  if (id.includes("sync") || id.includes("huawei")) return "sync";
  if (tags.includes("log") || id.includes("analyzer") || id.includes("pprof") || id.includes("preprocess")) return "analyzers";
  return "other";
}

function categoryLabel(category: Category, copy: ToolsCopy): string {
  switch (category) {
    case "analyzers":
      return copy.groupAnalyzers;
    case "metadata":
      return copy.groupMetadata;
    case "fetch":
      return copy.groupFetch;
    case "sync":
      return copy.groupSync;
    case "gemini":
      return copy.groupGemini;
    default:
      return copy.groupOther;
  }
}

export function ToolsView({ apiKey, language }: { apiKey: string; language: UiLanguage }) {
  return <ToolPluginsView apiKey={apiKey} language={language} />;
}

function ToolPluginsView({ apiKey, language }: { apiKey: string; language: UiLanguage }) {
  const copy = toolsCopy[language];
  const [tools, setTools] = useState<ToolDescriptor[]>([]);
  const [selectedToolId, setSelectedToolId] = useState("pprof_analyzer");
  const [runs, setRuns] = useState<ToolRunSummary[]>([]);
  const [selectedRun, setSelectedRun] = useState<ToolRunRecord | null>(null);
  const [result, setResult] = useState<ToolRunResultResponse | null>(null);
  const [files, setFiles] = useState<File[]>([]);
  const [paramsText, setParamsText] = useState("{}");
  const [status, setStatus] = useState<string>(copy.ready);
  const [uploadProgress, setUploadProgress] = useState(0);
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState("");
  const [sourceFilter, setSourceFilter] = useState<"all" | "built_in" | "configured">("all");
  const [runnableOnly, setRunnableOnly] = useState(false);

  const selectedTool = tools.find((tool) => tool.toolId === selectedToolId) ?? tools[0] ?? null;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return tools.filter((tool) => {
      if (sourceFilter !== "all" && tool.source !== sourceFilter) return false;
      if (runnableOnly && !tool.runnable) return false;
      if (!q) return true;
      return (
        tool.displayName.toLowerCase().includes(q) ||
        tool.toolId.toLowerCase().includes(q) ||
        tool.description.toLowerCase().includes(q) ||
        tool.tags.some((tag) => tag.toLowerCase().includes(q))
      );
    });
  }, [tools, query, sourceFilter, runnableOnly]);

  // Group by category only when not searching; while searching, render a flat
  // ranked list (command-palette style).
  const groups = useMemo(() => {
    if (query.trim()) return null;
    const buckets = new Map<Category, ToolDescriptor[]>(CATEGORY_ORDER.map((category) => [category, [] as ToolDescriptor[]]));
    for (const tool of filtered) buckets.get(categoryOf(tool))?.push(tool);
    return CATEGORY_ORDER.map((category) => ({ category, tools: buckets.get(category) ?? [] })).filter((group) => group.tools.length > 0);
  }, [filtered, query]);

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
    if (!selectedTool) return;
    setFiles([]);
    setUploadProgress(0);
    setParamsText(formatJson(selectedTool.paramsTemplate ?? {}));
  }, [selectedTool]);

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
      setStatus(copy.apiKeyRequired);
      return;
    }
    if (!selectedTool) {
      setStatus(copy.noToolSelected);
      return;
    }
    if (!selectedTool.enabled) {
      setStatus(copy.toolDisabled(selectedTool.displayName));
      return;
    }
    if (!selectedTool.runnable) {
      setStatus(copy.toolNotRunnable(selectedTool.displayName));
      return;
    }
    if (files.length < selectedTool.minFiles || files.length > selectedTool.maxFiles) {
      setStatus(copy.chooseFiles(selectedTool.minFiles, selectedTool.maxFiles));
      return;
    }
    let params: unknown;
    try {
      params = JSON.parse(paramsText);
    } catch (reason) {
      setStatus(copy.invalidParams(errorMessage(reason)));
      return;
    }
    if (!isJsonObject(params)) {
      setStatus(copy.paramsNotObject);
      return;
    }
    setLoading(true);
    setUploadProgress(0);
    setResult(null);
    try {
      const uploadIds: string[] = [];
      for (const [index, nextFile] of files.entries()) {
        setStatus(copy.uploading(nextFile.name));
        const upload = await uploadFile(nextFile, apiKey, (value) => {
          const completed = index + value;
          setUploadProgress(Math.round((completed / Math.max(files.length, 1)) * 100));
        });
        uploadIds.push(upload.uploadId);
      }
      if (!files.length) setUploadProgress(100);
      setStatus(copy.startingRun);
      const run = await fetchJson<ToolRunSummary>(`/api/tools/${encodeURIComponent(selectedTool.toolId)}/runs`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          uploadIds,
          params,
          idempotencyKey: `webui-${selectedTool.toolId}-${Date.now()}`
        })
      });
      setStatus(copy.created(run.taskId));
      await refreshRuns();
      await selectRun(run.taskId);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="grid gap-5 xl:grid-cols-[380px_1fr]">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <div>
              <CardTitle className="flex items-center gap-2">{copy.catalogTitle}<Badge variant="secondary">{copy.toolCount(filtered.length, tools.length)}</Badge></CardTitle>
              <CardDescription>{copy.catalogDesc}</CardDescription>
            </div>
            <Button className="h-8 px-3" variant="outline" onClick={() => void refreshTools()}><RefreshCw className="h-4 w-4" /></Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="relative">
            <Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
            <Input className="pl-9" placeholder={copy.searchPlaceholder} value={query} onChange={(event) => setQuery(event.target.value)} />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {(["all", "built_in", "configured"] as const).map((value) => (
              <button
                key={value}
                className={`rounded-md px-2.5 py-1 text-xs font-medium ${sourceFilter === value ? "bg-primary text-white" : "border border-border bg-white text-slate-600 hover:bg-slate-50"}`}
                onClick={() => setSourceFilter(value)}
              >
                {value === "all" ? copy.filterAll : value === "built_in" ? copy.filterBuiltIn : copy.filterConfigured}
              </button>
            ))}
            <button
              className={`rounded-md px-2.5 py-1 text-xs font-medium ${runnableOnly ? "bg-primary text-white" : "border border-border bg-white text-slate-600 hover:bg-slate-50"}`}
              onClick={() => setRunnableOnly((value) => !value)}
            >
              {copy.runnableOnly}
            </button>
          </div>
          <div className="space-y-3">
            {tools.length === 0 ? (
              <EmptyState>{copy.noTools}</EmptyState>
            ) : filtered.length === 0 ? (
              <EmptyState>{copy.noMatches}</EmptyState>
            ) : groups ? (
              groups.map((group) => (
                <div key={group.category} className="space-y-1">
                  <p className="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{categoryLabel(group.category, copy)} · {group.tools.length}</p>
                  {group.tools.map((tool) => (
                    <ToolRow key={tool.toolId} tool={tool} selected={selectedToolId === tool.toolId} onSelect={() => setSelectedToolId(tool.toolId)} copy={copy} />
                  ))}
                </div>
              ))
            ) : (
              <div className="space-y-1">
                <p className="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{copy.resultsLabel(filtered.length)}</p>
                {filtered
                  .slice()
                  .sort((a, b) => a.displayName.localeCompare(b.displayName))
                  .map((tool) => (
                    <ToolRow key={tool.toolId} tool={tool} selected={selectedToolId === tool.toolId} onSelect={() => setSelectedToolId(tool.toolId)} copy={copy} />
                  ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <div className="space-y-5">
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>{selectedTool?.displayName ?? copy.toolsFallback}</CardTitle>
                <CardDescription>{selectedTool ? toolSubtitle(selectedTool, copy) : copy.selectToolHint}</CardDescription>
              </div>
              {selectedTool ? (
                <div className="flex flex-wrap justify-end gap-2">
                  <Badge variant={selectedTool.enabled ? "success" : "destructive"}>{selectedTool.enabled ? copy.readyBadge : copy.disabledBadge}</Badge>
                  <SourceBadge source={selectedTool.source} copy={copy} />
                  {selectedTool.readOnly ? <Badge variant="secondary">{copy.readOnlyBadge}</Badge> : null}
                </div>
              ) : null}
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {selectedTool?.runnable ? (
              <>
                {selectedTool.maxFiles > 0 ? (
                  <label className="flex min-h-32 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 px-4 text-center text-sm text-muted-foreground">
                    <UploadCloud className="mb-2 h-7 w-7" />
                    {files.length ? files.map((nextFile) => nextFile.name).join(", ") : filePrompt(selectedTool, copy)}
                    <input
                      accept={fileAccept(selectedTool)}
                      className="hidden"
                      multiple={selectedTool.maxFiles > 1}
                      type="file"
                      onChange={(event) => setFiles(Array.from(event.target.files ?? []).slice(0, selectedTool.maxFiles))}
                    />
                  </label>
                ) : null}
                <div className="space-y-2">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">{copy.paramsJson}</p>
                    <Button className="h-8 px-3" variant="outline" onClick={() => setParamsText(formatJson(selectedTool.paramsTemplate ?? {}))}><RefreshCw className="mr-2 h-4 w-4" />{copy.resetTemplate}</Button>
                  </div>
                  <textarea
                    className="min-h-48 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20"
                    spellCheck={false}
                    value={paramsText}
                    onChange={(event) => setParamsText(event.target.value)}
                  />
                </div>
                <div>
                  <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>{copy.uploadLabel}</span><span>{uploadProgress}%</span></div>
                  <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
                </div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <span className="text-sm text-muted-foreground">{status}</span>
                  <Button disabled={loading || !selectedTool?.enabled || !selectedTool?.runnable} onClick={() => void runTool()}><Play className="mr-2 h-4 w-4" />{copy.runTool}</Button>
                </div>
              </>
            ) : selectedTool ? (
              <ToolDescriptorDetails tool={selectedTool} status={status} copy={copy} />
            ) : (
              <EmptyState>{copy.selectToolInspect}</EmptyState>
            )}
          </CardContent>
        </Card>

        <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between gap-3">
                <CardTitle>{copy.toolRuns}</CardTitle>
                <Button className="h-8 px-3" variant="outline" onClick={() => void refreshRuns()}><RefreshCw className="h-4 w-4" /></Button>
              </div>
            </CardHeader>
            <CardContent className="space-y-2">
              {runs.length ? runs.map((run) => (
                <button key={run.taskId} className={`w-full rounded-lg border p-3 text-left ${selectedRun?.taskId === run.taskId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => void selectRun(run.taskId)}>
                  <div className="flex items-center justify-between gap-2"><span className="font-mono text-xs">{run.taskId}</span><RunStatusBadge status={run.status} /></div>
                  <p className="mt-1 text-xs text-muted-foreground">{run.phase ?? copy.noActivePhase} · {new Date(run.createdAt).toLocaleString()}</p>
                </button>
              )) : <EmptyState>{copy.noToolRuns}</EmptyState>}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>{copy.runStatus}</CardTitle>
              <CardDescription>{selectedRun ? `${selectedRun.taskId} · ${copy.attempt(selectedRun.attempts ?? 0)}` : copy.selectRun}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {selectedRun ? (
                <>
                  <div className="flex flex-wrap items-center gap-2"><RunStatusBadge status={selectedRun.status} /><span className="text-sm text-muted-foreground">{selectedRun.phase ?? copy.noActivePhase}</span></div>
                  {selectedRun.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedRun.error?.phase ? `${selectedRun.error.phase}: ` : ""}{selectedRun.error?.message ?? copy.toolRunFailed}</div> : null}
                  {!isTerminal(selectedRun.status) ? <p className="text-sm text-muted-foreground">{copy.runningInBackground}</p> : null}
                  {selectedRun.status === "SUCCEEDED" && !result ? <Button onClick={() => void selectRun(selectedRun.taskId)}>{copy.loadResult}</Button> : null}
                  {result ? <ToolResultView result={result.result} resultPath={result.resultPath} toolId={result.toolId} copy={copy} /> : null}
                </>
              ) : <EmptyState>{copy.selectOrCreateRun}</EmptyState>}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

function SourceBadge({ source, copy }: { source: ToolDescriptor["source"]; copy: ToolsCopy }) {
  return <Badge variant={source === "built_in" ? "secondary" : "outline"}>{source === "built_in" ? copy.builtIn : copy.configured}</Badge>;
}

function ToolRow({ tool, selected, onSelect, copy }: { tool: ToolDescriptor; selected: boolean; onSelect: () => void; copy: ToolsCopy }) {
  const dotClass = !tool.enabled ? "bg-slate-300" : tool.runnable ? "bg-emerald-500" : "bg-amber-500";
  return (
    <button className={`flex w-full items-center gap-2 rounded-md border px-3 py-2 text-left ${selected ? "border-primary bg-slate-50" : "border-transparent hover:bg-slate-50"}`} onClick={onSelect}>
      <span className={`inline-block h-2 w-2 shrink-0 rounded-full ${dotClass}`} />
      <span className="truncate text-sm font-medium">{tool.displayName}</span>
      <span className="ml-auto shrink-0 text-xs text-muted-foreground">{tool.source === "built_in" ? copy.builtIn : copy.configured}</span>
    </button>
  );
}

function ToolTagList({ tags }: { tags: string[] }) {
  if (!tags.length) return null;
  return (
    <div className="mt-3 flex flex-wrap gap-1.5">
      {tags.map((tag) => <Badge key={tag} variant="outline">{tag}</Badge>)}
    </div>
  );
}

function ToolDescriptorDetails({ tool, status, copy }: { tool: ToolDescriptor; status: string; copy: ToolsCopy }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-4">
        <Metric label={copy.source} value={tool.source === "built_in" ? copy.builtIn : copy.configured} />
        <Metric label={copy.manualRun} value={tool.runnable ? copy.manualEnabled : copy.manualUnavailable} />
        <Metric label={copy.editable} value={tool.editable ? copy.yes : copy.no} />
        <Metric label={copy.exportable} value={tool.exportable ? copy.yes : copy.no} />
      </div>
      <div className="rounded-lg border border-border p-3">
        <p className="text-xs text-muted-foreground">{copy.tags}</p>
        <ToolTagList tags={tool.tags} />
      </div>
      <div className="rounded-lg border border-border p-3">
        <p className="text-xs text-muted-foreground">{copy.inputSchema}</p>
        <pre className="mt-2 max-h-72 overflow-auto rounded bg-slate-50 p-3 text-xs">{JSON.stringify(tool.paramsSchema ?? {}, null, 2)}</pre>
      </div>
      <p className="text-sm text-muted-foreground">{status}</p>
    </div>
  );
}

function toolSubtitle(tool: ToolDescriptor, copy: ToolsCopy) {
  if (tool.acceptedSuffixes.length) {
    return tool.acceptedSuffixes.join(", ");
  }
  return copy.noFileInput(tool.backend);
}

function ToolResultView({ result, resultPath, toolId, copy }: { result: unknown; resultPath: string; toolId: string; copy: ToolsCopy }) {
  if (toolId === "pprof_analyzer" && isPprofResult(result)) {
    return <PprofResultView result={result} resultPath={resultPath} copy={copy} />;
  }
  return (
    <div className="space-y-3">
      <ArtifactPath label={copy.result} value={resultPath} />
      <pre className="max-h-[560px] overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{formatJson(result)}</pre>
    </div>
  );
}

function PprofResultView({ result, resultPath, copy }: { result: PprofResult; resultPath: string; copy: ToolsCopy }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-4">
        <Metric label={copy.status} value={result.status} />
        <Metric label={copy.profile} value={result.profileType || copy.unknown} />
        <Metric label={copy.total} value={result.total ?? "-"} />
        <Metric label={copy.duration} value={`${result.durationMs}ms`} />
      </div>
      {result.error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{result.error}</div> : null}
      {result.warnings.length ? <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">{result.warnings.join(" · ")}</div> : null}
      <div className="max-h-[560px] overflow-auto rounded-lg border border-border">
        <table className="w-full text-left text-sm">
          <thead className="sticky top-0 z-10 bg-slate-50 text-xs text-muted-foreground shadow-[0_1px_0_hsl(var(--border))]">
            <tr>
              <th className="px-3 py-2">{copy.rank}</th>
              <th className="px-3 py-2">{copy.flat}</th>
              <th className="px-3 py-2">{copy.flatPercent}</th>
              <th className="px-3 py-2">{copy.cum}</th>
              <th className="px-3 py-2">{copy.cumPercent}</th>
              <th className="px-3 py-2">{copy.function}</th>
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
              <tr><td className="px-3 py-8 text-center text-sm text-muted-foreground" colSpan={6}>{copy.noTopEntries}</td></tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="grid gap-2 md:grid-cols-2">
        <ArtifactPath label={copy.result} value={resultPath} />
        <ArtifactPath label={copy.topText} value={result.artifacts.topTextPath} />
        <ArtifactPath label={copy.treeText} value={result.artifacts.treeTextPath} />
        <ArtifactPath label={copy.rawText} value={result.artifacts.rawTextPath} />
        <ArtifactPath label={copy.stderr} value={result.artifacts.stderrPath} />
        {result.artifacts.svgPath ? <ArtifactPath label={copy.svg} value={result.artifacts.svgPath} /> : null}
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

function isPprofResult(value: unknown): value is PprofResult {
  if (!isJsonObject(value)) return false;
  return value.toolId === "pprof_analyzer" && Array.isArray(value.top) && isJsonObject(value.artifacts);
}

function isJsonObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function formatJson(value: unknown) {
  return JSON.stringify(value, null, 2);
}

function filePrompt(tool: ToolDescriptor, copy: ToolsCopy) {
  if (tool.minFiles === tool.maxFiles) {
    return copy.chooseFile(tool.minFiles);
  }
  return copy.chooseFilesRange(tool.minFiles, tool.maxFiles);
}

function fileAccept(tool: ToolDescriptor) {
  return tool.acceptedSuffixes
    .map((suffix) => suffix.startsWith("*") ? suffix.slice(1) : suffix)
    .filter((suffix) => suffix.startsWith("."))
    .join(",");
}
