import { Check, Copy, FileJson, Globe2, Plug, RefreshCw, Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  EmptyState,
  Input,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger
} from "./components/ui";
import { fetchJson } from "./metadata/api";
import { mcpCopy, type UiLanguage } from "./i18n";

type McpTool = { name: string; description?: string; inputSchema?: unknown };
type McpResource = { uri: string; name?: string; description?: string; mimeType?: string };
type McpInitializeResult = {
  protocolVersion?: string;
  capabilities?: Record<string, unknown>;
  serverInfo?: { name?: string; version?: string };
};
type McpResourceReadResult = {
  contents?: Array<{ uri?: string; mimeType?: string; text?: string }>;
};
type McpMethod = {
  method: string;
  purpose: string;
  params: Record<string, unknown>;
};

const MCP_PROTOCOL_VERSION = "2025-06-18";
const STDIO_CONFIG = JSON.stringify(
  {
    mcpServers: {
      localtoolhub: {
        command: "logagent-server",
        args: ["mcp-serve"]
      }
    }
  },
  null,
  2
);

async function mcpCall<T>(apiKey: string, method: string, params: Record<string, unknown> = {}): Promise<T> {
  const body = await fetchJson<{ jsonrpc: string; result?: T; error?: { message?: string } }>("/api/mcp", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey.trim()}`,
      "Content-Type": "application/json",
      "MCP-Protocol-Version": MCP_PROTOCOL_VERSION
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: Date.now(), method, params })
  });
  if (body.error) throw new Error(body.error.message ?? "MCP error");
  if (body.result === undefined) throw new Error("MCP response missing result");
  return body.result;
}

function formatJson(value: unknown) {
  return JSON.stringify(value, null, 2);
}

function endpointUrl() {
  return `${window.location.origin}/api/mcp`;
}

function httpConfig(url: string) {
  return formatJson({
    mcpServers: {
      localtoolhub: {
        type: "http",
        url,
        headers: {
          Authorization: "Bearer <LOGAGENT_API_KEY>",
          "MCP-Protocol-Version": MCP_PROTOCOL_VERSION
        }
      }
    }
  });
}

function toolCallExample(tool?: McpTool, queued = false) {
  const argumentsValue = tool?.name === "logagent.runs.get" || tool?.name === "logagent.runs.result"
    ? { runId: "task_..." }
    : queued ? { runMode: "queued" } : {};
  return formatJson({
    jsonrpc: "2.0",
    id: 1,
    method: "tools/call",
    params: {
      name: tool?.name ?? "logagent.search_logs",
      arguments: argumentsValue
    }
  });
}

function runPollExample() {
  return formatJson({
    jsonrpc: "2.0",
    id: 2,
    method: "tools/call",
    params: {
      name: "logagent.runs.get",
      arguments: {
        runId: "task_..."
      }
    }
  });
}

function resourceReadExample(resource?: McpResource) {
  return formatJson({
    jsonrpc: "2.0",
    id: 3,
    method: "resources/read",
    params: {
      uri: resource?.uri ?? "logagent://tools/catalog"
    }
  });
}

function methodExample(method: McpMethod) {
  return formatJson({
    jsonrpc: "2.0",
    id: 1,
    method: method.method,
    params: method.params
  });
}

function filterTools(tools: McpTool[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return tools;
  return tools.filter((tool) => `${tool.name} ${tool.description ?? ""}`.toLowerCase().includes(normalized));
}

function filterResources(resources: McpResource[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return resources;
  return resources.filter((resource) => `${resource.uri} ${resource.name ?? ""} ${resource.description ?? ""}`.toLowerCase().includes(normalized));
}

function schemaSummary(schema: unknown) {
  if (!schema || typeof schema !== "object") return "{}";
  const properties = (schema as { properties?: Record<string, unknown> }).properties;
  if (!properties || typeof properties !== "object") return "{}";
  const keys = Object.keys(properties);
  return keys.length ? keys.slice(0, 4).join(", ") + (keys.length > 4 ? "..." : "") : "{}";
}

function isPlatformTool(tool?: McpTool) {
  return tool?.name.startsWith("logagent.runs.") ?? false;
}

export function McpView({ apiKey, language }: { apiKey: string; language: UiLanguage }) {
  const copy = mcpCopy[language];
  const endpoint = endpointUrl();
  const methods = useMemo<McpMethod[]>(
    () => [
      { method: "initialize", purpose: copy.methodInitialize, params: {} },
      { method: "ping", purpose: copy.methodPing, params: {} },
      { method: "tools/list", purpose: copy.methodToolsList, params: {} },
      {
        method: "tools/call",
        purpose: copy.methodToolsCall,
        params: { name: "logagent.search_logs", arguments: {} }
      },
      { method: "resources/list", purpose: copy.methodResourcesList, params: {} },
      { method: "resources/read", purpose: copy.methodResourcesRead, params: { uri: "logagent://tools/catalog" } }
    ],
    [copy]
  );
  const [tools, setTools] = useState<McpTool[]>([]);
  const [resources, setResources] = useState<McpResource[]>([]);
  const [initialize, setInitialize] = useState<McpInitializeResult | null>(null);
  const [pingOk, setPingOk] = useState(false);
  const [selectedToolName, setSelectedToolName] = useState("");
  const [selectedResourceUri, setSelectedResourceUri] = useState("");
  const [toolQuery, setToolQuery] = useState("");
  const [resourceQuery, setResourceQuery] = useState("");
  const [resourcePreview, setResourcePreview] = useState("");
  const [resourceLoading, setResourceLoading] = useState(false);
  const [copyStatus, setCopyStatus] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const selectedTool = tools.find((tool) => tool.name === selectedToolName) ?? tools[0];
  const selectedResource = resources.find((resource) => resource.uri === selectedResourceUri) ?? resources[0];
  const filteredTools = useMemo(() => filterTools(tools, toolQuery), [tools, toolQuery]);
  const filteredResources = useMemo(() => filterResources(resources, resourceQuery), [resources, resourceQuery]);

  const refresh = useCallback(async () => {
    if (!apiKey.trim()) {
      setError(copy.apiKeyRequired);
      return;
    }
    setLoading(true);
    setError("");
    try {
      const [initializeResult, toolsResult, resourcesResult] = await Promise.all([
        mcpCall<McpInitializeResult>(apiKey, "initialize"),
        mcpCall<{ tools: McpTool[] }>(apiKey, "tools/list"),
        mcpCall<{ resources: McpResource[] }>(apiKey, "resources/list")
      ]);
      await mcpCall<Record<string, never>>(apiKey, "ping");
      const nextTools = toolsResult.tools ?? [];
      const nextResources = resourcesResult.resources ?? [];
      setInitialize(initializeResult);
      setPingOk(true);
      setTools(nextTools);
      setResources(nextResources);
      setSelectedToolName((current) => current || nextTools[0]?.name || "");
      setSelectedResourceUri((current) => current || nextResources[0]?.uri || "");
    } catch (reason) {
      setPingOk(false);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, copy.apiKeyRequired]);

  const readResource = useCallback(async (resource?: McpResource) => {
    if (!apiKey.trim() || !resource) return;
    setResourceLoading(true);
    setResourcePreview("");
    try {
      const result = await mcpCall<McpResourceReadResult>(apiKey, "resources/read", { uri: resource.uri });
      const first = result.contents?.[0];
      setResourcePreview(first?.text ?? formatJson(result));
    } catch (reason) {
      setResourcePreview(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setResourceLoading(false);
    }
  }, [apiKey]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    void readResource(selectedResource);
  }, [readResource, selectedResource]);

  async function copyText(label: string, value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setCopyStatus(copy.copied(label));
    } catch (reason) {
      setCopyStatus(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{copy.title}</h2>
          <p className="text-sm text-muted-foreground">{copy.subtitle}</p>
        </div>
        <Button className="h-8 px-3" variant="outline" disabled={loading} onClick={() => void refresh()} title={copy.refresh}>
          <RefreshCw className="h-4 w-4" />
        </Button>
      </div>

      {error ? <p className="text-sm text-red-600">{error}</p> : null}
      {copyStatus ? <p className="text-sm text-emerald-700">{copyStatus}</p> : null}

      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
        <MetricCard icon={Globe2} label={copy.endpoint} value={endpoint} />
        <MetricCard icon={Plug} label={copy.serverInfo} value={`${initialize?.serverInfo?.name ?? "localtoolhub-mcp"} ${initialize?.serverInfo?.version ?? ""}`.trim()} />
        <MetricCard icon={Plug} label={copy.protocol} value={initialize?.protocolVersion ?? MCP_PROTOCOL_VERSION} />
        <MetricCard icon={Check} label={copy.status} value={pingOk ? copy.connected : copy.notConnected} />
        <MetricCard icon={FileJson} label={copy.catalog} value={`${tools.length} ${copy.toolsShort} / ${resources.length} ${copy.resourcesShort}`} />
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>{copy.httpTitle}</CardTitle>
                <CardDescription>{copy.httpDesc}</CardDescription>
              </div>
              <Button className="h-8 px-3" variant="outline" onClick={() => void copyText(copy.httpTitle, httpConfig(endpoint))}>
                <Copy className="mr-2 h-4 w-4" />
                {copy.copy}
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="grid gap-2 text-sm">
              <KeyValue label={copy.authorizationHeader} value="Authorization: Bearer <api-key>" />
              <KeyValue label={copy.protocolHeader} value={`MCP-Protocol-Version: ${MCP_PROTOCOL_VERSION}`} />
              <KeyValue label={copy.transport} value="POST /api/mcp · application/json or text/event-stream" />
            </div>
            <pre className="max-h-72 overflow-auto rounded-md bg-slate-950 p-3 text-xs leading-5 text-slate-100">{httpConfig(endpoint)}</pre>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>{copy.stdioTitle}</CardTitle>
                <CardDescription>{copy.stdioDesc}</CardDescription>
              </div>
              <Button className="h-8 px-3" variant="outline" onClick={() => void copyText(copy.stdioTitle, STDIO_CONFIG)}>
                <Copy className="mr-2 h-4 w-4" />
                {copy.copy}
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            <KeyValue label={copy.command} value="logagent-server mcp-serve" />
            <pre className="max-h-72 overflow-auto rounded-md bg-slate-950 p-3 text-xs leading-5 text-slate-100">{STDIO_CONFIG}</pre>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{copy.methodsTitle}</CardTitle>
          <CardDescription>{copy.methodsDesc}</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 lg:grid-cols-2">
          {methods.map((method) => (
            <div key={method.method} className="rounded-md border border-border p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="font-mono text-xs font-semibold">{method.method}</p>
                <Button className="h-8 px-3" variant="ghost" onClick={() => void copyText(method.method, methodExample(method))}>
                  <Copy className="mr-2 h-4 w-4" />
                  {copy.copy}
                </Button>
              </div>
              <p className="mt-1 text-xs text-muted-foreground">{method.purpose}</p>
              <pre className="mt-3 max-h-40 overflow-auto rounded-md bg-slate-50 p-3 text-xs text-slate-800">{methodExample(method)}</pre>
            </div>
          ))}
        </CardContent>
      </Card>

      <Tabs defaultValue="tools">
        <TabsList>
          <TabsTrigger value="tools">{copy.toolsTab}</TabsTrigger>
          <TabsTrigger value="resources">{copy.resourcesTab}</TabsTrigger>
          <TabsTrigger value="runs">{copy.queuedTab}</TabsTrigger>
        </TabsList>

        <TabsContent value="tools">
          <div className="grid gap-4 xl:grid-cols-[420px_1fr]">
            <Card>
              <CardHeader>
                <CardTitle>{copy.toolsTitle(tools.length)}</CardTitle>
                <CardDescription>{copy.toolsDesc}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="relative">
                  <Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
                  <Input className="pl-9" value={toolQuery} onChange={(event) => setToolQuery(event.target.value)} placeholder={copy.searchTools} />
                </div>
                <div className="max-h-[520px] space-y-2 overflow-auto pr-1">
                  {filteredTools.length === 0 ? <EmptyState>{tools.length === 0 ? copy.noTools : copy.noMatches}</EmptyState> : filteredTools.map((tool) => (
                    <button
                      key={tool.name}
                      className={`w-full rounded-md border p-3 text-left transition ${selectedTool?.name === tool.name ? "border-primary bg-teal-50" : "border-border bg-white hover:bg-slate-50"}`}
                      onClick={() => setSelectedToolName(tool.name)}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <span className="break-all font-mono text-xs font-medium">{tool.name}</span>
                        {isPlatformTool(tool) ? <Badge variant="secondary">{copy.platformTool}</Badge> : null}
                      </div>
                      {tool.description ? <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{tool.description}</p> : null}
                    </button>
                  ))}
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>{selectedTool?.name ?? copy.toolDetail}</CardTitle>
                <CardDescription>{selectedTool?.description ?? copy.selectTool}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                {selectedTool ? (
                  <>
                    <div className="flex flex-wrap gap-2">
                      <Badge variant="outline">{copy.schemaFields}: {schemaSummary(selectedTool.inputSchema)}</Badge>
                      {isPlatformTool(selectedTool) ? <Badge variant="secondary">{copy.noRunHistory}</Badge> : null}
                    </div>
                    <section className="space-y-2">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">{copy.inputSchema}</p>
                        <Button className="h-8 px-3" variant="ghost" onClick={() => void copyText(copy.inputSchema, formatJson(selectedTool.inputSchema ?? {}))}>
                          <Copy className="mr-2 h-4 w-4" />
                          {copy.copy}
                        </Button>
                      </div>
                      <pre className="max-h-72 overflow-auto rounded-md bg-slate-50 p-3 text-xs text-slate-800">{formatJson(selectedTool.inputSchema ?? {})}</pre>
                    </section>
                    <section className={`grid gap-3 ${isPlatformTool(selectedTool) ? "" : "lg:grid-cols-2"}`}>
                      <SnippetCard title={copy.syncCallExample} value={toolCallExample(selectedTool)} onCopy={(value) => void copyText(copy.syncCallExample, value)} copyLabel={copy.copy} />
                      {isPlatformTool(selectedTool) ? null : (
                        <SnippetCard title={copy.queuedCallExample} value={toolCallExample(selectedTool, true)} onCopy={(value) => void copyText(copy.queuedCallExample, value)} copyLabel={copy.copy} />
                      )}
                    </section>
                  </>
                ) : (
                  <EmptyState>{copy.selectTool}</EmptyState>
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="resources">
          <div className="grid gap-4 xl:grid-cols-[420px_1fr]">
            <Card>
              <CardHeader>
                <CardTitle>{copy.resourcesTitle(resources.length)}</CardTitle>
                <CardDescription>{copy.resourcesDesc}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="relative">
                  <Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
                  <Input className="pl-9" value={resourceQuery} onChange={(event) => setResourceQuery(event.target.value)} placeholder={copy.searchResources} />
                </div>
                <div className="max-h-[520px] space-y-2 overflow-auto pr-1">
                  {filteredResources.length === 0 ? <EmptyState>{resources.length === 0 ? copy.noResources : copy.noMatches}</EmptyState> : filteredResources.map((resource) => (
                    <button
                      key={resource.uri}
                      className={`w-full rounded-md border p-3 text-left transition ${selectedResource?.uri === resource.uri ? "border-primary bg-teal-50" : "border-border bg-white hover:bg-slate-50"}`}
                      onClick={() => setSelectedResourceUri(resource.uri)}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <span className="break-all font-mono text-xs font-medium">{resource.uri}</span>
                        {resource.name ? <Badge variant="secondary">{resource.name}</Badge> : null}
                      </div>
                      {resource.description ? <p className="mt-1 text-xs text-muted-foreground">{resource.description}</p> : null}
                    </button>
                  ))}
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <CardTitle>{selectedResource?.name ?? copy.resourcePreview}</CardTitle>
                    <CardDescription>{selectedResource?.uri ?? copy.selectResource}</CardDescription>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button className="h-8 px-3" variant="outline" disabled={!selectedResource || resourceLoading} onClick={() => void readResource(selectedResource)}>
                      <RefreshCw className="mr-2 h-4 w-4" />
                      {copy.readResource}
                    </Button>
                    <Button className="h-8 px-3" variant="outline" disabled={!resourcePreview} onClick={() => void copyText(copy.resourcePreview, resourcePreview)}>
                      <Copy className="mr-2 h-4 w-4" />
                      {copy.copy}
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {selectedResource ? (
                  <>
                    <SnippetCard title={copy.readExample} value={resourceReadExample(selectedResource)} onCopy={(value) => void copyText(copy.readExample, value)} copyLabel={copy.copy} />
                    <pre className="min-h-72 max-h-[520px] overflow-auto rounded-md bg-slate-950 p-3 text-xs leading-5 text-slate-100">{resourceLoading ? copy.loadingResource : resourcePreview}</pre>
                  </>
                ) : (
                  <EmptyState>{copy.selectResource}</EmptyState>
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="runs">
          <Card>
            <CardHeader>
              <CardTitle>{copy.queuedTitle}</CardTitle>
              <CardDescription>{copy.queuedDesc}</CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3 lg:grid-cols-2">
              <SnippetCard title={copy.queuedCallExample} value={toolCallExample(undefined, true)} onCopy={(value) => void copyText(copy.queuedCallExample, value)} copyLabel={copy.copy} />
              <SnippetCard title={copy.pollExample} value={runPollExample()} onCopy={(value) => void copyText(copy.pollExample, value)} copyLabel={copy.copy} />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function MetricCard({ icon: Icon, label, value }: { icon: typeof Globe2; label: string; value: string }) {
  return (
    <Card>
      <CardContent className="flex items-start gap-3 p-4">
        <Icon className="mt-0.5 h-4 w-4 shrink-0 text-teal-700" />
        <div className="min-w-0">
          <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</p>
          <p className="mt-1 break-all text-sm font-semibold">{value}</p>
        </div>
      </CardContent>
    </Card>
  );
}

function KeyValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 rounded-md border border-border bg-slate-50 p-3">
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className="break-all font-mono text-xs text-slate-800">{value}</p>
    </div>
  );
}

function SnippetCard({ title, value, onCopy, copyLabel }: { title: string; value: string; onCopy: (value: string) => void; copyLabel: string }) {
  return (
    <div className="rounded-md border border-border p-3">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</p>
        <Button className="h-8 px-3" variant="ghost" onClick={() => onCopy(value)}>
          <Copy className="mr-2 h-4 w-4" />
          {copyLabel}
        </Button>
      </div>
      <pre className="max-h-72 overflow-auto rounded-md bg-slate-50 p-3 text-xs text-slate-800">{value}</pre>
    </div>
  );
}
