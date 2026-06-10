import { Boxes, BrainCircuit, FileText, GitBranch, Plus, RefreshCw, Save, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input, Tabs, TabsContent, TabsList, TabsTrigger } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { MetadataDashboard } from "./metadata/MetadataDashboard";

type ContextKind = "prompt_pack" | "architecture_doc" | "runbook" | "glossary" | "tool_capability" | "metadata_instance" | "knowledge_note";
type ContextScope = "global" | "log_analysis" | "case_import" | "tool_run";
type VersionStatus = "draft" | "active" | "archived";
type ContentType = "text" | "markdown" | "mermaid" | "json_summary" | "metadata_adapter";

type PromptPolicy = {
  includeByDefault: boolean;
  maxChars: number;
  priority: number;
  allowedTaskKinds: Array<"log_analysis" | "tool_run">;
};

type ContextVersion = {
  versionId: string;
  revision: number;
  status: VersionStatus;
  contentType: ContentType;
  content: string;
  summary?: string | null;
  promptPolicy: PromptPolicy;
  createdAt: string;
  updatedAt: string;
};

type ContextResource = {
  schemaVersion: number;
  contextId: string;
  kind: ContextKind;
  title: string;
  description?: string | null;
  scope: ContextScope;
  enabled: boolean;
  tags: string[];
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  activeVersionId?: string | null;
  versions: ContextVersion[];
  createdAt: string;
  updatedAt: string;
};

type ContextSummary = Omit<ContextResource, "schemaVersion" | "versions" | "createdAt"> & {
  activeSummary?: string | null;
  contentType?: ContentType | null;
  source: string;
};

type Draft = {
  kind: ContextKind;
  title: string;
  description: string;
  scope: ContextScope;
  enabled: boolean;
  tags: string;
  product: string;
  version: string;
  environment: string;
  contentType: ContentType;
  summary: string;
  content: string;
  includeByDefault: boolean;
  maxChars: number;
  priority: number;
};

const DEFAULT_DRAFT: Draft = {
  kind: "prompt_pack",
  title: "",
  description: "",
  scope: "log_analysis",
  enabled: true,
  tags: "",
  product: "",
  version: "",
  environment: "",
  contentType: "markdown",
  summary: "",
  content: "",
  includeByDefault: true,
  maxChars: 4000,
  priority: 0
};

const KIND_OPTIONS: Array<{ value: ContextKind; label: string }> = [
  { value: "prompt_pack", label: "Prompt Pack" },
  { value: "architecture_doc", label: "Architecture" },
  { value: "runbook", label: "Runbook" },
  { value: "glossary", label: "Glossary" },
  { value: "tool_capability", label: "Tool Capability" },
  { value: "knowledge_note", label: "Knowledge Note" }
];

export function SystemContextView({ apiKey }: { apiKey: string }) {
  const [resources, setResources] = useState<ContextSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selected, setSelected] = useState<ContextResource | null>(null);
  const [draft, setDraft] = useState<Draft>(DEFAULT_DRAFT);
  const [status, setStatus] = useState("System Context ready");
  const [previewPrompt, setPreviewPrompt] = useState("");
  const [loading, setLoading] = useState(false);

  const filtered = useMemo(() => resources.filter((resource) => resource.kind !== "metadata_instance"), [resources]);
  const metadataResources = useMemo(() => resources.filter((resource) => resource.kind === "metadata_instance"), [resources]);

  const refresh = useCallback(async () => {
    if (!apiKey.trim()) {
      setResources([]);
      setSelected(null);
      return;
    }
    const response = await fetchJson<{ resources: ContextSummary[] }>("/api/system-context/resources", { headers: authHeaders(apiKey) });
    setResources(response.resources);
    setStatus(`Loaded ${response.resources.length} resources`);
  }, [apiKey]);

  const loadResource = useCallback(async (contextId: string) => {
    if (contextId.startsWith("meta_")) {
      setSelected(null);
      setSelectedId(contextId);
      setStatus("Metadata resources are edited from the Metadata tab");
      return;
    }
    const response = await fetchJson<ContextResource>(`/api/system-context/resources/${encodeURIComponent(contextId)}`, { headers: authHeaders(apiKey) });
    setSelected(response);
    setSelectedId(contextId);
    setDraft(fromResource(response));
  }, [apiKey]);

  useEffect(() => {
    void refresh().catch((reason) => setStatus(errorMessage(reason)));
  }, [refresh]);

  async function createResource() {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const resource = await fetchJson<ContextResource>("/api/system-context/resources", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify(toCreateBody(draft))
      });
      setStatus(`Created ${resource.contextId}`);
      await refresh();
      await loadResource(resource.contextId);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function saveResource() {
    if (!selected || !apiKey.trim()) return;
    setLoading(true);
    try {
      const updated = await fetchJson<ContextResource>(`/api/system-context/resources/${encodeURIComponent(selected.contextId)}`, {
        method: "PATCH",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify(toPatchBody(draft))
      });
      setSelected(updated);
      setStatus(`Saved ${updated.contextId}`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function addVersion(activate: boolean) {
    if (!selected || !apiKey.trim()) return;
    setLoading(true);
    try {
      const updated = await fetchJson<ContextResource>(`/api/system-context/resources/${encodeURIComponent(selected.contextId)}/versions`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ ...toVersionBody(draft), activate })
      });
      setSelected(updated);
      setDraft(fromResource(updated));
      setStatus(activate ? "Version added and activated" : "Draft version added");
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function activate(versionId: string) {
    if (!selected || !apiKey.trim()) return;
    setLoading(true);
    try {
      const updated = await fetchJson<ContextResource>(`/api/system-context/resources/${encodeURIComponent(selected.contextId)}/versions/${encodeURIComponent(versionId)}/activate`, {
        method: "POST",
        headers: authHeaders(apiKey)
      });
      setSelected(updated);
      setDraft(fromResource(updated));
      setStatus(`Activated ${versionId}`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function preview() {
    if (!apiKey.trim()) return;
    setLoading(true);
    try {
      const response = await fetchJson<{ prompt: string }>("/api/system-context/preview", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          taskKind: "log_analysis",
          contextIds: selectedId && !selectedId.startsWith("meta_") ? [selectedId] : [],
          product: draft.product || null,
          version: draft.version || null,
          environment: draft.environment || null
        })
      });
      setPreviewPrompt(response.prompt);
      setStatus("Preview refreshed");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <Tabs defaultValue="library">
        <TabsList>
          <TabsTrigger value="library"><BrainCircuit className="mr-2 h-4 w-4" />Library</TabsTrigger>
          <TabsTrigger value="architecture"><GitBranch className="mr-2 h-4 w-4" />Architecture</TabsTrigger>
          <TabsTrigger value="metadata"><Boxes className="mr-2 h-4 w-4" />Metadata</TabsTrigger>
        </TabsList>

        <TabsContent value="library">
          <div className="grid gap-5 xl:grid-cols-[380px_minmax(0,1fr)]">
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <CardTitle>System Context</CardTitle>
                    <CardDescription>{status}</CardDescription>
                  </div>
                  <Button className="h-8 px-3" variant="outline" onClick={() => void refresh()}><RefreshCw className="h-4 w-4" /></Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-2">
                {filtered.length ? filtered.map((resource) => (
                  <button className={`w-full rounded-lg border p-3 text-left ${selectedId === resource.contextId ? "border-primary bg-slate-50" : "border-border"}`} key={resource.contextId} onClick={() => void loadResource(resource.contextId)}>
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate text-sm font-medium">{resource.title}</span>
                      <Badge variant={resource.enabled ? "secondary" : "outline"}>{resource.enabled ? "enabled" : "disabled"}</Badge>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">{kindLabel(resource.kind)} · {resource.scope} · {resource.contentType ?? "no active content"}</p>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{resource.activeSummary ?? resource.description ?? resource.contextId}</p>
                  </button>
                )) : <EmptyState>暂无 System Context 资源。</EmptyState>}
                {metadataResources.length ? <div className="pt-3"><p className="mb-2 text-xs font-medium text-muted-foreground">Metadata adapters</p>{metadataResources.slice(0, 8).map((resource) => <button className="mb-2 w-full rounded-lg border border-border p-3 text-left" key={resource.contextId} onClick={() => void loadResource(resource.contextId)}><p className="truncate text-sm font-medium">{resource.title}</p><p className="mt-1 text-xs text-muted-foreground">{resource.activeSummary ?? resource.description}</p></button>)}</div> : null}
              </CardContent>
            </Card>

            <ResourceEditor draft={draft} loading={loading} selected={selected} onDraft={setDraft} onNew={() => { setSelected(null); setSelectedId(null); setDraft(DEFAULT_DRAFT); }} onCreate={() => void createResource()} onSave={() => void saveResource()} onAddVersion={(active) => void addVersion(active)} onActivate={(versionId) => void activate(versionId)} onPreview={() => void preview()} previewPrompt={previewPrompt} />
          </div>
        </TabsContent>

        <TabsContent value="architecture">
          <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_480px]">
            <ResourceEditor draft={{ ...draft, kind: "architecture_doc", contentType: draft.contentType === "mermaid" ? draft.contentType : "mermaid" }} loading={loading} selected={selected?.kind === "architecture_doc" ? selected : null} onDraft={setDraft} onNew={() => { setSelected(null); setSelectedId(null); setDraft({ ...DEFAULT_DRAFT, kind: "architecture_doc", contentType: "mermaid", title: "Product architecture" }); }} onCreate={() => void createResource()} onSave={() => void saveResource()} onAddVersion={(active) => void addVersion(active)} onActivate={(versionId) => void activate(versionId)} onPreview={() => void preview()} previewPrompt={previewPrompt} />
            <Card>
              <CardHeader><CardTitle>Mermaid Preview</CardTitle><CardDescription>Mermaid source is stored as text for diff and prompt summary</CardDescription></CardHeader>
              <CardContent>
                <pre className="max-h-[640px] overflow-auto rounded-lg border border-border bg-slate-950 p-4 text-xs text-slate-100">{draft.content || "flowchart LR\n  User-->Server\n  Server-->LLM"}</pre>
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="metadata">
          <MetadataDashboard apiKey={apiKey} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function ResourceEditor({ draft, loading, selected, previewPrompt, onDraft, onNew, onCreate, onSave, onAddVersion, onActivate, onPreview }: { draft: Draft; loading: boolean; selected: ContextResource | null; previewPrompt: string; onDraft: (draft: Draft) => void; onNew: () => void; onCreate: () => void; onSave: () => void; onAddVersion: (active: boolean) => void; onActivate: (versionId: string) => void; onPreview: () => void }) {
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>{selected ? selected.title : "New context resource"}</CardTitle>
            <CardDescription>{selected ? `${selected.contextId} · active ${selected.activeVersionId ?? "-"}` : "Create prompt packs, architecture docs, runbooks or notes"}</CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button className="h-8 px-3" variant="outline" onClick={onNew}><Plus className="mr-1 h-4 w-4" />New</Button>
            <Button className="h-8 px-3" disabled={loading || !draft.title.trim() || !draft.content.trim()} onClick={selected ? onSave : onCreate}><Save className="mr-1 h-4 w-4" />{selected ? "Save" : "Create"}</Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-3 md:grid-cols-3">
          <label className="text-xs text-muted-foreground">Kind<select className="mt-1 h-10 w-full rounded-md border border-border bg-white px-3 text-sm" value={draft.kind} onChange={(event) => onDraft({ ...draft, kind: event.target.value as ContextKind })}>{KIND_OPTIONS.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
          <label className="text-xs text-muted-foreground">Scope<select className="mt-1 h-10 w-full rounded-md border border-border bg-white px-3 text-sm" value={draft.scope} onChange={(event) => onDraft({ ...draft, scope: event.target.value as ContextScope })}><option value="global">Global</option><option value="log_analysis">Log analysis</option><option value="case_import">Case import</option><option value="tool_run">Tool run</option></select></label>
          <label className="text-xs text-muted-foreground">Content<select className="mt-1 h-10 w-full rounded-md border border-border bg-white px-3 text-sm" value={draft.contentType} onChange={(event) => onDraft({ ...draft, contentType: event.target.value as ContentType })}><option value="text">Text</option><option value="markdown">Markdown</option><option value="mermaid">Mermaid</option><option value="json_summary">JSON summary</option></select></label>
        </div>
        <Input value={draft.title} onChange={(event) => onDraft({ ...draft, title: event.target.value })} placeholder="Title" />
        <Input value={draft.description} onChange={(event) => onDraft({ ...draft, description: event.target.value })} placeholder="Description" />
        <div className="grid gap-3 md:grid-cols-3">
          <Input value={draft.product} onChange={(event) => onDraft({ ...draft, product: event.target.value })} placeholder="Product filter" />
          <Input value={draft.version} onChange={(event) => onDraft({ ...draft, version: event.target.value })} placeholder="Version filter" />
          <Input value={draft.environment} onChange={(event) => onDraft({ ...draft, environment: event.target.value })} placeholder="Environment filter" />
        </div>
        <Input value={draft.tags} onChange={(event) => onDraft({ ...draft, tags: event.target.value })} placeholder="Tags, comma separated" />
        <textarea className="min-h-16 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={draft.summary} onChange={(event) => onDraft({ ...draft, summary: event.target.value })} placeholder="Prompt summary" />
        <textarea className="min-h-64 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-sm" value={draft.content} onChange={(event) => onDraft({ ...draft, content: event.target.value })} placeholder="Context content" />
        <div className="grid gap-3 md:grid-cols-[auto_1fr_1fr] md:items-center">
          <label className="flex items-center gap-2 text-sm"><input className="h-4 w-4 accent-teal-700" type="checkbox" checked={draft.includeByDefault} onChange={(event) => onDraft({ ...draft, includeByDefault: event.target.checked })} />Include by default</label>
          <Input type="number" value={draft.maxChars} onChange={(event) => onDraft({ ...draft, maxChars: Number(event.target.value) || 4000 })} placeholder="Max prompt chars" />
          <Input type="number" value={draft.priority} onChange={(event) => onDraft({ ...draft, priority: Number(event.target.value) || 0 })} placeholder="Prompt priority" />
        </div>
        <div className="flex flex-wrap gap-2">
          {selected ? <Button disabled={loading || !draft.content.trim()} variant="outline" onClick={() => onAddVersion(false)}><FileText className="mr-2 h-4 w-4" />Add draft version</Button> : null}
          {selected ? <Button disabled={loading || !draft.content.trim()} onClick={() => onAddVersion(true)}><UploadCloud className="mr-2 h-4 w-4" />Add active version</Button> : null}
          <Button disabled={loading} variant="outline" onClick={onPreview}>Preview Prompt</Button>
        </div>
        {selected?.versions.length ? <div className="space-y-2"><p className="text-xs font-medium text-muted-foreground">Versions</p>{selected.versions.map((version) => <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border p-3" key={version.versionId}><div><p className="text-sm font-medium">rev {version.revision} · {version.contentType}</p><p className="text-xs text-muted-foreground">{version.versionId} · {version.status} · {version.summary ?? "no summary"}</p></div><Button className="h-8 px-3" disabled={loading || version.status === "active"} variant="outline" onClick={() => onActivate(version.versionId)}>Activate</Button></div>)}</div> : null}
        {previewPrompt ? <pre className="max-h-80 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{previewPrompt}</pre> : null}
      </CardContent>
    </Card>
  );
}

function fromResource(resource: ContextResource): Draft {
  const active = resource.versions.find((version) => version.versionId === resource.activeVersionId) ?? resource.versions[0];
  return {
    kind: resource.kind,
    title: resource.title,
    description: resource.description ?? "",
    scope: resource.scope,
    enabled: resource.enabled,
    tags: resource.tags.join(", "),
    product: resource.product ?? "",
    version: resource.version ?? "",
    environment: resource.environment ?? "",
    contentType: active?.contentType ?? "markdown",
    summary: active?.summary ?? "",
    content: active?.content ?? "",
    includeByDefault: active?.promptPolicy.includeByDefault ?? true,
    maxChars: active?.promptPolicy.maxChars ?? 4000,
    priority: active?.promptPolicy.priority ?? 0
  };
}

function toCreateBody(draft: Draft) {
  return { ...resourceFields(draft), ...toVersionBody(draft) };
}

function toPatchBody(draft: Draft) {
  return resourceFields(draft);
}

function toVersionBody(draft: Draft) {
  return {
    contentType: draft.contentType,
    content: draft.content,
    summary: draft.summary.trim() || null,
    promptPolicy: {
      includeByDefault: draft.includeByDefault,
      maxChars: draft.maxChars,
      priority: draft.priority,
      allowedTaskKinds: draft.scope === "tool_run" ? ["tool_run"] : ["log_analysis"]
    }
  };
}

function resourceFields(draft: Draft) {
  return {
    kind: draft.kind,
    title: draft.title,
    description: draft.description.trim() || null,
    scope: draft.scope,
    enabled: draft.enabled,
    tags: draft.tags.split(",").map((tag) => tag.trim()).filter(Boolean),
    product: draft.product.trim() || null,
    version: draft.version.trim() || null,
    environment: draft.environment.trim() || null
  };
}

function kindLabel(kind: ContextKind) {
  return KIND_OPTIONS.find((option) => option.value === kind)?.label ?? kind;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
