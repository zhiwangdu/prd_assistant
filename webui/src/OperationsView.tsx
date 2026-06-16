import { BookOpenCheck, BrainCircuit, CheckCircle2, ChevronDown, ChevronRight, Clock3, FileArchive, ListChecks, Plus, RefreshCw, Trash2, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import { analysisCopy, confidenceLabel, eventTypeLabel, sessionStatusLabel, taskPhaseLabel, taskStatusLabel, type AnalysisCopy, type UiLanguage } from "./i18n";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { type UploadResponse, uploadFile } from "./upload";
import { V2AnalyzeBridge } from "./V2AnalyzeBridge";

type TaskStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";
type TaskPhase = "EXTRACT" | "SEARCH_LOGS" | "RUN_TOOL" | "PLAN_ANALYSIS" | "GENERATE_RESULT";
type UserMessageResumeMode = "continue" | "finalize";
type AnalysisLanguage = UiLanguage;
type SessionStatus = "draft" | "ready" | "running" | "waiting_for_user" | "waiting_for_approval" | "succeeded" | "failed";
type SessionSummary = {
  sessionId: string;
  title: string;
  sourceUrl?: string | null;
  instanceId?: string | null;
  nodeId?: string | null;
  analysisLanguage: AnalysisLanguage;
  systemContextCount?: number;
  skillCount?: number;
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
  systemContextIds: string[];
  skillIds: string[];
  uploadIds: string[];
  taskIds: string[];
};
type TaskSummary = {
  taskId: string;
  alias?: string | null;
  url: string;
  taskKind?: "log_analysis" | "tool_run";
  sessionId?: string | null;
  status: TaskStatus;
  analysisLanguage: AnalysisLanguage;
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
    analysisLanguage?: AnalysisLanguage | null;
    currentPhase?: TaskPhase | null;
    budget: { rounds: number; llmCalls: number; actions: number };
    evidence: Array<{ evidenceType: string; artifactPath: string; summary: string; evidenceRefs: string[]; createdAt: string }>;
    actions: Array<{ actionId: string; actionType: string; status: string; summary: string; createdAt: string }>;
    userMessages: Array<{ messageId: string; questionId?: string | null; content: string; resumeMode?: UserMessageResumeMode; createdAt: string }>;
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
const FINALIZE_WITH_CURRENT_EVIDENCE_MESSAGE = "没有更多补充信息，请基于当前已有证据直接生成最终分析结果；如证据不足，请在缺失信息和置信度中说明。";
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
  textInput?: { question?: string } | null;
  metadataContext?: MetadataContext | null;
  caseContext?: CaseContext | null;
  systemContext?: SystemContextBundle | null;
  analysisPackagePath?: string | null;
  analysisPackage?: AgentAnalysisPackage | null;
  agentResponsePath?: string | null;
  agentResponse?: AgentResponseArtifact | null;
  claudeMcpConfigPath?: string | null;
  claudeMcpConfig?: Record<string, unknown> | null;
  claudeSessionPath?: string | null;
  claudeSession?: ClaudeSessionArtifact | null;
  mcpCallsPath?: string | null;
  mcpCalls?: Array<Record<string, unknown>>;
  toolResults?: ToolResult[];
};
type AgentAnalysisPackage = {
  runtimeStatus?: string;
  purpose?: string;
  generatedAt?: string;
  boundaries?: Record<string, unknown>;
};
type AgentResponseArtifact = {
  runtimeStatus?: string;
  claudeSessionId?: string | null;
  analysisMode?: string;
  permissionProfile?: string;
  reason?: string;
  durationMs?: number;
  structuredOutput?: unknown;
  usage?: unknown;
  cost?: unknown;
  nativeToolPolicy?: unknown;
  error?: string | null;
};
type ClaudeSessionArtifact = {
  runtimeStatus?: string;
  claudeSessionId?: string | null;
  analysisMode?: string;
  permissionProfile?: string;
  mcpConfigPath?: string;
  lastClaudeResponsePath?: string;
  durationMs?: number | null;
};
type SkillSummary = {
  skillId: string;
  displayName: string;
  description: string;
  managed: boolean;
  includeByDefault: boolean;
  priority: number;
  revision: string;
  products: string[];
  toolIds: string[];
  references: Array<{ referenceId: string; path: string; title: string; summary: string }>;
};
type SystemContextBundle = {
  resources?: Array<{ contextId: string; kind: string; title: string; summary?: string | null; source: string; skillId?: string | null; revision?: string | null; references?: Array<{ referenceId: string; path: string; title: string; summary: string }> }>;
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

export function OperationsView({ apiKey, language }: { apiKey: string; language: UiLanguage }) {
  const copy = analysisCopy[language];
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selectedSession, setSelectedSession] = useState<SessionRecord | null>(null);
  const [sessionTasks, setSessionTasks] = useState<TaskRecord[]>([]);
  const [selectedTask, setSelectedTask] = useState<TaskRecord | null>(null);
  const [timeline, setTimeline] = useState<SessionTimelineEvent[]>([]);
  const [files, setFiles] = useState<File[]>([]);
  const [title, setTitle] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [question, setQuestion] = useState<string>(copy.defaultQuestion);
  const [instanceId, setInstanceId] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>([]);
  const [uploadStatus, setUploadStatus] = useState<string>(copy.selectOrCreateSession);
  const [nativeStatus, setNativeStatus] = useState<string>(copy.nativeNotChecked);
  const [uploadProgress, setUploadProgress] = useState(0);
  const [artifacts, setArtifacts] = useState<Artifacts | null>(null);
  const [taskResult, setTaskResult] = useState<TaskResult | null>(null);
  const [analysisSnapshot, setAnalysisSnapshot] = useState<AnalysisSnapshot | null>(null);
  const [cases, setCases] = useState<CaseHit[]>([]);
  const [caseQuery, setCaseQuery] = useState("");
  const [caseStatus, setCaseStatus] = useState<string>(copy.caseStoreReady);
  const [caseDraft, setCaseDraft] = useState<CaseDraft>({ title: "", symptom: "", rootCause: "", solution: "" });
  const [loading, setLoading] = useState(false);
  const [userAnswer, setUserAnswer] = useState("");
  const [approvalReason, setApprovalReason] = useState("");
  const [draftExpanded, setDraftExpanded] = useState(true);
  const [timelineExpanded, setTimelineExpanded] = useState(true);
  const taskStatusRef = useRef<{ taskId: string; status: TaskStatus } | null>(null);
  const previousLanguageRef = useRef(language);

  useEffect(() => {
    const previousLanguage = previousLanguageRef.current;
    if (previousLanguage !== language) {
      const previousDefaultQuestion = analysisCopy[previousLanguage].defaultQuestion;
      if (question === previousDefaultQuestion) setQuestion(copy.defaultQuestion);
      if (uploadStatus === analysisCopy[previousLanguage].selectOrCreateSession) setUploadStatus(copy.selectOrCreateSession);
      if (nativeStatus === analysisCopy[previousLanguage].nativeNotChecked) setNativeStatus(copy.nativeNotChecked);
      if (caseStatus === analysisCopy[previousLanguage].caseStoreReady) setCaseStatus(copy.caseStoreReady);
      previousLanguageRef.current = language;
    }
  }, [caseStatus, copy.caseStoreReady, copy.defaultQuestion, copy.nativeNotChecked, copy.selectOrCreateSession, language, nativeStatus, question, uploadStatus]);

  const refreshSessions = useCallback(async () => {
    if (!apiKey.trim()) {
      setSessions([]);
      return;
    }
    const result = await fetchJson<{ sessions: SessionSummary[] }>("/api/sessions", { headers: authHeaders(apiKey) });
    setSessions(result.sessions);
  }, [apiKey]);

  const refreshSkills = useCallback(async () => {
    if (!apiKey.trim()) {
      setSkills([]);
      return;
    }
    const result = await fetchJson<{ skills: SkillSummary[] }>("/api/skills", { headers: authHeaders(apiKey) });
    setSkills(result.skills);
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
    setCaseStatus(copy.casesLoaded(result.cases.length));
  }, [apiKey, copy]);

  const loadTask = useCallback(async (taskId: string) => {
    const task = await fetchJson<TaskRecord>(`/api/tasks/${encodeURIComponent(taskId)}`, { headers: authHeaders(apiKey) });
    setSelectedTask(task);
    const previous = taskStatusRef.current;
    const sameTask = previous?.taskId === task.taskId;
    if (!sameTask) {
      setTimelineExpanded(!isTerminal(task.status));
    } else if (previous && !isTerminal(previous.status) && isTerminal(task.status)) {
      setTimelineExpanded(false);
    }
    taskStatusRef.current = { taskId: task.taskId, status: task.status };
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
      setSelectedSkillIds(session.skillIds ?? []);
      setDraftExpanded(session.taskIds.length === 0);
      setNativeStatus(copy.settingNativeSession);
      await setNativeCurrentSession(session.sessionId)
        .then(() => setNativeStatus(copy.nativeActive(session.sessionId)))
        .catch((reason) => setNativeStatus(copy.nativeNotConnected(errorMessage(reason))));
    }
    await loadSessionArtifacts(session, preferredTaskId);
  }, [apiKey, copy, loadSessionArtifacts]);

  useEffect(() => {
    setSelectedSession(null);
    setSessionTasks([]);
    setSelectedTask(null);
    setArtifacts(null);
    setTaskResult(null);
    setAnalysisSnapshot(null);
    setDraftExpanded(true);
    setTimelineExpanded(true);
    taskStatusRef.current = null;
    setTimeline([]);
    setCases([]);
    void refreshSessions().catch((reason) => setUploadStatus(errorMessage(reason)));
    void refreshSkills().catch((reason) => setUploadStatus(errorMessage(reason)));
    void refreshCases("").catch((reason) => setCaseStatus(errorMessage(reason)));
  }, [refreshCases, refreshSessions, refreshSkills]);

  useEffect(() => {
    if (!selectedSession || !apiKey.trim()) return;
    const patch = {
      title: title.trim(),
      question: question.trim(),
      sourceUrl: sourceUrl.trim(),
      instanceId: instanceId.trim(),
      nodeId: nodeId.trim(),
      analysisLanguage: language,
      skillIds: selectedSkillIds
    };
    const unchanged =
      patch.title === selectedSession.title &&
      patch.question === selectedSession.question &&
      (patch.sourceUrl || "") === (selectedSession.sourceUrl ?? "") &&
      (patch.instanceId || "") === (selectedSession.instanceId ?? "") &&
      (patch.nodeId || "") === (selectedSession.nodeId ?? "") &&
      patch.analysisLanguage === selectedSession.analysisLanguage &&
      sameStringList(patch.skillIds, selectedSession.skillIds ?? []);
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
          nodeId: patch.nodeId || null,
          analysisLanguage: patch.analysisLanguage,
          skillIds: patch.skillIds
        })
      })
        .then((session) => {
          setSelectedSession(session);
          void refreshSessions();
        })
        .catch((reason) => setUploadStatus(errorMessage(reason)));
    }, 500);
    return () => window.clearTimeout(timer);
  }, [apiKey, instanceId, language, nodeId, question, refreshSessions, selectedSkillIds, selectedSession, sourceUrl, title]);

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
      setUploadStatus(copy.apiKeyRequired);
      return;
    }
    setLoading(true);
    try {
      const session = await fetchJson<SessionRecord>("/api/sessions", {
        method: "POST",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({ title: copy.newSessionTitle, question, analysisLanguage: language, skillIds: selectedSkillIds })
      });
      setUploadStatus(copy.createdSession(session.sessionId));
      await refreshSessions();
      await selectSession(session.sessionId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function deleteSession(session: SessionSummary) {
    if (!apiKey.trim()) {
      setUploadStatus(copy.apiKeyRequired);
      return;
    }
    if (!window.confirm(copy.confirmDeleteSession(session.title, session.sessionId))) return;
    setLoading(true);
    try {
      await fetchJson<Record<string, never>>(`/api/sessions/${encodeURIComponent(session.sessionId)}`, {
        method: "DELETE",
        headers: authHeaders(apiKey)
      });
      if (selectedSession?.sessionId === session.sessionId) {
        setSelectedSession(null);
        setSessionTasks([]);
        setSelectedTask(null);
        setArtifacts(null);
        setTaskResult(null);
        setAnalysisSnapshot(null);
        setTimeline([]);
        setDraftExpanded(true);
        setTimelineExpanded(true);
        taskStatusRef.current = null;
      }
      setUploadStatus(copy.deletedSession(session.sessionId));
      await refreshSessions();
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function uploadToSession() {
    if (!selectedSession || !files.length || !apiKey.trim()) {
      setUploadStatus(!selectedSession ? copy.selectOrCreateSession : !files.length ? copy.chooseLogFile : copy.apiKeyRequired);
      return;
    }
    setLoading(true);
    try {
      const { session, uploadCount } = await uploadSelectedFilesToSession(selectedSession.sessionId);
      setUploadStatus(copy.attachedUploads(uploadCount, selectedSession.sessionId));
      await refreshSessions();
      await selectSession(session.sessionId, false, selectedTask?.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function uploadSelectedFilesToSession(sessionId: string): Promise<{ session: SessionRecord; uploadCount: number }> {
    const pendingFiles = files;
    const uploads: UploadResponse[] = [];
    setUploadProgress(0);
    for (let index = 0; index < pendingFiles.length; index += 1) {
      setUploadStatus(copy.uploadingFile(pendingFiles[index].name));
      uploads.push(await uploadFile(pendingFiles[index], apiKey, (value) => setUploadProgress(Math.round(((index + value) / pendingFiles.length) * 100))));
    }
    const session = await fetchJson<SessionRecord>(`/api/sessions/${encodeURIComponent(sessionId)}/uploads`, {
      method: "POST",
      headers: jsonHeaders(apiKey),
      body: JSON.stringify({ uploadIds: uploads.map((upload) => upload.uploadId) })
    });
    setUploadProgress(100);
    setFiles([]);
    return { session, uploadCount: uploads.length };
  }

  async function startAnalysis() {
    if (!selectedSession || !apiKey.trim()) return;
    const sessionId = selectedSession.sessionId;
    setLoading(true);
    setArtifacts(null);
    setTaskResult(null);
    setAnalysisSnapshot(null);
    try {
      let savedSession = await fetchJson<SessionRecord>(`/api/sessions/${encodeURIComponent(sessionId)}`, {
        method: "PATCH",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          title: title.trim() || null,
          question: question.trim() || null,
          sourceUrl: sourceUrl.trim() || null,
          instanceId: instanceId.trim() || null,
          nodeId: nodeId.trim() || null,
          analysisLanguage: language,
          skillIds: selectedSkillIds
        })
      });
      if (files.length) {
        const attached = await uploadSelectedFilesToSession(sessionId);
        savedSession = attached.session;
      }
      setSelectedSession(savedSession);
      const task = await fetchJson<TaskSummary>(`/api/sessions/${encodeURIComponent(savedSession.sessionId)}/tasks`, {
        method: "POST",
        headers: authHeaders(apiKey)
      });
      setDraftExpanded(false);
      setTimelineExpanded(true);
      setUploadStatus(copy.analysisRunCreated);
      await refreshSessions();
      await selectSession(savedSession.sessionId, false, task.taskId);
    } catch (reason) {
      setUploadStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function submitUserMessage(prompt: PendingUserPrompt, resumeMode: UserMessageResumeMode = "continue") {
    if (!selectedTask) return;
    const message = userAnswer.trim() || (resumeMode === "finalize" ? FINALIZE_WITH_CURRENT_EVIDENCE_MESSAGE : "");
    if (!message) {
      setUploadStatus(copy.answerRequired);
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
          resumeMode,
          idempotencyKey: `webui-${prompt.questionId}-${resumeMode}-${Date.now()}`
        })
      });
      setUserAnswer("");
      setUploadStatus(resumeMode === "finalize" ? copy.finalRequested : copy.answerSubmitted);
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
      setUploadStatus(decision === "approved" ? copy.approvalApproved : copy.approvalRejected);
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
      setCaseStatus(copy.savedCase(response.case.caseId));
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
      setCaseStatus(copy.disabledCase(caseId));
      await refreshCases(caseQuery);
    } catch (reason) {
      setCaseStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <V2AnalyzeBridge apiKey={apiKey} language={language} />

      <div className="grid gap-5 xl:grid-cols-[360px_1fr]">
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <CardTitle>{copy.sessionHistory}</CardTitle>
              <div className="flex gap-2">
                <Button className="h-8 px-3" variant="outline" title={copy.refresh} onClick={() => void refreshSessions()}><RefreshCw className="h-4 w-4" /></Button>
                <Button className="h-8 px-3" disabled={loading} onClick={() => void createSession()}><Plus className="mr-1 h-4 w-4" />{copy.new}</Button>
              </div>
            </div>
            <CardDescription>{nativeStatus}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-2">
            {sessions.length ? sessions.map((session) => (
              <div key={session.sessionId} className={`flex w-full items-start gap-2 rounded-lg border p-3 ${selectedSession?.sessionId === session.sessionId ? "border-primary bg-slate-50" : "border-border"}`}>
                <button className="min-w-0 flex-1 text-left" onClick={() => void selectSession(session.sessionId)}>
                  <div className="flex items-center justify-between gap-2"><span className="truncate text-sm font-medium">{session.title}</span><SessionBadge language={language} status={session.status} /></div>
                  <p className="mt-1 font-mono text-xs text-muted-foreground">{session.sessionId}</p>
                  <p className="mt-1 text-xs text-muted-foreground">{copy.uploadsRuns(session.uploadCount, session.taskCount, new Date(session.updatedAt).toLocaleString())}</p>
                </button>
                <Button className="h-8 w-8 shrink-0 px-0 text-red-600 hover:text-red-700" variant="ghost" disabled={loading} title={copy.deleteSession} aria-label={copy.deleteSession} onClick={() => void deleteSession(session)}>
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            )) : <EmptyState>{copy.noSessions}</EmptyState>}
          </CardContent>
        </Card>

        {selectedSession ? (
          <div className="space-y-5">
            <Card>
              <CardHeader>
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <CardTitle>{copy.sessionDraft}</CardTitle>
                    <CardDescription>{copy.sessionDraftDescription(selectedSession.sessionId, selectedSession.uploadIds.length)}</CardDescription>
                  </div>
                  <Button className="h-8 px-2" variant="outline" onClick={() => setDraftExpanded((value) => !value)} aria-label={draftExpanded ? copy.collapseSessionDraft : copy.expandSessionDraft}>
                    {draftExpanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {draftExpanded ? (
                  <>
                    <Input value={title} onChange={(event) => setTitle(event.target.value)} placeholder={copy.sessionTitlePlaceholder} />
                    <Input value={sourceUrl} onChange={(event) => setSourceUrl(event.target.value)} placeholder={copy.sourceUrlPlaceholder} />
                    <div className="grid gap-3 md:grid-cols-2">
                      <Input value={instanceId} onChange={(event) => setInstanceId(event.target.value)} placeholder={copy.instanceIdPlaceholder} />
                      <Input value={nodeId} onChange={(event) => setNodeId(event.target.value)} placeholder={copy.nodeIdPlaceholder} />
                    </div>
                    <textarea className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={question} onChange={(event) => setQuestion(event.target.value)} placeholder={copy.questionPlaceholder} />
                    <SkillPicker copy={copy} skills={skills} selectedIds={selectedSkillIds} onChange={setSelectedSkillIds} />
                    <label className="flex min-h-32 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
                      <UploadCloud className="mb-2 h-7 w-7" />
                      {files.length ? copy.selectedFiles(files.length, files.map((file) => file.name).join(", ")) : copy.chooseFiles}
                      <input className="hidden" type="file" multiple onChange={(event) => setFiles(Array.from(event.target.files ?? []))} />
                    </label>
                    <div>
                      <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>{copy.upload}</span><span>{uploadProgress}%</span></div>
                      <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
                    </div>
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <span className="text-sm text-muted-foreground">{uploadStatus}</span>
                      <div className="flex flex-wrap gap-2">
                        <Button disabled={loading || !files.length} variant="outline" onClick={() => void uploadToSession()}>{copy.uploadToSession}</Button>
                        <Button disabled={loading || !apiKey.trim()} onClick={() => void startAnalysis()}><ListChecks className="mr-2 h-4 w-4" />{copy.startAnalysis}</Button>
                      </div>
                    </div>
                  </>
                ) : <SessionDraftSummary copy={copy} language={language} session={selectedSession} title={title} question={question} sourceUrl={sourceUrl} instanceId={instanceId} nodeId={nodeId} selectedSkillIds={selectedSkillIds} uploadStatus={uploadStatus} />}
              </CardContent>
            </Card>

            <Card>
              <CardHeader><CardTitle>{copy.runs}</CardTitle><CardDescription>{selectedTask ? `${taskDisplayName(selectedTask, language)} · ${copy.attempt(selectedTask.attempts ?? 0)}` : copy.noRunSelected}</CardDescription></CardHeader>
              <CardContent className="space-y-4">
                {sessionTasks.length ? (
                  <div className="flex flex-wrap gap-2">
                    {sessionTasks.map((task) => <button className={`rounded-md border px-3 py-2 text-left text-xs ${selectedTask?.taskId === task.taskId ? "border-primary bg-slate-50" : "border-border"}`} key={task.taskId} onClick={() => void loadTask(task.taskId)}><span className="font-medium">{taskDisplayName(task, language)}</span><span className="ml-2"><StatusBadge language={language} status={task.status} /></span><p className="mt-1 text-muted-foreground">{new Date(task.createdAt).toLocaleString()}</p></button>)}
                  </div>
                ) : <EmptyState>{copy.noRuns}</EmptyState>}
                {selectedTask ? (
                  <div className="space-y-3">
                    <div className="flex items-center gap-2"><StatusBadge language={language} status={selectedTask.status} /><span className="text-sm text-muted-foreground">{taskPhaseLabel(language, selectedTask.phase)}</span></div>
                    {selectedTask.instanceId || selectedTask.nodeId ? <p className="text-xs text-muted-foreground">{copy.metadataBinding(selectedTask.instanceId, selectedTask.nodeId)}</p> : null}
                    {selectedTask.status === "FAILED" ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{selectedTask.error?.phase ? `${taskPhaseLabel(language, selectedTask.error.phase)}: ` : ""}{selectedTask.error?.message ?? copy.taskFailed}</div> : null}
                    <WaitingInteraction
                      copy={copy}
                      answer={userAnswer}
                      approvalReason={approvalReason}
                      loading={loading}
                      snapshot={analysisSnapshot}
                      status={selectedTask.status}
                      onAnswerChange={setUserAnswer}
                      onApprovalReasonChange={setApprovalReason}
                      onSubmitAnswer={(prompt, resumeMode) => void submitUserMessage(prompt, resumeMode)}
                      onSubmitApproval={(approval, decision) => void submitApproval(approval, decision)}
                    />
                  </div>
                ) : null}
              </CardContent>
            </Card>

            <SessionTimeline copy={copy} language={language} events={timeline} expanded={timelineExpanded} snapshot={analysisSnapshot} task={selectedTask} taskResult={taskResult} onToggle={() => setTimelineExpanded((value) => !value)} />
          </div>
        ) : (
          <Card>
            <CardHeader><CardTitle>{copy.logAnalysisSession}</CardTitle><CardDescription>{copy.createOrSelectSession}</CardDescription></CardHeader>
            <CardContent><Button disabled={loading || !apiKey.trim()} onClick={() => void createSession()}><Plus className="mr-2 h-4 w-4" />{copy.newSession}</Button></CardContent>
          </Card>
        )}
      </div>

      {taskResult ? <AnalysisResultView copy={copy} language={language} result={taskResult.result} /> : null}

      {taskResult && selectedTask ? (
        <CaseClosurePanel copy={copy} cases={cases} caseDraft={caseDraft} caseQuery={caseQuery} caseStatus={caseStatus} loading={loading} taskLabel={taskDisplayName(selectedTask, language)} onDraftChange={setCaseDraft} onQueryChange={setCaseQuery} onRefreshCases={() => void refreshCases(caseQuery)} onConfirmCase={() => void confirmCase()} onDisableCase={(caseId) => void disableCase(caseId)} />
      ) : null}

      {artifacts?.metadataContext ? <MetadataContextView copy={copy} context={artifacts.metadataContext} /> : null}
      {artifacts?.systemContext ? <SystemContextSnapshotView copy={copy} context={artifacts.systemContext} /> : null}
      {artifacts?.analysisPackage || artifacts?.claudeMcpConfig || artifacts?.claudeSession || artifacts?.agentResponse ? <AgentBackendPanel copy={copy} artifacts={artifacts} /> : null}
      {artifacts?.textInput ? <Evidence copy={copy} title={copy.sessionTextInput} count={1}><DataLine id="session-text-input" title={copy.question} detail={artifacts.textInput.question ?? ""} /></Evidence> : null}
      {artifacts?.caseContext ? <TaskCaseContextView copy={copy} context={artifacts.caseContext} /> : null}
      {artifacts?.toolResults?.length ? <Evidence copy={copy} title={copy.toolResults} count={artifacts.toolResults.length}>{artifacts.toolResults.map((result) => <ToolResultLine copy={copy} key={result.actionId} result={result} />)}</Evidence> : null}
      {artifacts ? (
        <div className="grid gap-5 xl:grid-cols-2">
          <Evidence copy={copy} title={copy.manifest} count={artifacts.manifest?.files?.length ?? 0}>{(artifacts.manifest?.files ?? []).map((file) => <DataLine key={file.path} title={file.path} detail={`${file.size.toLocaleString()} ${copy.bytes}`} />)}</Evidence>
          <Evidence copy={copy} title={copy.grepMatches} count={artifacts.grepResults?.matches?.length ?? 0}>{(artifacts.grepResults?.matches ?? []).map((match, index) => <DataLine id={`grep-match-${index}`} key={`${match.file}:${match.line}:${index}`} title={`${match.file}:${match.line}`} detail={`${match.keyword} · ${match.text}`} />)}</Evidence>
        </div>
      ) : null}
    </div>
  );
}

function WaitingInteraction({ copy, answer, approvalReason, loading, snapshot, status, onAnswerChange, onApprovalReasonChange, onSubmitAnswer, onSubmitApproval }: { copy: AnalysisCopy; answer: string; approvalReason: string; loading: boolean; snapshot: AnalysisSnapshot | null; status: TaskStatus; onAnswerChange: (value: string) => void; onApprovalReasonChange: (value: string) => void; onSubmitAnswer: (prompt: PendingUserPrompt, resumeMode?: UserMessageResumeMode) => void; onSubmitApproval: (approval: PendingApproval, decision: "approved" | "rejected") => void; }) {
  if (status === "WAITING_FOR_USER") {
    const prompt = snapshot?.state.pendingUserPrompts[0];
    if (!prompt) return <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">{copy.waitingNoPendingPrompt}</div>;
    return (
      <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-3">
        <div>
          <p className="text-sm font-medium text-amber-900">{copy.needMoreInfo}</p>
          <p className="mt-1 text-sm text-amber-800">{prompt.question}</p>
          <p className="mt-1 text-xs text-amber-700">{copy.promptMeta(prompt.reason, prompt.answerFormat ?? copy.freeText, prompt.required)}</p>
        </div>
        <textarea className="min-h-20 w-full rounded-md border border-amber-200 bg-white px-3 py-2 text-sm" value={answer} onChange={(event) => onAnswerChange(event.target.value)} placeholder={copy.answerPlaceholder} />
        <div className="flex flex-wrap gap-2">
          <Button disabled={loading} type="button" onClick={() => onSubmitAnswer(prompt)}>{copy.submitAnswer}</Button>
          <Button disabled={loading} type="button" variant="outline" onClick={() => onSubmitAnswer(prompt, "finalize")}><CheckCircle2 className="mr-2 h-4 w-4" />{copy.finalizeWithCurrentEvidence}</Button>
        </div>
      </div>
    );
  }
  if (status === "WAITING_FOR_APPROVAL") {
    const approval = snapshot?.state.pendingApprovals[0];
    if (!approval) return <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">{copy.waitingNoPendingApproval}</div>;
    return (
      <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-3">
        <div>
          <p className="text-sm font-medium text-amber-900">{copy.approvalRequired}</p>
          <p className="mt-1 text-sm text-amber-800">{approval.actionType} · {approval.actionId}</p>
          <p className="mt-1 text-xs text-amber-700">{copy.approvalMeta(approval.risk, approval.reason)}</p>
          <pre className="mt-2 max-h-32 overflow-auto rounded bg-white p-2 text-xs text-slate-700">{JSON.stringify(approval.input, null, 2)}</pre>
        </div>
        <Input value={approvalReason} onChange={(event) => onApprovalReasonChange(event.target.value)} placeholder={copy.approvalReasonPlaceholder} />
        <div className="flex flex-wrap gap-2">
          <Button disabled={loading} type="button" onClick={() => onSubmitApproval(approval, "approved")}>{copy.approveAndContinue}</Button>
          <Button disabled={loading} type="button" variant="outline" onClick={() => onSubmitApproval(approval, "rejected")}>{copy.rejectAndContinue}</Button>
        </div>
      </div>
    );
  }
  return null;
}

function SkillPicker({ copy, skills, selectedIds, onChange }: { copy: AnalysisCopy; skills: SkillSummary[]; selectedIds: string[]; onChange: (ids: string[]) => void }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium"><BrainCircuit className="mr-2 inline h-4 w-4 text-primary" />{copy.diagnosticSkills}</p>
          <p className="text-xs text-muted-foreground">{copy.skillsFrozenHint}</p>
        </div>
        <Badge variant="secondary">{copy.selectedCount(selectedIds.length)}</Badge>
      </div>
      {skills.length ? (
        <div className="grid gap-2 md:grid-cols-2">
          {skills.slice(0, 12).map((skill) => {
            const checked = selectedIds.includes(skill.skillId);
            return (
              <label className={`rounded-md border p-3 text-sm ${checked ? "border-primary bg-slate-50" : "border-border bg-white"}`} key={skill.skillId}>
                <div className="flex items-start gap-2">
                  <input className="mt-1 h-4 w-4 accent-teal-700" type="checkbox" checked={checked} onChange={() => onChange(toggleString(selectedIds, skill.skillId))} />
                  <div className="min-w-0">
                    <p className="truncate font-medium">{skill.displayName}</p>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{skill.skillId} · {skill.includeByDefault ? copy.auto : copy.explicit} · {skill.description}</p>
                  </div>
                </div>
              </label>
            );
          })}
        </div>
      ) : <p className="text-sm text-muted-foreground">{copy.noSkills}</p>}
    </div>
  );
}

function CaseClosurePanel({ copy, cases, caseDraft, caseQuery, caseStatus, loading, taskLabel, onDraftChange, onQueryChange, onRefreshCases, onConfirmCase, onDisableCase }: { copy: AnalysisCopy; cases: CaseHit[]; caseDraft: CaseDraft; caseQuery: string; caseStatus: string; loading: boolean; taskLabel: string; onDraftChange: (draft: CaseDraft) => void; onQueryChange: (value: string) => void; onRefreshCases: () => void; onConfirmCase: () => void; onDisableCase: (caseId: string) => void; }) {
  return <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_420px]"><Card><CardHeader><div className="flex items-center gap-2"><BookOpenCheck className="h-5 w-5 text-primary" /><CardTitle>{copy.confirmAsCase}</CardTitle></div><CardDescription>{copy.caseClosureDescription(taskLabel)}</CardDescription></CardHeader><CardContent className="space-y-3"><Input value={caseDraft.title} onChange={(event) => onDraftChange({ ...caseDraft, title: event.target.value })} placeholder={copy.caseTitlePlaceholder} /><textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.symptom} onChange={(event) => onDraftChange({ ...caseDraft, symptom: event.target.value })} placeholder={copy.symptomPlaceholder} /><textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.rootCause} onChange={(event) => onDraftChange({ ...caseDraft, rootCause: event.target.value })} placeholder={copy.rootCausePlaceholder} /><textarea className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm" value={caseDraft.solution} onChange={(event) => onDraftChange({ ...caseDraft, solution: event.target.value })} placeholder={copy.solutionPlaceholder} /><div className="flex flex-wrap items-center justify-between gap-3"><span className="text-sm text-muted-foreground">{caseStatus}</span><Button disabled={loading || !caseDraft.title.trim()} onClick={onConfirmCase}>{copy.saveCase}</Button></div></CardContent></Card><Card><CardHeader><div className="flex items-center justify-between gap-3"><CardTitle>{copy.similarCases}</CardTitle><Button className="h-8 px-3" variant="outline" title={copy.refresh} onClick={onRefreshCases}><RefreshCw className="h-4 w-4" /></Button></div><CardDescription>{copy.localCaseStore}</CardDescription></CardHeader><CardContent className="space-y-3"><Input value={caseQuery} onChange={(event) => onQueryChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") onRefreshCases(); }} placeholder={copy.searchCasesPlaceholder} />{cases.length ? cases.map((item) => <div className="rounded-lg border border-border p-3" key={item.caseId}><div className="flex items-start justify-between gap-3"><div><p className="text-sm font-medium">{item.title}</p><p className="mt-1 text-xs text-muted-foreground">{item.caseId} · {item.sourceType} · {copy.score} {item.score.toFixed(2)} · {new Date(item.createdAt).toLocaleDateString()}</p></div><Badge variant={item.enabled ? "secondary" : "destructive"}>{item.enabled ? copy.enabled : copy.disabled}</Badge></div><p className="mt-2 text-xs text-muted-foreground">{item.rootCause}</p><div className="mt-3 flex flex-wrap gap-2"><Button className="h-8 px-3" disabled={loading || !item.enabled} variant="outline" onClick={() => onDisableCase(item.caseId)}>{copy.disable}</Button></div></div>) : <EmptyState>{copy.noMatchedCases}</EmptyState>}</CardContent></Card></div>;
}

function StatusBadge({ language, status }: { language: UiLanguage; status: TaskStatus }) {
  return <Badge variant={status === "FAILED" ? "destructive" : status === "SUCCEEDED" ? "default" : "secondary"}>{taskStatusLabel(language, status)}</Badge>;
}

function SessionBadge({ language, status }: { language: UiLanguage; status: SessionStatus }) {
  return <Badge variant={status === "failed" ? "destructive" : status === "succeeded" ? "default" : status === "running" || status.startsWith("waiting") ? "warning" : "secondary"}>{sessionStatusLabel(language, status)}</Badge>;
}

function isTerminal(status: TaskStatus) {
  return status === "SUCCEEDED" || status === "FAILED";
}

function taskDisplayName(task: TaskRecord, language: UiLanguage) {
  const copy = analysisCopy[language];
  const alias = task.alias?.trim();
  if (alias) return alias;
  if (task.status === "FAILED") return task.error?.phase ? copy.taskNames.failedWithPhase(taskPhaseLabel(language, task.error.phase)) : copy.taskNames.failed;
  if (task.status === "SUCCEEDED") return copy.taskNames.succeeded;
  if (task.status === "WAITING_FOR_USER") return copy.taskNames.waitingForUser;
  if (task.status === "WAITING_FOR_APPROVAL") return copy.taskNames.waitingForApproval;
  if (task.status === "QUEUED") return copy.taskNames.queued;
  return task.phase ? copy.taskNames.runningWithPhase(taskPhaseLabel(language, task.phase)) : copy.taskNames.running;
}

function timelineTaskLabel(taskId: string, selectedTask: TaskRecord | null, language: UiLanguage) {
  if (selectedTask?.taskId === taskId) return taskDisplayName(selectedTask, language);
  return analysisCopy[language].historyRun;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}

function toggleString(values: string[], value: string) {
  return values.includes(value) ? values.filter((item) => item !== value) : [...values, value];
}

function sameStringList(left: string[], right: string[]) {
  if (left.length !== right.length) return false;
  return left.every((value, index) => value === right[index]);
}

function Evidence({ copy, title, count, children }: { copy: AnalysisCopy; title: string; count: number; children: React.ReactNode }) {
  return <Card><CardHeader><div className="flex items-center justify-between"><CardTitle>{title}</CardTitle><Badge variant="secondary">{count}</Badge></div></CardHeader><CardContent className="space-y-2">{count ? children : <EmptyState>{copy.empty}</EmptyState>}</CardContent></Card>;
}

function SessionDraftSummary({ copy, language, session, title, question, sourceUrl, instanceId, nodeId, selectedSkillIds, uploadStatus }: { copy: AnalysisCopy; language: UiLanguage; session: SessionRecord; title: string; question: string; sourceUrl: string; instanceId: string; nodeId: string; selectedSkillIds: string[]; uploadStatus: string }) {
  const rows = [
    [copy.draftSummaryRows.title, title || session.title || "-"],
    [copy.draftSummaryRows.question, question || session.question || "-"],
    [copy.draftSummaryRows.sourceUrl, sourceUrl || "-"],
    [copy.draftSummaryRows.metadata, `instance=${instanceId || "-"} · node=${nodeId || "-"}`],
    [copy.draftSummaryRows.skills, copy.selectedCount(selectedSkillIds.length)],
    [copy.draftSummaryRows.inputs, copy.inputsSummary(session.uploadIds.length, session.taskIds.length)],
    [copy.draftSummaryRows.status, copy.statusWithUpload(sessionStatusLabel(language, session.status), uploadStatus)]
  ];
  return <div className="grid gap-3 md:grid-cols-2">{rows.map(([label, value]) => <div className="rounded-lg border border-border p-3" key={label}><p className="text-xs text-muted-foreground">{label}</p><p className={`mt-1 break-words text-sm ${label === copy.draftSummaryRows.question ? "max-h-10 overflow-hidden" : ""}`}>{value}</p></div>)}</div>;
}

function SessionTimeline({ copy, language, events, expanded, snapshot, task, taskResult, onToggle }: { copy: AnalysisCopy; language: UiLanguage; events: SessionTimelineEvent[]; expanded: boolean; snapshot: AnalysisSnapshot | null; task: TaskRecord | null; taskResult: TaskResult | null; onToggle: () => void }) {
  const latest = events.slice(-18).reverse();
  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-3">
          <div>
            <CardTitle>{copy.evidenceTimeline}</CardTitle>
            <CardDescription>{snapshot ? `${copy.revision} ${snapshot.state.revision} · ${taskStatusLabel(language, snapshot.state.status)} · ${copy.phase} ${taskPhaseLabel(language, snapshot.state.currentPhase)}` : copy.sessionAndTaskEvents}</CardDescription>
          </div>
          <div className="flex flex-wrap items-center justify-end gap-2">
            {expanded && snapshot ? <div className="flex flex-wrap gap-2 text-xs"><Badge variant="secondary">{copy.rounds} {snapshot.state.budget.rounds}</Badge><Badge variant="secondary">{copy.backend} {snapshot.state.budget.llmCalls}</Badge><Badge variant="secondary">{copy.actions} {snapshot.state.budget.actions}</Badge><Badge variant="secondary">{copy.evidence} {snapshot.state.evidence.length}</Badge></div> : null}
            <Button className="h-8 px-2" variant="outline" onClick={onToggle} aria-label={expanded ? copy.collapseEvidenceTimeline : copy.expandEvidenceTimeline}>
              {expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {expanded ? (
          latest.length ? <ol className="space-y-2">{latest.map((event, index) => <li className="rounded-md border border-border bg-white p-3" key={`${event.createdAt}:${event.eventType}:${index}`}><div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"><Badge variant={event.eventType === "analysis_failed" ? "destructive" : event.eventType === "model_decision" ? "warning" : "outline"}>{event.source}:{eventTypeLabel(language, event.eventType)}</Badge>{event.phase ? <span>{taskPhaseLabel(language, event.phase)}</span> : null}{event.taskId ? <span>{timelineTaskLabel(event.taskId, task, language)}</span> : null}{event.actionId ? <span className="font-mono">{event.actionId}</span> : null}<span><Clock3 className="mr-1 inline h-3 w-3" />{new Date(event.createdAt).toLocaleTimeString()}</span></div><p className="mt-2 text-sm">{event.message}</p><EventDetails copy={copy} event={event} /></li>)}</ol> : <EmptyState>{copy.noTimelineEvents}</EmptyState>
        ) : <TimelineSummary copy={copy} language={language} latest={latest[0]} snapshot={snapshot} task={task} taskResult={taskResult} />}
      </CardContent>
    </Card>
  );
}

function TimelineSummary({ copy, language, latest, snapshot, task, taskResult }: { copy: AnalysisCopy; language: UiLanguage; latest?: SessionTimelineEvent; snapshot: AnalysisSnapshot | null; task: TaskRecord | null; taskResult: TaskResult | null }) {
  if (!task) return <EmptyState>{copy.noSelectedRun}</EmptyState>;
  if (task.status === "SUCCEEDED" && taskResult) {
    return <div className="rounded-lg border border-border p-3"><div className="flex flex-wrap items-center gap-2"><StatusBadge language={language} status={task.status} /><Badge variant="secondary">{copy.confidence} {confidenceLabel(language, taskResult.result.confidence)}</Badge><span className="text-xs text-muted-foreground">{taskDisplayName(task, language)}</span></div><p className="mt-2 text-sm">{taskResult.result.summary}</p></div>;
  }
  if (task.status === "FAILED") {
    return <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700"><div className="mb-1 flex flex-wrap items-center gap-2"><StatusBadge language={language} status={task.status} /><span className="text-xs">{taskDisplayName(task, language)}</span></div>{task.error?.phase ? `${taskPhaseLabel(language, task.error.phase)}: ` : ""}{task.error?.message ?? latest?.message ?? copy.taskFailed}</div>;
  }
  return <div className="rounded-lg border border-border p-3"><div className="flex flex-wrap items-center gap-2"><StatusBadge language={language} status={task.status} /><span className="text-xs text-muted-foreground">{taskPhaseLabel(language, task.phase ?? snapshot?.state.currentPhase)}</span><span className="text-xs text-muted-foreground">{taskDisplayName(task, language)}</span></div><p className="mt-2 text-sm">{latest?.message ?? copy.taskRunningHint}</p>{snapshot ? <p className="mt-1 text-xs text-muted-foreground">{copy.revision} {snapshot.state.revision} · {copy.rounds} {snapshot.state.budget.rounds} · {copy.evidence} {snapshot.state.evidence.length}</p> : null}</div>;
}

function EventDetails({ copy, event }: { copy: AnalysisCopy; event: SessionTimelineEvent }) {
  const detail = summarizeEventDetails(copy, event.details ?? {});
  const refs = event.evidenceRefs.slice(0, 4);
  if (!detail && !event.artifactPath && refs.length === 0) return null;
  return <div className="mt-2 space-y-1 text-xs text-muted-foreground">{detail ? <p>{detail}</p> : null}{event.artifactPath ? <p>{copy.artifact}: <span className="font-mono">{event.artifactPath}</span></p> : null}{refs.length ? <p>{copy.refs}: {refs.map((reference) => <span className="mr-2 font-mono" key={reference}>{reference}</span>)}{event.evidenceRefs.length > refs.length ? `+${event.evidenceRefs.length - refs.length}` : ""}</p> : null}</div>;
}

function summarizeEventDetails(copy: AnalysisCopy, details: Record<string, unknown>) {
  if (typeof details.callId === "string") {
    const attempt = typeof details.attempt === "number" ? `${copy.eventDetailLabels.attempt}=${details.attempt}` : "";
    const model = typeof details.model === "string" ? `${copy.eventDetailLabels.model}=${details.model}` : "";
    const error = typeof details.error === "string" ? ` · ${copy.eventDetailLabels.error}=${details.error}` : "";
    return [details.callId, attempt, model].filter(Boolean).join(" · ") + error;
  }
  if (typeof details.totalMatches === "number") {
    const keywords = Array.isArray(details.keywords) ? details.keywords.filter((item): item is string => typeof item === "string").slice(0, 6).join(", ") : "";
    return `${copy.matches}=${details.totalMatches}${keywords ? ` · ${copy.keywords}=${keywords}` : ""}`;
  }
  const decision = details.decision;
  if (isRecord(decision)) {
    const type = typeof decision.type === "string" ? decision.type : copy.eventDetailLabels.action;
    const reason = typeof decision.reason === "string" ? decision.reason : "";
    return `${copy.decision}=${type}${reason ? ` · ${reason}` : ""}`;
  }
  const result = details.result;
  if (isRecord(result) && typeof result.summary === "string") return `${copy.finalAnswer} · ${result.summary}`;
  if (typeof details.caseRecallCount === "number") return `${copy.caseRecallCount}=${details.caseRecallCount}`;
  if (typeof details.resourceCount === "number") return `${copy.systemContextResources}=${details.resourceCount}`;
  if (typeof details.error === "string") return details.error;
  return "";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function AnalysisResultView({ copy, language, result }: { copy: AnalysisCopy; language: UiLanguage; result: AnalysisResult }) {
  return <Card><CardHeader><div className="flex items-center justify-between gap-3"><CardTitle>{copy.agentAnalysis}</CardTitle><Badge variant="secondary">{copy.confidence}: {confidenceLabel(language, result.confidence)}</Badge></div><CardDescription>{result.summary}</CardDescription></CardHeader><CardContent className="grid gap-5 lg:grid-cols-2"><ResultList copy={copy} title={copy.symptoms} items={result.symptoms} /><div><h3 className="mb-2 text-sm font-semibold">{copy.likelyRootCauses}</h3>{result.likelyRootCauses.length ? result.likelyRootCauses.map((cause, index) => <div className="mb-2 rounded-lg border border-border p-3" key={`${cause.cause}:${index}`}><p className="text-sm">{cause.cause}</p><div className="mt-2 flex flex-wrap gap-2">{cause.evidenceRefs.map((reference) => <button className="font-mono text-xs text-primary underline" key={reference} onClick={() => scrollToEvidence(reference)}>{reference}</button>)}</div></div>) : <p className="text-sm text-muted-foreground">{copy.noRootCause}</p>}</div><ResultList copy={copy} title={copy.nextChecks} items={result.nextChecks} /><ResultList copy={copy} title={copy.fixSuggestions} items={result.fixSuggestions} /><ResultList copy={copy} title={copy.missingInformation} items={result.missingInformation} /></CardContent></Card>;
}

function MetadataContextView({ copy, context }: { copy: AnalysisCopy; context: MetadataContext }) {
  const partitions = context.cluster?.partitionViews ?? [];
  const abnormalPartitions = partitions.filter((partition) => partition.statusText && partition.statusText !== "online").length;
  const rows = [[copy.metadataRows.instance, context.instanceId], [copy.metadataRows.node, context.nodeId], [copy.metadataRows.product, context.product], [copy.metadataRows.version, context.version], [copy.metadataRows.environment, context.environment], [copy.metadataRows.nodeStatus, context.node?.status], [copy.metadataRows.clusterNodes, String(context.clusterNodes?.length ?? 0)], [copy.metadataRows.databases, (context.cluster?.databases ?? []).map((database) => database.name).join(", ") || "0"], [copy.metadataRows.partitions, copy.partitionsSummary(partitions.length, abnormalPartitions)]];
  return <Card><CardHeader><CardTitle>{copy.metadataContext}</CardTitle><CardDescription>{copy.metadataSnapshotDescription}</CardDescription></CardHeader><CardContent className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">{rows.map(([label, value]) => <div className="rounded-lg border border-border p-3" key={label}><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value || "-"}</p></div>)}</CardContent></Card>;
}

function SystemContextSnapshotView({ copy, context }: { copy: AnalysisCopy; context: SystemContextBundle }) {
  const resources = context.resources ?? [];
  const skillResources = resources.filter((resource) => resource.kind === "diagnostic_skill");
  const metadataResources = resources.filter((resource) => resource.kind === "metadata_instance");
  return (
    <Card>
      <CardHeader>
        <CardTitle>{copy.systemContextSnapshot}</CardTitle>
        <CardDescription>{copy.systemContextSnapshotDescription}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div>
          <p className="mb-2 text-xs font-medium text-muted-foreground">{copy.diagnosticSkills}</p>
          <div className="space-y-2">
            {skillResources.length ? skillResources.map((resource) => (
              <div className="rounded-lg border border-border p-3" key={`${resource.contextId}:${resource.title}`}>
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium">{resource.title}</span>
                  <Badge variant="secondary">{resource.skillId ?? resource.kind}</Badge>
                  <span className="text-xs text-muted-foreground">{copy.revisionShort} {resource.revision?.slice(0, 8) ?? "-"}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{resource.summary ?? resource.contextId}</p>
                {resource.references?.length ? <p className="mt-1 text-xs text-muted-foreground">{copy.referencesCount(resource.references.length)}</p> : null}
              </div>
            )) : <EmptyState>{copy.noDiagnosticSkill}</EmptyState>}
          </div>
        </div>
        <div>
          <p className="mb-2 text-xs font-medium text-muted-foreground">{copy.metadataContextSection}</p>
          <div className="space-y-2">
            {metadataResources.length ? metadataResources.map((resource) => (
              <div className="rounded-lg border border-border p-3" key={`${resource.contextId}:${resource.title}`}>
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium">{resource.title}</span>
                  <Badge variant="outline">{resource.kind}</Badge>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{resource.summary ?? resource.contextId}</p>
              </div>
            )) : <EmptyState>{copy.noMetadataInstance}</EmptyState>}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function AgentBackendPanel({ copy, artifacts }: { copy: AnalysisCopy; artifacts: Artifacts }) {
  const mcpCallCount = artifacts.mcpCalls?.length ?? 0;
  const rows = [
    [copy.agentRows.session, artifacts.agentResponse?.claudeSessionId ?? artifacts.claudeSession?.claudeSessionId ?? "-"],
    [copy.agentRows.analysisMode, artifacts.agentResponse?.analysisMode ?? artifacts.claudeSession?.analysisMode ?? "-"],
    [copy.agentRows.permission, artifacts.agentResponse?.permissionProfile ?? artifacts.claudeSession?.permissionProfile ?? "-"],
    [copy.agentRows.runtimeStatus, artifacts.agentResponse?.runtimeStatus ?? artifacts.analysisPackage?.runtimeStatus ?? "-"],
    [copy.agentRows.duration, typeof artifacts.agentResponse?.durationMs === "number" ? `${artifacts.agentResponse.durationMs} ms` : "-"],
    [copy.agentRows.package, artifacts.analysisPackagePath ?? "-"],
    [copy.agentRows.mcpConfig, artifacts.claudeMcpConfigPath ?? "-"],
    [copy.agentRows.sessionArtifact, artifacts.claudeSessionPath ?? "-"],
    [copy.agentRows.response, artifacts.agentResponsePath ?? "-"],
    [copy.agentRows.mcpCalls, artifacts.mcpCallsPath ? `${mcpCallCount} ${copy.calls} · ${artifacts.mcpCallsPath}` : `${mcpCallCount} ${copy.calls}`]
  ];
  return (
    <Card>
      <CardHeader>
        <CardTitle>{copy.claudeCodeSession}</CardTitle>
        <CardDescription>{copy.claudeSessionDescription}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid gap-2 md:grid-cols-2 lg:grid-cols-3">
          {rows.map(([label, value]) => (
            <div className="rounded-lg border border-border p-3" key={label}>
              <p className="text-xs text-muted-foreground">{label}</p>
              <p className="mt-1 break-all text-sm">{value}</p>
            </div>
          ))}
        </div>
        {artifacts.agentResponse?.error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{artifacts.agentResponse.error}</div> : null}
        {artifacts.agentResponse?.usage || artifacts.agentResponse?.cost ? <pre className="max-h-40 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{JSON.stringify({ usage: artifacts.agentResponse.usage, cost: artifacts.agentResponse.cost }, null, 2)}</pre> : null}
        {artifacts.agentResponse?.structuredOutput ? <pre className="max-h-56 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{JSON.stringify(artifacts.agentResponse.structuredOutput, null, 2)}</pre> : null}
        {artifacts.mcpCalls?.length ? <div className="space-y-2">{artifacts.mcpCalls.slice(-5).map((call, index) => <pre className="max-h-32 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs" key={index}>{JSON.stringify(call, null, 2)}</pre>)}</div> : null}
      </CardContent>
    </Card>
  );
}

function TaskCaseContextView({ copy, context }: { copy: AnalysisCopy; context: CaseContext }) {
  return <Card><CardHeader><CardTitle>{copy.caseContext}</CardTitle><CardDescription>{copy.caseContextDescription}</CardDescription></CardHeader><CardContent className="space-y-3"><p className="text-xs text-muted-foreground">{copy.query}: {context.query || "-"}</p>{context.cases.length ? context.cases.map((item, index) => <div id={`case-context-${index}`} className="rounded-lg border border-border p-3" key={item.caseId}><div className="flex flex-wrap items-center gap-2"><span className="text-sm font-medium">{item.title}</span><Badge variant="secondary">{copy.score} {item.score.toFixed(2)}</Badge></div><p className="mt-1 text-xs text-muted-foreground">{item.caseId} · {item.sourceType} · {item.product ?? copy.unknown} {item.version ?? ""}</p><p className="mt-2 text-sm">{item.rootCause}</p></div>) : <EmptyState>{copy.noRecalledCases}</EmptyState>}</CardContent></Card>;
}

function ToolResultLine({ copy, result }: { copy: AnalysisCopy; result: ToolResult }) {
  return <div className="rounded-lg border border-border p-3"><div className="flex items-center gap-2 text-sm font-medium"><FileArchive className="h-4 w-4 text-slate-400" />{result.tool} · {result.status}</div><p className="mt-1 break-words text-xs text-muted-foreground">{copy.exit}={result.exitCode ?? "-"} · {result.durationMs}ms · {result.summary} · stdout={result.stdoutPath} · stderr={result.stderrPath}</p>{result.findings?.length ? <ul className="mt-3 space-y-2">{result.findings.map((finding, index) => <li className="rounded-md bg-slate-50 p-2 text-xs" key={`${finding.message}:${index}`}><span className="font-medium">{finding.severity ?? copy.finding}</span><span className="text-muted-foreground"> · {finding.file ?? "-"}{finding.line ? `:${finding.line}` : ""}</span><p className="mt-1 text-slate-700">{finding.message}</p></li>)}</ul> : null}</div>;
}

function ResultList({ copy, title, items }: { copy: AnalysisCopy; title: string; items: string[] }) {
  return <div><h3 className="mb-2 text-sm font-semibold">{title}</h3>{items.length ? <ul className="space-y-2 text-sm text-muted-foreground">{items.map((item, index) => <li className="rounded-lg border border-border p-3" key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="text-sm text-muted-foreground">{copy.empty}</p>}</div>;
}

function scrollToEvidence(reference: string) {
  if (reference === "session_text_input.json#question") {
    document.getElementById("session-text-input")?.scrollIntoView({ behavior: "smooth", block: "center" });
    return;
  }
  const index = reference.match(/^grep_results\.json#matches\/(\d+)$/)?.[1];
  if (index) document.getElementById(`grep-match-${index}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  const caseIndex = reference.match(/^case_context\.json#cases\/(\d+)$/)?.[1];
  if (caseIndex) document.getElementById(`case-context-${caseIndex}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
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
