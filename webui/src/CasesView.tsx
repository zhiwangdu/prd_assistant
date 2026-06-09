import { BookOpenCheck, CheckCircle2, Plus, RefreshCw, Save, Search, XCircle } from "lucide-react";
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
  const [newDraft, setNewDraft] = useState<CaseDraft>(EMPTY_DRAFT);
  const [editDraft, setEditDraft] = useState<CaseDraft>(EMPTY_DRAFT);
  const [status, setStatus] = useState("等待加载 Case Store");
  const [loading, setLoading] = useState(false);

  const canCreate = Boolean(newDraft.title.trim() && newDraft.symptom.trim() && newDraft.rootCause.trim() && newDraft.solution.trim());
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

  async function createCase() {
    if (!canCreate) return;
    setLoading(true);
    try {
      const response = await fetchJson<CaseResponse>("/api/cases", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify(draftPayload(newDraft))
      });
      setNewDraft(EMPTY_DRAFT);
      setSelectedCase(response.case);
      setEditDraft(toDraft(response.case));
      setStatus(`Created ${response.case.caseId}`);
      await refreshCases();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
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
      <section className="grid gap-5 xl:grid-cols-[minmax(0,420px)_minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Plus className="h-5 w-5 text-primary" />
              <CardTitle>New case</CardTitle>
            </div>
            <CardDescription>手工录入不绑定任务的人工确认 Case</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <CaseDraftFields draft={newDraft} onChange={setNewDraft} />
            <div className="flex items-center justify-between gap-3">
              <span className="text-xs text-muted-foreground">{status}</span>
              <Button disabled={loading || !canCreate} onClick={() => void createCase()}><Plus className="mr-2 h-4 w-4" />新建</Button>
            </div>
          </CardContent>
        </Card>

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
      </section>

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
              <CaseDraftFields draft={editDraft} onChange={setEditDraft} />
              <div className="grid gap-3 text-xs text-muted-foreground md:grid-cols-2 lg:grid-cols-4">
                <Info label="Schema" value={String(selectedCase.schemaVersion)} />
                <Info label="Task" value={selectedCase.taskId ?? "-"} />
                <Info label="Source result" value={selectedCase.sourceResultPath ?? "-"} />
                <Info label="Updated" value={new Date(selectedCase.updatedAt).toLocaleString()} />
              </div>
            </div>
          ) : <EmptyState>选择左侧 Case 或新建一个 Case。</EmptyState>}
        </CardContent>
      </Card>
    </div>
  );
}

function CaseDraftFields({ draft, onChange }: { draft: CaseDraft; onChange: (draft: CaseDraft) => void }) {
  return (
    <div className="space-y-3">
      <Input value={draft.title} onChange={(event) => onChange({ ...draft, title: event.target.value })} placeholder="Title" />
      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-5">
        <Input value={draft.product} onChange={(event) => onChange({ ...draft, product: event.target.value })} placeholder="Product" />
        <Input value={draft.version} onChange={(event) => onChange({ ...draft, version: event.target.value })} placeholder="Version" />
        <Input value={draft.environment} onChange={(event) => onChange({ ...draft, environment: event.target.value })} placeholder="Environment" />
        <Input value={draft.instanceId} onChange={(event) => onChange({ ...draft, instanceId: event.target.value })} placeholder="InstanceID" />
        <Input value={draft.nodeId} onChange={(event) => onChange({ ...draft, nodeId: event.target.value })} placeholder="NodeID" />
      </div>
      <textarea className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20" value={draft.symptom} onChange={(event) => onChange({ ...draft, symptom: event.target.value })} placeholder="Symptom" />
      <textarea className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20" value={draft.rootCause} onChange={(event) => onChange({ ...draft, rootCause: event.target.value })} placeholder="Root cause" />
      <textarea className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-teal-600/20" value={draft.solution} onChange={(event) => onChange({ ...draft, solution: event.target.value })} placeholder="Solution" />
      <textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" value={draft.evidenceRefsText} onChange={(event) => onChange({ ...draft, evidenceRefsText: event.target.value })} placeholder="Evidence refs, one per line" />
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

function optionalString(value: string) {
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
