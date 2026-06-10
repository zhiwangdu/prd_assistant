import { BookOpenCheck, CheckCircle2, FileText, MessageSquare, RefreshCw, Save, Search, Send, UploadCloud, XCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";

type CaseRecord = {
  schemaVersion: number;
  caseId: string;
  sourceType: "task" | "manual";
  taskId?: string | null;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  instanceId?: string | null;
  nodeId?: string | null;
  title: string;
  symptom: string;
  rootCause: string;
  solution: string;
  evidenceRefs: string[];
  sourceResultPath?: string | null;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
};

type CaseHit = CaseRecord & { score: number };
type CaseListResponse = { cases: CaseHit[] };
type CaseResponse = { case: CaseRecord };

type CaseImportSession = {
  draftId: string;
  sourceType: "text" | "file";
  filename?: string | null;
  structuredCase: CaseImportDraft;
  missingFields: Array<{ field: string; label: string; question: string }>;
  assistantQuestion?: string | null;
  readyToConfirm: boolean;
  status: "needs_input" | "ready" | "saved";
  messages: Array<{ role: "user" | "assistant"; content: string; createdAt: string }>;
  confirmedCaseId?: string | null;
  createdAt: string;
  updatedAt: string;
};

type CaseImportDraft = {
  title?: string | null;
  symptom?: string | null;
  rootCause?: string | null;
  solution?: string | null;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  instanceId?: string | null;
  nodeId?: string | null;
  evidenceRefs: string[];
};

type CaseImportResponse = { draft: CaseImportSession };

type CaseDraft = {
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

const EMPTY_DRAFT: CaseDraft = {
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

export function CasesView({ apiKey }: { apiKey: string }) {
  const [query, setQuery] = useState("");
  const [includeDisabled, setIncludeDisabled] = useState(true);
  const [cases, setCases] = useState<CaseHit[]>([]);
  const [selectedCase, setSelectedCase] = useState<CaseRecord | null>(null);
  const [editDraft, setEditDraft] = useState<CaseDraft>(EMPTY_DRAFT);
  const [sourceText, setSourceText] = useState("");
  const [sourceFile, setSourceFile] = useState<File | null>(null);
  const [caseImport, setCaseImport] = useState<CaseImportSession | null>(null);
  const [importDraft, setImportDraft] = useState<CaseDraft>(EMPTY_DRAFT);
  const [importAnswer, setImportAnswer] = useState("");
  const [status, setStatus] = useState("等待加载 Case Store");
  const [loading, setLoading] = useState(false);

  const canStartImport = Boolean(apiKey.trim() && (sourceText.trim() || sourceFile));
  const importHasRequiredFields = Boolean(importDraft.title.trim() && importDraft.symptom.trim() && importDraft.rootCause.trim() && importDraft.solution.trim());
  const selectedSummary = useMemo(() => {
    if (!selectedCase) return "选择一个 Case 查看详情";
    return `${selectedCase.caseId} · ${selectedCase.sourceType} · ${selectedCase.enabled ? "enabled" : "disabled"}`;
  }, [selectedCase]);

  const refreshCases = useCallback(async () => {
    if (!apiKey.trim()) {
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const params = new URLSearchParams();
      params.set("limit", "50");
      params.set("includeDisabled", String(includeDisabled));
      if (query.trim()) params.set("query", query.trim());
      const response = await fetchJson<CaseListResponse>(`/api/cases?${params.toString()}`, {
        headers: authHeaders(apiKey)
      });
      setCases(response.cases);
      setStatus(`Loaded ${response.cases.length} cases`);
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

  function selectCase(record: CaseRecord) {
    setSelectedCase(record);
    setEditDraft(toDraft(record));
  }

  async function startCaseImport() {
    if (!canStartImport) return;
    setLoading(true);
    try {
      setStatus("整理 Case 文档中");
      const response = sourceFile
        ? await uploadCaseImportFile(sourceFile, apiKey)
        : await fetchJson<CaseImportResponse>("/api/cases/imports", {
          method: "POST",
          headers: jsonHeaders(apiKey),
          body: JSON.stringify({ text: sourceText })
        });
      setCaseImport(response.draft);
      setImportDraft(importToDraft(response.draft.structuredCase));
      setImportAnswer("");
      setStatus(response.draft.readyToConfirm ? "已整理为结构化 Case，可确认保存" : "已整理草稿，请补充缺失信息");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function submitImportAnswer() {
    if (!caseImport) return;
    const message = importAnswer.trim();
    if (!message) {
      setStatus("请填写补充信息");
      return;
    }
    setLoading(true);
    try {
      const response = await fetchJson<CaseImportResponse>(`/api/cases/imports/${encodeURIComponent(caseImport.draftId)}/messages`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ message })
      });
      setCaseImport(response.draft);
      setImportDraft(importToDraft(response.draft.structuredCase));
      setImportAnswer("");
      setStatus(response.draft.readyToConfirm ? "缺失信息已补齐，可确认保存" : "仍有缺失信息，请继续补充");
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function syncImportDraft() {
    if (!caseImport) return null;
    const response = await fetchJson<CaseImportResponse>(`/api/cases/imports/${encodeURIComponent(caseImport.draftId)}`, {
      method: "PATCH",
      headers: jsonHeaders(apiKey),
      body: JSON.stringify(draftPayload(importDraft))
    });
    setCaseImport(response.draft);
    setImportDraft(importToDraft(response.draft.structuredCase));
    return response.draft;
  }

  async function confirmImportedCase() {
    if (!caseImport) return;
    setLoading(true);
    try {
      const synced = await syncImportDraft();
      if (!synced?.readyToConfirm) {
        setStatus("仍有必填信息缺失，暂不能保存 Case");
        return;
      }
      const response = await fetchJson<CaseResponse>(`/api/cases/imports/${encodeURIComponent(caseImport.draftId)}/confirm`, {
        method: "POST",
        headers: authHeaders(apiKey)
      });
      setSelectedCase(response.case);
      setEditDraft(toDraft(response.case));
      setStatus(`Saved ${response.case.caseId}`);
      await refreshCases();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  function resetImport() {
    setSourceText("");
    setSourceFile(null);
    setCaseImport(null);
    setImportDraft(EMPTY_DRAFT);
    setImportAnswer("");
    setStatus("已清空导入草稿");
  }

  async function updateCase(enabled?: boolean) {
    if (!selectedCase) return;
    setLoading(true);
    try {
      const response = await fetchJson<CaseResponse>(`/api/cases/${encodeURIComponent(selectedCase.caseId)}`, {
        method: "PATCH",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ ...draftPayload(editDraft), enabled })
      });
      setSelectedCase(response.case);
      setEditDraft(toDraft(response.case));
      setStatus(`Updated ${response.case.caseId}`);
      await refreshCases();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <section className="grid gap-5 xl:grid-cols-[minmax(0,520px)_minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <UploadCloud className="h-5 w-5 text-primary" />
              <CardTitle>导入 Case</CardTitle>
            </div>
            <CardDescription>粘贴故障记录或上传 UTF-8 文本文件，由 LLM 整理为结构化 Case</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <textarea
              className="min-h-52 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20"
              value={sourceText}
              onChange={(event) => setSourceText(event.target.value)}
              placeholder="粘贴 Case 文档、复盘记录、聊天记录或口语化故障描述"
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
                <Button disabled={loading || !canStartImport} onClick={() => void startCaseImport()}><UploadCloud className="mr-2 h-4 w-4" />整理</Button>
                <Button disabled={loading} variant="outline" onClick={resetImport}>清空</Button>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <BookOpenCheck className="h-5 w-5 text-primary" />
                  <CardTitle>结构化草稿</CardTitle>
                </div>
                <CardDescription>{caseImport ? `${caseImport.draftId} · ${caseImport.sourceType} · ${caseImport.status}` : "等待导入文本生成草稿"}</CardDescription>
              </div>
              {caseImport ? (
                <Badge variant={caseImport.readyToConfirm ? "success" : "warning"}>{caseImport.readyToConfirm ? "ready" : "needs input"}</Badge>
              ) : null}
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {caseImport ? (
              <>
                <CaseDraftFields draft={importDraft} onChange={setImportDraft} compact />
                {caseImport.missingFields.length ? (
                  <div className="rounded-lg border border-amber-200 bg-amber-50 p-3">
                    <div className="flex flex-wrap gap-2">
                      {caseImport.missingFields.map((field) => <Badge key={field.field} variant="warning">{field.label}</Badge>)}
                    </div>
                    {caseImport.assistantQuestion ? <p className="mt-2 text-sm text-amber-800">{caseImport.assistantQuestion}</p> : null}
                  </div>
                ) : null}
                <CaseImportMessages messages={caseImport.messages} />
                {!caseImport.readyToConfirm ? (
                  <div className="space-y-2">
                    <textarea
                      className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20"
                      value={importAnswer}
                      onChange={(event) => setImportAnswer(event.target.value)}
                      placeholder="按自然语言补充缺失信息"
                    />
                    <Button disabled={loading || !importAnswer.trim()} onClick={() => void submitImportAnswer()}><Send className="mr-2 h-4 w-4" />提交补充</Button>
                  </div>
                ) : null}
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <span className="text-xs text-muted-foreground">{caseImport.filename ?? "pasted text"} · updated {new Date(caseImport.updatedAt).toLocaleString()}</span>
                  <Button disabled={loading || !importHasRequiredFields} onClick={() => void confirmImportedCase()}><Save className="mr-2 h-4 w-4" />确认保存</Button>
                </div>
              </>
            ) : <EmptyState>导入后会在这里编辑草稿、查看缺失信息并完成确认。</EmptyState>}
          </CardContent>
        </Card>
      </section>

      <section className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(420px,520px)]">
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <CardTitle>Case Store</CardTitle>
                <CardDescription>本地 JSON Case 列表、搜索和启用状态管理</CardDescription>
              </div>
              <Button className="h-9 px-3" disabled={loading} variant="outline" onClick={() => void refreshCases()}><RefreshCw className="h-4 w-4" /></Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
              <div className="relative">
                <Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" />
                <Input
                  className="pl-9"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => { if (event.key === "Enter") void refreshCases(); }}
                  placeholder="Search title, symptom, root cause, InstanceID, NodeID"
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
                  <p className="mt-2 break-all text-xs text-muted-foreground">{item.caseId} · score {item.score.toFixed(2)} · {item.instanceId ?? "no instance"} · {item.nodeId ?? "no node"}</p>
                </button>
              )) : <EmptyState>暂无 Case。</EmptyState>}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <BookOpenCheck className="h-5 w-5 text-primary" />
                  <CardTitle>Case detail</CardTitle>
                </div>
                <CardDescription>{selectedSummary}</CardDescription>
              </div>
              {selectedCase ? (
                <div className="flex flex-wrap gap-2">
                  <Button disabled={loading} variant="outline" onClick={() => void updateCase(!selectedCase.enabled)}>
                    {selectedCase.enabled ? <XCircle className="mr-2 h-4 w-4" /> : <CheckCircle2 className="mr-2 h-4 w-4" />}
                    {selectedCase.enabled ? "禁用" : "启用"}
                  </Button>
                  <Button disabled={loading} onClick={() => void updateCase()}><Save className="mr-2 h-4 w-4" />保存</Button>
                </div>
              ) : null}
            </div>
          </CardHeader>
          <CardContent>
            {selectedCase ? (
              <div className="space-y-4">
                <CaseDraftFields draft={editDraft} onChange={setEditDraft} compact />
                <div className="grid gap-3 text-xs text-muted-foreground md:grid-cols-2">
                  <Info label="Schema" value={String(selectedCase.schemaVersion)} />
                  <Info label="Task" value={selectedCase.taskId ?? "-"} />
                  <Info label="Source result" value={selectedCase.sourceResultPath ?? "-"} />
                  <Info label="Updated" value={new Date(selectedCase.updatedAt).toLocaleString()} />
                </div>
              </div>
            ) : <EmptyState>选择一个 Case 查看详情。</EmptyState>}
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

function CaseDraftFields({ draft, onChange, compact = false }: { draft: CaseDraft; onChange: (draft: CaseDraft) => void; compact?: boolean }) {
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

function CaseImportMessages({ messages }: { messages: CaseImportSession["messages"] }) {
  if (!messages.length) return null;
  return (
    <div className="max-h-52 space-y-2 overflow-auto rounded-lg border border-border bg-slate-50 p-3">
      {messages.map((message, index) => (
        <div className="rounded-md bg-white p-2 text-sm" key={`${message.createdAt}-${index}`}>
          <div className="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
            <MessageSquare className="h-3.5 w-3.5" />
            <span>{message.role}</span>
            <span>{new Date(message.createdAt).toLocaleString()}</span>
          </div>
          <p className="whitespace-pre-wrap break-words">{message.content}</p>
        </div>
      ))}
    </div>
  );
}

function Info({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p>{label}</p><p className="mt-1 break-all font-mono text-foreground">{value}</p></div>;
}

function toDraft(record: CaseRecord): CaseDraft {
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

function importToDraft(draft: CaseImportDraft): CaseDraft {
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
    evidenceRefsText: draft.evidenceRefs.join("\n")
  };
}

function draftPayload(draft: CaseDraft) {
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

async function uploadCaseImportFile(file: File, apiKey: string) {
  const form = new FormData();
  form.append("file", file);
  return fetchJson<CaseImportResponse>("/api/cases/imports", {
    method: "POST",
    headers: authHeaders(apiKey),
    body: form
  });
}

function optionalString(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
