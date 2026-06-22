import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState } from "./components/ui";
import { fetchJson, jsonHeaders } from "./metadata/api";
import { mcpCopy, type UiLanguage } from "./i18n";

type McpTool = { name: string; description?: string };
type McpResource = { uri: string; name?: string; description?: string };

async function mcpCall<T>(apiKey: string, method: string, params: Record<string, unknown> = {}): Promise<T> {
  const body = await fetchJson<{ jsonrpc: string; result?: T; error?: { message?: string } }>("/api/mcp", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params })
  });
  if (body.error) throw new Error(body.error.message ?? "MCP error");
  if (!body.result) throw new Error("MCP response missing result");
  return body.result;
}

const STDIO_CONFIG = `{
  "mcpServers": {
    "localtoolhub": {
      "command": "logagent-server",
      "args": ["mcp-serve"]
    }
  }
}`;

export function McpView({ apiKey, language }: { apiKey: string; language: UiLanguage }) {
  const copy = mcpCopy[language];
  const [tools, setTools] = useState<McpTool[]>([]);
  const [resources, setResources] = useState<McpResource[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    if (!apiKey.trim()) return;
    setLoading(true);
    setError("");
    try {
      const [toolsResult, resourcesResult] = await Promise.all([
        mcpCall<{ tools: McpTool[] }>(apiKey, "tools/list"),
        mcpCall<{ resources: McpResource[] }>(apiKey, "resources/list")
      ]);
      setTools(toolsResult.tools ?? []);
      setResources(resourcesResult.resources ?? []);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">{copy.title}</h2>
          <p className="text-sm text-muted-foreground">{copy.subtitle}</p>
        </div>
        <Button className="h-8 px-3" variant="outline" disabled={loading} onClick={() => void refresh()}><RefreshCw className="h-4 w-4" /></Button>
      </div>
      {error ? <p className="text-sm text-red-600">{error}</p> : null}
      <Card>
        <CardHeader>
          <CardTitle>{copy.stdioTitle}</CardTitle>
          <CardDescription>{copy.stdioDesc}</CardDescription>
        </CardHeader>
        <CardContent>
          <pre className="overflow-auto rounded-md bg-slate-900 p-3 text-xs text-slate-100">{STDIO_CONFIG}</pre>
        </CardContent>
      </Card>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>{copy.toolsTitle(tools.length)}</CardTitle>
            <CardDescription>{copy.toolsDesc}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-2">
            {tools.length === 0 ? <EmptyState>{copy.noTools}</EmptyState> : tools.map((tool) => (
              <div key={tool.name} className="rounded-md border border-border p-3">
                <p className="font-mono text-xs font-medium">{tool.name}</p>
                {tool.description ? <p className="mt-1 text-xs text-muted-foreground">{tool.description}</p> : null}
              </div>
            ))}
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>{copy.resourcesTitle(resources.length)}</CardTitle></CardHeader>
          <CardContent className="space-y-2">
            {resources.length === 0 ? <EmptyState>{copy.noResources}</EmptyState> : resources.map((resource) => (
              <div key={resource.uri} className="rounded-md border border-border p-3">
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate font-mono text-xs">{resource.uri}</span>
                  {resource.name ? <Badge variant="secondary">{resource.name}</Badge> : null}
                </div>
                {resource.description ? <p className="mt-1 text-xs text-muted-foreground">{resource.description}</p> : null}
              </div>
            ))}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
