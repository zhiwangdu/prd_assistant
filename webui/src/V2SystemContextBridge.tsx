import { BookOpenCheck, Boxes, BrainCircuit, CheckCircle2, Download, FileText, Layers, Plus, RefreshCw, Save, Upload, X } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  activateV2SystemContextVersion,
  createV2SystemContextResource,
  createV2SystemContextVersion,
  downloadV2SkillsZip,
  getV2SystemContextResource,
  getV2Skill,
  importV2Skill,
  listV2SystemContextResources,
  listV2MetadataInstances,
  listV2Skills,
  patchV2SystemContextResource,
  previewV2SystemContext,
  previewV2SystemContextResources,
  type V2MetadataInstanceSummary,
  type V2SkillSummary,
  type V2SystemContextContentType,
  type V2SystemContextPromptPolicy,
  type V2SystemContextResource,
  type V2SystemContextResourceKind,
  type V2SystemContextResourcePreview,
  type V2SystemContextResourceSummary,
  type V2SystemContextScope,
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

type ResourceForm = {
  kind: Exclude<V2SystemContextResourceKind, "metadata_instance">;
  title: string;
  description: string;
  scope: V2SystemContextScope;
  enabled: boolean;
  tagsText: string;
  product: string;
  version: string;
  environment: string;
  contentType: V2SystemContextContentType;
  content: string;
  summary: string;
  includeByDefault: boolean;
  maxChars: string;
  priority: string;
  allowLogAnalysis: boolean;
  allowToolRun: boolean;
  activate: boolean;
};

type PreviewFilterForm = {
  taskKind: "log_analysis" | "tool_run";
  product: string;
  version: string;
  environment: string;
  instanceId: string;
};

const resourceKinds: Array<Exclude<V2SystemContextResourceKind, "metadata_instance">> = [
  "prompt_pack",
  "architecture_doc",
  "runbook",
  "glossary",
  "tool_capability",
  "knowledge_note",
  "diagnostic_skill"
];

const scopes: V2SystemContextScope[] = ["log_analysis", "global", "tool_run", "case_import"];
const contentTypes: V2SystemContextContentType[] = ["markdown", "plain_text", "json"];

const emptyResourceForm: ResourceForm = {
  kind: "runbook",
  title: "",
  description: "",
  scope: "log_analysis",
  enabled: true,
  tagsText: "",
  product: "",
  version: "",
  environment: "",
  contentType: "markdown",
  content: "",
  summary: "",
  includeByDefault: false,
  maxChars: "4000",
  priority: "0",
  allowLogAnalysis: false,
  allowToolRun: false,
  activate: true
};

const emptyPreviewFilter: PreviewFilterForm = {
  taskKind: "log_analysis",
  product: "",
  version: "",
  environment: "",
  instanceId: ""
};

export function V2SystemContextBridge({ apiKey }: { apiKey: string }) {
  const [skills, setSkills] = useState<V2SkillSummary[]>([]);
  const [selectedSkillId, setSelectedSkillId] = useState("");
  const [selectedSkill, setSelectedSkill] = useState<V2SkillSummary | null>(null);
  const [selectedPreviewIds, setSelectedPreviewIds] = useState<string[]>([]);
  const [preview, setPreview] = useState<V2SystemContextPreview | null>(null);
  const [resources, setResources] = useState<V2SystemContextResourceSummary[]>([]);
  const [selectedResourceId, setSelectedResourceId] = useState("");
  const [selectedResource, setSelectedResource] = useState<V2SystemContextResource | null>(null);
  const [selectedResourcePreviewIds, setSelectedResourcePreviewIds] = useState<string[]>([]);
  const [resourcePreview, setResourcePreview] = useState<V2SystemContextResourcePreview | null>(null);
  const [resourceForm, setResourceForm] = useState<ResourceForm>(emptyResourceForm);
  const [versionForm, setVersionForm] = useState<ResourceForm>(emptyResourceForm);
  const [previewFilter, setPreviewFilter] = useState<PreviewFilterForm>(emptyPreviewFilter);
  const [metadataInstances, setMetadataInstances] = useState<V2MetadataInstanceSummary[]>([]);
  const [status, setStatus] = useState("V2 System Context waiting to load");
  const [importOpen, setImportOpen] = useState(false);
  const [resourceCreateOpen, setResourceCreateOpen] = useState(false);
  const [importForm, setImportForm] = useState<ImportForm>(emptyImportForm);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async (preferredSkillId?: string) => {
    if (!apiKey.trim()) {
      setSkills([]);
      setResources([]);
      setMetadataInstances([]);
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const [skillResponse, resourceResponse, metadataResponse] = await Promise.all([
        listV2Skills(apiKey),
        listV2SystemContextResources(apiKey),
        listV2MetadataInstances(apiKey)
      ]);
      setSkills(skillResponse.skills);
      setResources(resourceResponse.resources);
      setMetadataInstances(metadataResponse.instances);
      const nextSkillId = preferredSkillId || selectedSkillId || skillResponse.skills[0]?.skillId || "";
      if (nextSkillId) {
        setSelectedSkillId(nextSkillId);
        setSelectedSkill(await getV2Skill(apiKey, nextSkillId));
      } else {
        setSelectedSkill(null);
      }
      const nextResourceId = selectedResourceId || resourceResponse.resources.find((item) => item.source === "system_context")?.contextId || "";
      const nextResource = resourceResponse.resources.find((item) => item.contextId === nextResourceId);
      if (nextResourceId && nextResource?.source === "system_context") {
        setSelectedResourceId(nextResourceId);
        const detail = await getV2SystemContextResource(apiKey, nextResourceId);
        setSelectedResource(detail);
        setVersionForm(formFromActiveResource(detail));
      } else {
        setSelectedResource(null);
      }
      setStatus(`V2 loaded ${skillResponse.skills.length} skills, ${resourceResponse.resources.length} resources, and ${metadataResponse.instances.length} metadata instances`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, selectedResourceId, selectedSkillId]);

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

  async function selectResource(resourceId: string) {
    setSelectedResourceId(resourceId);
    const summary = resources.find((item) => item.contextId === resourceId);
    if (summary?.source !== "system_context") {
      setSelectedResource(null);
      setVersionForm(emptyResourceForm);
      setStatus(`Selected read-only ${summary?.title ?? resourceId}`);
      return;
    }
    setLoading(true);
    try {
      const detail = await getV2SystemContextResource(apiKey, resourceId);
      setSelectedResource(detail);
      setVersionForm(formFromActiveResource(detail));
      setStatus(`Loaded ${detail.title}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function reloadResourceSummaries() {
    const response = await listV2SystemContextResources(apiKey);
    setResources(response.resources);
  }

  async function createResource() {
    if (!apiKey.trim()) return;
    setLoading(true);
    try {
      const detail = await createV2SystemContextResource(apiKey, resourcePayload(resourceForm));
      setSelectedResourceId(detail.contextId);
      setSelectedResource(detail);
      setVersionForm(formFromActiveResource(detail));
      setSelectedResourcePreviewIds((current) => current.includes(detail.contextId) ? current : [...current, detail.contextId]);
      setResourceForm(emptyResourceForm);
      setResourceCreateOpen(false);
      await reloadResourceSummaries();
      setStatus(`Created ${detail.title}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function saveResourceDetails() {
    if (!selectedResource || !apiKey.trim()) return;
    setLoading(true);
    try {
      const detail = await patchV2SystemContextResource(apiKey, selectedResource.contextId, resourcePatchFromDetail(selectedResource));
      setSelectedResource(detail);
      setVersionForm(formFromActiveResource(detail));
      await reloadResourceSummaries();
      setStatus(`Saved ${detail.title}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function appendVersion() {
    if (!selectedResource || !apiKey.trim()) return;
    setLoading(true);
    try {
      const detail = await createV2SystemContextVersion(apiKey, selectedResource.contextId, versionPayload(versionForm));
      setSelectedResource(detail);
      setVersionForm(formFromActiveResource(detail));
      await reloadResourceSummaries();
      setStatus(`Added revision ${detail.versions.at(-1)?.revision ?? "-"}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function activateVersion(versionId: string) {
    if (!selectedResource || !apiKey.trim()) return;
    setLoading(true);
    try {
      const detail = await activateV2SystemContextVersion(apiKey, selectedResource.contextId, versionId);
      setSelectedResource(detail);
      setVersionForm(formFromActiveResource(detail));
      await reloadResourceSummaries();
      setStatus(`Activated ${versionId}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function previewSelectedResources() {
    setLoading(true);
    try {
      const response = await previewV2SystemContextResources(apiKey, {
        contextIds: selectedResourcePreviewIds,
        taskKind: previewFilter.taskKind,
        product: optionalText(previewFilter.product),
        version: optionalText(previewFilter.version),
        environment: optionalText(previewFilter.environment),
        instanceId: optionalText(previewFilter.instanceId)
      });
      setResourcePreview(response);
      setStatus(`V2 resource preview contains ${response.resources.length} resources`);
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
  const resourceCreateDisabled = loading || !resourceForm.title.trim() || !resourceForm.content.trim();
  const appendVersionDisabled = loading || !selectedResource || !versionForm.content.trim();
  const systemResources = resources.filter((item) => item.source === "system_context");
  const metadataResourceSummaries = resources.filter((item) => item.source === "metadata_adapter");

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <BrainCircuit className="h-5 w-5 text-primary" />
              <CardTitle>V2 System Context Workbench</CardTitle>
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

        <div className="space-y-4 rounded-lg border border-border p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                <BookOpenCheck className="h-5 w-5 text-primary" />
                <h3 className="text-sm font-semibold">V2 Compatibility resources</h3>
              </div>
              <p className="mt-1 text-xs text-muted-foreground">{systemResources.length} managed resources · {metadataResourceSummaries.length} metadata adapters</p>
            </div>
            <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => setResourceCreateOpen((value) => !value)}>
              {resourceCreateOpen ? <X className="h-4 w-4" /> : <Plus className="mr-2 h-4 w-4" />}
              {resourceCreateOpen ? null : "Resource"}
            </Button>
          </div>

          {resourceCreateOpen ? (
            <ResourceCreateForm
              disabled={resourceCreateDisabled}
              form={resourceForm}
              loading={loading}
              onCancel={() => { setResourceForm(emptyResourceForm); setResourceCreateOpen(false); }}
              onChange={setResourceForm}
              onSubmit={() => void createResource()}
            />
          ) : null}

          <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)_420px]">
            <div className="rounded-lg border border-border p-3">
              <div className="mb-3 flex items-center gap-2">
                <Layers className="h-4 w-4 text-primary" />
                <h4 className="text-sm font-semibold">Resources</h4>
              </div>
              <div className="max-h-[560px] space-y-2 overflow-auto">
                {resources.length ? resources.map((resource) => {
                  const checked = selectedResourcePreviewIds.includes(resource.contextId);
                  return (
                    <div className={`rounded-lg border p-3 ${selectedResourceId === resource.contextId ? "border-primary bg-slate-50" : "border-border"}`} key={resource.contextId}>
                      <button className="w-full text-left" onClick={() => void selectResource(resource.contextId)}>
                        <div className="flex items-start justify-between gap-2">
                          <p className="break-words text-sm font-medium">{resource.title}</p>
                          <Badge variant={resource.enabled ? "secondary" : "destructive"}>{resource.enabled ? resource.kind : "disabled"}</Badge>
                        </div>
                        <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{resource.contextId}</p>
                        <p className="mt-1 text-xs text-muted-foreground">{resource.source} · {resource.scope} · {resource.contentType ?? "-"}</p>
                        {resource.activeSummary ? <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{resource.activeSummary}</p> : null}
                      </button>
                      <label className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
                        <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={checked} onChange={() => setSelectedResourcePreviewIds(toggleString(selectedResourcePreviewIds, resource.contextId))} />
                        Include in resource preview
                      </label>
                    </div>
                  );
                }) : <EmptyState>No V2 compatibility resources.</EmptyState>}
              </div>
            </div>

            <ResourceDetailPanel
              appendDisabled={appendVersionDisabled}
              loading={loading}
              onActivate={(versionId) => void activateVersion(versionId)}
              onAppend={() => void appendVersion()}
              onChangeResource={setSelectedResource}
              onChangeVersion={setVersionForm}
              onSave={() => void saveResourceDetails()}
              resource={selectedResource}
              selectedSummary={resources.find((item) => item.contextId === selectedResourceId) ?? null}
              versionForm={versionForm}
            />

            <ResourcePreviewPanel
              disabled={loading || !apiKey.trim()}
              filter={previewFilter}
              onChangeFilter={setPreviewFilter}
              onPreview={() => void previewSelectedResources()}
              preview={resourcePreview}
              selectedCount={selectedResourcePreviewIds.length}
            />
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

function ResourceCreateForm({ disabled, form, loading, onCancel, onChange, onSubmit }: {
  disabled: boolean;
  form: ResourceForm;
  loading: boolean;
  onCancel: () => void;
  onChange: (form: ResourceForm) => void;
  onSubmit: () => void;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-border bg-slate-50 p-4">
      <div className="grid gap-3 lg:grid-cols-4">
        <SelectBox value={form.kind} onChange={(value) => onChange({ ...form, kind: value as ResourceForm["kind"] })}>
          {resourceKinds.map((kind) => <option key={kind} value={kind}>{kind}</option>)}
        </SelectBox>
        <Input value={form.title} onChange={(event) => onChange({ ...form, title: event.target.value })} placeholder="Title" />
        <SelectBox value={form.scope} onChange={(value) => onChange({ ...form, scope: value as V2SystemContextScope })}>
          {scopes.map((scope) => <option key={scope} value={scope}>{scope}</option>)}
        </SelectBox>
        <SelectBox value={form.contentType} onChange={(value) => onChange({ ...form, contentType: value as V2SystemContextContentType })}>
          {contentTypes.map((contentType) => <option key={contentType} value={contentType}>{contentType}</option>)}
        </SelectBox>
      </div>
      <div className="grid gap-3 lg:grid-cols-4">
        <Input value={form.product} onChange={(event) => onChange({ ...form, product: event.target.value })} placeholder="Product" />
        <Input value={form.version} onChange={(event) => onChange({ ...form, version: event.target.value })} placeholder="Version" />
        <Input value={form.environment} onChange={(event) => onChange({ ...form, environment: event.target.value })} placeholder="Environment" />
        <Input value={form.tagsText} onChange={(event) => onChange({ ...form, tagsText: event.target.value })} placeholder="tags, comma separated" />
      </div>
      <Input value={form.description} onChange={(event) => onChange({ ...form, description: event.target.value })} placeholder="Description" />
      <textarea className="min-h-36 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" spellCheck={false} value={form.content} onChange={(event) => onChange({ ...form, content: event.target.value })} placeholder="Resource content" />
      <Input value={form.summary} onChange={(event) => onChange({ ...form, summary: event.target.value })} placeholder="Summary" />
      <PromptPolicyEditor form={form} onChange={onChange} />
      <div className="flex flex-wrap items-center justify-between gap-3">
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={form.enabled} onChange={(event) => onChange({ ...form, enabled: event.target.checked })} />
          Enabled
        </label>
        <div className="flex gap-2">
          <Button className="h-8 px-3" disabled={loading} variant="outline" onClick={onCancel}>Cancel</Button>
          <Button className="h-8 px-3" disabled={disabled} onClick={onSubmit}><Plus className="mr-2 h-4 w-4" />Create</Button>
        </div>
      </div>
    </div>
  );
}

function ResourceDetailPanel({ appendDisabled, loading, onActivate, onAppend, onChangeResource, onChangeVersion, onSave, resource, selectedSummary, versionForm }: {
  appendDisabled: boolean;
  loading: boolean;
  onActivate: (versionId: string) => void;
  onAppend: () => void;
  onChangeResource: (resource: V2SystemContextResource | null) => void;
  onChangeVersion: (form: ResourceForm) => void;
  onSave: () => void;
  resource: V2SystemContextResource | null;
  selectedSummary: V2SystemContextResourceSummary | null;
  versionForm: ResourceForm;
}) {
  if (!selectedSummary) {
    return (
      <div className="rounded-lg border border-border p-4">
        <EmptyState>Select a resource.</EmptyState>
      </div>
    );
  }
  if (!resource) {
    return (
      <div className="rounded-lg border border-border p-4">
        <div className="mb-3 flex items-center gap-2">
          <Boxes className="h-5 w-5 text-primary" />
          <h4 className="text-sm font-semibold">{selectedSummary.title}</h4>
        </div>
        <div className="space-y-2 text-xs text-muted-foreground">
          <p className="break-all font-mono">{selectedSummary.contextId}</p>
          <p>{selectedSummary.kind} · {selectedSummary.scope} · {selectedSummary.source}</p>
          <p>{selectedSummary.description ?? selectedSummary.activeSummary ?? "-"}</p>
        </div>
      </div>
    );
  }
  return (
    <div className="space-y-4 rounded-lg border border-border p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h4 className="text-sm font-semibold">{resource.title}</h4>
          <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{resource.contextId}</p>
        </div>
        <Button className="h-8 px-3" disabled={loading || !resource.title.trim()} onClick={onSave}><Save className="mr-2 h-4 w-4" />Save</Button>
      </div>
      <div className="grid gap-3 lg:grid-cols-3">
        <Input value={resource.title} onChange={(event) => onChangeResource({ ...resource, title: event.target.value })} placeholder="Title" />
        <SelectBox value={resource.scope} onChange={(value) => onChangeResource({ ...resource, scope: value as V2SystemContextScope })}>
          {scopes.map((scope) => <option key={scope} value={scope}>{scope}</option>)}
        </SelectBox>
        <Input value={resource.tags.join(", ")} onChange={(event) => onChangeResource({ ...resource, tags: splitTags(event.target.value) })} placeholder="tags, comma separated" />
      </div>
      <div className="grid gap-3 lg:grid-cols-3">
        <Input value={resource.product ?? ""} onChange={(event) => onChangeResource({ ...resource, product: event.target.value })} placeholder="Product" />
        <Input value={resource.version ?? ""} onChange={(event) => onChangeResource({ ...resource, version: event.target.value })} placeholder="Version" />
        <Input value={resource.environment ?? ""} onChange={(event) => onChangeResource({ ...resource, environment: event.target.value })} placeholder="Environment" />
      </div>
      <Input value={resource.description ?? ""} onChange={(event) => onChangeResource({ ...resource, description: event.target.value })} placeholder="Description" />
      <label className="flex items-center gap-2 text-sm text-muted-foreground">
        <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={resource.enabled} onChange={(event) => onChangeResource({ ...resource, enabled: event.target.checked })} />
        Enabled
      </label>

      <div className="space-y-3 rounded-lg border border-border bg-slate-50 p-3">
        <div className="flex items-center gap-2">
          <FileText className="h-4 w-4 text-primary" />
          <h5 className="text-sm font-semibold">New version</h5>
        </div>
        <div className="grid gap-3 lg:grid-cols-[180px_minmax(0,1fr)]">
          <SelectBox value={versionForm.contentType} onChange={(value) => onChangeVersion({ ...versionForm, contentType: value as V2SystemContextContentType })}>
            {contentTypes.map((contentType) => <option key={contentType} value={contentType}>{contentType}</option>)}
          </SelectBox>
          <Input value={versionForm.summary} onChange={(event) => onChangeVersion({ ...versionForm, summary: event.target.value })} placeholder="Version summary" />
        </div>
        <textarea className="min-h-36 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" spellCheck={false} value={versionForm.content} onChange={(event) => onChangeVersion({ ...versionForm, content: event.target.value })} placeholder="Version content" />
        <PromptPolicyEditor form={versionForm} onChange={onChangeVersion} />
        <div className="flex flex-wrap items-center justify-between gap-3">
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={versionForm.activate} onChange={(event) => onChangeVersion({ ...versionForm, activate: event.target.checked })} />
            Activate after append
          </label>
          <Button className="h-8 px-3" disabled={appendDisabled} onClick={onAppend}><Plus className="mr-2 h-4 w-4" />Append version</Button>
        </div>
      </div>

      <div>
        <h5 className="mb-2 text-sm font-semibold">Versions</h5>
        <div className="space-y-2">
          {resource.versions.map((version) => (
            <div className="rounded-lg border border-border p-3" key={version.versionId}>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant={version.status === "active" ? "success" : "secondary"}>rev {version.revision}</Badge>
                  <span className="font-mono text-xs text-muted-foreground">{version.versionId}</span>
                </div>
                {version.status === "active" ? <CheckCircle2 className="h-4 w-4 text-emerald-600" /> : <Button className="h-8 px-3" disabled={loading} variant="outline" onClick={() => onActivate(version.versionId)}>Activate</Button>}
              </div>
              <p className="mt-2 text-xs text-muted-foreground">{version.contentType} · {version.summary ?? "-"}</p>
              <pre className="mt-2 max-h-28 overflow-auto rounded-md bg-slate-50 p-2 text-xs">{version.content}</pre>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function ResourcePreviewPanel({ disabled, filter, onChangeFilter, onPreview, preview, selectedCount }: {
  disabled: boolean;
  filter: PreviewFilterForm;
  onChangeFilter: (form: PreviewFilterForm) => void;
  onPreview: () => void;
  preview: V2SystemContextResourcePreview | null;
  selectedCount: number;
}) {
  return (
    <div className="space-y-4 rounded-lg border border-border p-4">
      <div className="flex items-center gap-2">
        <BrainCircuit className="h-5 w-5 text-primary" />
        <h4 className="text-sm font-semibold">Resource preview</h4>
      </div>
      <div className="grid gap-3">
        <SelectBox value={filter.taskKind} onChange={(value) => onChangeFilter({ ...filter, taskKind: value as PreviewFilterForm["taskKind"] })}>
          <option value="log_analysis">log_analysis</option>
          <option value="tool_run">tool_run</option>
        </SelectBox>
        <Input value={filter.product} onChange={(event) => onChangeFilter({ ...filter, product: event.target.value })} placeholder="Product filter" />
        <Input value={filter.version} onChange={(event) => onChangeFilter({ ...filter, version: event.target.value })} placeholder="Version filter" />
        <Input value={filter.environment} onChange={(event) => onChangeFilter({ ...filter, environment: event.target.value })} placeholder="Environment filter" />
        <Input value={filter.instanceId} onChange={(event) => onChangeFilter({ ...filter, instanceId: event.target.value })} placeholder="Metadata instance ID" />
      </div>
      <Button disabled={disabled} onClick={onPreview}>Preview {selectedCount} selected</Button>
      {preview ? (
        <div className="space-y-3">
          <div className="space-y-2">
            {preview.resources.length ? preview.resources.map((resource) => (
              <div className="rounded-lg border border-border p-3" key={`${resource.contextId}:${resource.versionId ?? "meta"}`}>
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="secondary">{resource.kind}</Badge>
                  <span className="break-words text-sm font-medium">{resource.title}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{resource.source} · priority {resource.promptPriority ?? 0} · {resource.summary ?? "-"}</p>
              </div>
            )) : <EmptyState>No resources selected.</EmptyState>}
          </div>
          <pre className="max-h-72 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{preview.prompt}</pre>
        </div>
      ) : null}
    </div>
  );
}

function PromptPolicyEditor({ form, onChange }: { form: ResourceForm; onChange: (form: ResourceForm) => void }) {
  return (
    <div className="grid gap-3 lg:grid-cols-[1fr_120px_120px]">
      <div className="flex flex-wrap items-center gap-4 rounded-md border border-border bg-white px-3 py-2 text-sm text-muted-foreground">
        <label className="flex items-center gap-2">
          <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={form.includeByDefault} onChange={(event) => onChange({ ...form, includeByDefault: event.target.checked })} />
          Include by default
        </label>
        <label className="flex items-center gap-2">
          <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={form.allowLogAnalysis} onChange={(event) => onChange({ ...form, allowLogAnalysis: event.target.checked })} />
          log_analysis
        </label>
        <label className="flex items-center gap-2">
          <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={form.allowToolRun} onChange={(event) => onChange({ ...form, allowToolRun: event.target.checked })} />
          tool_run
        </label>
      </div>
      <Input value={form.maxChars} onChange={(event) => onChange({ ...form, maxChars: event.target.value })} placeholder="Max chars" />
      <Input value={form.priority} onChange={(event) => onChange({ ...form, priority: event.target.value })} placeholder="Priority" />
    </div>
  );
}

function SelectBox({ children, onChange, value }: { children: ReactNode; onChange: (value: string) => void; value: string }) {
  return (
    <div className="h-10 rounded-md border border-border bg-white px-3 text-sm focus-within:ring-2 focus-within:ring-teal-600/20">
      <select className="h-full w-full bg-transparent outline-none" value={value} onChange={(event) => onChange(event.target.value)}>
        {children}
      </select>
    </div>
  );
}

function resourcePayload(form: ResourceForm) {
  return {
    kind: form.kind,
    title: form.title.trim(),
    description: optionalText(form.description),
    scope: form.scope,
    enabled: form.enabled,
    tags: splitTags(form.tagsText),
    product: optionalText(form.product),
    version: optionalText(form.version),
    environment: optionalText(form.environment),
    contentType: form.contentType,
    content: form.content,
    summary: optionalText(form.summary),
    promptPolicy: policyFromForm(form)
  };
}

function versionPayload(form: ResourceForm) {
  return {
    contentType: form.contentType,
    content: form.content,
    summary: optionalText(form.summary),
    promptPolicy: policyFromForm(form),
    activate: form.activate
  };
}

function resourcePatchFromDetail(resource: V2SystemContextResource) {
  return {
    title: resource.title.trim(),
    description: optionalText(resource.description ?? ""),
    scope: resource.scope,
    enabled: resource.enabled,
    tags: resource.tags,
    product: optionalText(resource.product ?? ""),
    version: optionalText(resource.version ?? ""),
    environment: optionalText(resource.environment ?? "")
  };
}

function formFromActiveResource(resource: V2SystemContextResource): ResourceForm {
  const version = resource.versions.find((item) => item.versionId === resource.activeVersionId) ?? resource.versions[0];
  const policy = normalizePromptPolicy(version?.promptPolicy);
  return {
    ...emptyResourceForm,
    kind: resource.kind as ResourceForm["kind"],
    title: resource.title,
    description: resource.description ?? "",
    scope: resource.scope,
    enabled: resource.enabled,
    tagsText: resource.tags.join(", "),
    product: resource.product ?? "",
    version: resource.version ?? "",
    environment: resource.environment ?? "",
    contentType: version?.contentType ?? "markdown",
    content: version?.content ?? "",
    summary: version?.summary ?? "",
    includeByDefault: policy.includeByDefault,
    maxChars: String(policy.maxChars),
    priority: String(policy.priority),
    allowLogAnalysis: policy.allowedTaskKinds.includes("log_analysis"),
    allowToolRun: policy.allowedTaskKinds.includes("tool_run"),
    activate: true
  };
}

function policyFromForm(form: ResourceForm): V2SystemContextPromptPolicy {
  const maxChars = Number.parseInt(form.maxChars, 10);
  const priority = Number.parseInt(form.priority, 10);
  const allowedTaskKinds: V2SystemContextPromptPolicy["allowedTaskKinds"] = [];
  if (form.allowLogAnalysis) allowedTaskKinds.push("log_analysis");
  if (form.allowToolRun) allowedTaskKinds.push("tool_run");
  return {
    includeByDefault: form.includeByDefault,
    maxChars: Number.isFinite(maxChars) ? Math.min(20000, Math.max(200, maxChars)) : 4000,
    priority: Number.isFinite(priority) ? priority : 0,
    allowedTaskKinds
  };
}

function normalizePromptPolicy(policy?: Partial<V2SystemContextPromptPolicy> | null): V2SystemContextPromptPolicy {
  return {
    includeByDefault: policy?.includeByDefault ?? true,
    maxChars: policy?.maxChars ?? 4000,
    priority: policy?.priority ?? 0,
    allowedTaskKinds: policy?.allowedTaskKinds ?? []
  };
}

function optionalText(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function splitTags(value: string) {
  return Array.from(new Set(value.split(/[,\n]/).map((item) => item.trim()).filter(Boolean))).slice(0, 32);
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
