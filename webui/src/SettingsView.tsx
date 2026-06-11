import { AlertTriangle, Bot, Boxes, CheckCircle2, MessageSquareText, PlugZap, RefreshCw, Send, ServerCog } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, jsonHeaders } from "./metadata/api";

type Props = { apiKey: string };

type LlmSummary = {
  provider: string;
  configuredModel: string;
  maxInputChars: number;
  maxOutputTokens: number;
  requestTimeoutSeconds: number;
  baseUrlConfigured: boolean;
  apiKeyConfigured: boolean;
  binaryPathConfigured: boolean;
};

type LlmSettingsResponse = { llm: LlmSummary };

type LlmTestResponse<T> = {
  ok: boolean;
  result?: T | null;
  error?: string | null;
};

type LlmModelsResult = {
  provider: string;
  configuredModel: string;
  models: string[];
  raw: unknown;
};

type LlmChatResult = {
  provider: string;
  model: string;
  response: string;
};

type AgentBackendSummary = {
  id: string;
  backendType: string;
  enabled: boolean;
  defaultBackend: boolean;
  commandConfigured: boolean;
  timeoutSeconds: number;
  maxInputBytes: number;
  maxOutputBytes: number;
  executionMode: string;
};

type AgentBackendsSummary = {
  defaultBackend: string;
  backends: AgentBackendSummary[];
};

type AgentBackendsResponse = { agentBackends: AgentBackendsSummary };

type AgentBackendDiagnosticResult = {
  backendId: string;
  backendType: string;
  enabled: boolean;
  status: string;
  executionMode: string;
  details: string[];
};

type DomainAdapterSummary = {
  id: string;
  displayName: string;
  status: string;
  products: string[];
  evidenceKinds: string[];
  plannedTools: string[];
  notes: string[];
};

type DomainAdaptersResponse = { domainAdapters: DomainAdapterSummary[] };

export function SettingsView({ apiKey }: Props) {
  const [summary, setSummary] = useState<LlmSummary | null>(null);
  const [agentBackends, setAgentBackends] = useState<AgentBackendsSummary | null>(null);
  const [domainAdapters, setDomainAdapters] = useState<DomainAdapterSummary[]>([]);
  const [summaryStatus, setSummaryStatus] = useState("等待加载 LLM 设置");
  const [modelsResult, setModelsResult] = useState<LlmTestResponse<LlmModelsResult> | null>(null);
  const [chatResult, setChatResult] = useState<LlmTestResponse<LlmChatResult> | null>(null);
  const [agentBackendResult, setAgentBackendResult] = useState<LlmTestResponse<AgentBackendDiagnosticResult> | null>(null);
  const [message, setMessage] = useState("hello");
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [loadingModels, setLoadingModels] = useState(false);
  const [loadingChat, setLoadingChat] = useState(false);
  const [testingBackendId, setTestingBackendId] = useState<string | null>(null);

  const loadSummary = useCallback(async () => {
    if (!apiKey.trim()) {
      setSummary(null);
      setSummaryStatus("请先填写 API Key");
      return;
    }
    setLoadingSummary(true);
    setSummaryStatus("加载 Settings 中...");
    try {
      const [llmResponse, backendResponse, domainResponse] = await Promise.all([
        requestJson<LlmSettingsResponse>("/api/settings/llm", {
          headers: authHeaders(apiKey)
        }),
        requestJson<AgentBackendsResponse>("/api/settings/agent-backends", {
          headers: authHeaders(apiKey)
        }),
        requestJson<DomainAdaptersResponse>("/api/settings/domain-adapters", {
          headers: authHeaders(apiKey)
        })
      ]);
      setSummary(llmResponse.llm);
      setAgentBackends(backendResponse.agentBackends);
      setDomainAdapters(domainResponse.domainAdapters);
      setSummaryStatus("Settings 已加载");
    } catch (reason) {
      setSummary(null);
      setAgentBackends(null);
      setDomainAdapters([]);
      setSummaryStatus(formatError(reason));
    } finally {
      setLoadingSummary(false);
    }
  }, [apiKey]);

  useEffect(() => {
    void loadSummary();
  }, [loadSummary]);

  async function testModels() {
    if (!apiKey.trim()) {
      setModelsResult({ ok: false, error: "请先填写 API Key" });
      return;
    }
    setLoadingModels(true);
    try {
      const response = await requestJson<LlmTestResponse<LlmModelsResult>>("/api/settings/llm/models", {
        headers: authHeaders(apiKey)
      });
      setModelsResult(response);
    } catch (reason) {
      setModelsResult({ ok: false, error: formatError(reason) });
    } finally {
      setLoadingModels(false);
    }
  }

  async function testChat() {
    if (!apiKey.trim()) {
      setChatResult({ ok: false, error: "请先填写 API Key" });
      return;
    }
    setLoadingChat(true);
    try {
      const response = await requestJson<LlmTestResponse<LlmChatResult>>("/api/settings/llm/chat", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ message })
      });
      setChatResult(response);
    } catch (reason) {
      setChatResult({ ok: false, error: formatError(reason) });
    } finally {
      setLoadingChat(false);
    }
  }

  async function testAgentBackend(backendId: string) {
    if (!apiKey.trim()) {
      setAgentBackendResult({ ok: false, error: "请先填写 API Key" });
      return;
    }
    setTestingBackendId(backendId);
    try {
      const response = await requestJson<LlmTestResponse<AgentBackendDiagnosticResult>>(`/api/settings/agent-backends/${encodeURIComponent(backendId)}/test`, {
        method: "POST",
        headers: authHeaders(apiKey)
      });
      setAgentBackendResult(response);
    } catch (reason) {
      setAgentBackendResult({ ok: false, error: formatError(reason) });
    } finally {
      setTestingBackendId(null);
    }
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle>Settings</CardTitle>
              <CardDescription>当前先提供 LLM 服务接口连通性测试。</CardDescription>
            </div>
            <Button variant="outline" onClick={() => void loadSummary()} disabled={loadingSummary}>
              <RefreshCw className={`mr-2 h-4 w-4 ${loadingSummary ? "animate-spin" : ""}`} />
              Reload
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {summary ? (
            <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
              <SettingMetric label="Provider" value={summary.provider} />
              <SettingMetric label="Model" value={summary.configuredModel} />
              <SettingMetric label="Timeout" value={`${summary.requestTimeoutSeconds}s`} />
              <SettingMetric label="Max output" value={summary.maxOutputTokens} />
              <SettingMetric label="Max input chars" value={summary.maxInputChars} />
              <SettingMetric label="Base URL" value={summary.baseUrlConfigured ? "configured" : "not configured"} />
              <SettingMetric label="API Key" value={summary.apiKeyConfigured ? "configured" : "not configured"} />
              <SettingMetric label="Binary path" value={summary.binaryPathConfigured ? "configured" : "not configured"} />
            </div>
          ) : <EmptyState>{summaryStatus}</EmptyState>}
          <p className="mt-3 text-xs text-muted-foreground">{summaryStatus}</p>
        </CardContent>
      </Card>

      <div className="grid gap-5 xl:grid-cols-[1.15fr_0.85fr]">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>Agent backends</CardTitle>
                <CardDescription>成熟 agent 后端适配器，当前阶段只做配置和 dry-run 诊断。</CardDescription>
              </div>
              <Bot className="h-5 w-5 text-muted-foreground" />
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {agentBackends ? (
              <div className="space-y-3">
                {agentBackends.backends.map((backend) => (
                  <div key={backend.id} className="rounded-lg border border-border bg-white p-3">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <p className="font-semibold">{backend.id}</p>
                          <Badge variant="secondary">{backend.backendType}</Badge>
                          {backend.defaultBackend ? <Badge>default</Badge> : null}
                          <Badge variant={backend.enabled ? "success" : "secondary"}>{backend.enabled ? "enabled" : "disabled"}</Badge>
                        </div>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {backend.executionMode} · timeout {backend.timeoutSeconds}s · command {backend.commandConfigured ? "configured" : "not configured"}
                        </p>
                      </div>
                      <Button variant="outline" onClick={() => void testAgentBackend(backend.id)} disabled={!backend.enabled || testingBackendId === backend.id}>
                        <PlugZap className={`mr-2 h-4 w-4 ${testingBackendId === backend.id ? "animate-pulse" : ""}`} />
                        Test
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            ) : <EmptyState>{summaryStatus}</EmptyState>}
            <TestResultView result={agentBackendResult} empty="选择一个 enabled backend 执行 dry-run 诊断。" renderSummary={(result) => (
              <div className="mb-3 flex flex-wrap gap-2">
                <StatusBadge ok={result.ok} />
                {result.result ? <Badge variant="secondary">{result.result.backendId}</Badge> : null}
                {result.result ? <Badge variant="secondary">{result.result.status}</Badge> : null}
              </div>
            )} />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>Domain adapters</CardTitle>
                <CardDescription>数据库和存储系统专项诊断能力包。</CardDescription>
              </div>
              <Boxes className="h-5 w-5 text-muted-foreground" />
            </div>
          </CardHeader>
          <CardContent>
            {domainAdapters.length > 0 ? (
              <div className="space-y-3">
                {domainAdapters.map((adapter) => (
                  <div key={adapter.id} className="rounded-lg border border-border bg-white p-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="font-semibold">{adapter.displayName}</p>
                      <Badge variant={adapter.status === "active" ? "success" : "secondary"}>{adapter.status}</Badge>
                    </div>
                    <div className="mt-2 flex flex-wrap gap-2">
                      {adapter.products.map((product) => <Badge key={product} variant="secondary">{product}</Badge>)}
                    </div>
                    <p className="mt-2 text-xs text-muted-foreground">{adapter.notes[0] ?? "No notes"}</p>
                    {adapter.plannedTools.length > 0 ? (
                      <p className="mt-2 text-xs text-muted-foreground">Tools: {adapter.plannedTools.join(", ")}</p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : <EmptyState>{summaryStatus}</EmptyState>}
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-5 xl:grid-cols-2">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>Model list test</CardTitle>
                <CardDescription>调用当前 Provider 的模型列表接口并展示原始响应。</CardDescription>
              </div>
              <Button onClick={() => void testModels()} disabled={loadingModels}>
                <ServerCog className="mr-2 h-4 w-4" />
                Fetch models
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            <TestResultView result={modelsResult} empty="点击 Fetch models 测试模型列表获取。" renderSummary={(result) => (
              <div className="mb-3 flex flex-wrap gap-2">
                <StatusBadge ok={result.ok} />
                {result.result?.models.map((model) => <Badge key={model} variant="secondary">{model}</Badge>)}
              </div>
            )} />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle>Message test</CardTitle>
                <CardDescription>发送一条简单 user message，并显示模型响应或完整异常。</CardDescription>
              </div>
              <Button onClick={() => void testChat()} disabled={loadingChat}>
                <Send className="mr-2 h-4 w-4" />
                Send
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="relative">
              <MessageSquareText className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
              <Input className="pl-9" value={message} onChange={(event) => setMessage(event.target.value)} placeholder="输入测试消息" />
            </div>
            <TestResultView result={chatResult} empty="输入消息后点击 Send 测试 chat/completions。" renderSummary={(result) => (
              <div className="mb-3 flex flex-wrap gap-2">
                <StatusBadge ok={result.ok} />
                {result.result ? <Badge variant="secondary">{result.result.model}</Badge> : null}
              </div>
            )} />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function SettingMetric({ label, value }: { label: string; value: string | number }) {
  return <div className="rounded-lg border border-border bg-white p-3"><p className="text-xs uppercase tracking-wide text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm font-semibold">{value}</p></div>;
}

function TestResultView<T>({ result, empty, renderSummary }: { result: LlmTestResponse<T> | null; empty: string; renderSummary: (result: LlmTestResponse<T>) => ReactNode }) {
  if (!result) return <EmptyState>{empty}</EmptyState>;
  return (
    <div>
      {renderSummary(result)}
      <pre className={`max-h-[520px] overflow-auto rounded-lg p-4 text-xs leading-5 ${result.ok ? "bg-slate-950 text-slate-100" : "border border-red-200 bg-red-50 text-red-800"}`}>
        {JSON.stringify(result, null, 2)}
      </pre>
    </div>
  );
}

function StatusBadge({ ok }: { ok: boolean }) {
  return ok ? <Badge variant="success"><CheckCircle2 className="mr-1 h-3 w-3" />OK</Badge> : <Badge variant="destructive"><AlertTriangle className="mr-1 h-3 w-3" />Failed</Badge>;
}

async function requestJson<T>(url: string, options: RequestInit): Promise<T> {
  const response = await fetch(url, options);
  const text = await response.text();
  let body: unknown = {};
  try {
    body = text ? JSON.parse(text) : {};
  } catch {
    body = text;
  }
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${typeof body === "string" ? body : JSON.stringify(body, null, 2)}`);
  }
  return body as T;
}

function formatError(reason: unknown) {
  return reason instanceof Error ? reason.stack ?? reason.message : String(reason);
}
