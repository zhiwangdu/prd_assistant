import { Activity, BookOpenCheck, BrainCircuit, FileSearch, KeyRound, Layers3, Settings, Wrench } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge, Card, CardContent, Input } from "./components/ui";
import { fetchJson, jsonHeaders, authHeaders } from "./metadata/api";
import { CasesView } from "./CasesView";
import { OperationsView } from "./OperationsView";
import { ToolsView } from "./ToolsView";
import { SystemContextView } from "./SystemContextView";
import { SettingsView } from "./SettingsView";

const API_KEY_STORAGE = "logagent.webui.apiKey";

export function App() {
  const [apiKey, setApiKey] = useState("");
  const [healthy, setHealthy] = useState<boolean | null>(null);
  const [llmDebugEnabled, setLlmDebugEnabled] = useState(false);
  const [llmDebugStatus, setLlmDebugStatus] = useState("LLM output logs off");
  const [view, setView] = useState<"operations" | "cases" | "system-context" | "tools" | "settings">("operations");

  useEffect(() => {
    setApiKey(localStorage.getItem(API_KEY_STORAGE) ?? "");
    void fetch("/health").then((response) => setHealthy(response.ok)).catch(() => setHealthy(false));
  }, []);

  useEffect(() => {
    localStorage.setItem(API_KEY_STORAGE, apiKey.trim());
  }, [apiKey]);

  useEffect(() => {
    if (!apiKey.trim()) {
      setLlmDebugEnabled(false);
      setLlmDebugStatus("API Key required");
      return;
    }
    void fetchJson<{ llmOutputLogging: boolean }>("/api/debug/llm", { headers: authHeaders(apiKey) })
      .then((response) => {
        setLlmDebugEnabled(response.llmOutputLogging);
        setLlmDebugStatus(response.llmOutputLogging ? "LLM output logs on" : "LLM output logs off");
      })
      .catch((reason) => setLlmDebugStatus(errorMessage(reason)));
  }, [apiKey]);

  async function toggleLlmDebug(enabled: boolean) {
    if (!apiKey.trim()) {
      setLlmDebugStatus("API Key required");
      return;
    }
    setLlmDebugEnabled(enabled);
    setLlmDebugStatus(enabled ? "Enabling LLM output logs..." : "Disabling LLM output logs...");
    try {
      const response = await fetchJson<{ llmOutputLogging: boolean }>("/api/debug/llm", {
        method: "PUT",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ llmOutputLogging: enabled })
      });
      setLlmDebugEnabled(response.llmOutputLogging);
      setLlmDebugStatus(response.llmOutputLogging ? "LLM output logs on" : "LLM output logs off");
    } catch (reason) {
      setLlmDebugEnabled(!enabled);
      setLlmDebugStatus(errorMessage(reason));
    }
  }

  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="sticky top-0 z-20 border-b border-border bg-white/95 backdrop-blur">
        <div className="mx-auto flex max-w-[1680px] flex-col gap-3 px-5 py-4 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex items-center gap-3">
            <div className="rounded-lg bg-primary p-2 text-primary-foreground"><Layers3 className="h-5 w-5" /></div>
            <div><h1 className="font-semibold">LogAgent Analysis Workbench</h1><p className="text-xs text-muted-foreground">Evidence, memory, system context, and tools</p></div>
            <Badge variant={healthy ? "success" : healthy === false ? "destructive" : "secondary"}><Activity className="mr-1 h-3 w-3" />{healthy ? "Server healthy" : healthy === false ? "Server unavailable" : "Checking"}</Badge>
          </div>
          <Card className="shadow-none lg:w-[560px]">
            <CardContent className="grid gap-3 p-3 md:grid-cols-[1fr_auto] md:items-center">
              <div className="relative">
                <KeyRound className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
                <Input className="border-0 pl-9 shadow-none" type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="API Key" />
              </div>
              <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2 text-xs text-muted-foreground">
                <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={llmDebugEnabled} onChange={(event) => void toggleLlmDebug(event.target.checked)} />
                <span className="whitespace-nowrap">LLM debug</span>
                <span className="hidden max-w-40 truncate text-slate-400 xl:inline" title={llmDebugStatus}>{llmDebugStatus}</span>
              </label>
            </CardContent>
          </Card>
        </div>
      </header>
      <main className="mx-auto max-w-[1680px] px-5 py-6">
        <nav className="mb-5 flex gap-2">
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "operations" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("operations")}><FileSearch className="mr-2 inline h-4 w-4" />Analyze</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "cases" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("cases")}><BookOpenCheck className="mr-2 inline h-4 w-4" />Memory</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "system-context" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("system-context")}><BrainCircuit className="mr-2 inline h-4 w-4" />System Context</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "tools" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("tools")}><Wrench className="mr-2 inline h-4 w-4" />Tools</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "settings" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("settings")}><Settings className="mr-2 inline h-4 w-4" />Settings</button>
        </nav>
        {view === "operations" ? <OperationsView apiKey={apiKey} /> : view === "cases" ? <CasesView apiKey={apiKey} /> : view === "system-context" ? <SystemContextView apiKey={apiKey} /> : view === "tools" ? <ToolsView apiKey={apiKey} /> : <SettingsView apiKey={apiKey} />}
      </main>
    </div>
  );
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
