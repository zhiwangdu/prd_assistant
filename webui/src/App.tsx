import { Activity, Cable, History, KeyRound, Layers3, Settings, Wrench, type LucideIcon } from "lucide-react";
import { Fragment, useEffect, useState } from "react";
import { Badge, Card, CardContent, Input } from "./components/ui";
import { McpView } from "./McpView";
import { RunsView } from "./RunsView";
import { ToolsView } from "./ToolsView";
import { SettingsView } from "./SettingsView";
import { DEFAULT_UI_LANGUAGE, UI_LANGUAGE_STORAGE_KEY, appCopy, languageOptions, normalizeUiLanguage, type UiLanguage } from "./i18n";

const API_KEY_STORAGE = "logagent.webui.apiKey";

type ViewKey = "tools" | "runs" | "mcp" | "settings";

// Top-level navigation is English-only (no bilingual labels). "Runs History" is
// a sub-item nested under Tools rather than a standalone top-level tab.
type NavChild = { key: ViewKey; label: string; icon: LucideIcon };
type NavItem = { key: ViewKey; label: string; icon: LucideIcon; children?: NavChild[] };

const navItems: NavItem[] = [
  { key: "tools", label: "Tools", icon: Wrench, children: [{ key: "runs", label: "Runs History", icon: History }] },
  { key: "mcp", label: "MCP", icon: Cable },
  { key: "settings", label: "Settings", icon: Settings }
];

export function App() {
  const [apiKey, setApiKey] = useState("");
  const [healthy, setHealthy] = useState<boolean | null>(null);
  const [language, setLanguage] = useState<UiLanguage>(DEFAULT_UI_LANGUAGE);
  const copy = appCopy[language];
  const [view, setView] = useState<ViewKey>("tools");

  useEffect(() => {
    setApiKey(localStorage.getItem(API_KEY_STORAGE) ?? "");
    setLanguage(normalizeUiLanguage(localStorage.getItem(UI_LANGUAGE_STORAGE_KEY)));
    void fetch("/health").then((response) => setHealthy(response.ok)).catch(() => setHealthy(false));
  }, []);

  useEffect(() => {
    localStorage.setItem(API_KEY_STORAGE, apiKey.trim());
  }, [apiKey]);

  useEffect(() => {
    localStorage.setItem(UI_LANGUAGE_STORAGE_KEY, language);
  }, [language]);

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
            <CardContent className="grid gap-3 p-3 md:grid-cols-[1fr_auto] md:items-center">
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
            </CardContent>
          </Card>
        </div>
      </header>
      <main className="mx-auto max-w-[1680px] px-5 py-6">
        <nav className="mb-5 flex flex-wrap items-center gap-2">
          {navItems.map((item) => {
            const Icon = item.icon;
            const active = view === item.key;
            return (
              <Fragment key={item.key}>
                <button className={`rounded-lg px-4 py-2 text-sm font-medium ${active ? "bg-primary text-white" : "bg-white text-slate-600"}`} onClick={() => setView(item.key)}>
                  <Icon className="mr-2 inline h-4 w-4" />{item.label}
                </button>
                {item.children?.map((child) => {
                  const ChildIcon = child.icon;
                  const childActive = view === child.key;
                  return (
                    <button
                      key={child.key}
                      className={`ml-1 rounded-lg px-3 py-1.5 text-xs font-medium ${childActive ? "bg-primary text-white" : "border border-dashed border-border bg-white text-slate-500"}`}
                      onClick={() => setView(child.key)}
                    >
                      <ChildIcon className="mr-1.5 inline h-3.5 w-3.5" />{child.label}
                    </button>
                  );
                })}
              </Fragment>
            );
          })}
        </nav>
        {view === "tools" ? <ToolsView apiKey={apiKey} language={language} />
          : view === "runs" ? <RunsView apiKey={apiKey} />
          : view === "mcp" ? <McpView apiKey={apiKey} language={language} />
          : <SettingsView apiKey={apiKey} />}
      </main>
    </div>
  );
}
