import { Download, Globe2, Play, RefreshCw, Save, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  createV2FetchEndpointRun,
  deleteV2FetchEndpoint,
  downloadV2Artifact,
  getV2ToolRun,
  getV2ToolRunArtifacts,
  getV2ToolRunResult,
  importV2FetchCurl,
  listV2FetchEndpoints,
  listV2FetchRuns,
  previewV2FetchCurl,
  runV2FetchEndpoint,
  updateV2FetchEndpoint,
  type V2Artifact,
  type V2Evidence,
  type V2FetchEndpoint,
  type V2FetchPreview,
  type V2FetchRunResult,
  type V2ToolRun,
  type V2ToolRunArtifacts,
  type V2ToolRunResult
} from "./v2-api";

export function V2FetchBridge({ apiKey }: { apiKey: string }) {
  const [enabled, setEnabled] = useState(false);
  const [allowedHosts, setAllowedHosts] = useState<string[]>([]);
  const [endpoints, setEndpoints] = useState<V2FetchEndpoint[]>([]);
  const [selectedEndpointId, setSelectedEndpointId] = useState("");
  const [curlText, setCurlText] = useState("");
  const [name, setName] = useState("");
  const [preview, setPreview] = useState<V2FetchPreview | null>(null);
  const [runId, setRunId] = useState("");
  const [runParamsText, setRunParamsText] = useState(JSON.stringify({ variables: {}, headers: {}, body: null }, null, 2));
  const [result, setResult] = useState<V2FetchRunResult | null>(null);
  const [standaloneWorkspaceId, setStandaloneWorkspaceId] = useState("");
  const [fetchRuns, setFetchRuns] = useState<V2ToolRun[]>([]);
  const [selectedFetchRunId, setSelectedFetchRunId] = useState("");
  const [fetchToolRunResult, setFetchToolRunResult] = useState<V2ToolRunResult | null>(null);
  const [fetchRunArtifacts, setFetchRunArtifacts] = useState<V2ToolRunArtifacts | null>(null);
  const [status, setStatus] = useState("V2 Fetch waiting to load");
  const [loading, setLoading] = useState(false);

  const selectedEndpoint = useMemo(() => endpoints.find((endpoint) => endpoint.id === selectedEndpointId) ?? endpoints[0] ?? null, [endpoints, selectedEndpointId]);
  const selectedFetchRun = useMemo(() => fetchRuns.find((run) => run.id === selectedFetchRunId) ?? null, [fetchRuns, selectedFetchRunId]);

  const refreshEndpoints = useCallback(async () => {
    if (!apiKey.trim()) {
      setEndpoints([]);
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const response = await listV2FetchEndpoints(apiKey);
      setEnabled(response.enabled);
      setAllowedHosts(response.allowedHosts);
      setEndpoints(response.endpoints);
      if (!response.endpoints.some((endpoint) => endpoint.id === selectedEndpointId) && response.endpoints.length) {
        setSelectedEndpointId(response.endpoints[0].id);
      }
      setStatus(`V2 loaded ${response.endpoints.length} fetch endpoints`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, selectedEndpointId]);

  useEffect(() => {
    void refreshEndpoints();
  }, [refreshEndpoints]);

  const loadFetchRun = useCallback(async (targetRunId: string) => {
    if (!apiKey.trim()) return null;
    const run = await getV2ToolRun(apiKey, targetRunId);
    setFetchRuns((current) => upsertRun(current, run));
    const artifacts = await getV2ToolRunArtifacts(apiKey, targetRunId);
    setFetchRunArtifacts(artifacts);
    if (run.status === "succeeded") {
      const response = await getV2ToolRunResult(apiKey, targetRunId);
      setFetchToolRunResult(response);
    } else {
      setFetchToolRunResult(null);
    }
    return run;
  }, [apiKey]);

  const refreshFetchRuns = useCallback(async () => {
    if (!apiKey.trim()) {
      setFetchRuns([]);
      setFetchRunArtifacts(null);
      setFetchToolRunResult(null);
      return;
    }
    const response = await listV2FetchRuns(apiKey, {
      endpointId: selectedEndpoint?.id,
      workspaceId: standaloneWorkspaceId.trim() || undefined,
      limit: 20
    });
    setFetchRuns(response.runs);
    if (selectedFetchRunId) {
      const current = response.runs.find((run) => run.id === selectedFetchRunId);
      if (current) {
        await loadFetchRun(current.id);
      } else {
        setSelectedFetchRunId("");
        setFetchRunArtifacts(null);
        setFetchToolRunResult(null);
      }
    }
  }, [apiKey, loadFetchRun, selectedEndpoint?.id, selectedFetchRunId, standaloneWorkspaceId]);

  useEffect(() => {
    void refreshFetchRuns().catch(() => undefined);
  }, [refreshFetchRuns]);

  useEffect(() => {
    if (!selectedFetchRunId || !selectedFetchRun || isTerminalToolRun(selectedFetchRun.status)) return;
    const timer = window.setInterval(() => {
      void loadFetchRun(selectedFetchRunId).catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [loadFetchRun, selectedFetchRun, selectedFetchRunId]);

  async function previewCurl() {
    if (!curlText.trim()) {
      setStatus("Paste a curl command first");
      return;
    }
    setLoading(true);
    try {
      const response = await previewV2FetchCurl(apiKey, curlText);
      setPreview(response);
      if (!name.trim()) setName(response.endpoint.name === "Preview" ? "" : response.endpoint.name);
      setStatus(`V2 preview detected ${response.detectedSensitiveFields.length} sensitive fields`);
    } catch (reason) {
      setPreview(null);
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function saveCurlEndpoint() {
    if (!curlText.trim()) {
      setStatus("Paste a curl command first");
      return;
    }
    setLoading(true);
    try {
      const endpoint = await importV2FetchCurl(apiKey, { curl: curlText, name: name.trim() || null, enabled: true });
      setSelectedEndpointId(endpoint.id);
      setStatus(`V2 saved ${endpoint.id}`);
      await refreshEndpoints();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function toggleEndpoint(endpoint: V2FetchEndpoint) {
    setLoading(true);
    try {
      await updateV2FetchEndpoint(apiKey, endpoint.id, { enabled: !endpoint.enabled });
      await refreshEndpoints();
      setStatus(`V2 ${endpoint.enabled ? "disabled" : "enabled"} ${endpoint.id}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function deleteEndpoint(endpoint: V2FetchEndpoint) {
    if (!window.confirm(`Delete V2 fetch endpoint ${endpoint.name}?`)) return;
    setLoading(true);
    try {
      await deleteV2FetchEndpoint(apiKey, endpoint.id);
      setSelectedEndpointId("");
      setResult(null);
      setSelectedFetchRunId("");
      setFetchToolRunResult(null);
      setFetchRunArtifacts(null);
      await refreshEndpoints();
      setStatus(`V2 deleted ${endpoint.id}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function runEndpoint() {
    if (!selectedEndpoint) {
      setStatus("Select a V2 fetch endpoint");
      return;
    }
    if (!runId.trim()) {
      setStatus("V2 fetch execution requires a run id");
      return;
    }
    let runParams: unknown;
    try {
      runParams = JSON.parse(runParamsText);
    } catch (reason) {
      setStatus(`Invalid run params JSON: ${errorMessage(reason)}`);
      return;
    }
    if (!isRecord(runParams)) {
      setStatus("Run params must be a JSON object");
      return;
    }
    setLoading(true);
    setResult(null);
    try {
      const response = await runV2FetchEndpoint(apiKey, runId.trim(), selectedEndpoint.id, runParams);
      setResult(response);
      setStatus(String(response.result.summary ?? `V2 fetch recorded ${response.artifact.id ?? response.artifact.relative_path}`));
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function createStandaloneFetchRun() {
    if (!selectedEndpoint) {
      setStatus("Select a V2 fetch endpoint");
      return;
    }
    let runParams: unknown;
    try {
      runParams = JSON.parse(runParamsText);
    } catch (reason) {
      setStatus(`Invalid run params JSON: ${errorMessage(reason)}`);
      return;
    }
    if (!isRecord(runParams)) {
      setStatus("Run params must be a JSON object");
      return;
    }
    const payload: Record<string, unknown> = { ...runParams };
    const workspaceId = standaloneWorkspaceId.trim();
    if (workspaceId) payload.workspaceId = workspaceId;
    setLoading(true);
    setFetchToolRunResult(null);
    setFetchRunArtifacts(null);
    try {
      const run = await createV2FetchEndpointRun(apiKey, selectedEndpoint.id, payload);
      setSelectedFetchRunId(run.id);
      setFetchRuns((current) => upsertRun(current, run));
      setStatus(`Created V2 fetch tool_run ${run.id}`);
      await loadFetchRun(run.id);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function selectFetchRun(runId: string) {
    setSelectedFetchRunId(runId);
    setFetchToolRunResult(null);
    setFetchRunArtifacts(null);
    setLoading(true);
    try {
      await loadFetchRun(runId);
      setStatus(`Loaded V2 fetch tool_run ${runId}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function downloadFetchArtifact(artifactId: string, relativePath: string) {
    try {
      await downloadV2Artifact(apiKey, artifactId, filenameFromPath(relativePath));
      setStatus(`Downloaded artifact ${relativePath}`);
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
              <Globe2 className="h-5 w-5 text-primary" />
              <CardTitle>V2 Fetch Workbench</CardTitle>
            </div>
            <CardDescription>V2 cURL import, endpoint management, run-scoped Fetch, and standalone Fetch tool_run execution</CardDescription>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={enabled ? "success" : "destructive"}>{enabled ? "enabled" : "disabled"}</Badge>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refreshEndpoints()}><RefreshCw className="mr-2 h-4 w-4" />刷新</Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)_420px]">
          <div className="rounded-lg border border-border p-3">
            <h3 className="mb-3 text-sm font-semibold">V2 endpoints</h3>
            <div className="mb-3 rounded-md bg-slate-50 p-2 text-xs text-muted-foreground">Allowed hosts: {allowedHosts.length ? allowedHosts.join(", ") : "*"}</div>
            <div className="max-h-[420px] space-y-2 overflow-auto">
              {endpoints.length ? endpoints.map((endpoint) => (
                <button className={`w-full rounded-lg border p-3 text-left ${selectedEndpoint?.id === endpoint.id ? "border-primary bg-slate-50" : "border-border"}`} key={endpoint.id} onClick={() => setSelectedEndpointId(endpoint.id)}>
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium">{endpoint.name}</p>
                      <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{endpoint.method} {endpoint.url}</p>
                    </div>
                    <Badge variant={endpoint.enabled ? "success" : "destructive"}>{endpoint.enabled ? "on" : "off"}</Badge>
                  </div>
                  {endpoint.hasCredentials ? <p className="mt-2 text-xs text-muted-foreground">encrypted credentials stored</p> : null}
                </button>
              )) : <EmptyState>No V2 fetch endpoints.</EmptyState>}
            </div>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            <div>
              <h3 className="text-sm font-semibold">Import cURL</h3>
              <p className="mt-1 text-xs text-muted-foreground">Sensitive query/header/body fields are redacted by V2 before storage or display.</p>
            </div>
            <textarea className="min-h-40 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" spellCheck={false} value={curlText} onChange={(event) => setCurlText(event.target.value)} />
            <Input value={name} onChange={(event) => setName(event.target.value)} placeholder="Endpoint name override" />
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span className="text-xs text-muted-foreground">{status}</span>
              <div className="flex flex-wrap gap-2">
                <Button disabled={loading || !curlText.trim()} variant="outline" onClick={() => void previewCurl()}><Globe2 className="mr-2 h-4 w-4" />Preview</Button>
                <Button disabled={loading || !curlText.trim()} onClick={() => void saveCurlEndpoint()}><Save className="mr-2 h-4 w-4" />Save</Button>
              </div>
            </div>
            {preview ? <EndpointDetails endpoint={preview.endpoint} sensitive={preview.detectedSensitiveFields} /> : null}
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            <div>
              <h3 className="text-sm font-semibold">Run in V2 run</h3>
              <p className="mt-1 text-xs text-muted-foreground">{selectedEndpoint ? `${selectedEndpoint.method} ${selectedEndpoint.url}` : "Select an endpoint"}</p>
            </div>
            <Input value={runId} onChange={(event) => setRunId(event.target.value)} placeholder="V2 run id, e.g. run_..." />
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground">Run overrides JSON</p>
              <textarea
                className="min-h-28 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20"
                spellCheck={false}
                value={runParamsText}
                onChange={(event) => setRunParamsText(event.target.value)}
              />
            </div>
            {selectedEndpoint ? (
              <div className="flex flex-wrap gap-2">
                <Button disabled={loading} variant="outline" onClick={() => void toggleEndpoint(selectedEndpoint)}>{selectedEndpoint.enabled ? "Disable" : "Enable"}</Button>
                <Button disabled={loading} variant="outline" onClick={() => void deleteEndpoint(selectedEndpoint)}><Trash2 className="mr-2 h-4 w-4" />Delete</Button>
                <Button disabled={loading || !selectedEndpoint.enabled || !runId.trim()} onClick={() => void runEndpoint()}><Play className="mr-2 h-4 w-4" />Run fetch</Button>
              </div>
            ) : null}
            {selectedEndpoint ? <EndpointDetails endpoint={selectedEndpoint} sensitive={[]} /> : <EmptyState>Select a V2 endpoint.</EmptyState>}
            {result ? <FetchResult result={result} onDownload={(artifactId, relativePath) => void downloadFetchArtifact(artifactId, relativePath)} /> : null}
            <div className="space-y-3 rounded-lg border border-border p-3">
              <div>
                <h3 className="text-sm font-semibold">Standalone fetch tool_run</h3>
                <p className="mt-1 text-xs text-muted-foreground">Queue `/api/v2/fetch/endpoints/:endpoint_id/runs`; leave Workspace blank to create an isolated Workspace.</p>
              </div>
              <Input value={standaloneWorkspaceId} onChange={(event) => setStandaloneWorkspaceId(event.target.value)} placeholder="Optional Workspace id" />
              <div className="flex flex-wrap items-center justify-between gap-2">
                <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refreshFetchRuns()}>
                  <RefreshCw className="mr-2 h-4 w-4" />Fetch runs
                </Button>
                <Button disabled={loading || !selectedEndpoint?.enabled} onClick={() => void createStandaloneFetchRun()}>
                  <Play className="mr-2 h-4 w-4" />Create fetch_run
                </Button>
              </div>
              {fetchRuns.length ? (
                <div className="max-h-44 space-y-2 overflow-auto">
                  {fetchRuns.map((run) => (
                    <button className={`w-full rounded-md border p-2 text-left ${selectedFetchRun?.id === run.id ? "border-primary bg-slate-50" : "border-border"}`} key={run.id} onClick={() => void selectFetchRun(run.id)}>
                      <div className="flex items-center justify-between gap-2">
                        <span className="break-all font-mono text-xs">{run.id}</span>
                        <Badge variant={runStatusVariant(run.status)}>{run.status}</Badge>
                      </div>
                      <p className="mt-1 text-xs text-muted-foreground">{run.phase} · {new Date(run.created_at).toLocaleString()}</p>
                    </button>
                  ))}
                </div>
              ) : <EmptyState>No V2 fetch tool runs.</EmptyState>}
              {selectedFetchRun ? (
                <div className="grid gap-2 rounded-lg border border-border p-3 text-xs sm:grid-cols-2">
                  <div>
                    <p className="text-muted-foreground">Selected run</p>
                    <p className="mt-1 break-all font-mono">{selectedFetchRun.id}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Status</p>
                    <div className="mt-1"><Badge variant={runStatusVariant(selectedFetchRun.status)}>{selectedFetchRun.status}</Badge></div>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Phase</p>
                    <p className="mt-1 break-all">{selectedFetchRun.phase}</p>
                  </div>
                  <div>
                    <p className="text-muted-foreground">Artifacts</p>
                    <p className="mt-1">{artifactCount(fetchRunArtifacts)}</p>
                  </div>
                </div>
              ) : null}
              {fetchToolRunResult ? <FetchToolRunResult result={fetchToolRunResult} onDownload={(artifactId, relativePath) => void downloadFetchArtifact(artifactId, relativePath)} /> : null}
              {fetchRunArtifacts ? <FetchRunArtifactList artifacts={fetchRunArtifacts} onDownload={(artifactId, relativePath) => void downloadFetchArtifact(artifactId, relativePath)} /> : null}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function EndpointDetails({ endpoint, sensitive }: { endpoint: V2FetchEndpoint; sensitive: Array<{ location: string; name: string }> }) {
  return (
    <div className="space-y-3">
      <div className="grid gap-2 md:grid-cols-3">
        <Metric label="ID" value={endpoint.id || "-"} />
        <Metric label="Method" value={endpoint.method} />
        <Metric label="Updated" value={endpoint.updatedAt ? new Date(endpoint.updatedAt).toLocaleString() : "-"} />
      </div>
      <PathLine label="URL" value={endpoint.url} />
      <JsonBlock title="Headers" value={endpoint.headers} />
      {endpoint.bodyPreview ? <pre className="max-h-36 overflow-auto whitespace-pre-wrap rounded-lg border border-border bg-slate-50 p-3 text-xs">{endpoint.bodyPreview}</pre> : null}
      {sensitive.length ? <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800">Sensitive: {sensitive.map((item) => `${item.location}:${item.name}`).join(", ")}</div> : null}
    </div>
  );
}

function FetchResult({ result, onDownload }: { result: V2FetchRunResult; onDownload: (artifactId: string, relativePath: string) => void }) {
  return <FetchResultPanel artifact={result.artifact} evidence={result.evidence} result={result.result} onDownload={onDownload} />;
}

function FetchToolRunResult({ result, onDownload }: { result: V2ToolRunResult; onDownload: (artifactId: string, relativePath: string) => void }) {
  return <FetchResultPanel artifact={result.artifact} result={result.result} onDownload={onDownload} />;
}

function FetchResultPanel({ artifact, evidence, result, onDownload }: { artifact: V2Artifact; evidence?: V2Evidence; result: Record<string, unknown>; onDownload: (artifactId: string, relativePath: string) => void }) {
  const response = isRecord(result.response) ? result.response : null;
  const resultArtifactId = artifact.id ?? artifact.artifact_id ?? null;
  const bodyArtifact = fetchBodyArtifact(result);
  return (
    <div className="space-y-3 rounded-lg border border-border p-3">
      <div className="grid gap-2 md:grid-cols-3">
        <Metric label="Status" value={String(result.status ?? "-")} />
        <Metric label="HTTP" value={String(response?.statusCode ?? "-")} />
        <Metric label="Duration" value={`${String(result.durationMs ?? "-")}ms`} />
      </div>
      <ArtifactLine
        artifactId={resultArtifactId}
        label="Result artifact"
        logicalPath={String(result.evidenceRef ?? artifact.relative_path)}
        relativePath={artifact.relative_path}
        onDownload={onDownload}
      />
      {bodyArtifact ? (
        <ArtifactLine
          artifactId={bodyArtifact.artifactId}
          label="Response body"
          logicalPath={bodyArtifact.logicalPath}
          relativePath={bodyArtifact.relativePath}
          onDownload={onDownload}
        />
      ) : null}
      {evidence ? <PathLine label="Evidence" value={`${evidence.id} · ${evidence.summary}`} /> : null}
      {response ? <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-lg border border-border bg-slate-50 p-3 text-xs">{String(response.bodyPreview ?? "")}</pre> : null}
      <JsonBlock title="Result JSON" value={result} />
    </div>
  );
}

function FetchRunArtifactList({ artifacts, onDownload }: { artifacts: V2ToolRunArtifacts; onDownload: (artifactId: string, relativePath: string) => void }) {
  type FetchRunArtifactItem = {
    id: string;
    kind: string;
    summary: string;
    relativePath: string;
    logicalPath?: string;
    sizeBytes: number;
    contentType: string;
  };
  const items: FetchRunArtifactItem[] = [
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
        <p className="text-sm font-semibold">Run artifacts</p>
        <Badge variant="secondary">{items.length}</Badge>
      </div>
      {items.length ? (
        <div className="max-h-56 space-y-2 overflow-auto">
          {items.map((item) => (
            <div className="rounded-md border border-border p-2" key={`${item.kind}:${item.id}:${item.relativePath}`}>
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="truncate text-xs font-medium">{item.kind}</p>
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

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value}</p></div>;
}

function PathLine({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all font-mono text-xs">{value}</p></div>;
}

function ArtifactLine({ artifactId, label, logicalPath, relativePath, onDownload }: { artifactId: string | null; label: string; logicalPath: string; relativePath: string; onDownload: (artifactId: string, relativePath: string) => void }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-xs text-muted-foreground">{label}</p>
          <p className="mt-1 break-all font-mono text-xs">{logicalPath}</p>
          {logicalPath !== relativePath ? <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">{relativePath}</p> : null}
        </div>
        <Button className="h-8 w-8 shrink-0 px-0" disabled={!artifactId} variant="outline" title="Download artifact" aria-label="Download artifact" onClick={() => artifactId ? onDownload(artifactId, relativePath) : undefined}>
          <Download className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return <div><p className="mb-2 text-xs text-muted-foreground">{title}</p><pre className="max-h-52 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{JSON.stringify(value, null, 2)}</pre></div>;
}

function fetchBodyArtifact(result: Record<string, unknown>) {
  const response = isRecord(result.response) ? result.response : {};
  const artifactId = stringValue(result.bodyArtifactId) ?? stringValue(response.bodyArtifactId);
  const relativePath = stringValue(result.bodyArtifactRelativePath) ?? stringValue(response.bodyArtifactRelativePath);
  const logicalPath = stringValue(result.bodyArtifactPath) ?? stringValue(response.bodyArtifactPath) ?? relativePath;
  if (!artifactId || !relativePath) return null;
  return { artifactId, relativePath, logicalPath: logicalPath ?? relativePath };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value : null;
}

function filenameFromPath(path: string) {
  const value = path.split("/").filter(Boolean).pop();
  return value || "artifact";
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

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
