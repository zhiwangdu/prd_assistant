import { Activity, FileSearch, KeyRound, Layers3, Network } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge, Card, CardContent, Input } from "./components/ui";
import { MetadataDashboard } from "./metadata/MetadataDashboard";
import { OperationsView } from "./OperationsView";

const API_KEY_STORAGE = "logagent.webui.apiKey";

export function App() {
  const [apiKey, setApiKey] = useState("");
  const [healthy, setHealthy] = useState<boolean | null>(null);
  const [view, setView] = useState<"metadata" | "operations">("metadata");

  useEffect(() => {
    setApiKey(localStorage.getItem(API_KEY_STORAGE) ?? "");
    void fetch("/health").then((response) => setHealthy(response.ok)).catch(() => setHealthy(false));
  }, []);

  useEffect(() => {
    localStorage.setItem(API_KEY_STORAGE, apiKey.trim());
  }, [apiKey]);

  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="sticky top-0 z-20 border-b border-border bg-white/95 backdrop-blur">
        <div className="mx-auto flex max-w-[1680px] flex-col gap-3 px-5 py-4 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex items-center gap-3">
            <div className="rounded-lg bg-primary p-2 text-primary-foreground"><Layers3 className="h-5 w-5" /></div>
            <div><h1 className="font-semibold">LogAgent Metadata Console</h1><p className="text-xs text-muted-foreground">openGemini cluster model and diagnostics</p></div>
            <Badge variant={healthy ? "success" : healthy === false ? "destructive" : "secondary"}><Activity className="mr-1 h-3 w-3" />{healthy ? "Server healthy" : healthy === false ? "Server unavailable" : "Checking"}</Badge>
          </div>
          <Card className="shadow-none lg:w-[360px]">
            <CardContent className="relative p-0">
              <KeyRound className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
              <Input className="border-0 pl-9 shadow-none" type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="API Key" />
            </CardContent>
          </Card>
        </div>
      </header>
      <main className="mx-auto max-w-[1680px] px-5 py-6">
        <nav className="mb-5 flex gap-2">
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "metadata" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("metadata")}><Network className="mr-2 inline h-4 w-4" />Metadata</button>
          <button className={`rounded-lg px-4 py-2 text-sm font-medium ${view === "operations" ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView("operations")}><FileSearch className="mr-2 inline h-4 w-4" />Log analysis</button>
        </nav>
        {view === "metadata" ? <MetadataDashboard apiKey={apiKey} /> : <OperationsView apiKey={apiKey} />}
      </main>
    </div>
  );
}
