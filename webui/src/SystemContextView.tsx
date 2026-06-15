import { Boxes, BrainCircuit, FileText, RefreshCw, Upload, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input, Tabs, TabsContent, TabsList, TabsTrigger } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { MetadataDashboard } from "./metadata/MetadataDashboard";

type SkillReference = {
  referenceId: string;
  path: string;
  title: string;
  summary: string;
};

type SkillSummary = {
  skillId: string;
  name: string;
  displayName: string;
  description: string;
  managed: boolean;
  includeByDefault: boolean;
  priority: number;
  products: string[];
  domainAdapters: string[];
  toolIds: string[];
  taskKinds: string[];
  revision: string;
  sourceRoot: string;
  sourcePath: string;
  references: SkillReference[];
  updatedAt: string;
};

type SkillDetail = SkillSummary & {
  injectionContent: string;
};

type ImportForm = {
  skillId: string;
  name: string;
  description: string;
  markdown: string;
  filename: string;
};

const emptyImportForm: ImportForm = {
  skillId: "",
  name: "",
  description: "",
  markdown: "",
  filename: ""
};

export function SystemContextView({ apiKey }: { apiKey: string }) {
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);
  const [selectedSkill, setSelectedSkill] = useState<SkillDetail | null>(null);
  const [status, setStatus] = useState("Skills ready");
  const [importOpen, setImportOpen] = useState(false);
  const [importForm, setImportForm] = useState<ImportForm>(emptyImportForm);
  const [importing, setImporting] = useState(false);

  const refresh = useCallback(async (preferredSkillId?: string) => {
    if (!apiKey.trim()) {
      setSkills([]);
      setSelectedSkill(null);
      return;
    }
    const response = await fetchJson<{ skills: SkillSummary[] }>("/api/skills", { headers: authHeaders(apiKey) });
    setSkills(response.skills);
    setStatus(`${response.skills.length} skill(s) loaded`);
    const nextSkillId = preferredSkillId ?? selectedSkillId;
    if (nextSkillId && response.skills.some((skill) => skill.skillId === nextSkillId)) {
      setSelectedSkillId(nextSkillId);
    } else if (response.skills[0]) {
      setSelectedSkillId(response.skills[0].skillId);
    } else {
      setSelectedSkillId(null);
      setSelectedSkill(null);
    }
  }, [apiKey, selectedSkillId]);

  const loadSkill = useCallback(async (skillId: string) => {
    if (!apiKey.trim()) return;
    const response = await fetchJson<SkillDetail>(`/api/skills/${encodeURIComponent(skillId)}`, { headers: authHeaders(apiKey) });
    setSelectedSkill(response);
    setSelectedSkillId(skillId);
    setStatus(`${response.displayName} loaded`);
  }, [apiKey]);

  useEffect(() => {
    void refresh().catch((reason) => setStatus(errorMessage(reason)));
  }, [refresh]);

  useEffect(() => {
    if (selectedSkillId && (!selectedSkill || selectedSkill.skillId !== selectedSkillId)) {
      void loadSkill(selectedSkillId).catch((reason) => setStatus(errorMessage(reason)));
    }
  }, [loadSkill, selectedSkill, selectedSkillId]);

  const handleFileSelected = async (file?: File) => {
    if (!file) return;
    const lower = file.name.toLowerCase();
    if (!lower.endsWith(".md") && !lower.endsWith(".markdown")) {
      setStatus("Only .md and .markdown files can be imported");
      return;
    }
    try {
      const raw = await file.text();
      const parsed = parseMarkdownImport(raw);
      setImportForm((current) => {
        const nextName = parsed.name ?? (current.name || titleFromFilename(file.name));
        return {
          ...current,
          skillId: current.skillId.trim() || slugFromFilename(file.name),
          name: nextName,
          description: parsed.description ?? current.description,
          markdown: parsed.markdown,
          filename: file.name
        };
      });
      setStatus(`${file.name} loaded`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    }
  };

  const submitImport = async () => {
    if (!apiKey.trim()) return;
    setImporting(true);
    setStatus("Importing skill...");
    try {
      const detail = await fetchJson<SkillDetail>("/api/skills/imports", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          skillId: importForm.skillId,
          name: importForm.name,
          description: importForm.description,
          markdown: importForm.markdown,
          filename: importForm.filename || null
        })
      });
      setSelectedSkill(detail);
      setSelectedSkillId(detail.skillId);
      setImportForm(emptyImportForm);
      setImportOpen(false);
      await refresh(detail.skillId);
      setStatus(`${detail.displayName} imported`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setImporting(false);
    }
  };

  const importDisabled = importing || !importForm.skillId.trim() || !importForm.name.trim() || !importForm.description.trim() || !importForm.markdown.trim();

  return (
    <div className="space-y-5">
      <Tabs defaultValue="skills">
        <TabsList>
          <TabsTrigger value="skills"><BrainCircuit className="mr-2 h-4 w-4" />Skills</TabsTrigger>
          <TabsTrigger value="metadata"><Boxes className="mr-2 h-4 w-4" />Metadata</TabsTrigger>
        </TabsList>

        <TabsContent value="skills">
          <div className="grid gap-5 xl:grid-cols-[380px_minmax(0,1fr)]">
            <Card>
              <CardHeader>
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <CardTitle>Diagnostic Skills</CardTitle>
                    <CardDescription>{status}</CardDescription>
                  </div>
                  <div className="flex items-center gap-2">
                    <Button className="h-8 px-3" variant="outline" onClick={() => setImportOpen((value) => !value)} aria-label={importOpen ? "Close import" : "Import skill"}>
                      {importOpen ? <X className="h-4 w-4" /> : <Upload className="h-4 w-4" />}
                    </Button>
                    <Button className="h-8 px-3" variant="outline" onClick={() => void refresh()} aria-label="Refresh skills"><RefreshCw className="h-4 w-4" /></Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-2">
                {importOpen ? (
                  <div className="mb-4 space-y-3 rounded-lg border border-border bg-slate-50 p-3">
                    <div className="grid gap-2">
                      <Input value={importForm.skillId} onChange={(event) => setImportForm({ ...importForm, skillId: event.target.value })} placeholder="Skill ID" />
                      <Input value={importForm.name} onChange={(event) => setImportForm({ ...importForm, name: event.target.value })} placeholder="Name" />
                      <Input value={importForm.description} onChange={(event) => setImportForm({ ...importForm, description: event.target.value })} placeholder="Description" />
                    </div>
                    <label className="flex min-h-24 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-white px-3 text-center text-sm text-muted-foreground transition hover:border-primary">
                      <Upload className="mb-2 h-4 w-4" />
                      {importForm.filename || "Select Markdown file"}
                      <input className="hidden" type="file" accept=".md,.markdown,text/markdown,text/plain" onChange={(event) => void handleFileSelected(event.target.files?.[0])} />
                    </label>
                    <textarea
                      className="min-h-40 w-full rounded-md border border-border bg-white px-3 py-2 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20"
                      value={importForm.markdown}
                      onChange={(event) => setImportForm({ ...importForm, markdown: event.target.value })}
                      placeholder="Markdown"
                    />
                    <div className="flex flex-wrap justify-end gap-2">
                      <Button className="h-8 px-3" variant="outline" disabled={importing} onClick={() => { setImportForm(emptyImportForm); setImportOpen(false); }}>Cancel</Button>
                      <Button className="h-8 px-3" disabled={importDisabled} onClick={() => void submitImport()}><Upload className="mr-2 h-4 w-4" />Import</Button>
                    </div>
                  </div>
                ) : null}
                {skills.length ? skills.map((skill) => (
                  <button className={`w-full rounded-lg border p-3 text-left ${selectedSkillId === skill.skillId ? "border-primary bg-slate-50" : "border-border"}`} key={skill.skillId} onClick={() => void loadSkill(skill.skillId)}>
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate text-sm font-medium">{skill.displayName}</span>
                      <Badge variant={skill.managed ? "secondary" : "outline"}>{skill.managed ? "managed" : "external"}</Badge>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">{skill.skillId} · priority {skill.priority} · rev {skill.revision.slice(0, 8)}</p>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{skill.description}</p>
                  </button>
                )) : <EmptyState>暂无 Skill。</EmptyState>}
              </CardContent>
            </Card>

            <SkillDetailPanel skill={selectedSkill} />
          </div>
        </TabsContent>

        <TabsContent value="metadata">
          <MetadataDashboard apiKey={apiKey} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function SkillDetailPanel({ skill }: { skill: SkillDetail | null }) {
  if (!skill) {
    return <Card><CardHeader><CardTitle>Skill detail</CardTitle><CardDescription>Select a Skill.</CardDescription></CardHeader><CardContent><EmptyState>暂无选中 Skill。</EmptyState></CardContent></Card>;
  }
  const tags = [
    ...skill.products.map((value) => `product:${value}`),
    ...skill.domainAdapters.map((value) => `adapter:${value}`),
    ...skill.toolIds.map((value) => `tool:${value}`),
    ...skill.taskKinds.map((value) => `task:${value}`)
  ];
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>{skill.displayName}</CardTitle>
            <CardDescription>{skill.skillId} · rev {skill.revision}</CardDescription>
          </div>
          <Badge variant={skill.includeByDefault ? "default" : "outline"}>{skill.includeByDefault ? "auto" : "explicit"}</Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">{skill.description}</p>
        <div className="flex flex-wrap gap-2">
          {tags.length ? tags.map((tag) => <Badge key={tag} variant="secondary">{tag}</Badge>) : <Badge variant="outline">no match metadata</Badge>}
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <DataBox label="Source" value={skill.sourcePath} />
          <DataBox label="Updated" value={new Date(skill.updatedAt).toLocaleString()} />
        </div>
        <div>
          <div className="mb-2 flex items-center gap-2 text-sm font-medium"><FileText className="h-4 w-4 text-primary" />References</div>
          <div className="space-y-2">
            {skill.references.length ? skill.references.map((reference) => (
              <div className="rounded-lg border border-border p-3" key={reference.referenceId}>
                <p className="text-sm font-medium">{reference.title}</p>
                <p className="mt-1 font-mono text-xs text-muted-foreground">{reference.path} · {reference.referenceId}</p>
                <p className="mt-1 text-xs text-muted-foreground">{reference.summary}</p>
              </div>
            )) : <EmptyState>该 Skill 未声明 references。</EmptyState>}
          </div>
        </div>
        <pre className="max-h-96 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{skill.injectionContent}</pre>
      </CardContent>
    </Card>
  );
}

function DataBox({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value || "-"}</p></div>;
}

function parseMarkdownImport(raw: string): { name?: string; description?: string; markdown: string } {
  const lines = raw.replace(/^\uFEFF/, "").split(/\r?\n/);
  if (lines[0]?.trim() !== "---") {
    return { markdown: raw.trim() };
  }
  const end = lines.findIndex((line, index) => index > 0 && line.trim() === "---");
  if (end < 0) {
    return { markdown: raw.trim() };
  }
  const metadata = new Map<string, string>();
  for (const line of lines.slice(1, end)) {
    const separator = line.indexOf(":");
    if (separator <= 0) continue;
    const key = line.slice(0, separator).trim();
    const value = cleanYamlScalar(line.slice(separator + 1));
    if (key && value) metadata.set(key, value);
  }
  return {
    name: metadata.get("name"),
    description: metadata.get("description"),
    markdown: lines.slice(end + 1).join("\n").trim()
  };
}

function cleanYamlScalar(value: string) {
  const trimmed = value.trim();
  if ((trimmed.startsWith("\"") && trimmed.endsWith("\"")) || (trimmed.startsWith("'") && trimmed.endsWith("'"))) {
    return trimmed.slice(1, -1).trim();
  }
  return trimmed;
}

function slugFromFilename(filename: string) {
  const base = filename.replace(/\.(markdown|md)$/i, "");
  return base.toLowerCase().replace(/[^a-z0-9_.-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 120);
}

function titleFromFilename(filename: string) {
  const base = filename.replace(/\.(markdown|md)$/i, "").replace(/[-_]+/g, " ").trim();
  return base || "Imported Skill";
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
