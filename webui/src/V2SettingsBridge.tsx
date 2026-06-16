import { Bot, Boxes, CheckCircle2, Copy, Download, MessageSquareText, Network, PlugZap, RefreshCw, Send, ServerCog, Settings2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  downloadV2SkillsZip,
  downloadV2ToolsZip,
  getV2AgentBackends,
  getV2DomainAdapters,
  getV2LlmDebug,
  getV2LlmSettings,
  setV2LlmDebug,
  testV2AgentBackend,
  testV2LlmChat,
  testV2LlmModels,
  type V2AgentBackendDiagnosticResult,
  type V2AgentBackendsSummary,
  type V2DomainAdapterSummary,
  type V2LlmChatResult,
  type V2LlmModelsResult,
  type V2LlmSummary,
  type V2LlmTestResponse
} from "./v2-api";

type Props = { apiKey: string };

export function V2SettingsBridge({ apiKey }: Props) {
  const [summary, setSummary] = useState<V2LlmSummary | null>(null);
  const [agentBackends, setAgentBackends] = useState<V2AgentBackendsSummary | null>(null);
  const [domainAdapters, setDomainAdapters] = useState<V2DomainAdapterSummary[]>([]);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [status, setStatus] = useState("V2 Settings waiting to load");
  const [message, setMessage] = useState("hello");
  const [modelsResult, setModelsResult] = useState<V2LlmTestResponse<V2LlmModelsResult> | null>(null);
  const [chatResult, setChatResult] = useState<V2LlmTestResponse<V2LlmChatResult> | null>(null);
  const [backendResult, setBackendResult] = useState<V2LlmTestResponse<V2AgentBackendDiagnosticResult> | null>(null);
  const [loading, setLoading] = useState(false);
  const [testingBackendId, setTestingBackendId] = useState<string | null>(null);

  const readonlyMcpUrl = `${window.location.origin}/api/v2/mcp/readonly`;
  const claudeConfigExample = useMemo(() => JSON.stringify({
    mcpServers: {
      "logagent-v2-readonly": {
        type: "http",
        url: readonlyMcpUrl,
        headers: {
          Authorization: "Bearer <LOGAGENT_V2_API_KEY>"
        }
      }
    }
  }, null, 2), [readonlyMcpUrl]);

  const refresh = useCallback(async () => {
    if (!apiKey.trim()) {
      setSummary(null);
      setAgentBackends(null);
      setDomainAdapters([]);
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const [llmResponse, backendResponse, domainResponse, debugResponse] = await Promise.all([
        getV2LlmSettings(apiKey),
        getV2AgentBackends(apiKey),
        getV2DomainAdapters(apiKey),
        getV2LlmDebug(apiKey)
      ]);
      setSummary(llmResponse.llm);
      setAgentBackends(backendResponse.agentBackends);
      setDomainAdapters(domainResponse.domainAdapters);
      setDebugEnabled(debugResponse.llmOutputLogging);
      setStatus(`V2 Settings loaded: ${llmResponse.llm.provider} / ${llmResponse.llm.configuredModel || "no model"}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function toggleDebug(enabled: boolean) {
    setLoading(true);
    try {
      const response = await setV2LlmDebug(apiKey, enabled);
      setDebugEnabled(response.llmOutputLogging);
      setStatus(response.llmOutputLogging ? "V2 LLM response logging enabled" : "V2 LLM response logging disabled");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function fetchModels() {
    setLoading(true);
    try {
      const response = await testV2LlmModels(apiKey);
      setModelsResult(response);
      setStatus(response.ok ? "V2 model list test passed" : response.error ?? "V2 model list test failed");
    } catch (reason) {
      setModelsResult({ ok: false, error: errorMessage(reason) });
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function sendMessage() {
    setLoading(true);
    try {
      const response = await testV2LlmChat(apiKey, message);
      setChatResult(response);
      setStatus(response.ok ? "V2 message test passed" : response.error ?? "V2 message test failed");
    } catch (reason) {
      setChatResult({ ok: false, error: errorMessage(reason) });
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function runBackendTest(backendId: string) {
    setTestingBackendId(backendId);
    try {
      const response = await testV2AgentBackend(apiKey, backendId);
      setBackendResult(response);
      setStatus(response.ok ? "V2 Agent backend diagnostic passed" : response.error ?? "V2 Agent backend diagnostic failed");
    } catch (reason) {
      setBackendResult({ ok: false, error: errorMessage(reason) });
      setStatus(errorMessage(reason));
    } finally {
      setTestingBackendId(null);
    }
  }

  async function downloadBundle(kind: "skills" | "tools") {
    setLoading(true);
    try {
      if (kind === "skills") {
        await downloadV2SkillsZip(apiKey);
      } else {
        await downloadV2ToolsZip(apiKey);
      }
      setStatus(`Downloaded V2 ${kind}.zip`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function copyConfig() {
    try {
      await navigator.clipboard.writeText(claudeConfigExample);
      setStatus("V2 readonly MCP config copied");
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
              <Settings2 className="h-5 w-5 text-primary" />
              <CardTitle>V2 Settings Bridge</CardTitle>
            </div>
            <CardDescription>Python V2 Agent provider diagnostics, readonly MCP exports, and built-in Domain Adapter summaries</CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refresh()}><RefreshCw className="mr-2 h-4 w-4" />刷新</Button>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void downloadBundle("skills")}><Download className="mr-2 h-4 w-4" />skills.zip</Button>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void downloadBundle("tools")}><Download className="mr-2 h-4 w-4" />tools.zip</Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        {summary ? (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
            <Metric label="Provider" value={summary.provider} />
            <Metric label="Model" value={summary.configuredModel || "-"} />
            <Metric label="Timeout" value={`${summary.requestTimeoutSeconds}s`} />
            <Metric label="Max output" value={summary.maxOutputTokens} />
            <Metric label="Max input chars" value={summary.maxInputChars} />
            <Metric label="Base URL" value={summary.baseUrlConfigured ? "configured" : "not configured"} />
            <Metric label="API Key" value={summary.apiKeyConfigured ? "configured" : "not configured"} />
            <Metric label="Debug log" value={debugEnabled ? "on" : "off"} />
          </div>
        ) : <EmptyState>{status}</EmptyState>}

        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-slate-50 p-3">
          <div>
            <p className="text-sm font-semibold">V2 LLM response-content debug</p>
            <p className="text-xs text-muted-foreground">Logs only provider response content in the V2 process.</p>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={debugEnabled} disabled={loading || !apiKey.trim()} onChange={(event) => void toggleDebug(event.target.checked)} />
            {debugEnabled ? "Enabled" : "Disabled"}
          </label>
        </div>

        <div className="grid gap-5 xl:grid-cols-[1fr_0.9fr]">
          <div className="rounded-lg border border-border p-4">
            <div className="mb-3 flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <Bot className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-semibold">V2 Agent backend</h3>
              </div>
              <Badge variant="secondary">{agentBackends?.defaultBackend ?? "-"}</Badge>
            </div>
            {agentBackends ? (
              <div className="space-y-3">
                {agentBackends.backends.map((backend) => (
                  <div className="rounded-lg border border-border bg-white p-3" key={backend.id}>
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <p className="font-semibold">{backend.id}</p>
                          <Badge variant="secondary">{backend.backendType}</Badge>
                          <Badge variant={backend.commandConfigured ? "success" : "warning"}>{backend.commandConfigured ? "configured" : "incomplete"}</Badge>
                        </div>
                        <p className="mt-1 text-xs text-muted-foreground">{backend.executionMode} · {backend.permissionProfile ?? "-"} · timeout {backend.timeoutSeconds}s</p>
                      </div>
                      <Button className="h-8 px-3" disabled={testingBackendId === backend.id} variant="outline" onClick={() => void runBackendTest(backend.id)}>
                        <PlugZap className={`mr-2 h-4 w-4 ${testingBackendId === backend.id ? "animate-pulse" : ""}`} />
                        Test
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            ) : <EmptyState>{status}</EmptyState>}
            <ResultView result={backendResult} empty="Run backend dry-run diagnostic." renderSummary={(result) => <StatusLine result={result} />} />
          </div>

          <div className="rounded-lg border border-border p-4">
            <div className="mb-3 flex items-center gap-2">
              <Network className="h-4 w-4 text-muted-foreground" />
              <h3 className="text-sm font-semibold">V2 readonly MCP</h3>
            </div>
            <div className="space-y-3">
              <Metric label="URL" value={readonlyMcpUrl} />
              <div className="flex flex-wrap gap-2">
                <Button className="h-8 px-3" variant="outline" onClick={() => void copyConfig()}><Copy className="mr-2 h-4 w-4" />Copy config</Button>
              </div>
              <pre className="max-h-[240px] overflow-auto rounded-lg bg-slate-950 p-3 text-xs leading-5 text-slate-100">{claudeConfigExample}</pre>
            </div>
          </div>
        </div>

        <div className="grid gap-5 xl:grid-cols-2">
          <div className="rounded-lg border border-border p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <ServerCog className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-semibold">Model list test</h3>
              </div>
              <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} onClick={() => void fetchModels()}><RefreshCw className="mr-2 h-4 w-4" />Fetch</Button>
            </div>
            <ResultView result={modelsResult} empty="Fetch V2 provider models." renderSummary={(result) => (
              <div className="mb-3 flex flex-wrap gap-2">
                <StatusLine result={result} />
                {result.result?.models.map((model) => <Badge key={model} variant="secondary">{model}</Badge>)}
              </div>
            )} />
          </div>

          <div className="rounded-lg border border-border p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <MessageSquareText className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-semibold">Message test</h3>
              </div>
              <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} onClick={() => void sendMessage()}><Send className="mr-2 h-4 w-4" />Send</Button>
            </div>
            <Input value={message} onChange={(event) => setMessage(event.target.value)} placeholder="Test message" />
            <ResultView result={chatResult} empty="Send a V2 provider test message." renderSummary={(result) => (
              <div className="mb-3 flex flex-wrap gap-2">
                <StatusLine result={result} />
                {result.result ? <Badge variant="secondary">{result.result.model || result.result.provider}</Badge> : null}
              </div>
            )} />
          </div>
        </div>

        <div className="rounded-lg border border-border p-4">
          <div className="mb-3 flex items-center gap-2">
            <Boxes className="h-4 w-4 text-muted-foreground" />
            <h3 className="text-sm font-semibold">V2 Domain adapters</h3>
          </div>
          {domainAdapters.length ? (
            <div className="grid gap-3 lg:grid-cols-3">
              {domainAdapters.map((adapter) => (
                <div className="rounded-lg border border-border bg-white p-3" key={adapter.id}>
                  <div className="flex flex-wrap items-center gap-2">
                    <p className="font-semibold">{adapter.displayName}</p>
                    <Badge variant={adapter.status === "active" ? "success" : "secondary"}>{adapter.status}</Badge>
                  </div>
                  <p className="mt-1 font-mono text-xs text-muted-foreground">{adapter.id}</p>
                  <div className="mt-2 flex flex-wrap gap-2">
                    {adapter.products.map((product) => <Badge key={product} variant="secondary">{product}</Badge>)}
                  </div>
                  <p className="mt-2 text-xs text-muted-foreground">{adapter.notes[0] ?? "No notes"}</p>
                </div>
              ))}
            </div>
          ) : <EmptyState>{status}</EmptyState>}
        </div>

        <p className="text-xs text-muted-foreground">{status}</p>
      </CardContent>
    </Card>
  );
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return <div className="rounded-lg border border-border bg-white p-3"><p className="text-xs uppercase tracking-wide text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm font-semibold">{value}</p></div>;
}

function ResultView<T>({ result, empty, renderSummary }: { result: V2LlmTestResponse<T> | null; empty: string; renderSummary: (result: V2LlmTestResponse<T>) => ReactNode }) {
  if (!result) return <EmptyState>{empty}</EmptyState>;
  return (
    <div className="mt-3">
      {renderSummary(result)}
      <pre className={`max-h-[360px] overflow-auto rounded-lg p-3 text-xs leading-5 ${result.ok ? "bg-slate-950 text-slate-100" : "border border-red-200 bg-red-50 text-red-800"}`}>
        {JSON.stringify(result, null, 2)}
      </pre>
    </div>
  );
}

function StatusLine<T>({ result }: { result: V2LlmTestResponse<T> }) {
  return result.ok ? <Badge variant="success"><CheckCircle2 className="mr-1 h-3 w-3" />OK</Badge> : <Badge variant="destructive">Failed</Badge>;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
