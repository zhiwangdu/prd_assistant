import { Boxes, BrainCircuit, Download, FileText, RefreshCw, Upload, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  downloadV2SkillsZip,
  getV2Skill,
  importV2Skill,
  listV2MetadataInstances,
  listV2Skills,
  previewV2SystemContext,
  type V2MetadataInstanceSummary,
  type V2SkillSummary,
  type V2SystemContextPreview
} from "./v2-api";

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

export function V2SystemContextBridge({ apiKey }: { apiKey: string }) {
  const [skills, setSkills] = useState<V2SkillSummary[]>([]);
  const [selectedSkillId, setSelectedSkillId] = useState("");
  const [selectedSkill, setSelectedSkill] = useState<V2SkillSummary | null>(null);
  const [selectedPreviewIds, setSelectedPreviewIds] = useState<string[]>([]);
  const [preview, setPreview] = useState<V2SystemContextPreview | null>(null);
  const [metadataInstances, setMetadataInstances] = useState<V2MetadataInstanceSummary[]>([]);
  const [status, setStatus] = useState("V2 System Context waiting to load");
  const [importOpen, setImportOpen] = useState(false);
  const [importForm, setImportForm] = useState<ImportForm>(emptyImportForm);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async (preferredSkillId?: string) => {
    if (!apiKey.trim()) {
      setSkills([]);
      setMetadataInstances([]);
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const [skillResponse, metadataResponse] = await Promise.all([
        listV2Skills(apiKey),
        listV2MetadataInstances(apiKey)
      ]);
      setSkills(skillResponse.skills);
      setMetadataInstances(metadataResponse.instances);
      const nextSkillId = preferredSkillId || selectedSkillId || skillResponse.skills[0]?.skillId || "";
      if (nextSkillId) {
        setSelectedSkillId(nextSkillId);
        setSelectedSkill(await getV2Skill(apiKey, nextSkillId));
      } else {
        setSelectedSkill(null);
      }
      setStatus(`V2 loaded ${skillResponse.skills.length} skills and ${metadataResponse.instances.length} metadata instances`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, selectedSkillId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function selectSkill(skillId: string) {
    setSelectedSkillId(skillId);
    setLoading(true);
    try {
      const detail = await getV2Skill(apiKey, skillId);
      setSelectedSkill(detail);
      setStatus(`V2 loaded ${detail.displayName}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function previewSelectedSkills() {
    setLoading(true);
    try {
      const response = await previewV2SystemContext(apiKey, selectedPreviewIds);
      setPreview(response);
      setStatus(`V2 preview contains ${response.resources.length} resources`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function submitImport() {
    if (!apiKey.trim()) return;
    setLoading(true);
    try {
      const detail = await importV2Skill(apiKey, {
        skillId: importForm.skillId,
        name: importForm.name,
        description: importForm.description,
        markdown: importForm.markdown,
        filename: importForm.filename || null
      });
      setImportForm(emptyImportForm);
      setImportOpen(false);
      setSelectedPreviewIds((current) => current.includes(detail.skillId) ? current : [...current, detail.skillId]);
      await refresh(detail.skillId);
      setStatus(`V2 imported ${detail.displayName}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function downloadSkills() {
    setLoading(true);
    try {
      await downloadV2SkillsZip(apiKey);
      setStatus("Downloaded V2 skills.zip");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function handleFileSelected(file?: File) {
    if (!file) return;
    const lower = file.name.toLowerCase();
    if (!lower.endsWith(".md") && !lower.endsWith(".markdown")) {
      setStatus("Only .md and .markdown files can be imported");
      return;
    }
    try {
      const parsed = parseMarkdownImport(await file.text());
      setImportForm((current) => ({
        ...current,
        skillId: current.skillId.trim() || slugFromFilename(file.name),
        name: (parsed.name ?? current.name) || titleFromFilename(file.name),
        description: parsed.description ?? current.description,
        markdown: parsed.markdown,
        filename: file.name
      }));
      setStatus(`${file.name} loaded`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    }
  }

  const importDisabled = loading || !importForm.skillId.trim() || !importForm.name.trim() || !importForm.description.trim() || !importForm.markdown.trim();

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <BrainCircuit className="h-5 w-5 text-primary" />
              <CardTitle>V2 System Context Bridge</CardTitle>
            </div>
            <CardDescription>V2 Diagnostic Skills, System Context preview, skills export, and Metadata instance summary</CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refresh()}><RefreshCw className="mr-2 h-4 w-4" />刷新</Button>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void downloadSkills()}><Download className="mr-2 h-4 w-4" />skills.zip</Button>
            <Button className="h-8 px-3" disabled={loading} variant="outline" onClick={() => setImportOpen((value) => !value)}>{importOpen ? <X className="h-4 w-4" /> : <Upload className="h-4 w-4" />}</Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        {importOpen ? (
          <div className="space-y-3 rounded-lg border border-border bg-slate-50 p-4">
            <div className="grid gap-3 md:grid-cols-3">
              <Input value={importForm.skillId} onChange={(event) => setImportForm({ ...importForm, skillId: event.target.value })} placeholder="Skill ID" />
              <Input value={importForm.name} onChange={(event) => setImportForm({ ...importForm, name: event.target.value })} placeholder="Name" />
              <Input value={importForm.description} onChange={(event) => setImportForm({ ...importForm, description: event.target.value })} placeholder="Description" />
            </div>
            <label className="flex min-h-20 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-white px-3 text-center text-sm text-muted-foreground transition hover:border-primary">
              <Upload className="mb-2 h-4 w-4" />
              {importForm.filename || "Select Markdown file"}
              <input className="hidden" type="file" accept=".md,.markdown,text/markdown,text/plain" onChange={(event) => void handleFileSelected(event.target.files?.[0])} />
            </label>
            <textarea className="min-h-40 w-full rounded-md border border-border bg-white px-3 py-2 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" value={importForm.markdown} onChange={(event) => setImportForm({ ...importForm, markdown: event.target.value })} placeholder="Markdown" />
            <div className="flex justify-end gap-2">
              <Button className="h-8 px-3" disabled={loading} variant="outline" onClick={() => { setImportForm(emptyImportForm); setImportOpen(false); }}>Cancel</Button>
              <Button className="h-8 px-3" disabled={importDisabled} onClick={() => void submitImport()}><Upload className="mr-2 h-4 w-4" />Import</Button>
            </div>
          </div>
        ) : null}

        <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)_420px]">
          <div className="rounded-lg border border-border p-3">
            <h3 className="mb-3 text-sm font-semibold">V2 Skills</h3>
            <div className="max-h-[480px] space-y-2 overflow-auto">
              {skills.length ? skills.map((skill) => {
                const checked = selectedPreviewIds.includes(skill.skillId);
                return (
                  <div className={`rounded-lg border p-3 ${selectedSkillId === skill.skillId ? "border-primary bg-slate-50" : "border-border"}`} key={skill.skillId}>
                    <button className="w-full text-left" onClick={() => void selectSkill(skill.skillId)}>
                      <div className="flex items-start justify-between gap-2">
                        <p className="text-sm font-medium">{skill.displayName}</p>
                        <Badge variant={skill.includeByDefault ? "success" : "secondary"}>{skill.includeByDefault ? "auto" : "explicit"}</Badge>
                      </div>
                      <p className="mt-1 font-mono text-xs text-muted-foreground">{skill.skillId} · rev {skill.revision.slice(0, 8)}</p>
                      <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{skill.description}</p>
                    </button>
                    <label className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
                      <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={checked} onChange={() => setSelectedPreviewIds(toggleString(selectedPreviewIds, skill.skillId))} />
                      Include in preview
                    </label>
                  </div>
                );
              }) : <EmptyState>No V2 skills.</EmptyState>}
            </div>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            {selectedSkill ? <V2SkillDetail skill={selectedSkill} /> : <EmptyState>Select a V2 skill.</EmptyState>}
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span className="text-sm text-muted-foreground">{status}</span>
              <Button disabled={loading || !apiKey.trim()} onClick={() => void previewSelectedSkills()}>Preview System Context</Button>
            </div>
            {preview ? (
              <div>
                <h3 className="mb-2 text-sm font-semibold">Preview resources</h3>
                <div className="space-y-2">
                  {preview.resources.length ? preview.resources.map((resource, index) => (
                    <div className="rounded-lg border border-border p-3" key={`${resource.skillId}:${index}`}>
                      <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">{resource.kind}</Badge>
                        <span className="text-sm font-medium">{resource.skillId ?? "resource"}</span>
                        <span className="text-xs text-muted-foreground">{resource.selectionReason ?? "-"} · score {resource.matchScore ?? 0}</span>
                      </div>
                      <p className="mt-1 text-xs text-muted-foreground">{resource.summary ?? "-"}</p>
                    </div>
                  )) : <EmptyState>No resources selected.</EmptyState>}
                </div>
              </div>
            ) : null}
          </div>

          <div className="rounded-lg border border-border p-4">
            <div className="mb-3 flex items-center gap-2">
              <Boxes className="h-5 w-5 text-primary" />
              <h3 className="text-sm font-semibold">V2 Metadata instances</h3>
            </div>
            <div className="max-h-[520px] space-y-2 overflow-auto">
              {metadataInstances.length ? metadataInstances.map((instance) => (
                <div className="rounded-lg border border-border p-3" key={instance.instanceId}>
                  <p className="break-all text-sm font-medium">{instance.instanceId}</p>
                  <p className="mt-1 text-xs text-muted-foreground">{instance.product ?? "-"} {instance.version ?? ""} · {instance.environment ?? "-"} · {instance.templateType}</p>
                  <p className="mt-1 text-xs text-muted-foreground">{instance.nodeCount} nodes · {instance.databaseCount} DBs · {instance.remark ?? "no remark"}</p>
                </div>
              )) : <EmptyState>No V2 metadata instances.</EmptyState>}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function V2SkillDetail({ skill }: { skill: V2SkillSummary }) {
  const tags = [
    ...skill.products.map((value) => `product:${value}`),
    ...skill.domainAdapters.map((value) => `adapter:${value}`),
    ...skill.toolIds.map((value) => `tool:${value}`),
    ...skill.taskKinds.map((value) => `task:${value}`),
    ...skill.keywords.map((value) => `keyword:${value}`)
  ];
  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold">{skill.displayName}</h3>
        <p className="mt-1 font-mono text-xs text-muted-foreground">{skill.skillId} · {skill.sourcePath}</p>
      </div>
      <p className="text-sm text-muted-foreground">{skill.description}</p>
      <div className="flex flex-wrap gap-2">
        {tags.length ? tags.map((tag) => <Badge key={tag} variant="secondary">{tag}</Badge>) : <Badge variant="outline">no match metadata</Badge>}
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
          )) : <EmptyState>No references.</EmptyState>}
        </div>
      </div>
      {skill.content ? <pre className="max-h-72 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{skill.content}</pre> : null}
    </div>
  );
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

function toggleString(values: string[], value: string) {
  return values.includes(value) ? values.filter((item) => item !== value) : [...values, value];
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
