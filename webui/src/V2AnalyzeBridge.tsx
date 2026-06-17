import { BookOpenCheck, CheckCircle2, Download, FileArchive, MessageSquare, Play, RefreshCw, Save, Trash2, UploadCloud, Workflow, XCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState } from "./components/ui";
import type { UiLanguage } from "./i18n";
import {
  createV2Run,
  createV2Workspace,
  confirmV2RunCase,
  decideV2Action,
  deleteV2Workspace,
  downloadV2Artifact,
  getV2Workspace,
  getV2RunAnalysis,
  listV2WorkspaceRuns,
  listV2Workspaces,
  listV2WorkspaceUploads,
  postV2RunMessage,
  updateV2Workspace,
  uploadV2Files,
  type V2Action,
  type V2CaseDraft,
  type V2CaseRecord,
  type V2EvidenceArtifact,
  type V2FinalAnswer,
  type V2Mode,
  type V2Run,
  type V2RunAnalysis,
  type V2RunStatus,
  type V2Upload,
  type V2Workspace
} from "./v2-api";

type BridgeCopy = (typeof copyByLanguage)[UiLanguage];
type RunCaseDraft = {
  title: string;
  symptom: string;
  rootCause: string;
  solution: string;
  evidenceRefsText: string;
};

const copyByLanguage = {
  "zh-CN": {
    title: "V2 分析桥接",
    description: "连接 Python V2 的 Workspace、上传、Run、Analysis 和 Artifact 能力",
    defaultQuestion: "分析日志中的主要异常、可能原因和建议检查项。",
    questionPlaceholder: "希望 V2 Agent 分析的问题",
    apiKeyRequired: "请填写 API Key",
    refresh: "刷新",
    mode: "模式",
    modeLabels: {
      diagnose: "诊断",
      code_investigation: "代码调查",
      fix: "修复建议"
    },
    chooseFiles: "选择上传文件",
    selectedFiles: (count: number, names: string) => `${count} 个文件：${names}`,
    uploadProgress: "上传进度",
    createWorkspaceRun: "新建 V2 Workspace 并运行",
    runSelectedWorkspace: "运行选中 Workspace",
    saveWorkspace: "保存选中 Workspace",
    deleteWorkspace: "删除选中 Workspace",
    noWorkspaceSelected: "请选择或创建 V2 Workspace",
    createdWorkspace: (workspaceId: string) => `已创建 Workspace ${workspaceId}`,
    savedWorkspace: (workspaceId: string) => `已保存 Workspace ${workspaceId}`,
    deletedWorkspace: (workspaceId: string) => `已删除 Workspace ${workspaceId}`,
    deleteConfirm: (workspaceId: string) => `删除 V2 Workspace ${workspaceId}？历史列表会隐藏它，但已有 run 和 artifacts 会保留在服务端。`,
    uploadingFileCount: (count: number) => `正在上传 ${count} 个文件`,
    createdRun: (runId: string) => `已创建 Run ${runId}`,
    refreshed: "V2 数据已刷新",
    workspaceHistory: "V2 Workspace",
    noWorkspaces: "暂无 V2 Workspace。",
    runs: "V2 Runs",
    noRuns: "当前 Workspace 暂无 Run。",
    uploads: "上传",
    runStatus: "运行状态",
    phase: "阶段",
    updated: "更新",
    evidence: "证据",
    artifacts: "Artifacts",
    timeline: "Timeline",
    resources: "Resources",
    result: "最终结果",
    confidence: "置信度",
    symptoms: "现象",
    likelyRootCauses: "可能根因",
    nextChecks: "后续检查",
    fixSuggestions: "修复建议",
    missingInformation: "缺失信息",
    saveAsCase: "沉淀为 V2 Case",
    caseDescription: "成功 Run 可以人工确认后写入 V2 Memory，供后续相似问题召回。",
    saveCase: "保存 Case",
    savedCase: (caseId: string) => `已保存 V2 Case ${caseId}`,
    caseTitle: "标题",
    caseSymptom: "现象",
    caseRootCause: "根因",
    caseSolution: "解决方案",
    caseEvidenceRefs: "Evidence refs",
    noResult: "Run 尚未生成最终结果。",
    waitingAction: "等待动作",
    answerPlaceholder: "补充 V2 Agent 需要的信息",
    sendAnswer: "提交补充",
    finalizeNow: "没有更多信息，直接生成最终结果",
    finalizeMessage: "没有更多信息，请基于当前证据生成最终结果。",
    approvalRequest: "审批请求",
    approve: "批准",
    reject: "拒绝",
    reasonPlaceholder: "可选：审批原因",
    noPendingAction: "Run 处于等待状态，但没有 pending action。",
    latestEvents: "最近事件",
    empty: "暂无",
    download: "下载",
    downloadFailed: (message: string) => `下载失败：${message}`,
    loadedRun: (runId: string) => `已加载 Run ${runId}`,
    statusLabels: {
      queued: "等待中",
      running: "运行中",
      waiting_for_user: "等待用户",
      waiting_for_approval: "等待审批",
      succeeded: "已成功",
      failed: "已失败"
    }
  },
  "en-US": {
    title: "V2 Analyze Bridge",
    description: "Connects Python V2 workspace, upload, run, analysis, and artifact capabilities",
    defaultQuestion: "Analyze the main log anomalies, possible causes, and suggested checks.",
    questionPlaceholder: "Question for the V2 Agent",
    apiKeyRequired: "API Key required",
    refresh: "Refresh",
    mode: "Mode",
    modeLabels: {
      diagnose: "Diagnose",
      code_investigation: "Code investigation",
      fix: "Fix"
    },
    chooseFiles: "Choose files",
    selectedFiles: (count: number, names: string) => `${count} file(s): ${names}`,
    uploadProgress: "Upload progress",
    createWorkspaceRun: "Create V2 workspace and run",
    runSelectedWorkspace: "Run selected workspace",
    saveWorkspace: "Save selected workspace",
    deleteWorkspace: "Delete selected workspace",
    noWorkspaceSelected: "Select or create a V2 workspace",
    createdWorkspace: (workspaceId: string) => `Created workspace ${workspaceId}`,
    savedWorkspace: (workspaceId: string) => `Saved workspace ${workspaceId}`,
    deletedWorkspace: (workspaceId: string) => `Deleted workspace ${workspaceId}`,
    deleteConfirm: (workspaceId: string) => `Delete V2 workspace ${workspaceId}? It will be hidden from history, while existing runs and artifacts remain on the server.`,
    uploadingFileCount: (count: number) => `Uploading ${count} file(s)`,
    createdRun: (runId: string) => `Created run ${runId}`,
    refreshed: "V2 data refreshed",
    workspaceHistory: "V2 Workspaces",
    noWorkspaces: "No V2 workspace yet.",
    runs: "V2 Runs",
    noRuns: "This workspace has no run yet.",
    uploads: "Uploads",
    runStatus: "Run status",
    phase: "Phase",
    updated: "Updated",
    evidence: "Evidence",
    artifacts: "Artifacts",
    timeline: "Timeline",
    resources: "Resources",
    result: "Final result",
    confidence: "confidence",
    symptoms: "Symptoms",
    likelyRootCauses: "Likely root causes",
    nextChecks: "Next checks",
    fixSuggestions: "Fix suggestions",
    missingInformation: "Missing information",
    saveAsCase: "Save as V2 Case",
    caseDescription: "A succeeded run can be confirmed into V2 Memory for future recall.",
    saveCase: "Save Case",
    savedCase: (caseId: string) => `Saved V2 Case ${caseId}`,
    caseTitle: "Title",
    caseSymptom: "Symptom",
    caseRootCause: "Root cause",
    caseSolution: "Solution",
    caseEvidenceRefs: "Evidence refs",
    noResult: "The run has no final result yet.",
    waitingAction: "Waiting action",
    answerPlaceholder: "Provide the information requested by the V2 Agent",
    sendAnswer: "Send answer",
    finalizeNow: "No more information, finalize with current evidence",
    finalizeMessage: "No more information is available. Please finalize with the current evidence.",
    approvalRequest: "Approval request",
    approve: "Approve",
    reject: "Reject",
    reasonPlaceholder: "Optional approval reason",
    noPendingAction: "The run is waiting, but no pending action is available.",
    latestEvents: "Latest events",
    empty: "No data",
    download: "Download",
    downloadFailed: (message: string) => `Download failed: ${message}`,
    loadedRun: (runId: string) => `Loaded run ${runId}`,
    statusLabels: {
      queued: "Queued",
      running: "Running",
      waiting_for_user: "Waiting for user",
      waiting_for_approval: "Waiting for approval",
      succeeded: "Succeeded",
      failed: "Failed"
    }
  }
} as const;

export function V2AnalyzeBridge({ apiKey, language }: { apiKey: string; language: UiLanguage }) {
  const copy = copyByLanguage[language];
  const [question, setQuestion] = useState<string>(copy.defaultQuestion);
  const [mode, setMode] = useState<V2Mode>("diagnose");
  const [files, setFiles] = useState<File[]>([]);
  const [workspaces, setWorkspaces] = useState<V2Workspace[]>([]);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("");
  const [uploads, setUploads] = useState<V2Upload[]>([]);
  const [runs, setRuns] = useState<V2Run[]>([]);
  const [selectedRunId, setSelectedRunId] = useState("");
  const [analysis, setAnalysis] = useState<V2RunAnalysis | null>(null);
  const [waitingMessage, setWaitingMessage] = useState("");
  const [decisionReason, setDecisionReason] = useState("");
  const [caseDraft, setCaseDraft] = useState<RunCaseDraft>(emptyRunCaseDraft());
  const [caseDraftRunId, setCaseDraftRunId] = useState("");
  const [savedCase, setSavedCase] = useState<V2CaseRecord | null>(null);
  const [status, setStatus] = useState<string>(copy.apiKeyRequired);
  const [uploadProgress, setUploadProgress] = useState(0);
  const [loading, setLoading] = useState(false);
  const previousLanguageRef = useRef(language);

  useEffect(() => {
    const previousLanguage = previousLanguageRef.current;
    if (previousLanguage !== language) {
      if (question === copyByLanguage[previousLanguage].defaultQuestion) {
        setQuestion(copy.defaultQuestion);
      }
      previousLanguageRef.current = language;
    }
  }, [copy.defaultQuestion, language, question]);

  const loadRun = useCallback(async (runId: string, quiet = false) => {
    if (!apiKey.trim()) return;
    const nextAnalysis = await getV2RunAnalysis(apiKey, runId);
    setAnalysis(nextAnalysis);
    setSelectedRunId(nextAnalysis.run.id);
    if (!quiet) setStatus(copy.loadedRun(nextAnalysis.run.id));
  }, [apiKey, copy]);

  const loadWorkspace = useCallback(async (workspaceId: string, preferredRunId?: string) => {
    if (!apiKey.trim()) return;
    setSelectedWorkspaceId(workspaceId);
    const [workspace, uploadResponse, runResponse] = await Promise.all([
      getV2Workspace(apiKey, workspaceId),
      listV2WorkspaceUploads(apiKey, workspaceId),
      listV2WorkspaceRuns(apiKey, workspaceId)
    ]);
    setQuestion(workspace.question);
    setMode(workspace.mode);
    setUploads(uploadResponse.uploads);
    setRuns(runResponse.runs);
    const nextRunId = preferredRunId || runResponse.runs[0]?.id || "";
    if (nextRunId) {
      await loadRun(nextRunId, true);
    } else {
      setSelectedRunId("");
      setAnalysis(null);
    }
  }, [apiKey, loadRun]);

  const refreshWorkspaces = useCallback(async (quiet = false) => {
    if (!apiKey.trim()) {
      setWorkspaces([]);
      setUploads([]);
      setRuns([]);
      setAnalysis(null);
      setStatus(copy.apiKeyRequired);
      return;
    }
    const result = await listV2Workspaces(apiKey);
    setWorkspaces(result.workspaces);
    const nextWorkspaceId = selectedWorkspaceId && result.workspaces.some((workspace) => workspace.id === selectedWorkspaceId)
      ? selectedWorkspaceId
      : result.workspaces[0]?.id || "";
    if (nextWorkspaceId) {
      await loadWorkspace(nextWorkspaceId, selectedRunId || undefined);
    }
    if (!quiet) setStatus(copy.refreshed);
  }, [apiKey, copy.apiKeyRequired, copy.refreshed, loadWorkspace, selectedRunId, selectedWorkspaceId]);

  useEffect(() => {
    void refreshWorkspaces(true).catch((reason) => setStatus(errorMessage(reason)));
  }, [refreshWorkspaces]);

  useEffect(() => {
    if (!apiKey.trim() || !selectedRunId || !analysis || isTerminal(analysis.run.status)) return;
    const timer = window.setInterval(() => {
      void loadWorkspace(selectedWorkspaceId, selectedRunId).catch(() => undefined);
    }, 1500);
    return () => window.clearInterval(timer);
  }, [analysis, apiKey, loadWorkspace, selectedRunId, selectedWorkspaceId]);

  async function createWorkspaceAndRun() {
    if (!apiKey.trim()) {
      setStatus(copy.apiKeyRequired);
      return;
    }
    const trimmedQuestion = question.trim();
    if (!trimmedQuestion) return;
    setLoading(true);
    setUploadProgress(0);
    try {
      const workspace = await createV2Workspace(apiKey, { question: trimmedQuestion, mode, language });
      setStatus(copy.createdWorkspace(workspace.id));
      let uploadedFiles: V2Upload[] = [];
      if (files.length) {
        setStatus(copy.uploadingFileCount(files.length));
        uploadedFiles = await uploadV2Files(apiKey, workspace.id, files, setUploadProgress);
      }
      const run = await createV2Run(apiKey, workspace.id);
      setFiles([]);
      setWorkspaces((current) => [workspace, ...current.filter((item) => item.id !== workspace.id)]);
      setUploads(uploadedFiles);
      setRuns([run]);
      setStatus(copy.createdRun(run.id));
      await loadWorkspace(workspace.id, run.id);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function runSelectedWorkspace() {
    if (!apiKey.trim()) {
      setStatus(copy.apiKeyRequired);
      return;
    }
    if (!selectedWorkspaceId) {
      setStatus(copy.noWorkspaceSelected);
      return;
    }
    setLoading(true);
    setUploadProgress(0);
    try {
      const trimmedQuestion = question.trim();
      if (!trimmedQuestion) return;
      const workspace = await updateV2Workspace(apiKey, selectedWorkspaceId, { question: trimmedQuestion, mode, language });
      setWorkspaces((current) => current.map((item) => item.id === workspace.id ? workspace : item));
      if (files.length) {
        setStatus(copy.uploadingFileCount(files.length));
        await uploadV2Files(apiKey, selectedWorkspaceId, files, setUploadProgress);
      }
      const run = await createV2Run(apiKey, selectedWorkspaceId);
      setFiles([]);
      setStatus(copy.createdRun(run.id));
      await loadWorkspace(selectedWorkspaceId, run.id);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function saveSelectedWorkspace() {
    if (!apiKey.trim() || !selectedWorkspaceId || !question.trim()) return;
    setLoading(true);
    try {
      const workspace = await updateV2Workspace(apiKey, selectedWorkspaceId, { question: question.trim(), mode, language });
      setWorkspaces((current) => current.map((item) => item.id === workspace.id ? workspace : item));
      setStatus(copy.savedWorkspace(workspace.id));
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function deleteSelectedWorkspace() {
    if (!apiKey.trim() || !selectedWorkspaceId) return;
    const workspaceId = selectedWorkspaceId;
    if (!window.confirm(copy.deleteConfirm(workspaceId))) return;
    setLoading(true);
    try {
      await deleteV2Workspace(apiKey, workspaceId);
      setSelectedWorkspaceId("");
      setSelectedRunId("");
      setUploads([]);
      setRuns([]);
      setAnalysis(null);
      setStatus(copy.deletedWorkspace(workspaceId));
      await refreshWorkspaces(true);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function refreshSelectedRun() {
    if (selectedWorkspaceId && selectedRunId) {
      await loadWorkspace(selectedWorkspaceId, selectedRunId);
    } else if (selectedRunId) {
      await loadRun(selectedRunId, true);
    }
  }

  async function sendWaitingMessage(action: V2Action, resumeMode: "continue" | "finalize") {
    if (!selectedRunId) return;
    const message = resumeMode === "finalize"
      ? waitingMessage.trim() || copy.finalizeMessage
      : waitingMessage.trim();
    if (!message) return;
    setLoading(true);
    try {
      const questionId = stringPayload(action.payload, "questionId") || undefined;
      await postV2RunMessage(apiKey, selectedRunId, {
        message,
        resumeMode,
        questionId,
        idempotencyKey: v2IdempotencyKey("message", selectedRunId, action.id, resumeMode, message)
      });
      setWaitingMessage("");
      setStatus(copy.refreshed);
      await refreshSelectedRun();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function decideWaitingAction(action: V2Action, decision: "approved" | "rejected") {
    setLoading(true);
    try {
      const reason = decisionReason.trim() || null;
      await decideV2Action(apiKey, action.id, {
        decision,
        reason,
        idempotencyKey: v2IdempotencyKey("decision", selectedRunId || action.run_id, action.id, decision, reason ?? "")
      });
      setDecisionReason("");
      setStatus(copy.refreshed);
      await refreshSelectedRun();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function handleDownloadArtifact(artifactId: string, relativePath: string) {
    try {
      await downloadV2Artifact(apiKey, artifactId, filenameFromPath(relativePath));
    } catch (reason) {
      setStatus(copy.downloadFailed(errorMessage(reason)));
    }
  }

  const selectedWorkspace = workspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? analysis?.workspace ?? null;
  const selectedRun = runs.find((run) => run.id === selectedRunId) ?? analysis?.run ?? null;
  const finalAnswer = analysis?.result?.finalAnswer ?? analysis?.run.finalAnswer ?? null;
  const selectedRunStatus = selectedRun?.status;
  const selectedRunCaseId = selectedRun?.id ?? "";

  useEffect(() => {
    if (selectedRunStatus === "succeeded" && finalAnswer && caseDraftRunId !== selectedRunCaseId) {
      setCaseDraft(caseDraftFromFinalAnswer(finalAnswer));
      setSavedCase(null);
      setCaseDraftRunId(selectedRunCaseId);
    }
    if (!selectedRunCaseId || selectedRunStatus !== "succeeded") {
      setCaseDraftRunId("");
      setSavedCase(null);
    }
  }, [caseDraftRunId, finalAnswer, selectedRunCaseId, selectedRunStatus]);

  async function saveRunCase() {
    if (!selectedRunId || !isCaseDraftComplete(caseDraft)) return;
    setLoading(true);
    try {
      const record = await confirmV2RunCase(apiKey, selectedRunId, runCasePayload(caseDraft));
      setSavedCase(record);
      setStatus(copy.savedCase(record.caseId));
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Workflow className="h-5 w-5 text-primary" />
              <CardTitle>{copy.title}</CardTitle>
              {selectedRun ? <RunStatusBadge copy={copy} status={selectedRun.status} /> : null}
            </div>
            <CardDescription>{copy.description}</CardDescription>
          </div>
          <Button className="h-8 px-3" variant="outline" disabled={loading || !apiKey.trim()} onClick={() => void refreshWorkspaces()}>
            <RefreshCw className="mr-2 h-4 w-4" />{copy.refresh}
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_420px]">
          <div className="space-y-4">
            <textarea
              className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
              value={question}
              onChange={(event) => setQuestion(event.target.value)}
              placeholder={copy.questionPlaceholder}
            />
            <div className="grid gap-3 md:grid-cols-[220px_1fr]">
              <label className="rounded-lg border border-border p-3 text-sm">
                <span className="mb-2 block text-xs text-muted-foreground">{copy.mode}</span>
                <select className="w-full bg-transparent text-sm outline-none" value={mode} onChange={(event) => setMode(event.target.value as V2Mode)}>
                  {(["diagnose", "code_investigation", "fix"] as const).map((item) => <option key={item} value={item}>{copy.modeLabels[item]}</option>)}
                </select>
              </label>
              <label className="flex min-h-20 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 text-sm text-muted-foreground">
                <UploadCloud className="mb-2 h-6 w-6" />
                {files.length ? copy.selectedFiles(files.length, files.map((file) => file.name).join(", ")) : copy.chooseFiles}
                <input className="hidden" type="file" multiple onChange={(event) => setFiles(Array.from(event.target.files ?? []))} />
              </label>
            </div>
            <div>
              <div className="mb-1 flex justify-between text-xs text-muted-foreground"><span>{copy.uploadProgress}</span><span>{uploadProgress}%</span></div>
              <div className="h-2 overflow-hidden rounded bg-slate-100"><div className="h-full bg-primary transition-all" style={{ width: `${uploadProgress}%` }} /></div>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span className="text-sm text-muted-foreground">{status}</span>
              <div className="flex flex-wrap gap-2">
                <Button disabled={loading || !apiKey.trim() || !question.trim()} onClick={() => void createWorkspaceAndRun()}>
                  <Play className="mr-2 h-4 w-4" />{copy.createWorkspaceRun}
                </Button>
                <Button disabled={loading || !apiKey.trim() || !selectedWorkspaceId} variant="outline" onClick={() => void runSelectedWorkspace()}>
                  <Play className="mr-2 h-4 w-4" />{copy.runSelectedWorkspace}
                </Button>
                <Button disabled={loading || !apiKey.trim() || !selectedWorkspaceId || !question.trim()} variant="outline" onClick={() => void saveSelectedWorkspace()}>
                  <Save className="mr-2 h-4 w-4" />{copy.saveWorkspace}
                </Button>
                <Button disabled={loading || !apiKey.trim() || !selectedWorkspaceId} variant="outline" onClick={() => void deleteSelectedWorkspace()}>
                  <Trash2 className="mr-2 h-4 w-4" />{copy.deleteWorkspace}
                </Button>
              </div>
            </div>
          </div>

          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-1">
            <HistoryList copy={copy} workspaces={workspaces} selectedWorkspaceId={selectedWorkspaceId} onSelect={(workspaceId) => void loadWorkspace(workspaceId)} />
            <RunList copy={copy} runs={runs} selectedRunId={selectedRunId} onSelect={(runId) => void loadRun(runId)} />
          </div>
        </div>

        {selectedWorkspace || selectedRun ? (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
            <Metric label="Workspace" value={selectedWorkspace?.id ?? "-"} />
            <Metric label={copy.runStatus} value={selectedRun ? `${statusLabel(copy, selectedRun.status)} · ${selectedRun.id}` : "-"} />
            <Metric label={copy.phase} value={selectedRun?.phase ?? "-"} />
            <Metric label={copy.updated} value={selectedRun?.updated_at ? new Date(selectedRun.updated_at).toLocaleString() : selectedWorkspace?.updated_at ? new Date(selectedWorkspace.updated_at).toLocaleString() : "-"} />
            <Metric label={copy.uploads} value={String(uploads.length)} />
            <Metric label={copy.evidence} value={String(analysis?.evidence.length ?? 0)} />
            <Metric label={copy.artifacts} value={String((analysis?.artifacts.uploads.length ?? 0) + (analysis?.artifacts.evidenceArtifacts.length ?? 0))} />
            <Metric label={copy.resources} value={String(countResources(analysis))} />
          </div>
        ) : null}

        <WaitingActionsPanel
          copy={copy}
          run={selectedRun}
          actions={analysis?.pendingActions ?? []}
          message={waitingMessage}
          reason={decisionReason}
          loading={loading}
          onMessageChange={setWaitingMessage}
          onReasonChange={setDecisionReason}
          onSend={(action, resumeMode) => void sendWaitingMessage(action, resumeMode)}
          onDecision={(action, decision) => void decideWaitingAction(action, decision)}
        />

        <div className="grid gap-5 xl:grid-cols-2">
          <FinalAnswerView copy={copy} answer={finalAnswer} />
          <TimelineView copy={copy} analysis={analysis} />
        </div>

        {selectedRun?.status === "succeeded" && finalAnswer ? (
          <RunCasePanel
            copy={copy}
            draft={caseDraft}
            savedCase={savedCase}
            loading={loading}
            onDraftChange={setCaseDraft}
            onSave={() => void saveRunCase()}
          />
        ) : null}

        {analysis ? (
          <ArtifactList
            copy={copy}
            apiKey={apiKey}
            artifacts={analysis.artifacts.evidenceArtifacts}
            uploads={analysis.artifacts.uploads}
            onDownload={(artifactId, relativePath) => void handleDownloadArtifact(artifactId, relativePath)}
          />
        ) : null}
      </CardContent>
    </Card>
  );
}

function HistoryList({ copy, workspaces, selectedWorkspaceId, onSelect }: { copy: BridgeCopy; workspaces: V2Workspace[]; selectedWorkspaceId: string; onSelect: (workspaceId: string) => void }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <h3 className="mb-2 text-sm font-semibold">{copy.workspaceHistory}</h3>
      <div className="max-h-56 space-y-2 overflow-auto">
        {workspaces.length ? workspaces.map((workspace) => (
          <button className={`w-full rounded-md border p-2 text-left ${selectedWorkspaceId === workspace.id ? "border-primary bg-slate-50" : "border-border"}`} key={workspace.id} onClick={() => onSelect(workspace.id)}>
            <p className="truncate text-sm font-medium">{workspace.question}</p>
            <p className="mt-1 font-mono text-xs text-muted-foreground">{workspace.id}</p>
            <p className="mt-1 text-xs text-muted-foreground">{copy.modeLabels[workspace.mode]} · {new Date(workspace.created_at).toLocaleString()}</p>
          </button>
        )) : <EmptyState>{copy.noWorkspaces}</EmptyState>}
      </div>
    </div>
  );
}

function RunList({ copy, runs, selectedRunId, onSelect }: { copy: BridgeCopy; runs: V2Run[]; selectedRunId: string; onSelect: (runId: string) => void }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <h3 className="mb-2 text-sm font-semibold">{copy.runs}</h3>
      <div className="max-h-56 space-y-2 overflow-auto">
        {runs.length ? runs.map((run) => (
          <button className={`w-full rounded-md border p-2 text-left ${selectedRunId === run.id ? "border-primary bg-slate-50" : "border-border"}`} key={run.id} onClick={() => onSelect(run.id)}>
            <div className="flex items-center justify-between gap-2">
              <span className="font-mono text-xs">{run.id}</span>
              <RunStatusBadge copy={copy} status={run.status} />
            </div>
            <p className="mt-1 text-xs text-muted-foreground">{run.phase} · {new Date(run.created_at).toLocaleString()}</p>
          </button>
        )) : <EmptyState>{copy.noRuns}</EmptyState>}
      </div>
    </div>
  );
}

function RunCasePanel({
  copy,
  draft,
  savedCase,
  loading,
  onDraftChange,
  onSave
}: {
  copy: BridgeCopy;
  draft: RunCaseDraft;
  savedCase: V2CaseRecord | null;
  loading: boolean;
  onDraftChange: (draft: RunCaseDraft) => void;
  onSave: () => void;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-border p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <BookOpenCheck className="h-5 w-5 text-primary" />
            <h3 className="text-sm font-semibold">{copy.saveAsCase}</h3>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">{copy.caseDescription}</p>
        </div>
        {savedCase ? <Badge variant="success">{savedCase.caseId}</Badge> : null}
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label className="space-y-1 text-xs text-muted-foreground">
          {copy.caseTitle}
          <input
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20"
            value={draft.title}
            onChange={(event) => onDraftChange({ ...draft, title: event.target.value })}
          />
        </label>
        <label className="space-y-1 text-xs text-muted-foreground">
          {copy.caseEvidenceRefs}
          <input
            className="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-xs text-foreground outline-none focus:ring-2 focus:ring-primary/20"
            value={draft.evidenceRefsText}
            onChange={(event) => onDraftChange({ ...draft, evidenceRefsText: event.target.value })}
          />
        </label>
      </div>
      <div className="grid gap-3 md:grid-cols-3">
        <LabeledCaseTextarea label={copy.caseSymptom} value={draft.symptom} onChange={(value) => onDraftChange({ ...draft, symptom: value })} />
        <LabeledCaseTextarea label={copy.caseRootCause} value={draft.rootCause} onChange={(value) => onDraftChange({ ...draft, rootCause: value })} />
        <LabeledCaseTextarea label={copy.caseSolution} value={draft.solution} onChange={(value) => onDraftChange({ ...draft, solution: value })} />
      </div>
      <div className="flex justify-end">
        <Button disabled={loading || !isCaseDraftComplete(draft)} onClick={onSave}>
          <BookOpenCheck className="mr-2 h-4 w-4" />{copy.saveCase}
        </Button>
      </div>
    </div>
  );
}

function LabeledCaseTextarea({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label className="space-y-1 text-xs text-muted-foreground">
      {label}
      <textarea
        className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

function WaitingActionsPanel({
  copy,
  run,
  actions,
  message,
  reason,
  loading,
  onMessageChange,
  onReasonChange,
  onSend,
  onDecision
}: {
  copy: BridgeCopy;
  run: V2Run | null;
  actions: V2Action[];
  message: string;
  reason: string;
  loading: boolean;
  onMessageChange: (value: string) => void;
  onReasonChange: (value: string) => void;
  onSend: (action: V2Action, resumeMode: "continue" | "finalize") => void;
  onDecision: (action: V2Action, decision: "approved" | "rejected") => void;
}) {
  if (!run || !run.status.startsWith("waiting")) return null;
  const action = actions[0];
  if (!action) {
    return (
      <div className="rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-800">
        <h3 className="mb-2 font-semibold">{copy.waitingAction}</h3>
        <p>{copy.noPendingAction}</p>
      </div>
    );
  }
  if (run.status === "waiting_for_approval" || action.kind === "approval") {
    return (
      <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-4">
        <div className="flex items-center gap-2">
          <CheckCircle2 className="h-5 w-5 text-amber-700" />
          <h3 className="text-sm font-semibold text-amber-900">{copy.approvalRequest}</h3>
        </div>
        <div className="grid gap-3 text-sm md:grid-cols-3">
          <Metric label="Action" value={stringPayload(action.payload, "actionType") || action.kind} />
          <Metric label="Reason" value={stringPayload(action.payload, "reason") || "-"} />
          <Metric label="Action ID" value={action.id} />
        </div>
        <pre className="max-h-48 overflow-auto rounded-md border border-amber-200 bg-white/70 p-3 text-xs">{JSON.stringify(action.payload.input ?? action.payload, null, 2)}</pre>
        <textarea
          className="min-h-20 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-amber-500/20"
          value={reason}
          onChange={(event) => onReasonChange(event.target.value)}
          placeholder={copy.reasonPlaceholder}
        />
        <div className="flex flex-wrap justify-end gap-2">
          <Button disabled={loading} variant="outline" onClick={() => onDecision(action, "rejected")}><XCircle className="mr-2 h-4 w-4" />{copy.reject}</Button>
          <Button disabled={loading} onClick={() => onDecision(action, "approved")}><CheckCircle2 className="mr-2 h-4 w-4" />{copy.approve}</Button>
        </div>
      </div>
    );
  }
  return (
    <div className="space-y-3 rounded-lg border border-amber-200 bg-amber-50 p-4">
      <div className="flex items-center gap-2">
        <MessageSquare className="h-5 w-5 text-amber-700" />
        <h3 className="text-sm font-semibold text-amber-900">{copy.waitingAction}</h3>
      </div>
      <p className="text-sm text-amber-900">{stringPayload(action.payload, "question") || copy.noPendingAction}</p>
      {stringPayload(action.payload, "reason") ? <p className="text-xs text-amber-800">{stringPayload(action.payload, "reason")}</p> : null}
      <textarea
        className="min-h-24 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-amber-500/20"
        value={message}
        onChange={(event) => onMessageChange(event.target.value)}
        placeholder={copy.answerPlaceholder}
      />
      <div className="flex flex-wrap justify-end gap-2">
        <Button disabled={loading} variant="outline" onClick={() => onSend(action, "finalize")}>{copy.finalizeNow}</Button>
        <Button disabled={loading || !message.trim()} onClick={() => onSend(action, "continue")}><MessageSquare className="mr-2 h-4 w-4" />{copy.sendAnswer}</Button>
      </div>
    </div>
  );
}

function FinalAnswerView({ copy, answer }: { copy: BridgeCopy; answer: V2FinalAnswer | null }) {
  if (!answer) {
    return <div className="rounded-lg border border-border p-4"><h3 className="text-sm font-semibold">{copy.result}</h3><EmptyState>{copy.noResult}</EmptyState></div>;
  }
  return (
    <div className="rounded-lg border border-border p-4">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold">{copy.result}</h3>
        {answer.confidence ? <Badge variant="secondary">{copy.confidence}: {answer.confidence}</Badge> : null}
      </div>
      <p className="text-sm">{answer.summary || copy.empty}</p>
      <AnswerList title={copy.symptoms} items={answer.symptoms ?? []} />
      <div className="mt-4">
        <h4 className="mb-2 text-xs font-medium text-muted-foreground">{copy.likelyRootCauses}</h4>
        {(answer.likelyRootCauses ?? []).length ? answer.likelyRootCauses?.map((cause, index) => (
          <div className="mb-2 rounded-md bg-slate-50 p-2 text-sm" key={`${cause.cause}:${index}`}>
            <p>{cause.cause}</p>
            {cause.evidenceRefs?.length ? <p className="mt-1 font-mono text-xs text-muted-foreground">{cause.evidenceRefs.join(", ")}</p> : null}
          </div>
        )) : <p className="text-sm text-muted-foreground">{copy.empty}</p>}
      </div>
      <AnswerList title={copy.nextChecks} items={answer.nextChecks ?? []} />
      <AnswerList title={copy.fixSuggestions} items={answer.fixSuggestions ?? []} />
      <AnswerList title={copy.missingInformation} items={answer.missingInformation ?? []} />
    </div>
  );
}

function TimelineView({ copy, analysis }: { copy: BridgeCopy; analysis: V2RunAnalysis | null }) {
  const latest = (analysis?.timeline ?? []).slice(-8).reverse();
  return (
    <div className="rounded-lg border border-border p-4">
      <h3 className="mb-3 text-sm font-semibold">{copy.latestEvents}</h3>
      {latest.length ? (
        <ol className="space-y-2">
          {latest.map((event) => (
            <li className="rounded-md bg-slate-50 p-2" key={event.id}>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <Badge variant="outline">{eventKind(event)}</Badge>
                <span>{new Date(event.created_at).toLocaleTimeString()}</span>
              </div>
              <p className="mt-1 break-words text-xs text-muted-foreground">{summarizePayload(event.payload)}</p>
            </li>
          ))}
        </ol>
      ) : <EmptyState>{copy.empty}</EmptyState>}
    </div>
  );
}

function ArtifactList({ copy, artifacts, uploads, onDownload }: { copy: BridgeCopy; apiKey: string; artifacts: V2EvidenceArtifact[]; uploads: V2RunAnalysis["artifacts"]["uploads"]; onDownload: (artifactId: string, relativePath: string) => void }) {
  const items = [
    ...uploads.map((upload) => ({
      id: upload.artifact_id,
      kind: copy.uploads,
      summary: upload.filename,
      relativePath: upload.relative_path,
      sizeBytes: upload.size_bytes,
      contentType: upload.content_type
    })),
    ...artifacts.map((artifact) => ({
      id: artifact.artifact_id,
      kind: artifact.evidence_kind,
      summary: artifact.evidence_summary,
      relativePath: artifact.relative_path,
      sizeBytes: artifact.size_bytes,
      contentType: artifact.content_type
    }))
  ];
  return (
    <div className="rounded-lg border border-border p-4">
      <div className="mb-3 flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">{copy.artifacts}</h3>
        <Badge variant="secondary">{items.length}</Badge>
      </div>
      {items.length ? (
        <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
          {items.map((item) => (
            <div className="rounded-md border border-border p-3" key={`${item.kind}:${item.id}`}>
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium"><FileArchive className="mr-2 inline h-4 w-4 text-slate-400" />{item.kind}</p>
                  <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.summary}</p>
                </div>
                <Button className="h-8 w-8 shrink-0 px-0" variant="outline" title={copy.download} aria-label={copy.download} onClick={() => onDownload(item.id, item.relativePath)}>
                  <Download className="h-4 w-4" />
                </Button>
              </div>
              <p className="mt-2 break-all font-mono text-xs text-muted-foreground">{item.relativePath}</p>
              <p className="mt-1 text-xs text-muted-foreground">{item.contentType} · {item.sizeBytes.toLocaleString()} bytes</p>
            </div>
          ))}
        </div>
      ) : <EmptyState>{copy.empty}</EmptyState>}
    </div>
  );
}

function AnswerList({ title, items }: { title: string; items: string[] }) {
  return (
    <div className="mt-4">
      <h4 className="mb-2 text-xs font-medium text-muted-foreground">{title}</h4>
      {items.length ? <ul className="space-y-1 text-sm">{items.map((item, index) => <li className="rounded-md bg-slate-50 p-2" key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="text-sm text-muted-foreground">-</p>}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value}</p></div>;
}

function RunStatusBadge({ copy, status }: { copy: BridgeCopy; status: V2RunStatus }) {
  const variant = status === "failed" ? "destructive" : status === "succeeded" ? "success" : status.startsWith("waiting") ? "warning" : "secondary";
  return <Badge variant={variant}>{statusLabel(copy, status)}</Badge>;
}

function statusLabel(copy: BridgeCopy, status: V2RunStatus) {
  return copy.statusLabels[status] ?? status;
}

function isTerminal(status: V2RunStatus) {
  return status === "succeeded" || status === "failed";
}

function countResources(analysis: V2RunAnalysis | null) {
  if (!analysis) return 0;
  return Object.values(analysis.resources).filter(Boolean).length;
}

function summarizePayload(payload: Record<string, unknown>) {
  const message = payload.message;
  if (typeof message === "string") return message;
  const summary = payload.summary;
  if (typeof summary === "string") return summary;
  const question = payload.question;
  if (typeof question === "string") return question;
  return JSON.stringify(payload);
}

function eventKind(event: { kind?: string; event_type?: string }) {
  return event.kind || event.event_type || "event";
}

function stringPayload(payload: Record<string, unknown>, key: string) {
  const value = payload[key];
  return typeof value === "string" ? value : "";
}

function v2IdempotencyKey(kind: "message" | "decision", runId: string, actionId: string, intent: string, content: string) {
  return `v2:${kind}:${runId}:${actionId}:${intent}:${stableHash(content)}`.slice(0, 200);
}

function stableHash(value: string) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function filenameFromPath(relativePath: string) {
  return relativePath.split("/").filter(Boolean).pop() || "artifact";
}

function emptyRunCaseDraft(): RunCaseDraft {
  return {
    title: "",
    symptom: "",
    rootCause: "",
    solution: "",
    evidenceRefsText: ""
  };
}

function caseDraftFromFinalAnswer(answer: V2FinalAnswer): RunCaseDraft {
  const rootCauses = (answer.likelyRootCauses ?? [])
    .map((item) => item.cause)
    .filter(Boolean);
  const evidenceRefs = collectFinalEvidenceRefs(answer);
  return {
    title: answer.summary || "",
    symptom: (answer.symptoms ?? []).join("\n"),
    rootCause: rootCauses.join("\n"),
    solution: (answer.fixSuggestions ?? []).join("\n"),
    evidenceRefsText: evidenceRefs.join("\n")
  };
}

function collectFinalEvidenceRefs(answer: V2FinalAnswer): string[] {
  const refs = new Set<string>();
  for (const ref of answer.evidenceRefs ?? []) {
    if (ref.trim()) refs.add(ref.trim());
  }
  for (const cause of answer.likelyRootCauses ?? []) {
    for (const ref of cause.evidenceRefs ?? []) {
      if (ref.trim()) refs.add(ref.trim());
    }
  }
  return Array.from(refs);
}

function runCasePayload(draft: RunCaseDraft): V2CaseDraft {
  return {
    title: draft.title.trim(),
    symptom: draft.symptom.trim(),
    rootCause: draft.rootCause.trim(),
    solution: draft.solution.trim(),
    evidenceRefs: draft.evidenceRefsText
      .split(/[\n,]/)
      .map((item) => item.trim())
      .filter(Boolean)
  };
}

function isCaseDraftComplete(draft: RunCaseDraft) {
  return Boolean(
    draft.title.trim() &&
    draft.symptom.trim() &&
    draft.rootCause.trim() &&
    draft.solution.trim()
  );
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
