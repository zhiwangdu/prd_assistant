import { BookOpenCheck, Clock3, FileArchive, ListChecks, Plus, RefreshCw, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { type UploadResponse, uploadFile } from "./upload";

type TaskStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";
type TaskPhase = "EXTRACT" | "SEARCH_LOGS" | "RUN_TOOL" | "PLAN_ANALYSIS" | "GENERATE_RESULT";
type SessionStatus = "draft" | "ready" | "running" | "waiting_for_user" | "waiting_for_approval" | "succeeded" | "failed";
type SessionSummary = {
  sessionId: string;
  title: string;
  sourceUrl?: string | null;
  instanceId?: string | null;
  nodeId?: string | null;
  uploadCount: number;
  taskCount: number;
  activeTaskId?: string | null;
  status: SessionStatus;
  createdAt: string;
  updatedAt: string;
};
type SessionRecord = Omit<SessionSummary, "uploadCount" | "taskCount"> & {
  schemaVersion: number;
  question: string;
  uploadIds: string[];
  taskIds: string[];
};
type TaskSummary = {
  taskId: string;
  url: string;
  taskKind?: "log_analysis" | "tool_run";
  sessionId?: string | null;
  status: TaskStatus;
  phase?: TaskPhase | null;
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
    currentPhase?: TaskPhase | null;
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
  phase?: TaskPhase | null;
  actionId?: string | null;
  message: string;
  evidenceRefs: string[];
  artifactPath?: string | null;
  details?: Record<string, unknown>;
  createdAt: string;
};
type SessionTimelineEvent = {
  source: "session" | "task" | string;
  eventType: string;
  sessionId: string;
  taskId?: string | null;
  phase?: TaskPhase | null;
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
  caseContext?: CaseContext | null;
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
type CaseContext = {
  schemaVersion: number;
  query: string;
  cases: CaseHit[];
};
type CaseDraft = {
  title: string;
  symptom: string;
  rootCause: string;
  solution: string;
};

export function OperationsView({ apiKey }: { apiKey: string }) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selectedSession, setSelectedSession] = useState<SessionRecord | null>(null);
  const [sessionTasks, setSessionTasks] = useState<TaskRecord[]>([]);
  const [selectedTask, setSelectedTask] = useState<TaskRecord | null>(null);
  const [timeline, setTimeline] = useState<SessionTimelineEvent[]>([]);
  const [files, setFiles] = useState<File[]>([]);
  const [title, setTitle] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [question, setQuestion] = useState("分析日志中的主要异常、可能原因和建议检查项。");
  const [instanceId, setInstanceId] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [uploadStatus, setUploadStatus] = useState("请选择或创建 Session");
  const [nativeStatus, setNativeStatus] = useState("Native Agent not checked");
  const [uploadProgress, setUploadProgress] = useState(0);
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

  const refreshSessions = useCallback(async () => {
    if (!apiKey.trim()) {
      setSessions([]);
      return;
    }
    const result = await fetchJson<{ sessions: SessionSummary[] }>("/api/sessions", { headers: authHeaders(apiKey) });
    setSessions(result.sessions);
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

  const loadTask = useCallback(async (taskId: string) => {
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
    return task;
  }, [apiKey]);

  const loadSessionArtifacts = useCallback(async (session: SessionRecord, preferredTaskId?: string | null) => {
    const taskIds = [...session.taskIds].reverse();
    const tasks = await Promise.all(taskIds.map((taskId) => fetchJson<TaskRecord>(`/api/tasks/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) }).catch(() => null)));
    const validTasks = tasks.filter((task): task is TaskRecord => Boolean(task));
    setSessionTasks(validTasks);
    const activeTaskId = preferredTaskId ?? session.activeTaskId ?? validTasks[0]?.taskId ?? null;
    if (activeTaskId) {
      await loadTask(activeTaskId);
    } else {
      setSelectedTask(null);
      setArtifacts(null);
      setTaskResult(null);
      setAnalysisSnapshot(null);
    }
    const timelineResponse = await fetchJson<{ events: SessionTimelineEvent[] }>(`/api/sessions/${encodeURIComponent(session.sessionId)}/timeline`, { headers: authHeaders(apiKey) });
    setTimeline(timelineResponse.events);
  }, [apiKey, loadTask]);

  const selectSession = useCallback(async (sessionId: string, syncDraft = true, preferredTaskId?: string | null) => {
    const session = await fetchJson<SessionRecord>(`/api/sessions/${encodeURIComponent(sessionId)}`, { headers: authHeaders(apiKey) });
    setSelectedSession(session);
    if (syncDraft) {
      setTitle(session.title);
      setQuestion(session.question);
      setSourceUrl(session.sourceUrl ?? "");
      setInstanceId(session.instanceId ?? "");
      setNodeId(session.nodeId ?? "");
      setNativeStatus("Setting Native Agent session...");
      await setNativeCurrentSession(session.sessionId)
        .then(() => setNativeStatus(`Native Agent active: ${session.sessionId}`))
        .catch((reason) => setNativeStatus(`Native Agent not connected: ${errorMessage(reason)}`));
    }
    await loadSessionArtifacts(session, preferredTaskId);
  }, [apiKey, loadSessionArtifacts]);

  useEffect(() => {
    setSelectedSession(null);
    setSessionTasks([]);
    setSelectedTask(null);
    setArtifacts(null);
    setTaskResult(null);
    setAnalysisSnapshot(null);
    setTimeline([]);
    setCases([]);
    void refreshSessions().catch((reason) => setUploadStatus(errorMessage(reason)));
    void refreshCases("").catch((reason) => setCaseStatus(errorMessage(reason)));
  }, [refreshCases, refreshSessions]);

  useEffect(() => {
    if (!selectedSession || !apiKey.trim()) return;
    const patch = {
      title: title.trim(),
      question: question.trim(),
      sourceUrl: sourceUrl.trim(),
      instanceId: instanceId.trim(),
      nodeId: nodeId.trim()
    };
    const unchanged =
      patch.title === selectedSession.title &&
      patch.question === selectedSession.question &&
      (patch.sourceUrl || "") === (selectedSession.sourceUrl ?? "") &&
      (patch.instanceId || "") === (selectedSession.instanceId ?? "") &&
      (patch.nodeId || "") === (selectedSession.nodeId ?? "");
    if (unchanged) return;
    const timer = window.setTimeout(() => {
      void fetchJson<SessionRecord>(`/api/sessions/${encodeURIComponent(selectedSession.sessionId)}`, {
        method: "PATCH",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          title: patch.title || null,
          question: patch.question || null,
          sourceUrl: patch.sourceUrl || null,
          instanceId: patch.instanceId || null,
          nodeId: patch.nodeId || null
        })
      })
        .then((session) => {
          setSelectedSession(session);
          void refreshSessions();
        })
        .catch((reason) => setUploadStatus(errorMessage(reason)));
    }, 500);
    return () => window.clearTimeout(timer);
  }, [apiKey, instanceId, nodeId, question, refreshSessions, selectedSession, sourceUrl, title]);

  useEffect(() => {
    if (!taskResult) {
      setCaseDraft({ title: "", symptom: "", rootCause: "", solution: "" });
      return;
    }
    setCaseDraft(defaultCaseDraft(taskResult.result));
  }, [taskResult]);

  useEffect(() => {
    if (!apiKey.trim() || !selectedSession) return;
    const timer = window.setInterval(() => {
      void refreshSessions().catch(() => undefined);
      void selectSession(selectedSession.sessionId, false, selectedTask?.taskId).catch(() => undefined);
    }, selectedTask && !isTerminal(selectedTask.status) ? 1000 : 3000);
    return () => window.clearInterval(timer);
  }, [apiKey, refreshSessions, selectSession, selectedSession, selectedTask]);

  async function createSession() {
    if (!apiKey.trim()) {
      setUploadStatus("请填写 API Key");
      return;
    }
    setLoading(true);
    try {
      const session = await fetchJson<SessionRecord>("/api/sessions", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ title: "New session", question })
      });
      setUploadStatus(`已创建 Session ${session.sessionId}`);
      await refreshSessions();
      await selectSession(session.sessionId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function uploadToSession() {
    if (!selectedSession || !files.length || !apiKey.trim()) {
      setUploadStatus(!selectedSession ? "请选择或创建 Session" : !files.length ? "请选择日志文件" : "请填写 API Key");
      return;
    }
    setLoading(true);
    try {
      const uploads: UploadResponse[] = [];
      for (let index = 0; index < files.length; index += 1) {
        setUploadStatus(`上传 ${files[index].name}`);
        uploads.push(await uploadFile(files[index], apiKey, (value) => setUploadProgress(Math.round(((index + value) / files.length) * 100))));
      }
      await fetchJson<SessionRecord>(`/api/sessions/${encodeURIComponent(selectedSession.sessionId)}/uploads`, {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ uploadIds: uploads.map((upload) => upload.uploadId) })
      });
      setUploadProgress(100);
      setFiles([]);
      setUploadStatus(`已附加 ${uploads.length} 个上传到 ${selectedSession.sessionId}`);
      await refreshSessions();
      await selectSession(selectedSession.sessionId, false, selectedTask?.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function startAnalysis() {
    if (!selectedSession || !apiKey.trim()) return;
    if (!selectedSession.uploadIds.length) {
      setUploadStatus("当前 Session 没有日志输入");
      return;
    }
    setLoading(true);
    setArtifacts(null);
    setTaskResult(null);
    setAnalysisSnapshot(null);
    try {
      const savedSession = await fetchJson<SessionRecord>(`/api/sessions/${encodeURIComponent(selectedSession.sessionId)}`, {
        method: "PATCH",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          title: title.trim() || null,
          question: question.trim() || null,
          sourceUrl: sourceUrl.trim() || null,
          instanceId: instanceId.trim() || null,
          nodeId: nodeId.trim() || null
        })
      });
      setSelectedSession(savedSession);
      const task = await fetchJson<TaskSummary>(`/api/sessions/${encodeURIComponent(selectedSession.sessionId)}/tasks`, {
        method: "POST",
        headers: authHeaders(apiKey)
      });
      setUploadStatus(`已创建分析 run ${task.taskId}`);
      await refreshSessions();
      await selectSession(selectedSession.sessionId, false, task.taskId);
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
      if (selectedSession) await selectSession(selectedSession.sessionId, false, selectedTask.taskId);
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
      if (selectedSession) await selectSession(selectedSession.sessionId, false, selectedTask.taskId);
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
      <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <CardTitle>Session history</CardTitle>
              <div className="flex gap-2">
                <Button className="h-8 px-3" variant="outline" onClick={() => void refreshSessions()}><RefreshCw className="h-4 w-4" /></Button>
                <Button className="h-8 px-3" disabled={loading} onClick={() => void createSession()}><Plus className="mr-1 h-4 w-4" />New</Button>
              </div>
            </div>
            <CardDescription>{nativeStatus}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-2">
            {sessions.length ? sessions.map((session) => (
              <button key={session.sessionId} className={`w-full rounded-lg border p-3 text-left ${selectedSession?.sessionId === session.sessionId ? "border-primary bg-slate-50" : "border-border"}`} onClick={() => void selectSession(session.sessionId)}>
                <div className="flex items-center justify-between gap-2"><span className="truncate text-sm font-medium">{session.title}</span><SessionBadge status={session.status} /></div>
                <p className="mt-1 font-mono text-xs text-muted-foreground">{session.sessionId}</p>
                <p className="mt-1 text-xs text-muted-foreground">uploads {session.uploadCount} · runs {session.taskCount} · {new Date(session.updatedAt).toLocaleString()}</p>
              </button>
            )) : <EmptyState>暂无 Session。</EmptyState>}
          </CardContent>
        </Card>

        {selectedSession ? (
          <div className="space-y-5">
            <Card>
              <CardHeader><CardTitle>Session draft</CardTitle><CardDescription>{selectedSession.sessionId} · {selectedSession.uploadIds.length} upload(s)</CardDescription></CardHeader>
              <CardContent className="space-y-4">
                <Input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Session title" />
                <Input value={sourceUrl} onChange={(event) => setSourceUrl(event.target.value)} placeholder="Source URL (optional)" />
                <div className="grid gap-3 md:grid-cols-2">
                  <Input value={instanceId} onChange={(event) => setInstanceId(event.target.value)} placeholder="Instance ID (optional)" />
                  <Input value={nodeId} onChange={(event) => setNodeId(event.target.value)} placeholder="Node ID (optional)" />
                </div>
                <textarea className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={question} onChange={(event) => setQuestion(event.target.value)} placeholder="希望 LLM 分析的问题" />
                <label className="flex min-h-32 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
                  <UploadCloud className="mb-2 h-7 w-7" />
                  {files.length ? `${files.length} file(s): ${files.map((file) => file.name).join(", ")}` : "选择 .log / .txt / .zip / .tar.gz / .tgz / .tar"}
                  <input className="hidden" type="file" multiple onChange={(event) => setFiles(Array.from(event.target.files ?? []))} />
                </label>
                <div>
                  <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>Upload</span><span>{uploadProgress}%</span></div>
                  <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
                </div>
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <span className="text-sm text-muted-foreground">{uploadStatus}</span>
                  <div className="flex flex-wrap gap-2">
                    <Button disabled={loading || !files.length} variant="outline" onClick={() => void uploadToSession()}>Upload to session</Button>
                    <Button disabled={loading || !selectedSession.uploadIds.length} onClick={() => void startAnalysis()}><ListChecks className="mr-2 h-4 w-4" />Start analysis</Button>
                  </div>
                </div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader><CardTitle>Runs</CardTitle><CardDescription>{selectedTask ? `${selectedTask.taskId} · attempt ${selectedTask.attempts ?? 0}` : "No run selected"}</CardDescription></CardHeader>
              <CardContent className="space-y-4">
                {sessionTasks.length ? (
                  <div className="flex flex-wrap gap-2">
                    {sessionTasks.map((task) => <button className={`rounded-md border px-3 py-2 text-left text-xs ${selectedTask?.taskId === task.taskId ? "border-primary bg-slate-50" : "border-border"}`} key={task.taskId} onClick={() => void loadTask(task.taskId)}><span className="font-mono">{task.taskId}</span><span className="ml-2"><StatusBadge status={task.status} /></span></button>)}
                  </div>
                ) : <EmptyState>当前 Session 还没有分析 run。</EmptyState>}
                {selectedTask ? (
                  <div className="space-y-3">
                    <div className="flex items-center gap-2"><StatusBadge status={selectedTask.status} /><span className="text-sm text-muted-foreground">{selectedTask.phase ?? "No active phase"}</span></div>
                    {selectedTask.instanceId || selectedTask.nodeId ? <p className="text-xs text-muted-foreground">Metadata: instance={selectedTask.instanceId ?? "-"} · node={selectedTask.nodeId ?? "-"}</p> : null}
                    {selectedTask.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedTask.error?.phase ? `${selectedTask.error.phase}: ` : ""}{selectedTask.error?.message ?? "Task failed"}</div> : null}
                    <WaitingInteraction answer={userAnswer} approvalReason={approvalReason} loading={loading} snapshot={analysisSnapshot} status={selectedTask.status} onAnswerChange={setUserAnswer} onApprovalReasonChange={setApprovalReason} onSubmitAnswer={(prompt) => void submitUserMessage(prompt)} onSubmitApproval={(approval, decision) => void submitApproval(approval, decision)} />
                  </div>
                ) : null}
              </CardContent>
            </Card>

            <SessionTimeline events={timeline} snapshot={analysisSnapshot} />
          </div>
        ) : (
          <Card>
            <CardHeader><CardTitle>Log analysis Session</CardTitle><CardDescription>Create or select a Session to start.</CardDescription></CardHeader>
            <CardContent><Button disabled={loading || !apiKey.trim()} onClick={() => void createSession()}><Plus className="mr-2 h-4 w-4" />New session</Button></CardContent>
          </Card>
        )}
      </div>

      {taskResult ? <AnalysisResultView result={taskResult.result} /> : null}

      {taskResult && selectedTask ? (
        <CaseClosurePanel cases={cases} caseDraft={caseDraft} caseQuery={caseQuery} caseStatus={caseStatus} loading={loading} taskId={selectedTask.taskId} onDraftChange={setCaseDraft} onQueryChange={setCaseQuery} onRefreshCases={() => void refreshCases(caseQuery)} onConfirmCase={() => void confirmCase()} onDisableCase={(caseId) => void disableCase(caseId)} />
      ) : null}

      {artifacts?.metadataContext ? <MetadataContextView context={artifacts.metadataContext} /> : null}
      {artifacts?.caseContext ? <TaskCaseContextView context={artifacts.caseContext} /> : null}
      {artifacts?.toolResults?.length ? <Evidence title="Tool results" count={artifacts.toolResults.length}>{artifacts.toolResults.map((result) => <ToolResultLine key={result.actionId} result={result} />)}</Evidence> : null}
      {artifacts ? (
        <div className="grid gap-5 xl:grid-cols-2">
          <Evidence title="Manifest" count={artifacts.manifest?.files?.length ?? 0}>{(artifacts.manifest?.files ?? []).map((file) => <DataLine key={file.path} title={file.path} detail={`${file.size.toLocaleString()} bytes`} />)}</Evidence>
          <Evidence title="Grep matches" count={artifacts.grepResults?.matches?.length ?? 0}>{(artifacts.grepResults?.matches ?? []).map((match, index) => <DataLine id={`grep-match-${index}`} key={`${match.file}:${match.line}:${index}`} title={`${match.file}:${match.line}`} detail={`${match.keyword} · ${match.text}`} />)}</Evidence>
        </div>
      ) : null}
    </div>
  );
}

function WaitingInteraction({ answer, approvalReason, loading, snapshot, status, onAnswerChange, onApprovalReasonChange, onSubmitAnswer, onSubmitApproval }: { answer: string; approvalReason: string; loading: boolean; snapshot: AnalysisSnapshot | null; status: TaskStatus; onAnswerChange: (value: string) => void; onApprovalReasonChange: (value: string) => void; onSubmitAnswer: (prompt: PendingUserPrompt) => void; onSubmitApproval: (approval: PendingApproval, decision: "approved" | "rejected") => void; }) {
  if (status === "WAITING_FOR_USER") {
    const prompt = snapshot?.state.pendingUserPrompts[0];
    if (!prompt) return <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">任务正在等待用户输入，但 analysis state 中暂无 pending prompt。</div>;
    return <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-3"><div><p className="text-sm font-medium text-amber-900">需要补充信息</p><p className="mt-1 text-sm text-amber-800">{prompt.question}</p><p className="mt-1 text-xs text-amber-700">reason: {prompt.reason} · format: {prompt.answerFormat ?? "free text"} · required: {prompt.required ? "yes" : "no"}</p></div><textarea className="min-h-20 w-full rounded-md border border-amber-200 bg-white px-3 py-2 text-sm" value={answer} onChange={(event) => onAnswerChange(event.target.value)} placeholder="填写补充信息后继续分析" /><Button disabled={loading} onClick={() => onSubmitAnswer(prompt)}>提交回答并继续</Button></div>;
  }
  if (status === "WAITING_FOR_APPROVAL") {
    const approval = snapshot?.state.pendingApprovals[0];
    if (!approval) return <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">任务正在等待审批，但 analysis state 中暂无 pending approval。</div>;
    return <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-3"><div><p className="text-sm font-medium text-amber-900">需要审批动作</p><p className="mt-1 text-sm text-amber-800">{approval.actionType} · {approval.actionId}</p><p className="mt-1 text-xs text-amber-700">risk: {approval.risk} · reason: {approval.reason}</p><pre className="mt-2 max-h-32 overflow-auto rounded bg-white p-2 text-xs text-slate-700">{JSON.stringify(approval.input, null, 2)}</pre></div><Input value={approvalReason} onChange={(event) => onApprovalReasonChange(event.target.value)} placeholder="审批备注或拒绝原因（可选）" /><div className="flex flex-wrap gap-2"><Button disabled={loading} onClick={() => onSubmitApproval(approval, "approved")}>批准并继续</Button><Button disabled={loading} variant="outline" onClick={() => onSubmitApproval(approval, "rejected")}>拒绝并继续</Button></div></div>;
  }
  return null;
}

function CaseClosurePanel({ cases, caseDraft, caseQuery, caseStatus, loading, taskId, onDraftChange, onQueryChange, onRefreshCases, onConfirmCase, onDisableCase }: { cases: CaseHit[]; caseDraft: CaseDraft; caseQuery: string; caseStatus: string; loading: boolean; taskId: string; onDraftChange: (draft: CaseDraft) => void; onQueryChange: (value: string) => void; onRefreshCases: () => void; onConfirmCase: () => void; onDisableCase: (caseId: string) => void; }) {
  return <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_420px]"><Card><CardHeader><div className="flex items-center gap-2"><BookOpenCheck className="h-5 w-5 text-primary" /><CardTitle>Confirm as Case</CardTitle></div><CardDescription>{taskId} 的最终结果可人工确认后沉淀为可召回 Case</CardDescription></CardHeader><CardContent className="space-y-3"><Input value={caseDraft.title} onChange={(event) => onDraftChange({ ...caseDraft, title: event.target.value })} placeholder="Case title" /><textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.symptom} onChange={(event) => onDraftChange({ ...caseDraft, symptom: event.target.value })} placeholder="Symptom" /><textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.rootCause} onChange={(event) => onDraftChange({ ...caseDraft, rootCause: event.target.value })} placeholder="Root cause" /><textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.solution} onChange={(event) => onDraftChange({ ...caseDraft, solution: event.target.value })} placeholder="Solution" /><div className="flex flex-wrap items-center justify-between gap-3"><span className="text-sm text-muted-foreground">{caseStatus}</span><Button disabled={loading || !caseDraft.title.trim()} onClick={onConfirmCase}>保存 Case</Button></div></CardContent></Card><Card><CardHeader><div className="flex items-center justify-between gap-3"><CardTitle>Similar cases</CardTitle><Button className="h-8 px-3" variant="outline" onClick={onRefreshCases}><RefreshCw className="h-4 w-4" /></Button></div><CardDescription>本地 JSON Case Store 关键词召回</CardDescription></CardHeader><CardContent className="space-y-3"><Input value={caseQuery} onChange={(event) => onQueryChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") onRefreshCases(); }} placeholder="Search cases" />{cases.length ? cases.map((item) => <div className="rounded-lg border border-border p-3" key={item.caseId}><div className="flex items-start justify-between gap-3"><div><p className="text-sm font-medium">{item.title}</p><p className="mt-1 text-xs text-muted-foreground">{item.caseId} · {item.sourceType} · score {item.score.toFixed(2)} · {new Date(item.createdAt).toLocaleDateString()}</p></div><Badge variant={item.enabled ? "secondary" : "destructive"}>{item.enabled ? "enabled" : "disabled"}</Badge></div><p className="mt-2 text-xs text-muted-foreground">{item.rootCause}</p><div className="mt-3 flex flex-wrap gap-2"><Button className="h-8 px-3" disabled={loading || !item.enabled} variant="outline" onClick={() => onDisableCase(item.caseId)}>禁用</Button></div></div>) : <EmptyState>暂无匹配 Case。</EmptyState>}</CardContent></Card></div>;
}

function StatusBadge({ status }: { status: TaskStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{status}</Badge>;
}

function SessionBadge({ status }: { status: SessionStatus }) {
  return <Badge variant={status === "failed" ? "destructive" : status === "succeeded" ? "default" : status === "running" || status.startsWith("waiting") ? "warning" : "secondary"}>{status}</Badge>;
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

function SessionTimeline({ events, snapshot }: { events: SessionTimelineEvent[]; snapshot: AnalysisSnapshot | null }) {
  const latest = events.slice(-18).reverse();
  return <Card><CardHeader><div className="flex items-center justify-between gap-3"><div><CardTitle>Evidence timeline</CardTitle><CardDescription>{snapshot ? `revision ${snapshot.state.revision} · ${snapshot.state.status} · phase ${snapshot.state.currentPhase ?? "none"}` : "Session and task events"}</CardDescription></div>{snapshot ? <div className="flex flex-wrap gap-2 text-xs"><Badge variant="secondary">rounds {snapshot.state.budget.rounds}</Badge><Badge variant="secondary">LLM {snapshot.state.budget.llmCalls}</Badge><Badge variant="secondary">actions {snapshot.state.budget.actions}</Badge><Badge variant="secondary">evidence {snapshot.state.evidence.length}</Badge></div> : null}</div></CardHeader><CardContent>{latest.length ? <ol className="space-y-2">{latest.map((event, index) => <li className="rounded-md border border-border bg-white p-3" key={`${event.createdAt}:${event.eventType}:${index}`}><div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"><Badge variant={event.eventType === "analysis_failed" ? "destructive" : event.eventType === "model_decision" ? "warning" : "outline"}>{event.source}:{event.eventType}</Badge>{event.phase ? <span>{event.phase}</span> : null}{event.taskId ? <span className="font-mono">{event.taskId}</span> : null}{event.actionId ? <span className="font-mono">{event.actionId}</span> : null}<span><Clock3 className="mr-1 inline h-3 w-3" />{new Date(event.createdAt).toLocaleTimeString()}</span></div><p className="mt-2 text-sm">{event.message}</p><EventDetails event={event} /></li>)}</ol> : <EmptyState>暂无 timeline 事件。</EmptyState>}</CardContent></Card>;
}

function EventDetails({ event }: { event: SessionTimelineEvent }) {
  const detail = summarizeEventDetails(event.details ?? {});
  const refs = event.evidenceRefs.slice(0, 4);
  if (!detail && !event.artifactPath && refs.length === 0) return null;
  return <div className="mt-2 space-y-1 text-xs text-muted-foreground">{detail ? <p>{detail}</p> : null}{event.artifactPath ? <p>artifact: <span className="font-mono">{event.artifactPath}</span></p> : null}{refs.length ? <p>refs: {refs.map((reference) => <span className="mr-2 font-mono" key={reference}>{reference}</span>)}{event.evidenceRefs.length > refs.length ? `+${event.evidenceRefs.length - refs.length}` : ""}</p> : null}</div>;
}

function summarizeEventDetails(details: Record<string, unknown>) {
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
  if (isRecord(result) && typeof result.summary === "string") return `final_answer · ${result.summary}`;
  if (typeof details.caseRecallCount === "number") return `case recall count=${details.caseRecallCount}`;
  if (typeof details.error === "string") return details.error;
  return "";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function AnalysisResultView({ result }: { result: AnalysisResult }) {
  return <Card><CardHeader><div className="flex items-center justify-between gap-3"><CardTitle>LLM analysis result</CardTitle><Badge variant="secondary">confidence: {result.confidence}</Badge></div><CardDescription>{result.summary}</CardDescription></CardHeader><CardContent className="grid gap-5 lg:grid-cols-2"><ResultList title="Symptoms" items={result.symptoms} /><div><h3 className="mb-2 text-sm font-semibold">Likely root causes</h3>{result.likelyRootCauses.length ? result.likelyRootCauses.map((cause, index) => <div className="mb-2 rounded-lg border border-border p-3" key={`${cause.cause}:${index}`}><p className="text-sm">{cause.cause}</p><div className="mt-2 flex flex-wrap gap-2">{cause.evidenceRefs.map((reference) => <button className="font-mono text-xs text-primary underline" key={reference} onClick={() => scrollToEvidence(reference)}>{reference}</button>)}</div></div>) : <p className="text-sm text-muted-foreground">当前证据不足以提出根因。</p>}</div><ResultList title="Next checks" items={result.nextChecks} /><ResultList title="Fix suggestions" items={result.fixSuggestions} /><ResultList title="Missing information" items={result.missingInformation} /></CardContent></Card>;
}

function MetadataContextView({ context }: { context: MetadataContext }) {
  const partitions = context.cluster?.partitionViews ?? [];
  const abnormalPartitions = partitions.filter((partition) => partition.statusText && partition.statusText !== "online").length;
  const rows = [["Instance", context.instanceId], ["Node", context.nodeId], ["Product", context.product], ["Version", context.version], ["Environment", context.environment], ["Node status", context.node?.status], ["Cluster nodes", String(context.clusterNodes?.length ?? 0)], ["Databases", (context.cluster?.databases ?? []).map((database) => database.name).join(", ") || "0"], ["Partitions", `${partitions.length} total, ${abnormalPartitions} non-online`]];
  return <Card><CardHeader><CardTitle>Metadata context</CardTitle><CardDescription>任务创建时固化的 Metadata 快照</CardDescription></CardHeader><CardContent className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">{rows.map(([label, value]) => <div className="rounded-lg border border-border p-3" key={label}><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value || "-"}</p></div>)}</CardContent></Card>;
}

function TaskCaseContextView({ context }: { context: CaseContext }) {
  return <Card><CardHeader><CardTitle>Case context</CardTitle><CardDescription>任务创建时按问题召回的历史 Case，仅作为分析参考</CardDescription></CardHeader><CardContent className="space-y-3"><p className="text-xs text-muted-foreground">query: {context.query || "-"}</p>{context.cases.length ? context.cases.map((item) => <div className="rounded-lg border border-border p-3" key={item.caseId}><div className="flex flex-wrap items-center gap-2"><span className="text-sm font-medium">{item.title}</span><Badge variant="secondary">score {item.score.toFixed(2)}</Badge></div><p className="mt-1 text-xs text-muted-foreground">{item.caseId} · {item.sourceType} · {item.product ?? "unknown"} {item.version ?? ""}</p><p className="mt-2 text-sm">{item.rootCause}</p></div>) : <EmptyState>任务创建时未召回相似 Case。</EmptyState>}</CardContent></Card>;
}

function ToolResultLine({ result }: { result: ToolResult }) {
  return <div className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-sm font-medium"><FileArchive className="h-4 w-4 text-slate-400" />{result.tool} · {result.status}</div><p className="mt-1 break-words text-xs text-muted-foreground">exit={result.exitCode ?? "-"} · {result.durationMs}ms · {result.summary} · stdout={result.stdoutPath} · stderr={result.stderrPath}</p>{result.findings?.length ? <ul className="mt-3 space-y-2">{result.findings.map((finding, index) => <li className="rounded-md bg-slate-50 p-2 text-xs" key={`${finding.message}:${index}`}><span className="font-medium">{finding.severity ?? "finding"}</span><span className="text-muted-foreground"> · {finding.file ?? "-"}{finding.line ? `:${finding.line}` : ""}</span><p className="mt-1 text-slate-700">{finding.message}</p></li>)}</ul> : null}</div>;
}

function ResultList({ title, items }: { title: string; items: string[] }) {
  return <div><h3 className="mb-2 text-sm font-semibold">{title}</h3>{items.length ? <ul className="space-y-2 text-sm text-muted-foreground">{items.map((item, index) => <li className="rounded-lg border border-border p-3" key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="text-sm text-muted-foreground">暂无</p>}</div>;
}

function scrollToEvidence(reference: string) {
  const index = reference.match(/^grep_results\.json#matches\/(\d+)$/)?.[1];
  if (index) document.getElementById(`grep-match-${index}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
}

function defaultCaseDraft(result: AnalysisResult): CaseDraft {
  return { title: result.summary.slice(0, 140), symptom: result.symptoms.join("\n"), rootCause: result.likelyRootCauses[0]?.cause ?? "", solution: result.fixSuggestions.length ? result.fixSuggestions.join("\n") : result.nextChecks.join("\n") };
}

function uniqueEvidenceRefs(result: AnalysisResult) {
  const refs: string[] = [];
  for (const cause of result.likelyRootCauses) for (const reference of cause.evidenceRefs) if (!refs.includes(reference)) refs.push(reference);
  return refs;
}

function DataLine({ id, title, detail }: { id?: string; title: string; detail: string }) {
  return <div id={id} className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-sm font-medium"><FileArchive className="h-4 w-4 text-slate-400" />{title}</div><p className="mt-1 break-words text-xs text-muted-foreground">{detail}</p></div>;
}

async function fetchTaskAnalysis(taskId: string, apiKey: string) {
  try {
    return await fetchJson<AnalysisSnapshot>(`/api/tasks/${encodeURIComponent(taskId)}/analysis`, { headers: authHeaders(apiKey) });
  } catch {
    return null;
  }
}

async function setNativeCurrentSession(sessionId: string) {
  const response = await fetch("http://127.0.0.1:17321/workspace/current", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ sessionId })
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}
