import { BookOpenCheck, CheckCircle2, FileText, MessageSquare, RefreshCw, Save, Search, Send, UploadCloud, XCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  appendV2CaseImportMessage,
  confirmV2CaseImport,
  previewV2CaseImport,
  searchV2Cases,
  updateV2Case,
  type V2CaseDraft,
  type V2CaseHit,
  type V2CaseImport,
  type V2CaseRecord
} from "./v2-api";

type UiCaseDraft = {
  title: string;
  symptom: string;
  rootCause: string;
  solution: string;
  product: string;
  version: string;
  environment: string;
  instanceId: string;
  nodeId: string;
  evidenceRefsText: string;
};

const EMPTY_DRAFT: UiCaseDraft = {
  title: "",
  symptom: "",
  rootCause: "",
  solution: "",
  product: "",
  version: "",
  environment: "",
  instanceId: "",
  nodeId: "",
  evidenceRefsText: ""
};

export function V2MemoryBridge({ apiKey }: { apiKey: string }) {
  const [query, setQuery] = useState("");
  const [includeDisabled, setIncludeDisabled] = useState(true);
  const [cases, setCases] = useState<V2CaseHit[]>([]);
  const [selectedCase, setSelectedCase] = useState<V2CaseRecord | V2CaseHit | null>(null);
  const [editDraft, setEditDraft] = useState<UiCaseDraft>(EMPTY_DRAFT);
  const [sourceText, setSourceText] = useState("");
  const [sourceFile, setSourceFile] = useState<File | null>(null);
  const [caseImport, setCaseImport] = useState<V2CaseImport | null>(null);
  const [importDraft, setImportDraft] = useState<UiCaseDraft>(EMPTY_DRAFT);
  const [importMessage, setImportMessage] = useState("");
  const [status, setStatus] = useState("V2 Memory 等待加载");
  const [loading, setLoading] = useState(false);

  const canPreview = Boolean(apiKey.trim() && (sourceText.trim() || sourceFile));
  const importReady = hasRequiredFields(importDraft);
  const selectedSummary = useMemo(() => {
    if (!selectedCase) return "选择一个 V2 Case 查看详情";
    return `${selectedCase.caseId} · ${selectedCase.sourceType} · ${selectedCase.enabled ? "enabled" : "disabled"}`;
  }, [selectedCase]);

  const refreshCases = useCallback(async () => {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const response = await searchV2Cases(apiKey, { query, includeDisabled, limit: 50 });
      setCases(response.cases);
      setStatus(`V2 loaded ${response.cases.length} cases`);
      if (selectedCase && !response.cases.some((item) => item.caseId === selectedCase.caseId)) {
        setSelectedCase(null);
      }
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, includeDisabled, query, selectedCase]);

  useEffect(() => {
    void refreshCases();
  }, [refreshCases]);

  async function previewImport() {
    if (!canPreview) return;
    setLoading(true);
    try {
      const content = sourceFile ? await sourceFile.text() : sourceText;
      const response = await previewV2CaseImport(apiKey, { content, filename: sourceFile?.name ?? null });
      setCaseImport(response.import);
      setImportDraft(fromV2Draft(response.import.draft));
      setImportMessage("");
      setStatus(response.import.validationErrors.length ? `V2 preview has ${response.import.validationErrors.length} validation errors` : "V2 preview ready to confirm");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function confirmImport() {
    if (!caseImport) return;
    setLoading(true);
    try {
      const response = await confirmV2CaseImport(apiKey, caseImport.importId, draftPayload(importDraft));
      setCaseImport(response.import);
      setSelectedCase(response.case);
      setEditDraft(toDraft(response.case));
      setStatus(`V2 saved ${response.case.caseId}`);
      await refreshCases();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function sendImportMessage() {
    if (!caseImport || !importMessage.trim()) return;
    setLoading(true);
    try {
      const response = await appendV2CaseImportMessage(apiKey, caseImport.importId, importMessage);
      setCaseImport(response.import);
      setImportDraft(fromV2Draft(response.import.draft));
      setImportMessage("");
      setStatus(response.import.validationErrors.length ? `V2 import still has ${response.import.validationErrors.length} validation errors` : "V2 import draft completed");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function saveCase(enabled?: boolean) {
    if (!selectedCase) return;
    setLoading(true);
    try {
      const response = await updateV2Case(apiKey, selectedCase.caseId, { ...draftPayload(editDraft), enabled });
      setSelectedCase(response);
      setEditDraft(toDraft(response));
      setStatus(`V2 updated ${response.caseId}`);
      await refreshCases();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  function selectCase(record: V2CaseRecord | V2CaseHit) {
    setSelectedCase(record);
    setEditDraft(toDraft(record));
  }

  function resetImport() {
    setSourceText("");
    setSourceFile(null);
    setCaseImport(null);
    setImportDraft(EMPTY_DRAFT);
    setImportMessage("");
    setStatus("V2 import draft cleared");
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <BookOpenCheck className="h-5 w-5 text-primary" />
              <CardTitle>V2 Memory Bridge</CardTitle>
            </div>
            <CardDescription>直接调用 Python V2 Case Memory：search、preview import、confirm、edit、enable/disable</CardDescription>
          </div>
          <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refreshCases()}>
            <RefreshCw className="mr-2 h-4 w-4" />刷新
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid gap-5 xl:grid-cols-[minmax(0,520px)_minmax(0,1fr)]">
          <div className="space-y-4 rounded-lg border border-border p-4">
            <div className="flex items-center gap-2">
              <UploadCloud className="h-5 w-5 text-primary" />
              <h3 className="text-sm font-semibold">V2 import preview</h3>
            </div>
            <textarea
              className="min-h-40 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20"
              value={sourceText}
              onChange={(event) => setSourceText(event.target.value)}
              placeholder="粘贴 Case 文档、复盘记录或 JSON 字段"
            />
            <label className="flex cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 px-4 py-5 text-center text-sm text-muted-foreground transition hover:border-primary hover:bg-white">
              <FileText className="mb-2 h-6 w-6" />
              <span>{sourceFile ? sourceFile.name : "上传 .txt / .md / .log / .json / .yaml / .csv"}</span>
              <input
                accept=".txt,.text,.md,.markdown,.log,.json,.yaml,.yml,.csv,text/*,application/json"
                className="hidden"
                type="file"
                onChange={(event) => setSourceFile(event.target.files?.[0] ?? null)}
              />
            </label>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span className="text-xs text-muted-foreground">{status}</span>
              <div className="flex flex-wrap gap-2">
                <Button disabled={loading || !canPreview} onClick={() => void previewImport()}><UploadCloud className="mr-2 h-4 w-4" />Preview</Button>
                <Button disabled={loading} variant="outline" onClick={resetImport}>清空</Button>
              </div>
            </div>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className="text-sm font-semibold">V2 structured draft</h3>
                <p className="mt-1 text-xs text-muted-foreground">{caseImport ? `${caseImport.importId} · ${caseImport.status} · ${caseImport.sourceSizeBytes.toLocaleString()} bytes` : "等待 V2 preview"}</p>
              </div>
              {caseImport ? <Badge variant={caseImport.validationErrors.length ? "warning" : "success"}>{caseImport.validationErrors.length ? "needs edit" : "ready"}</Badge> : null}
            </div>
            {caseImport ? (
              <>
                <DraftFields draft={importDraft} onChange={setImportDraft} compact />
                {caseImport.validationErrors.length ? (
                  <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800">
                    {caseImport.validationErrors.map((item) => <p key={item}>{item}</p>)}
                  </div>
                ) : null}
                <CaseImportMessages messages={caseImport.messages ?? []} />
                {caseImport.validationErrors.length ? (
                  <div className="space-y-3 rounded-lg border border-border bg-slate-50 p-3">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <MessageSquare className="h-4 w-4 text-primary" />
                      <span>补充缺失信息</span>
                    </div>
                    <textarea
                      className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20"
                      value={importMessage}
                      onChange={(event) => setImportMessage(event.target.value)}
                      placeholder="填写缺失的标题、现象、根因或解决方案，V2 会重新整理草稿"
                    />
                    <div className="flex justify-end">
                      <Button disabled={loading || !importMessage.trim()} onClick={() => void sendImportMessage()}>
                        <Send className="mr-2 h-4 w-4" />Submit supplement
                      </Button>
                    </div>
                  </div>
                ) : null}
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <span className="text-xs text-muted-foreground">{caseImport.filename ?? "pasted text"} · updated {new Date(caseImport.updatedAt).toLocaleString()}</span>
                  <Button disabled={loading || !importReady} onClick={() => void confirmImport()}><Save className="mr-2 h-4 w-4" />Confirm</Button>
                </div>
              </>
            ) : <EmptyState>Preview 后可编辑字段，再确认写入 V2 Case Memory。</EmptyState>}
          </div>
        </div>

        <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(420px,520px)]">
          <div className="space-y-4 rounded-lg border border-border p-4">
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
              <div className="relative">
                <Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
                <Input
                  className="pl-9"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => { if (event.key === "Enter") void refreshCases(); }}
                  placeholder="Search V2 Memory"
                />
              </div>
              <label className="flex items-center gap-2 rounded-md border border-border px-3 py-2 text-xs text-muted-foreground">
                <input className="h-4 w-4 accent-teal-700" type="checkbox" checked={includeDisabled} onChange={(event) => setIncludeDisabled(event.target.checked)} />
                <span>显示禁用 Case</span>
              </label>
            </div>
            <div className="grid gap-3 lg:grid-cols-2">
              {cases.length ? cases.map((item) => (
                <button
                  className={`rounded-lg border p-3 text-left transition hover:border-primary ${selectedCase?.caseId === item.caseId ? "border-primary bg-slate-50" : "border-border bg-white"}`}
                  key={item.caseId}
                  onClick={() => selectCase(item)}
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium">{item.title}</span>
                    <Badge variant={item.sourceType === "manual" ? "warning" : "secondary"}>{item.sourceType}</Badge>
                    <Badge variant={item.enabled ? "success" : "destructive"}>{item.enabled ? "enabled" : "disabled"}</Badge>
                  </div>
                  <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{item.rootCause}</p>
                  <CaseScoreSummary caseHit={item} />
                  <p className="mt-2 break-all text-xs text-muted-foreground">{item.caseId}</p>
                </button>
              )) : <EmptyState>暂无 V2 Case。</EmptyState>}
            </div>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className="text-sm font-semibold">V2 Case detail</h3>
                <p className="mt-1 text-xs text-muted-foreground">{selectedSummary}</p>
              </div>
              {selectedCase ? (
                <div className="flex flex-wrap gap-2">
                  <Button disabled={loading} variant="outline" onClick={() => void saveCase(!selectedCase.enabled)}>
                    {selectedCase.enabled ? <XCircle className="mr-2 h-4 w-4" /> : <CheckCircle2 className="mr-2 h-4 w-4" />}
                    {selectedCase.enabled ? "禁用" : "启用"}
                  </Button>
                  <Button disabled={loading} onClick={() => void saveCase()}><Save className="mr-2 h-4 w-4" />保存</Button>
                </div>
              ) : null}
            </div>
            {selectedCase ? (
              <>
                <DraftFields draft={editDraft} onChange={setEditDraft} compact />
                <div className="grid gap-3 text-xs text-muted-foreground md:grid-cols-2">
                  <Info label="Schema" value={String(selectedCase.schemaVersion)} />
                  <Info label="Source type" value={selectedCase.sourceType} />
                  <Info label="Task" value={selectedCase.taskId ?? "-"} />
                  <Info label="Source result" value={selectedCase.sourceResultPath ?? "-"} />
                  <Info label="Created" value={formatDate(selectedCase.createdAt)} />
                  <Info label="Updated" value={formatDate(selectedCase.updatedAt)} />
                  <Info label="Search backend" value={caseSearchBackend(selectedCase)} />
                  <Info label="Score" value={caseScoreValue(selectedCase, "score")} />
                  <Info label="FTS score" value={caseScoreValue(selectedCase, "ftsScore")} />
                  <Info label="Vector score" value={caseScoreValue(selectedCase, "vectorScore")} />
                </div>
              </>
            ) : <EmptyState>选择一个 V2 Case 查看详情。</EmptyState>}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function CaseImportMessages({ messages }: { messages: NonNullable<V2CaseImport["messages"]> }) {
  if (!messages.length) return null;
  return (
    <div className="space-y-2 rounded-lg border border-border p-3">
      <div className="flex items-center gap-2 text-sm font-medium">
        <MessageSquare className="h-4 w-4 text-primary" />
        <span>Import messages</span>
      </div>
      <div className="max-h-44 space-y-2 overflow-y-auto pr-1">
        {messages.map((message, index) => (
          <div className="rounded-md bg-slate-50 p-2 text-xs" key={`${message.createdAt}-${index}`}>
            <div className="mb-1 flex flex-wrap items-center gap-2">
              <Badge variant={message.role === "assistant" ? "secondary" : "success"}>{message.role}</Badge>
              <span className="text-muted-foreground">{new Date(message.createdAt).toLocaleString()}</span>
            </div>
            <p className="whitespace-pre-wrap text-foreground">{message.content}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

function DraftFields({ draft, onChange, compact = false }: { draft: UiCaseDraft; onChange: (draft: UiCaseDraft) => void; compact?: boolean }) {
  return (
    <div className="space-y-3">
      <Input value={draft.title} onChange={(event) => onChange({ ...draft, title: event.target.value })} placeholder="Title" />
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
        <Input value={draft.product} onChange={(event) => onChange({ ...draft, product: event.target.value })} placeholder="Product" />
        <Input value={draft.version} onChange={(event) => onChange({ ...draft, version: event.target.value })} placeholder="Version" />
        <Input value={draft.environment} onChange={(event) => onChange({ ...draft, environment: event.target.value })} placeholder="Environment" />
        <Input value={draft.instanceId} onChange={(event) => onChange({ ...draft, instanceId: event.target.value })} placeholder="InstanceID" />
        <Input value={draft.nodeId} onChange={(event) => onChange({ ...draft, nodeId: event.target.value })} placeholder="NodeID" />
      </div>
      <textarea className={`${compact ? "min-h-20" : "min-h-24"} w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20`} value={draft.symptom} onChange={(event) => onChange({ ...draft, symptom: event.target.value })} placeholder="Symptom" />
      <textarea className={`${compact ? "min-h-20" : "min-h-24"} w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20`} value={draft.rootCause} onChange={(event) => onChange({ ...draft, rootCause: event.target.value })} placeholder="Root cause" />
      <textarea className={`${compact ? "min-h-20" : "min-h-24"} w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20`} value={draft.solution} onChange={(event) => onChange({ ...draft, solution: event.target.value })} placeholder="Solution" />
      <textarea className="min-h-16 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" value={draft.evidenceRefsText} onChange={(event) => onChange({ ...draft, evidenceRefsText: event.target.value })} placeholder="Evidence refs, one per line" />
    </div>
  );
}

function Info({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p>{label}</p><p className="mt-1 break-all font-mono text-foreground">{value}</p></div>;
}

function CaseScoreSummary({ caseHit }: { caseHit: V2CaseHit }) {
  const parts = [
    `score ${formatScore(caseHit.score)}`,
    caseHit.searchBackend ?? "recent",
    caseHit.ftsScore == null ? null : `fts ${formatScore(caseHit.ftsScore)}`,
    caseHit.vectorScore == null ? null : `vector ${formatScore(caseHit.vectorScore)}`
  ].filter(Boolean);
  return <p className="mt-2 text-xs text-muted-foreground">{parts.join(" · ")}</p>;
}

function caseSearchBackend(record: V2CaseRecord | V2CaseHit) {
  const hit = record as Partial<V2CaseHit>;
  return hit.searchBackend ?? (typeof hit.score === "number" ? "recent" : "-");
}

function caseScoreValue(record: V2CaseRecord | V2CaseHit, key: keyof Pick<V2CaseHit, "score" | "ftsScore" | "vectorScore">) {
  const value = (record as Partial<V2CaseHit>)[key];
  return typeof value === "number" ? formatScore(value) : "-";
}

function formatScore(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(3);
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function toDraft(record: V2CaseRecord): UiCaseDraft {
  return {
    title: record.title,
    symptom: record.symptom,
    rootCause: record.rootCause,
    solution: record.solution,
    product: record.product ?? "",
    version: record.version ?? "",
    environment: record.environment ?? "",
    instanceId: record.instanceId ?? "",
    nodeId: record.nodeId ?? "",
    evidenceRefsText: record.evidenceRefs.join("\n")
  };
}

function fromV2Draft(draft: V2CaseDraft): UiCaseDraft {
  return {
    title: draft.title ?? "",
    symptom: draft.symptom ?? "",
    rootCause: draft.rootCause ?? "",
    solution: draft.solution ?? "",
    product: draft.product ?? "",
    version: draft.version ?? "",
    environment: draft.environment ?? "",
    instanceId: draft.instanceId ?? "",
    nodeId: draft.nodeId ?? "",
    evidenceRefsText: (draft.evidenceRefs ?? []).join("\n")
  };
}

function draftPayload(draft: UiCaseDraft): V2CaseDraft {
  return {
    title: draft.title,
    symptom: draft.symptom,
    rootCause: draft.rootCause,
    solution: draft.solution,
    product: optionalString(draft.product),
    version: optionalString(draft.version),
    environment: optionalString(draft.environment),
    instanceId: optionalString(draft.instanceId),
    nodeId: optionalString(draft.nodeId),
    evidenceRefs: draft.evidenceRefsText.split(/\r?\n/).map((value) => value.trim()).filter(Boolean)
  };
}

function hasRequiredFields(draft: UiCaseDraft) {
  return Boolean(draft.title.trim() && draft.symptom.trim() && draft.rootCause.trim() && draft.solution.trim());
}

function optionalString(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
