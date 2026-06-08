import { BookOpenCheck, FileArchive, RefreshCw, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";

const CHUNK_BYTES = 512 * 1024;

type UploadResponse = { uploadId: string; filename: string; size: number };
type TaskStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";
type TaskSummary = {
  taskId: string;
  url: string;
  status: TaskStatus;
  phase?: "EXTRACT" | "SEARCH_LOGS" | "RUN_TOOL" | "PLAN_ANALYSIS" | "GENERATE_RESULT" | null;
  createdAt: string;
};
type TaskRecord = TaskSummary & {
  attempts?: number;
  error?: { phase?: string | null; message: string } | null;
  instanceId?: string | null;
  clusterId?: string | null;
  nodeId?: string | null;
};
type AnalysisResult = {
  schemaVersion: number;
  summary: string;
  symptoms: string[];
  likelyRootCauses: Array<{ cause: string; evidenceRefs: string[] }>;
  nextChecks: string[];
  fixSuggestions: string[];
  missingInformation: string[];
  confidence: "low" | "medium" | "high";
};
type TaskResult = { taskId: string; result: AnalysisResult };
type AnalysisSnapshot = {
  taskId: string;
  state: {
    revision: number;
    status: "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";
    currentPhase?: TaskSummary["phase"];
    budget: { rounds: number; llmCalls: number; actions: number };
    evidence: Array<{ evidenceType: string; artifactPath: string; summary: string; evidenceRefs: string[]; createdAt: string }>;
    actions: Array<{ actionId: string; actionType: string; status: string; summary: string; createdAt: string }>;
    userMessages: Array<{ messageId: string; questionId?: string | null; content: string; createdAt: string }>;
    pendingUserPrompts: PendingUserPrompt[];
    pendingApprovals: PendingApproval[];
  };
  events: AnalysisEvent[];
};
type PendingUserPrompt = {
  questionId: string;
  actionId: string;
  question: string;
  reason: string;
  required: boolean;
  answerFormat?: string | null;
  createdAt: string;
};
type PendingApproval = {
  actionId: string;
  actionType: string;
  reason: string;
  risk: string;
  input: unknown;
  evidenceRefs: string[];
  createdAt: string;
};
type AnalysisEvent = {
  revision: number;
  eventType: string;
  phase?: TaskSummary["phase"];
  actionId?: string | null;
  message: string;
  evidenceRefs: string[];
  artifactPath?: string | null;
  details?: Record<string, unknown>;
  createdAt: string;
};
type Artifacts = {
  taskId?: string;
  manifest?: { files?: Array<{ path: string; size: number }> };
  grepResults?: { matches?: Array<{ file: string; line: number; keyword: string; text: string }> };
  metadataContext?: MetadataContext | null;
  toolResults?: ToolResult[];
};
type ToolResult = {
  tool: string;
  actionId: string;
  status: string;
  exitCode?: number | null;
  durationMs: number;
  summary: string;
  findings?: ToolFinding[];
  stdoutPath: string;
  stderrPath: string;
};
type ToolFinding = {
  severity?: string;
  file?: string;
  line?: number;
  message: string;
};
type MetadataContext = {
  instanceId?: string | null;
  clusterId?: string | null;
  nodeId?: string | null;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  node?: { kind?: string | null; host?: string | null; role?: string | null; status?: string | null } | null;
  clusterNodes?: Array<{ nodeId: string }>;
  cluster?: { databases?: Array<{ name: string }>; partitionViews?: Array<{ statusText?: string | null }> } | null;
};
type CaseRecord = {
  caseId: string;
  taskId: string;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  title: string;
  symptom: string;
  rootCause: string;
  solution: string;
  evidenceRefs: string[];
  enabled: boolean;
  createdAt: string;
};
type CaseHit = CaseRecord & { score: number };
type CaseDraft = {
  title: string;
  symptom: string;
  rootCause: string;
  solution: string;
};

export function OperationsView({ apiKey }: { apiKey: string }) {
  const [files, setFiles] = useState<File[]>([]);
  const [sourceUrl, setSourceUrl] = useState("");
  const [question, setQuestion] = useState("分析日志中的主要异常、可能原因和建议检查项。");
  const [instanceId, setInstanceId] = useState("");
  const [clusterId, setClusterId] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [uploadStatus, setUploadStatus] = useState("等待上传");
  const [uploadProgress, setUploadProgress] = useState(0);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [selectedTask, setSelectedTask] = useState<TaskRecord | null>(null);
  const [artifacts, setArtifacts] = useState<Artifacts | null>(null);
  const [taskResult, setTaskResult] = useState<TaskResult | null>(null);
  const [analysisSnapshot, setAnalysisSnapshot] = useState<AnalysisSnapshot | null>(null);
  const [cases, setCases] = useState<CaseHit[]>([]);
  const [caseQuery, setCaseQuery] = useState("");
  const [caseStatus, setCaseStatus] = useState("Case Store ready");
  const [caseDraft, setCaseDraft] = useState<CaseDraft>({ title: "", symptom: "", rootCause: "", solution: "" });
  const [loading, setLoading] = useState(false);
  const [userAnswer, setUserAnswer] = useState("");
  const [approvalReason, setApprovalReason] = useState("");

  const refreshTasks = useCallback(async () => {
    if (!apiKey.trim()) {
      setTasks([]);
      return;
    }
    const result = await fetchJson<{ tasks: TaskSummary[] }>("/api/tasks", { headers: authHeaders(apiKey) });
    setTasks(result.tasks);
    setSelectedTask((current) => {
      if (current || !result.tasks.length) return current;
      return result.tasks[0] as TaskRecord;
    });
  }, [apiKey]);

  const refreshCases = useCallback(async (queryText: string) => {
    if (!apiKey.trim()) {
      setCases([]);
      return;
    }
    const params = new URLSearchParams();
    if (queryText.trim()) params.set("query", queryText.trim());
    params.set("limit", "8");
    const suffix = params.toString() ? `?${params.toString()}` : "";
    const result = await fetchJson<{ cases: CaseHit[] }>(`/api/cases${suffix}`, { headers: authHeaders(apiKey) });
    setCases(result.cases);
    setCaseStatus(`${result.cases.length} case(s) loaded`);
  }, [apiKey]);

  const selectTask = useCallback(async (taskId: string) => {
    const task = await fetchJson<TaskRecord>(`/api/tasks/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) });
    setSelectedTask(task);
    const nextAnalysis = await fetchTaskAnalysis(taskId, apiKey);
    setAnalysisSnapshot(nextAnalysis);
    if (task.status === "SUCCEEDED") {
      const [nextArtifacts, nextResult] = await Promise.all([
        fetchJson<Artifacts>(`/api/tasks/${encodeURIComponent(taskId)}/artifacts`, { headers: authHeaders(apiKey) }),
        fetchJson<TaskResult>(`/api/tasks/${encodeURIComponent(taskId)}/result`, { headers: authHeaders(apiKey) })
      ]);
      setArtifacts(nextArtifacts);
      setTaskResult(nextResult);
    } else {
      setArtifacts(null);
      setTaskResult(null);
    }
  }, [apiKey]);

  useEffect(() => {
    setSelectedTask(null);
    setArtifacts(null);
    setTaskResult(null);
    setAnalysisSnapshot(null);
    setCases([]);
    void refreshTasks().catch((reason) => setUploadStatus(errorMessage(reason)));
    void refreshCases("").catch((reason) => setCaseStatus(errorMessage(reason)));
  }, [refreshCases, refreshTasks]);

  useEffect(() => {
    if (!taskResult) {
      setCaseDraft({ title: "", symptom: "", rootCause: "", solution: "" });
      return;
    }
    setCaseDraft(defaultCaseDraft(taskResult.result));
  }, [taskResult]);

  useEffect(() => {
    if (!apiKey.trim()) return;
    const timer = window.setInterval(() => {
      void refreshTasks().catch(() => undefined);
      if (selectedTask && !isTerminal(selectedTask.status)) {
        void selectTask(selectedTask.taskId).catch((reason) => setUploadStatus(errorMessage(reason)));
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [apiKey, refreshTasks, selectTask, selectedTask]);

  useEffect(() => {
    if (selectedTask && selectedTask.attempts === undefined) {
      void selectTask(selectedTask.taskId).catch((reason) => setUploadStatus(errorMessage(reason)));
    }
  }, [selectTask, selectedTask]);

  async function run() {
    if (!files.length || !apiKey.trim()) {
      setUploadStatus(!files.length ? "请选择日志文件" : "请填写 API Key");
      return;
    }
    setLoading(true);
    setArtifacts(null);
    setTaskResult(null);
    setAnalysisSnapshot(null);
    try {
      const uploads: UploadResponse[] = [];
      for (let index = 0; index < files.length; index += 1) {
        setUploadStatus(`上传 ${files[index].name}`);
        uploads.push(await uploadFile(files[index], apiKey, (value) => setUploadProgress(Math.round(((index + value) / files.length) * 100))));
      }
      setUploadStatus("上传完成，创建分析任务");
      const task = await fetchJson<TaskSummary>("/api/tasks", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          uploadIds: uploads.map((upload) => upload.uploadId),
          sourceUrl: sourceUrl || null,
          question: question.trim() || null,
          instanceId: instanceId.trim() || null,
          clusterId: clusterId.trim() || null,
          nodeId: nodeId.trim() || null
        })
      });
      setUploadProgress(100);
      setUploadStatus(`已创建任务 ${task.taskId}`);
      await refreshTasks();
      await selectTask(task.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function submitUserMessage(prompt: PendingUserPrompt) {
    if (!selectedTask) return;
    const message = userAnswer.trim();
    if (!message) {
      setUploadStatus("请填写回答内容");
      return;
    }
    setLoading(true);
    try {
      await fetchJson<TaskSummary>(`/api/tasks/${encodeURIComponent(selectedTask.taskId)}/messages`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          questionId: prompt.questionId,
          message,
          idempotencyKey: `webui-${prompt.questionId}-${Date.now()}`
        })
      });
      setUserAnswer("");
      setUploadStatus("回答已提交，任务继续执行");
      await selectTask(selectedTask.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function submitApproval(approval: PendingApproval, decision: "approved" | "rejected") {
    if (!selectedTask) return;
    setLoading(true);
    try {
      await fetchJson<TaskSummary>(`/api/tasks/${encodeURIComponent(selectedTask.taskId)}/actions/${encodeURIComponent(approval.actionId)}/decision`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          decision,
          reason: approvalReason.trim() || null,
          idempotencyKey: `webui-${approval.actionId}-${decision}-${Date.now()}`
        })
      });
      setApprovalReason("");
      setUploadStatus(decision === "approved" ? "审批已批准，任务继续执行" : "审批已拒绝，任务继续执行");
      await selectTask(selectedTask.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function confirmCase() {
    if (!selectedTask || !taskResult) return;
    setLoading(true);
    try {
      const evidenceRefs = uniqueEvidenceRefs(taskResult.result);
      const response = await fetchJson<{ case: CaseRecord }>(`/api/tasks/${encodeURIComponent(selectedTask.taskId)}/case`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          title: caseDraft.title,
          symptom: caseDraft.symptom,
          rootCause: caseDraft.rootCause,
          solution: caseDraft.solution,
          evidenceRefs,
          product: artifacts?.metadataContext?.product ?? null,
          version: artifacts?.metadataContext?.version ?? null,
          environment: artifacts?.metadataContext?.environment ?? null
        })
      });
      setCaseStatus(`Saved ${response.case.caseId}`);
      await refreshCases(caseQuery || response.case.title);
    } catch (reason) {
      setCaseStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function disableCase(caseId: string) {
    setLoading(true);
    try {
      await fetchJson<{ case: CaseRecord }>(`/api/cases/${encodeURIComponent(caseId)}`, {
        method: "PATCH",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ enabled: false })
      });
      setCaseStatus(`Disabled ${caseId}`);
      await refreshCases(caseQuery);
    } catch (reason) {
      setCaseStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader><CardTitle>Log import and evidence</CardTitle><CardDescription>上传进度与 Server 后台任务执行状态独立展示</CardDescription></CardHeader>
        <CardContent className="space-y-4">
          <Input value={sourceUrl} onChange={(event) => setSourceUrl(event.target.value)} placeholder="Source URL (optional)" />
          <div className="grid gap-3 md:grid-cols-3">
            <Input value={instanceId} onChange={(event) => setInstanceId(event.target.value)} placeholder="Instance ID (optional)" />
            <Input value={clusterId} onChange={(event) => setClusterId(event.target.value)} placeholder="Cluster ID (optional)" />
            <Input value={nodeId} onChange={(event) => setNodeId(event.target.value)} placeholder="Node ID (optional)" />
          </div>
          <textarea className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={question} onChange={(event) => setQuestion(event.target.value)} placeholder="希望 LLM 分析的问题" />
          <label className="flex min-h-36 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
            <UploadCloud className="mb-2 h-7 w-7" />
            {files.length ? `${files.length} file(s): ${files.map((file) => file.name).join(", ")}` : "选择 .log / .txt / .zip / .tar.gz / .tgz / .tar"}
            <input className="hidden" type="file" multiple onChange={(event) => setFiles(Array.from(event.target.files ?? []))} />
          </label>
          <div>
            <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>Upload</span><span>{uploadProgress}%</span></div>
            <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
          </div>
          <div className="flex items-center justify-between gap-3"><span className="text-sm text-muted-foreground">{uploadStatus}</span><Button disabled={loading} onClick={() => void run()}>{loading ? "上传中" : "上传并分析"}</Button></div>
        </CardContent>
      </Card>

      <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
        <Card>
          <CardHeader><div className="flex items-center justify-between"><CardTitle>Server tasks</CardTitle><Button className="h-8 px-3" variant="outline" onClick={() => void refreshTasks()}><RefreshCw className="h-4 w-4" /></Button></div><CardDescription>刷新页面后仍可查看历史和运行中任务</CardDescription></CardHeader>
          <CardContent className="space-y-2">
            {tasks.length ? tasks.map((task) => (
              <button key={task.taskId} className={`w-full rounded-lg border p-3 text-left ${selectedTask?.taskId === task.taskId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => void selectTask(task.taskId)}>
                <div className="flex items-center justify-between gap-2"><span className="font-mono text-xs">{task.taskId}</span><StatusBadge status={task.status} /></div>
                <p className="mt-1 text-xs text-muted-foreground">{task.phase ?? "No active phase"} · {new Date(task.createdAt).toLocaleString()}</p>
              </button>
            )) : <EmptyState>Server 暂无持久化任务。</EmptyState>}
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>Task execution</CardTitle><CardDescription>{selectedTask ? `${selectedTask.taskId} · attempt ${selectedTask.attempts ?? 0}` : "选择一个任务查看执行状态"}</CardDescription></CardHeader>
          <CardContent>
            {selectedTask ? (
              <div className="space-y-3">
                <div className="flex items-center gap-2"><StatusBadge status={selectedTask.status} /><span className="text-sm text-muted-foreground">{selectedTask.phase ?? "No active phase"}</span></div>
                {selectedTask.instanceId || selectedTask.clusterId || selectedTask.nodeId ? (
                  <p className="text-xs text-muted-foreground">Metadata: instance={selectedTask.instanceId ?? "-"} · cluster={selectedTask.clusterId ?? "-"} · node={selectedTask.nodeId ?? "-"}</p>
                ) : null}
                {selectedTask.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedTask.error?.phase ? `${selectedTask.error.phase}: ` : ""}{selectedTask.error?.message ?? "Task failed"}</div> : null}
                <WaitingInteraction
                  answer={userAnswer}
                  approvalReason={approvalReason}
                  loading={loading}
                  snapshot={analysisSnapshot}
                  status={selectedTask.status}
                  onAnswerChange={setUserAnswer}
                  onApprovalReasonChange={setApprovalReason}
                  onSubmitAnswer={(prompt) => void submitUserMessage(prompt)}
                  onSubmitApproval={(approval, decision) => void submitApproval(approval, decision)}
                />
                {!isTerminal(selectedTask.status) ? <p className="text-sm text-muted-foreground">任务由 Server 后台执行，每秒自动刷新。</p> : null}
                {selectedTask.status === "SUCCEEDED" && !artifacts ? <Button onClick={() => void selectTask(selectedTask.taskId)}>加载 artifacts</Button> : null}
                <ExecutionTimeline snapshot={analysisSnapshot} />
              </div>
            ) : <EmptyState>选择或创建任务后查看执行状态。</EmptyState>}
          </CardContent>
        </Card>
      </div>

      {taskResult ? <AnalysisResultView result={taskResult.result} /> : null}

      {taskResult && selectedTask ? (
        <CaseClosurePanel
          cases={cases}
          caseDraft={caseDraft}
          caseQuery={caseQuery}
          caseStatus={caseStatus}
          loading={loading}
          taskId={selectedTask.taskId}
          onDraftChange={setCaseDraft}
          onQueryChange={setCaseQuery}
          onRefreshCases={() => void refreshCases(caseQuery)}
          onConfirmCase={() => void confirmCase()}
          onDisableCase={(caseId) => void disableCase(caseId)}
        />
      ) : null}

      {artifacts?.metadataContext ? <MetadataContextView context={artifacts.metadataContext} /> : null}

      {artifacts?.toolResults?.length ? (
        <Evidence title="Tool results" count={artifacts.toolResults.length}>
          {artifacts.toolResults.map((result) => (
            <ToolResultLine key={result.actionId} result={result} />
          ))}
        </Evidence>
      ) : null}

      {artifacts ? (
        <div className="grid gap-5 xl:grid-cols-2">
          <Evidence title="Manifest" count={artifacts.manifest?.files?.length ?? 0}>
            {(artifacts.manifest?.files ?? []).map((file) => <DataLine key={file.path} title={file.path} detail={`${file.size.toLocaleString()} bytes`} />)}
          </Evidence>
          <Evidence title="Grep matches" count={artifacts.grepResults?.matches?.length ?? 0}>
            {(artifacts.grepResults?.matches ?? []).map((match, index) => <DataLine id={`grep-match-${index}`} key={`${match.file}:${match.line}:${index}`} title={`${match.file}:${match.line}`} detail={`${match.keyword} · ${match.text}`} />)}
          </Evidence>
        </div>
      ) : <EmptyState>成功任务的 manifest 和 grep evidence 会显示在这里。</EmptyState>}
    </div>
  );
}

function WaitingInteraction({
  answer,
  approvalReason,
  loading,
  snapshot,
  status,
  onAnswerChange,
  onApprovalReasonChange,
  onSubmitAnswer,
  onSubmitApproval
}: {
  answer: string;
  approvalReason: string;
  loading: boolean;
  snapshot: AnalysisSnapshot | null;
  status: TaskStatus;
  onAnswerChange: (value: string) => void;
  onApprovalReasonChange: (value: string) => void;
  onSubmitAnswer: (prompt: PendingUserPrompt) => void;
  onSubmitApproval: (approval: PendingApproval, decision: "approved" | "rejected") => void;
}) {
  if (status === "WAITING_FOR_USER") {
    const prompt = snapshot?.state.pendingUserPrompts[0];
    if (!prompt) {
      return <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">任务正在等待用户输入，但 analysis state 中暂无 pending prompt。</div>;
    }
    return (
      <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-3">
        <div>
          <p className="text-sm font-medium text-amber-900">需要补充信息</p>
          <p className="mt-1 text-sm text-amber-800">{prompt.question}</p>
          <p className="mt-1 text-xs text-amber-700">reason: {prompt.reason} · format: {prompt.answerFormat ?? "free text"} · required: {prompt.required ? "yes" : "no"}</p>
        </div>
        <textarea className="min-h-20 w-full rounded-md border border-amber-200 bg-white px-3 py-2 text-sm" value={answer} onChange={(event) => onAnswerChange(event.target.value)} placeholder="填写补充信息后继续分析" />
        <Button disabled={loading} onClick={() => onSubmitAnswer(prompt)}>提交回答并继续</Button>
      </div>
    );
  }
  if (status === "WAITING_FOR_APPROVAL") {
    const approval = snapshot?.state.pendingApprovals[0];
    if (!approval) {
      return <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">任务正在等待审批，但 analysis state 中暂无 pending approval。</div>;
    }
    return (
      <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-3">
        <div>
          <p className="text-sm font-medium text-amber-900">需要审批动作</p>
          <p className="mt-1 text-sm text-amber-800">{approval.actionType} · {approval.actionId}</p>
          <p className="mt-1 text-xs text-amber-700">risk: {approval.risk} · reason: {approval.reason}</p>
          <pre className="mt-2 max-h-32 overflow-auto rounded bg-white p-2 text-xs text-slate-700">{JSON.stringify(approval.input, null, 2)}</pre>
        </div>
        <Input value={approvalReason} onChange={(event) => onApprovalReasonChange(event.target.value)} placeholder="审批备注或拒绝原因（可选）" />
        <div className="flex flex-wrap gap-2">
          <Button disabled={loading} onClick={() => onSubmitApproval(approval, "approved")}>批准并继续</Button>
          <Button disabled={loading} variant="outline" onClick={() => onSubmitApproval(approval, "rejected")}>拒绝并继续</Button>
        </div>
      </div>
    );
  }
  return null;
}

function CaseClosurePanel({
  cases,
  caseDraft,
  caseQuery,
  caseStatus,
  loading,
  taskId,
  onDraftChange,
  onQueryChange,
  onRefreshCases,
  onConfirmCase,
  onDisableCase
}: {
  cases: CaseHit[];
  caseDraft: CaseDraft;
  caseQuery: string;
  caseStatus: string;
  loading: boolean;
  taskId: string;
  onDraftChange: (draft: CaseDraft) => void;
  onQueryChange: (value: string) => void;
  onRefreshCases: () => void;
  onConfirmCase: () => void;
  onDisableCase: (caseId: string) => void;
}) {
  return (
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_420px]">
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <BookOpenCheck className="h-5 w-5 text-primary" />
            <CardTitle>Confirm as Case</CardTitle>
          </div>
          <CardDescription>{taskId} 的最终结果可人工确认后沉淀为可召回 Case</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input value={caseDraft.title} onChange={(event) => onDraftChange({ ...caseDraft, title: event.target.value })} placeholder="Case title" />
          <textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.symptom} onChange={(event) => onDraftChange({ ...caseDraft, symptom: event.target.value })} placeholder="Symptom" />
          <textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.rootCause} onChange={(event) => onDraftChange({ ...caseDraft, rootCause: event.target.value })} placeholder="Root cause" />
          <textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.solution} onChange={(event) => onDraftChange({ ...caseDraft, solution: event.target.value })} placeholder="Solution" />
          <div className="flex flex-wrap items-center justify-between gap-3">
            <span className="text-sm text-muted-foreground">{caseStatus}</span>
            <Button disabled={loading || !caseDraft.title.trim()} onClick={onConfirmCase}>保存 Case</Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle>Similar cases</CardTitle>
            <Button className="h-8 px-3" variant="outline" onClick={onRefreshCases}><RefreshCw className="h-4 w-4" /></Button>
          </div>
          <CardDescription>本地 JSON Case Store 关键词召回</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <Input value={caseQuery} onChange={(event) => onQueryChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") onRefreshCases(); }} placeholder="Search cases" />
          {cases.length ? cases.map((item) => (
            <div className="rounded-lg border border-border p-3" key={item.caseId}>
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">{item.title}</p>
                  <p className="mt-1 text-xs text-muted-foreground">{item.caseId} · score {item.score.toFixed(2)} · {new Date(item.createdAt).toLocaleDateString()}</p>
                </div>
                <Badge variant={item.enabled ? "secondary" : "destructive"}>{item.enabled ? "enabled" : "disabled"}</Badge>
              </div>
              <p className="mt-2 text-xs text-muted-foreground">{item.rootCause}</p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button className="h-8 px-3" disabled={loading || !item.enabled} variant="outline" onClick={() => onDisableCase(item.caseId)}>禁用</Button>
              </div>
            </div>
          )) : <EmptyState>暂无匹配 Case。</EmptyState>}
        </CardContent>
      </Card>
    </div>
  );
}

function StatusBadge({ status }: { status: TaskStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{status}</Badge>;
}

function isTerminal(status: TaskStatus) {
  return status === "SUCCEEDED" || status === "FAILED";
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}

function Evidence({ title, count, children }: { title: string; count: number; children: React.ReactNode }) {
  return <Card><CardHeader><div className="flex items-center justify-between"><CardTitle>{title}</CardTitle><Badge variant="secondary">{count}</Badge></div></CardHeader><CardContent className="space-y-2">{count ? children : <EmptyState>暂无数据</EmptyState>}</CardContent></Card>;
}

function ExecutionTimeline({ snapshot }: { snapshot: AnalysisSnapshot | null }) {
  if (!snapshot) {
    return <EmptyState>Analysis loop 事件会在任务开始执行后实时显示。</EmptyState>;
  }
  const events = snapshot.events.slice(-12).reverse();
  return (
    <div className="space-y-3 rounded-lg border border-border bg-slate-50 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="text-sm font-medium">Analysis loop summary</p>
          <p className="text-xs text-muted-foreground">revision {snapshot.state.revision} · {snapshot.state.status} · phase {snapshot.state.currentPhase ?? "none"}</p>
        </div>
        <div className="flex flex-wrap gap-2 text-xs">
          <Badge variant="secondary">rounds {snapshot.state.budget.rounds}</Badge>
          <Badge variant="secondary">LLM {snapshot.state.budget.llmCalls}</Badge>
          <Badge variant="secondary">actions {snapshot.state.budget.actions}</Badge>
          <Badge variant="secondary">evidence {snapshot.state.evidence.length}</Badge>
        </div>
      </div>
      {events.length ? (
        <ol className="space-y-2">
          {events.map((event) => (
            <li className="rounded-md border border-border bg-white p-3" key={`${event.revision}:${event.eventType}:${event.createdAt}`}>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <Badge variant={event.eventType === "analysis_failed" ? "destructive" : event.eventType === "model_decision" ? "warning" : "outline"}>{event.eventType}</Badge>
                <span>rev {event.revision}</span>
                <span>{event.phase ?? "no phase"}</span>
                {event.actionId ? <span className="font-mono">{event.actionId}</span> : null}
                <span>{new Date(event.createdAt).toLocaleTimeString()}</span>
              </div>
              <p className="mt-2 text-sm">{event.message}</p>
              <EventDetails event={event} />
            </li>
          ))}
        </ol>
      ) : <EmptyState>暂无 loop 事件。</EmptyState>}
    </div>
  );
}

function EventDetails({ event }: { event: AnalysisEvent }) {
  const detail = summarizeEventDetails(event);
  const refs = event.evidenceRefs.slice(0, 4);
  if (!detail && !event.artifactPath && refs.length === 0) return null;
  return (
    <div className="mt-2 space-y-1 text-xs text-muted-foreground">
      {detail ? <p>{detail}</p> : null}
      {event.artifactPath ? <p>artifact: <span className="font-mono">{event.artifactPath}</span></p> : null}
      {refs.length ? <p>refs: {refs.map((reference) => <span className="mr-2 font-mono" key={reference}>{reference}</span>)}{event.evidenceRefs.length > refs.length ? `+${event.evidenceRefs.length - refs.length}` : ""}</p> : null}
    </div>
  );
}

function summarizeEventDetails(event: AnalysisEvent) {
  const details = event.details ?? {};
  if (typeof details.callId === "string") {
    const attempt = typeof details.attempt === "number" ? `attempt=${details.attempt}` : "";
    const model = typeof details.model === "string" ? `model=${details.model}` : "";
    const error = typeof details.error === "string" ? ` · error=${details.error}` : "";
    return [details.callId, attempt, model].filter(Boolean).join(" · ") + error;
  }
  if (typeof details.totalMatches === "number") {
    const keywords = Array.isArray(details.keywords) ? details.keywords.filter((item): item is string => typeof item === "string").slice(0, 6).join(", ") : "";
    return `matches=${details.totalMatches}${keywords ? ` · keywords=${keywords}` : ""}`;
  }
  const decision = details.decision;
  if (isRecord(decision)) {
    const type = typeof decision.type === "string" ? decision.type : "action";
    const reason = typeof decision.reason === "string" ? decision.reason : "";
    return `decision=${type}${reason ? ` · ${reason}` : ""}`;
  }
  const result = details.result;
  if (isRecord(result) && typeof result.summary === "string") {
    return `final_answer · ${result.summary}`;
  }
  if (typeof details.error === "string") {
    return details.error;
  }
  return "";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function AnalysisResultView({ result }: { result: AnalysisResult }) {
  return (
    <Card>
      <CardHeader><div className="flex items-center justify-between gap-3"><CardTitle>LLM analysis result</CardTitle><Badge variant="secondary">confidence: {result.confidence}</Badge></div><CardDescription>{result.summary}</CardDescription></CardHeader>
      <CardContent className="grid gap-5 lg:grid-cols-2">
        <ResultList title="Symptoms" items={result.symptoms} />
        <div><h3 className="mb-2 text-sm font-semibold">Likely root causes</h3>{result.likelyRootCauses.length ? result.likelyRootCauses.map((cause, index) => (
          <div className="mb-2 rounded-lg border border-border p-3" key={`${cause.cause}:${index}`}>
            <p className="text-sm">{cause.cause}</p>
            <div className="mt-2 flex flex-wrap gap-2">{cause.evidenceRefs.map((reference) => <button className="font-mono text-xs text-primary underline" key={reference} onClick={() => scrollToEvidence(reference)}>{reference}</button>)}</div>
          </div>
        )) : <p className="text-sm text-muted-foreground">当前证据不足以提出根因。</p>}</div>
        <ResultList title="Next checks" items={result.nextChecks} />
        <ResultList title="Fix suggestions" items={result.fixSuggestions} />
        <ResultList title="Missing information" items={result.missingInformation} />
      </CardContent>
    </Card>
  );
}

function MetadataContextView({ context }: { context: MetadataContext }) {
  const partitions = context.cluster?.partitionViews ?? [];
  const abnormalPartitions = partitions.filter((partition) => partition.statusText && partition.statusText !== "online").length;
  const rows = [
    ["Instance", context.instanceId],
    ["Cluster", context.clusterId],
    ["Node", context.nodeId],
    ["Product", context.product],
    ["Version", context.version],
    ["Environment", context.environment],
    ["Node status", context.node?.status],
    ["Cluster nodes", String(context.clusterNodes?.length ?? 0)],
    ["Databases", (context.cluster?.databases ?? []).map((database) => database.name).join(", ") || "0"],
    ["Partitions", `${partitions.length} total, ${abnormalPartitions} non-online`]
  ];
  return (
    <Card>
      <CardHeader><CardTitle>Metadata context</CardTitle><CardDescription>任务创建时固化的 Metadata 快照</CardDescription></CardHeader>
      <CardContent className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">
        {rows.map(([label, value]) => <div className="rounded-lg border border-border p-3" key={label}><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value || "-"}</p></div>)}
      </CardContent>
    </Card>
  );
}

function ToolResultLine({ result }: { result: ToolResult }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <div className="flex items-center gap-2 text-sm font-medium"><FileArchive className="h-4 w-4 text-slate-400" />{result.tool} · {result.status}</div>
      <p className="mt-1 break-words text-xs text-muted-foreground">exit={result.exitCode ?? "-"} · {result.durationMs}ms · {result.summary} · stdout={result.stdoutPath} · stderr={result.stderrPath}</p>
      {result.findings?.length ? (
        <ul className="mt-3 space-y-2">
          {result.findings.map((finding, index) => (
            <li className="rounded-md bg-slate-50 p-2 text-xs" key={`${finding.message}:${index}`}>
              <span className="font-medium">{finding.severity ?? "finding"}</span>
              <span className="text-muted-foreground"> · {finding.file ?? "-"}{finding.line ? `:${finding.line}` : ""}</span>
              <p className="mt-1 text-slate-700">{finding.message}</p>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function ResultList({ title, items }: { title: string; items: string[] }) {
  return <div><h3 className="mb-2 text-sm font-semibold">{title}</h3>{items.length ? <ul className="space-y-2 text-sm text-muted-foreground">{items.map((item, index) => <li className="rounded-lg border border-border p-3" key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="text-sm text-muted-foreground">暂无</p>}</div>;
}

function scrollToEvidence(reference: string) {
  const index = reference.match(/^grep_results\.json#matches\/(\d+)$/)?.[1];
  if (index) document.getElementById(`grep-match-${index}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
}

function defaultCaseDraft(result: AnalysisResult): CaseDraft {
  return {
    title: result.summary.slice(0, 140),
    symptom: result.symptoms.join("\n"),
    rootCause: result.likelyRootCauses[0]?.cause ?? "",
    solution: result.fixSuggestions.length ? result.fixSuggestions.join("\n") : result.nextChecks.join("\n")
  };
}

function uniqueEvidenceRefs(result: AnalysisResult) {
  const refs: string[] = [];
  for (const cause of result.likelyRootCauses) {
    for (const reference of cause.evidenceRefs) {
      if (!refs.includes(reference)) refs.push(reference);
    }
  }
  return refs;
}

function DataLine({ id, title, detail }: { id?: string; title: string; detail: string }) {
  return <div id={id} className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-sm font-medium"><FileArchive className="h-4 w-4 text-slate-400" />{title}</div><p className="mt-1 break-words text-xs text-muted-foreground">{detail}</p></div>;
}

async function uploadFile(file: File, apiKey: string, onProgress: (value: number) => void) {
  if (file.size <= CHUNK_BYTES) {
    const form = new FormData();
    form.append("filename", file.name);
    form.append("file", file, file.name);
    const result = await fetchJson<UploadResponse>("/api/uploads", { method: "POST", headers: authHeaders(apiKey), body: form });
    onProgress(1);
    return result;
  }
  const upload = await fetchJson<UploadResponse>("/api/uploads/init", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ filename: file.name, size: file.size })
  });
  for (let offset = 0; offset < file.size; offset += CHUNK_BYTES) {
    const next = Math.min(offset + CHUNK_BYTES, file.size);
    await fetchJson(`/api/uploads/${encodeURIComponent(upload.uploadId)}/chunks?offset=${offset}`, {
      method: "POST",
      headers: authHeaders(apiKey),
      body: file.slice(offset, next)
    });
    onProgress(next / file.size);
  }
  return fetchJson<UploadResponse>(`/api/uploads/${encodeURIComponent(upload.uploadId)}/complete`, { method: "POST", headers: authHeaders(apiKey) });
}

async function fetchTaskAnalysis(taskId: string, apiKey: string) {
  try {
    return await fetchJson<AnalysisSnapshot>(`/api/tasks/${encodeURIComponent(taskId)}/analysis`, { headers: authHeaders(apiKey) });
  } catch {
    return null;
  }
}
