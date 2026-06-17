import { Globe2, Play, RefreshCw, Save, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  deleteV2FetchEndpoint,
  importV2FetchCurl,
  listV2FetchEndpoints,
  previewV2FetchCurl,
  runV2FetchEndpoint,
  updateV2FetchEndpoint,
  type V2FetchEndpoint,
  type V2FetchPreview,
  type V2FetchRunResult
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
  const [status, setStatus] = useState("V2 Fetch waiting to load");
  const [loading, setLoading] = useState(false);

  const selectedEndpoint = useMemo(() => endpoints.find((endpoint) => endpoint.id === selectedEndpointId) ?? endpoints[0] ?? null, [endpoints, selectedEndpointId]);

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

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Globe2 className="h-5 w-5 text-primary" />
              <CardTitle>V2 Fetch Workbench</CardTitle>
            </div>
            <CardDescription>V2 cURL import, endpoint management, and run-scoped Fetch execution</CardDescription>
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
            {result ? <FetchResult result={result} /> : null}
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

function FetchResult({ result }: { result: V2FetchRunResult }) {
  const response = isRecord(result.result.response) ? result.result.response : null;
  return (
    <div className="space-y-3 rounded-lg border border-border p-3">
      <div className="grid gap-2 md:grid-cols-3">
        <Metric label="Status" value={String(result.result.status ?? "-")} />
        <Metric label="HTTP" value={String(response?.statusCode ?? "-")} />
        <Metric label="Duration" value={`${String(result.result.durationMs ?? "-")}ms`} />
      </div>
      <PathLine label="Artifact" value={result.artifact.relative_path} />
      <PathLine label="Evidence" value={`${result.evidence.id} · ${result.evidence.summary}`} />
      {response ? <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-lg border border-border bg-slate-50 p-3 text-xs">{String(response.bodyPreview ?? "")}</pre> : null}
      <JsonBlock title="Result JSON" value={result.result} />
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value}</p></div>;
}

function PathLine({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all font-mono text-xs">{value}</p></div>;
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return <div><p className="mb-2 text-xs text-muted-foreground">{title}</p><pre className="max-h-52 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{JSON.stringify(value, null, 2)}</pre></div>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
