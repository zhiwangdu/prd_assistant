import { Activity, BookOpenCheck, BrainCircuit, FileSearch, Globe2, KeyRound, Layers3, Server, Settings, Wrench } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, Input } from "./components/ui";
import { fetchJson, jsonHeaders, authHeaders } from "./metadata/api";
import { DEFAULT_UI_LANGUAGE, UI_LANGUAGE_STORAGE_KEY, appCopy, languageOptions, normalizeUiLanguage, type UiLanguage } from "./i18n";
import { V2AnalyzeBridge } from "./V2AnalyzeBridge";
import { V2ExecutorsBridge } from "./V2ExecutorsBridge";
import { V2FetchBridge } from "./V2FetchBridge";
import { V2MemoryBridge } from "./V2MemoryBridge";
import { V2MetadataBridge } from "./V2MetadataBridge";
import { V2SettingsBridge } from "./V2SettingsBridge";
import { V2SystemContextBridge } from "./V2SystemContextBridge";
import { V2ToolsBridge } from "./V2ToolsBridge";

const API_KEY_STORAGE = "logagent.webui.apiKey";
const LOCAL_DEV_API_KEY = "dev-token";

export function App() {
  const [apiKey, setApiKey] = useState(initialApiKey);
  const [healthy, setHealthy] = useState<boolean | null>(null);
  const [llmDebugEnabled, setLlmDebugEnabled] = useState(false);
  const [language, setLanguage] = useState<UiLanguage>(DEFAULT_UI_LANGUAGE);
  const copy = appCopy[language];
  const [llmDebugStatus, setLlmDebugStatus] = useState<string>(copy.llmLogsOff);
  const [view, setView] = useState<"operations" | "cases" | "system-context" | "tools" | "settings">("operations");

  useEffect(() => {
    setLanguage(normalizeUiLanguage(localStorage.getItem(UI_LANGUAGE_STORAGE_KEY)));
    void fetch("/health").then((response) => setHealthy(response.ok)).catch(() => setHealthy(false));
  }, []);

  useEffect(() => {
    localStorage.setItem(API_KEY_STORAGE, apiKey.trim());
  }, [apiKey]);

  useEffect(() => {
    localStorage.setItem(UI_LANGUAGE_STORAGE_KEY, language);
  }, [language]);

  useEffect(() => {
    if (!apiKey.trim()) {
      setLlmDebugEnabled(false);
      setLlmDebugStatus(copy.apiKeyRequired);
      return;
    }
    void fetchJson<{ llmOutputLogging: boolean }>("/api/v2/debug/llm", { headers: authHeaders(apiKey) })
      .then((response) => {
        setLlmDebugEnabled(response.llmOutputLogging);
        setLlmDebugStatus(response.llmOutputLogging ? copy.llmLogsOn : copy.llmLogsOff);
      })
      .catch((reason) => setLlmDebugStatus(errorMessage(reason)));
  }, [apiKey, copy.apiKeyRequired, copy.llmLogsOff, copy.llmLogsOn]);

  async function toggleLlmDebug(enabled: boolean) {
    if (!apiKey.trim()) {
      setLlmDebugStatus(copy.apiKeyRequired);
      return;
    }
    setLlmDebugEnabled(enabled);
    setLlmDebugStatus(enabled ? copy.enablingLlmLogs : copy.disablingLlmLogs);
    try {
      const response = await fetchJson<{ llmOutputLogging: boolean }>("/api/v2/debug/llm", {
        method: "PUT",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ llmOutputLogging: enabled })
      });
      setLlmDebugEnabled(response.llmOutputLogging);
      setLlmDebugStatus(response.llmOutputLogging ? copy.llmLogsOn : copy.llmLogsOff);
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
            <div><h1 className="font-semibold">{copy.productName}</h1><p className="text-xs text-muted-foreground">{copy.productSubtitle}</p></div>
            <Badge variant={healthy ? "success" : healthy === false ? "destructive" : "secondary"}><Activity className="mr-1 h-3 w-3" />{healthy ? copy.serverHealthy : healthy === false ? copy.serverUnavailable : copy.checking}</Badge>
          </div>
          <Card className="shadow-none lg:w-[560px]">
            <CardContent className="grid gap-3 p-3 md:grid-cols-[1fr_auto_auto] md:items-center">
              <div className="relative">
                <KeyRound className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
                <Input className="border-0 pl-9 shadow-none" type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={copy.apiKeyPlaceholder} />
              </div>
              <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2 text-xs text-muted-foreground">
                <span className="whitespace-nowrap">{copy.languageLabel}</span>
                <select className="bg-transparent text-xs outline-none" value={language} onChange={(event) => setLanguage(normalizeUiLanguage(event.target.value))}>
                  {languageOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
                </select>
              </label>
              <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2 text-xs text-muted-foreground">
                <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={llmDebugEnabled} onChange={(event) => void toggleLlmDebug(event.target.checked)} />
                <span className="whitespace-nowrap">{copy.llmDebug}</span>
                <span className="hidden max-w-40 truncate text-slate-400 xl:inline" title={llmDebugStatus}>{llmDebugStatus}</span>
              </label>
            </CardContent>
          </Card>
        </div>
      </header>
      <main className="mx-auto max-w-[1680px] px-5 py-6">
        <nav className="mb-5 flex gap-2">
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "operations" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("operations")}><FileSearch className="mr-2 inline h-4 w-4" />{copy.navAnalyze}</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "cases" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("cases")}><BookOpenCheck className="mr-2 inline h-4 w-4" />{copy.navMemory}</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "system-context" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("system-context")}><BrainCircuit className="mr-2 inline h-4 w-4" />{copy.navSystemContext}</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "tools" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("tools")}><Wrench className="mr-2 inline h-4 w-4" />{copy.navTools}</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "settings" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("settings")}><Settings className="mr-2 inline h-4 w-4" />{copy.navSettings}</button>
        </nav>
        {view === "operations" ? <V2AnalyzeBridge apiKey={apiKey} language={language} /> : view === "cases" ? <V2MemoryBridge apiKey={apiKey} /> : view === "system-context" ? <V2SystemContextWorkbench apiKey={apiKey} /> : view === "tools" ? <V2ToolsWorkbench apiKey={apiKey} /> : <V2SettingsBridge apiKey={apiKey} />}
      </main>
    </div>
  );
}

function V2SystemContextWorkbench({ apiKey }: { apiKey: string }) {
  return (
    <div className="space-y-5">
      <V2SystemContextBridge apiKey={apiKey} />
      <V2MetadataBridge apiKey={apiKey} />
    </div>
  );
}

function V2ToolsWorkbench({ apiKey }: { apiKey: string }) {
  const [section, setSection] = useState<"tools" | "fetch" | "executors">("tools");
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap gap-2">
        <Button variant={section === "tools" ? "default" : "outline"} onClick={() => setSection("tools")}><Wrench className="mr-2 h-4 w-4" />Tool plugins</Button>
        <Button variant={section === "fetch" ? "default" : "outline"} onClick={() => setSection("fetch")}><Globe2 className="mr-2 h-4 w-4" />Fetch</Button>
        <Button variant={section === "executors" ? "default" : "outline"} onClick={() => setSection("executors")}><Server className="mr-2 h-4 w-4" />Executors</Button>
      </div>
      {section === "tools" ? <V2ToolsBridge apiKey={apiKey} /> : section === "fetch" ? <V2FetchBridge apiKey={apiKey} /> : <V2ExecutorsBridge apiKey={apiKey} />}
    </div>
  );
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}

function initialApiKey() {
  const stored = localStorage.getItem(API_KEY_STORAGE)?.trim();
  if (stored) return stored;
  return isLocalDevHost() ? LOCAL_DEV_API_KEY : "";
}

function isLocalDevHost() {
  return ["127.0.0.1", "localhost", "::1"].includes(window.location.hostname);
}
