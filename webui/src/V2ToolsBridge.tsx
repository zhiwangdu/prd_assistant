import { Download, Play, RefreshCw, Wrench } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { callV2TaskTool, downloadV2ToolsZip, listV2Tools, type V2ToolDescriptor } from "./v2-api";

export function V2ToolsBridge({ apiKey }: { apiKey: string }) {
  const [tools, setTools] = useState<V2ToolDescriptor[]>([]);
  const [selectedToolId, setSelectedToolId] = useState("");
  const [runId, setRunId] = useState("");
  const [paramsText, setParamsText] = useState("{}");
  const [resultText, setResultText] = useState("");
  const [status, setStatus] = useState("V2 tools waiting to load");
  const [loading, setLoading] = useState(false);

  const selectedTool = useMemo(() => tools.find((tool) => tool.toolId === selectedToolId) ?? tools[0] ?? null, [selectedToolId, tools]);

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

  useEffect(() => {
    void refreshTools();
  }, [refreshTools]);

  useEffect(() => {
    setParamsText(JSON.stringify(selectedTool?.paramsTemplate ?? {}, null, 2));
    setResultText("");
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
        <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)_420px]">
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

function isJsonObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
