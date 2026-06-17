import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";

const CHUNK_BYTES = 512 * 1024;

export type V2Mode = "diagnose" | "code_investigation" | "fix";
export type V2RunStatus = "queued" | "running" | "waiting_for_user" | "waiting_for_approval" | "succeeded" | "failed";

export type V2Workspace = {
  id: string;
  question: string;
  mode: V2Mode;
  language: "zh-CN" | "en-US";
  status: string;
  skillIds: string[];
  created_at: string;
  updated_at: string;
};

export type V2Run = {
  id: string;
  workspace_id: string;
  status: V2RunStatus;
  phase: string;
  budget: Record<string, unknown>;
  finalAnswer?: V2FinalAnswer | null;
  created_at: string;
  updated_at: string;
};

export type V2Upload = {
  id: string;
  workspace_id: string;
  filename: string;
  artifact_id: string;
  created_at: string;
  artifact_relative_path?: string;
  artifact_size_bytes?: number;
};

export type V2Artifact = {
  id?: string;
  artifact_id?: string;
  relative_path: string;
  size_bytes: number;
  content_type: string;
  schema_name?: string | null;
  created_at?: string;
};

export type V2Evidence = {
  id: string;
  workspace_id: string;
  run_id?: string | null;
  kind: string;
  summary: string;
  artifact_id?: string | null;
  final_allowed: boolean;
  payload: Record<string, unknown>;
  created_at: string;
};

export type V2TimelineEvent = {
  id: string;
  workspace_id: string;
  run_id?: string | null;
  kind: string;
  event_type?: string;
  payload: Record<string, unknown>;
  created_at: string;
};

export type V2Action = {
  id: string;
  run_id: string;
  kind: "user_input" | "approval" | string;
  status: "pending" | "answered" | "approved" | "rejected" | string;
  payload: Record<string, unknown>;
  result?: Record<string, unknown> | null;
  created_at: string;
  updated_at: string;
};

export type V2EvidenceArtifact = {
  evidence_id: string;
  evidence_kind: string;
  evidence_summary: string;
  artifact_id: string;
  relative_path: string;
  size_bytes: number;
  content_type: string;
  schema_name?: string | null;
  preview?: Record<string, unknown>;
  artifact_created_at: string;
};

export type V2SupportArtifact = {
  artifact_id: string;
  logical_path: string;
  relative_path: string;
  size_bytes: number;
  content_type: string;
  schema_name?: string | null;
  preview?: Record<string, unknown>;
  created_at: string;
  role?: string | null;
  action_id?: string | null;
  source_evidence_id?: string | null;
  source_evidence_kind?: string | null;
};

export type V2RunArtifacts = {
  run: V2Run;
  uploads: Array<{
    upload_id: string;
    filename: string;
    artifact_id: string;
    relative_path: string;
    size_bytes: number;
    content_type: string;
    created_at: string;
  }>;
  evidenceArtifacts: V2EvidenceArtifact[];
  supportArtifacts?: V2SupportArtifact[];
};

export type V2FinalAnswer = {
  summary?: string;
  symptoms?: string[];
  likelyRootCauses?: Array<{ cause: string; evidenceRefs?: string[] }>;
  nextChecks?: string[];
  fixSuggestions?: string[];
  missingInformation?: string[];
  confidence?: "low" | "medium" | "high" | string;
  evidenceRefs?: string[];
};

export type V2RunResult = {
  run: V2Run;
  finalAnswer: V2FinalAnswer;
  result: Record<string, unknown>;
  artifacts: {
    json?: V2Artifact;
    markdown?: V2Artifact;
  };
  evidence: Record<string, unknown>;
};

export type V2RunAnalysis = {
  run: V2Run;
  workspace: V2Workspace;
  timeline: V2TimelineEvent[];
  evidence: V2Evidence[];
  actions: V2Action[];
  pendingActions: V2Action[];
  artifacts: V2RunArtifacts;
  resources: Record<string, unknown | null>;
  result: V2RunResult | null;
};

export type V2CaseRecord = {
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

export type V2CaseHit = V2CaseRecord & {
  score: number;
  searchBackend?: string;
  ftsScore?: number;
  vectorScore?: number;
};

export type V2CaseDraft = {
  title?: string | null;
  symptom?: string | null;
  rootCause?: string | null;
  solution?: string | null;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  instanceId?: string | null;
  nodeId?: string | null;
  evidenceRefs?: string[];
};

export type V2CaseImportMessage = {
  role: "user" | "assistant" | string;
  content: string;
  createdAt: string;
};

export type V2CaseImport = {
  importId: string;
  status: "previewed" | "confirmed" | string;
  filename?: string | null;
  caseId?: string | null;
  draft: V2CaseDraft;
  validationErrors: string[];
  messages?: V2CaseImportMessage[];
  sourceSizeBytes: number;
  createdAt: string;
  updatedAt: string;
};

export type V2ToolDescriptor = {
  toolId: string;
  displayName: string;
  enabled: boolean;
  backend: string;
  readOnly: boolean;
  editable?: boolean;
  exportable?: boolean;
  runnable: boolean;
  tags?: string[];
  source?: "built_in" | "configured" | string;
  manualOnly?: boolean;
  minFiles?: number;
  maxFiles?: number;
  maxInputFiles?: number;
  acceptedSuffixes?: string[];
  match?: {
    filePatterns?: string[];
    keywords?: string[];
  };
  paramsSchema?: Record<string, unknown>;
  paramsTemplate?: Record<string, unknown>;
  outputViews?: string[];
  allowedHosts?: string[];
};

export type V2McpResponse = {
  jsonrpc: "2.0";
  id?: string | number | null;
  result?: unknown;
  error?: { code: number; message: string };
};

export type V2SkillReference = {
  referenceId: string;
  path: string;
  title: string;
  summary: string;
};

export type V2SkillSummary = {
  skillId: string;
  name: string;
  description: string;
  displayName: string;
  includeByDefault: boolean;
  priority: number;
  products: string[];
  taskKinds: string[];
  toolIds: string[];
  keywords: string[];
  domainAdapters: string[];
  references: V2SkillReference[];
  revision: string;
  sourcePath: string;
  content?: string;
};

export type V2SystemContextPreview = {
  schemaVersion: number;
  workspaceId: string | null;
  runId: string | null;
  resources: Array<{
    kind: string;
    skillId?: string;
    selectionReason?: string;
    matchScore?: number;
    revision?: string;
    sourcePath?: string;
    summary?: string;
    content?: string;
    references?: V2SkillReference[];
  }>;
};

export type V2MetadataInstanceSummary = {
  instanceId: string;
  remark?: string | null;
  templateType: string;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  nodeCount: number;
  databaseCount: number;
  created_at: string;
  updated_at: string;
};

export type V2MetadataImport = {
  importId: string;
  instanceId: string;
  templateType: "json" | "yaml" | "opengemini" | string;
  remark?: string | null;
  status: string;
  sourceUrl?: string | null;
  nodeCount: number;
  databaseCount: number;
  createdAt: string;
  updatedAt: string;
};

export type V2MetadataImportResponse = {
  import: V2MetadataImport;
  snapshot: Record<string, unknown>;
};

export type V2FetchEndpoint = {
  id: string;
  name: string;
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | string;
  url: string;
  headers: Record<string, string>;
  bodyPreview?: string;
  enabled: boolean;
  hasCredentials?: boolean;
  credentialSet?: {
    id: string;
    redacted: unknown;
    updatedAt: string;
  };
  createdAt: string;
  updatedAt: string;
};

export type V2FetchPreview = {
  schemaVersion: number;
  endpoint: V2FetchEndpoint;
  detectedSensitiveFields: Array<{ location: string; name: string }>;
  unsupportedWarnings: string[];
};

export type V2FetchRunResult = {
  result: Record<string, unknown>;
  artifact: V2Artifact;
  evidence: V2Evidence;
};

export type V2FetchRunOverrides = {
  variables?: Record<string, string>;
  headers?: Record<string, string>;
  body?: string | null;
};

export type V2LlmSummary = {
  provider: string;
  configuredModel: string;
  maxInputChars: number;
  maxOutputTokens: number;
  requestTimeoutSeconds: number;
  baseUrlConfigured: boolean;
  apiKeyConfigured: boolean;
  binaryPathConfigured: boolean;
};

export type V2LlmTestResponse<T> = {
  ok: boolean;
  result?: T | null;
  error?: string | null;
};

export type V2LlmModelsResult = {
  provider: string;
  configuredModel: string;
  models: string[];
  raw: unknown;
};

export type V2LlmChatResult = {
  provider: string;
  model: string;
  response: string;
};

export type V2GraphRuntime = {
  engine: string;
  graph: string;
  nodes: string[];
};

export type V2AgentBackendSummary = {
  id: string;
  backendType: string;
  graphRuntime?: V2GraphRuntime;
  enabled: boolean;
  defaultBackend: boolean;
  commandConfigured: boolean;
  timeoutSeconds: number;
  maxInputBytes: number;
  maxOutputBytes: number;
  executionMode: string;
  defaultMode?: string;
  permissionProfile?: string;
};

export type V2AgentBackendsSummary = {
  defaultBackend: string;
  backends: V2AgentBackendSummary[];
};

export type V2AgentBackendDiagnosticResult = {
  backendId: string;
  backendType: string;
  enabled: boolean;
  status: string;
  executionMode: string;
  graphRuntime?: V2GraphRuntime;
  details: string[];
};

export type V2DomainAdapterSummary = {
  id: string;
  displayName: string;
  status: string;
  products: string[];
  evidenceKinds: string[];
  plannedTools: string[];
  notes: string[];
};

export type V2RemoteRunStatus = "QUEUED" | "RUNNING" | "WAITING_FOR_USER" | "WAITING_FOR_APPROVAL" | "SUCCEEDED" | "FAILED";

export type V2RemoteExecutorRecord = {
  executorId: string;
  name: string;
  host: string;
  port: number;
  user: string;
  tags: string[];
  enabled: boolean;
  notes?: string | null;
  lastCheck?: { checkedAt: string; status: "OK" | "FAILED"; message: string } | null;
  createdAt: string;
  updatedAt: string;
};

export type V2RemoteCommandTemplate = {
  commandId: string;
  displayName: string;
  description: string;
  enabled: boolean;
  argv: string[];
  timeoutSeconds?: number | null;
};

export type V2RemoteRunSummary = {
  taskId: string;
  alias?: string | null;
  taskKind: "remote_command_run";
  status: V2RemoteRunStatus;
  phase?: string | null;
  createdAt: string;
};

export type V2RemoteRunRecord = V2RemoteRunSummary & {
  attempts?: number;
  remoteExecutorId?: string | null;
  remoteCommandId?: string | null;
  error?: { phase?: string | null; message: string } | null;
  updatedAt?: string;
};

export type V2RemoteRunResult = {
  taskId: string;
  executorId: string;
  commandId: string;
  resultPath: string;
  result: {
    status: "OK" | "FAILED" | "TIMED_OUT";
    exitCode?: number | null;
    durationMs: number;
    commandArgv: string[];
    sshArgvPreview?: string[];
    stdoutPath: string;
    stderrPath: string;
    stdoutPreview: string;
    stderrPreview: string;
    warnings: string[];
    error?: string | null;
    startedAt: string;
    completedAt: string;
  };
};

export async function listV2Workspaces(apiKey: string) {
  return fetchJson<{ workspaces: V2Workspace[] }>("/api/v2/workspaces", { headers: authHeaders(apiKey) });
}

export async function createV2Workspace(apiKey: string, input: { question: string; mode: V2Mode; language: "zh-CN" | "en-US"; skillIds?: string[] }) {
  return fetchJson<V2Workspace>("/api/v2/workspaces", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function getV2Workspace(apiKey: string, workspaceId: string) {
  return fetchJson<V2Workspace>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}`, { headers: authHeaders(apiKey) });
}

export async function updateV2Workspace(apiKey: string, workspaceId: string, input: Partial<{ question: string; mode: V2Mode; language: "zh-CN" | "en-US"; skillIds: string[] }>) {
  return fetchJson<V2Workspace>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}`, {
    method: "PATCH",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function deleteV2Workspace(apiKey: string, workspaceId: string) {
  return fetchJson<{ deleted: boolean; workspaceId: string; workspace: V2Workspace }>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}`, {
    method: "DELETE",
    headers: authHeaders(apiKey)
  });
}

export async function listV2WorkspaceRuns(apiKey: string, workspaceId: string) {
  return fetchJson<{ runs: V2Run[] }>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}/runs`, { headers: authHeaders(apiKey) });
}

export async function listV2WorkspaceUploads(apiKey: string, workspaceId: string) {
  return fetchJson<{ uploads: V2Upload[] }>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}/uploads`, { headers: authHeaders(apiKey) });
}

export async function createV2Run(apiKey: string, workspaceId: string) {
  return fetchJson<V2Run>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}/runs`, {
    method: "POST",
    headers: authHeaders(apiKey)
  });
}

export async function getV2Run(apiKey: string, runId: string) {
  return fetchJson<V2Run>(`/api/v2/runs/${encodeURIComponent(runId)}`, { headers: authHeaders(apiKey) });
}

export async function getV2RunAnalysis(apiKey: string, runId: string) {
  return fetchJson<V2RunAnalysis>(`/api/v2/runs/${encodeURIComponent(runId)}/analysis`, { headers: authHeaders(apiKey) });
}

export async function postV2RunMessage(apiKey: string, runId: string, input: { message: string; resumeMode?: "continue" | "finalize"; questionId?: string; idempotencyKey?: string }) {
  return fetchJson<{ event: V2TimelineEvent; answeredActions?: V2Action[]; job?: Record<string, unknown> | null }>(`/api/v2/runs/${encodeURIComponent(runId)}/messages`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({
      message: input.message,
      resumeMode: input.resumeMode ?? "continue",
      questionId: input.questionId,
      idempotencyKey: input.idempotencyKey
    })
  });
}

export async function decideV2Action(apiKey: string, actionId: string, input: { decision: "approved" | "rejected"; reason?: string | null; idempotencyKey?: string }) {
  return fetchJson<{ action: V2Action; job?: Record<string, unknown> | null }>(`/api/v2/actions/${encodeURIComponent(actionId)}/decisions`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function uploadV2Files(apiKey: string, workspaceId: string, files: File[], onProgress: (progress: number) => void) {
  const uploads: V2Upload[] = [];
  onProgress(0);
  for (let index = 0; index < files.length; index += 1) {
    const upload = await uploadV2File(apiKey, workspaceId, files[index], (fileProgress) => {
      onProgress(Math.round(((index + fileProgress) / files.length) * 100));
    });
    uploads.push(upload);
  }
  onProgress(100);
  return uploads;
}

async function uploadV2File(apiKey: string, workspaceId: string, file: File, onProgress: (progress: number) => void) {
  if (file.size <= CHUNK_BYTES) {
    const form = new FormData();
    form.append("file", file, file.name);
    const result = await fetchJson<{ upload: V2Upload }>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}/uploads`, {
      method: "POST",
      headers: authHeaders(apiKey),
      body: form
    });
    onProgress(1);
    return result.upload;
  }

  const initialized = await fetchJson<{ session: { id: string; received_bytes: number } }>(`/api/v2/workspaces/${encodeURIComponent(workspaceId)}/uploads/init`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ filename: file.name, contentType: file.type || "application/octet-stream", sizeBytes: file.size })
  });
  for (let offset = 0; offset < file.size; offset += CHUNK_BYTES) {
    const next = Math.min(offset + CHUNK_BYTES, file.size);
    await fetchJson(`/api/v2/uploads/${encodeURIComponent(initialized.session.id)}/chunks?offset=${offset}`, {
      method: "POST",
      headers: authHeaders(apiKey),
      body: file.slice(offset, next)
    });
    onProgress(next / file.size);
  }
  const completed = await fetchJson<{ upload: V2Upload }>(`/api/v2/uploads/${encodeURIComponent(initialized.session.id)}/complete`, {
    method: "POST",
    headers: authHeaders(apiKey)
  });
  return completed.upload;
}

export async function downloadV2Artifact(apiKey: string, artifactId: string, filename: string) {
  const response = await fetch(`/api/v2/artifacts/${encodeURIComponent(artifactId)}`, { headers: authHeaders(apiKey) });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }
  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export async function searchV2Cases(apiKey: string, input: { query?: string; includeDisabled?: boolean; limit?: number }) {
  const params = new URLSearchParams();
  params.set("limit", String(input.limit ?? 50));
  params.set("includeDisabled", String(Boolean(input.includeDisabled)));
  if (input.query?.trim()) params.set("query", input.query.trim());
  return fetchJson<{ cases: V2CaseHit[] }>(`/api/v2/cases?${params.toString()}`, { headers: authHeaders(apiKey) });
}

export async function previewV2CaseImport(apiKey: string, input: { content: string; filename?: string | null }) {
  return fetchJson<{ import: V2CaseImport }>("/api/v2/cases/imports/preview", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function confirmV2CaseImport(apiKey: string, importId: string, overrides: V2CaseDraft) {
  return fetchJson<{ import: V2CaseImport; case: V2CaseRecord }>(`/api/v2/cases/imports/${encodeURIComponent(importId)}/confirm`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(overrides)
  });
}

export async function confirmV2RunCase(apiKey: string, runId: string, overrides: V2CaseDraft) {
  return fetchJson<V2CaseRecord>(`/api/v2/runs/${encodeURIComponent(runId)}/case`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(overrides)
  });
}

export async function appendV2CaseImportMessage(apiKey: string, importId: string, message: string) {
  return fetchJson<{ import: V2CaseImport }>(`/api/v2/cases/imports/${encodeURIComponent(importId)}/messages`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ message })
  });
}

export async function updateV2Case(apiKey: string, caseId: string, updates: V2CaseDraft & { enabled?: boolean }) {
  return fetchJson<V2CaseRecord>(`/api/v2/cases/${encodeURIComponent(caseId)}`, {
    method: "PATCH",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(updates)
  });
}

export async function listV2Tools(apiKey: string) {
  return fetchJson<{ tools: V2ToolDescriptor[] }>("/api/v2/tools", { headers: authHeaders(apiKey) });
}

export async function callV2TaskTool(apiKey: string, runId: string, name: string, args: Record<string, unknown>) {
  return fetchJson<V2McpResponse>(`/api/v2/mcp/task/${encodeURIComponent(runId)}`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: `webui-${Date.now()}`,
      method: "tools/call",
      params: { name, arguments: args }
    })
  });
}

export async function downloadV2ToolsZip(apiKey: string) {
  await downloadV2File(apiKey, "/api/v2/exports/tools.zip", "logagent-v2-tools.zip");
}

export async function listV2Skills(apiKey: string) {
  return fetchJson<{ skills: V2SkillSummary[] }>("/api/v2/skills", { headers: authHeaders(apiKey) });
}

export async function getV2Skill(apiKey: string, skillId: string) {
  return fetchJson<V2SkillSummary>(`/api/v2/skills/${encodeURIComponent(skillId)}`, { headers: authHeaders(apiKey) });
}

export async function importV2Skill(apiKey: string, input: { skillId: string; name: string; description: string; markdown: string; filename?: string | null }) {
  return fetchJson<V2SkillSummary>("/api/v2/skills/imports", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function previewV2SystemContext(apiKey: string, skillIds: string[]) {
  return fetchJson<V2SystemContextPreview>("/api/v2/skills/preview", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ skillIds })
  });
}

export async function listV2MetadataInstances(apiKey: string) {
  return fetchJson<{ instances: V2MetadataInstanceSummary[] }>("/api/v2/metadata/instances", { headers: authHeaders(apiKey) });
}

export async function downloadV2SkillsZip(apiKey: string) {
  await downloadV2File(apiKey, "/api/v2/exports/skills.zip", "logagent-v2-skills.zip");
}

export async function listV2MetadataImports(apiKey: string) {
  return fetchJson<{ imports: V2MetadataImport[] }>("/api/v2/metadata/imports", { headers: authHeaders(apiKey) });
}

export async function getV2MetadataSnapshot(apiKey: string, instanceId: string) {
  return fetchJson<Record<string, unknown>>(`/api/v2/metadata/instances/${encodeURIComponent(instanceId)}/snapshot`, { headers: authHeaders(apiKey) });
}

export async function refreshV2MetadataInstance(apiKey: string, instanceId: string) {
  return fetchJson<{ instance: V2MetadataInstanceSummary; snapshot: Record<string, unknown> }>(`/api/v2/metadata/instances/${encodeURIComponent(instanceId)}/refresh`, {
    method: "POST",
    headers: authHeaders(apiKey)
  });
}

export async function deleteV2MetadataInstance(apiKey: string, instanceId: string) {
  return fetchJson<{ deleted: boolean; instanceId: string }>(`/api/v2/metadata/instances/${encodeURIComponent(instanceId)}`, {
    method: "DELETE",
    headers: authHeaders(apiKey)
  });
}

export async function previewV2MetadataImport(apiKey: string, input: { instanceId: string; templateType: string; content: string; remark?: string | null }) {
  return fetchJson<V2MetadataImportResponse>("/api/v2/metadata/imports/preview", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function previewV2MetadataFetchImport(apiKey: string, input: { instanceId: string; templateType: string; url: string; remark?: string | null }) {
  return fetchJson<V2MetadataImportResponse>("/api/v2/metadata/imports/fetch/preview", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function confirmV2MetadataImport(apiKey: string, importId: string) {
  return fetchJson<V2MetadataImportResponse & { instance: V2MetadataInstanceSummary }>(`/api/v2/metadata/imports/${encodeURIComponent(importId)}/confirm`, {
    method: "POST",
    headers: authHeaders(apiKey)
  });
}

export async function importV2Metadata(apiKey: string, input: { instanceId: string; templateType: string; content: string; remark?: string | null }) {
  return fetchJson<{ instance: V2MetadataInstanceSummary; snapshot: Record<string, unknown> }>("/api/v2/metadata/imports", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function importV2MetadataFromUrl(apiKey: string, input: { instanceId: string; templateType: string; url: string; remark?: string | null }) {
  return fetchJson<{ instance: V2MetadataInstanceSummary; snapshot: Record<string, unknown> }>("/api/v2/metadata/imports/fetch", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function listV2FetchEndpoints(apiKey: string) {
  return fetchJson<{ enabled: boolean; allowedHosts: string[]; endpoints: V2FetchEndpoint[] }>("/api/v2/fetch/endpoints", { headers: authHeaders(apiKey) });
}

export async function previewV2FetchCurl(apiKey: string, curl: string) {
  return fetchJson<V2FetchPreview>("/api/v2/fetch/imports/preview", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ curl })
  });
}

export async function importV2FetchCurl(apiKey: string, input: { curl: string; name?: string | null; enabled?: boolean }) {
  return fetchJson<V2FetchEndpoint>("/api/v2/fetch/imports", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function updateV2FetchEndpoint(apiKey: string, endpointId: string, updates: Partial<Pick<V2FetchEndpoint, "name" | "method" | "url" | "headers" | "enabled">> & { body?: string | null }) {
  return fetchJson<V2FetchEndpoint>(`/api/v2/fetch/endpoints/${encodeURIComponent(endpointId)}`, {
    method: "PATCH",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(updates)
  });
}

export async function deleteV2FetchEndpoint(apiKey: string, endpointId: string) {
  return fetchJson<{ deleted: boolean; endpointId: string }>(`/api/v2/fetch/endpoints/${encodeURIComponent(endpointId)}`, {
    method: "DELETE",
    headers: authHeaders(apiKey)
  });
}

export async function runV2FetchEndpoint(apiKey: string, runId: string, endpointId: string, input: V2FetchRunOverrides = {}) {
  return fetchJson<V2FetchRunResult>(`/api/v2/runs/${encodeURIComponent(runId)}/fetch/${encodeURIComponent(endpointId)}`, {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function getV2LlmDebug(apiKey: string) {
  return fetchJson<{ llmOutputLogging: boolean }>("/api/v2/debug/llm", { headers: authHeaders(apiKey) });
}

export async function setV2LlmDebug(apiKey: string, enabled: boolean) {
  return fetchJson<{ llmOutputLogging: boolean }>("/api/v2/debug/llm", {
    method: "PUT",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ llmOutputLogging: enabled })
  });
}

export async function getV2LlmSettings(apiKey: string) {
  return fetchJson<{ llm: V2LlmSummary }>("/api/v2/settings/llm", { headers: authHeaders(apiKey) });
}

export async function testV2LlmModels(apiKey: string) {
  return fetchJson<V2LlmTestResponse<V2LlmModelsResult>>("/api/v2/settings/llm/models", { headers: authHeaders(apiKey) });
}

export async function testV2LlmChat(apiKey: string, message: string) {
  return fetchJson<V2LlmTestResponse<V2LlmChatResult>>("/api/v2/settings/llm/chat", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ message })
  });
}

export async function getV2AgentBackends(apiKey: string) {
  return fetchJson<{ agentBackends: V2AgentBackendsSummary }>("/api/v2/settings/agent-backends", { headers: authHeaders(apiKey) });
}

export async function testV2AgentBackend(apiKey: string, backendId: string) {
  return fetchJson<V2LlmTestResponse<V2AgentBackendDiagnosticResult>>(`/api/v2/settings/agent-backends/${encodeURIComponent(backendId)}/test`, {
    method: "POST",
    headers: authHeaders(apiKey)
  });
}

export async function getV2DomainAdapters(apiKey: string) {
  return fetchJson<{ domainAdapters: V2DomainAdapterSummary[] }>("/api/v2/settings/domain-adapters", { headers: authHeaders(apiKey) });
}

export async function listV2Executors(apiKey: string) {
  return fetchJson<{ executors: V2RemoteExecutorRecord[] }>("/api/v2/executors", { headers: authHeaders(apiKey) });
}

export async function createV2Executor(apiKey: string, input: { name: string; host: string; port: number; user: string; tags: string[]; notes?: string | null; enabled: boolean }) {
  return fetchJson<V2RemoteExecutorRecord>("/api/v2/executors", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function updateV2Executor(apiKey: string, executorId: string, updates: Partial<{ name: string; host: string; port: number; user: string; tags: string[]; notes: string | null; enabled: boolean }>) {
  return fetchJson<V2RemoteExecutorRecord>(`/api/v2/executors/${encodeURIComponent(executorId)}`, {
    method: "PATCH",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(updates)
  });
}

export async function disableV2Executor(apiKey: string, executorId: string) {
  return fetchJson<V2RemoteExecutorRecord>(`/api/v2/executors/${encodeURIComponent(executorId)}`, {
    method: "DELETE",
    headers: authHeaders(apiKey)
  });
}

export async function listV2ExecutorCommandTemplates(apiKey: string) {
  return fetchJson<{ enabled: boolean; commands: V2RemoteCommandTemplate[] }>("/api/v2/executor-command-templates", { headers: authHeaders(apiKey) });
}

export async function listV2ExecutorRuns(apiKey: string, input: { executorId?: string; limit?: number }) {
  const params = new URLSearchParams();
  params.set("limit", String(input.limit ?? 50));
  if (input.executorId) params.set("executorId", input.executorId);
  return fetchJson<{ runs: V2RemoteRunSummary[] }>(`/api/v2/executor-runs?${params.toString()}`, { headers: authHeaders(apiKey) });
}

export async function createV2ExecutorRun(apiKey: string, input: { executorId: string; commandId: string; idempotencyKey?: string | null }) {
  return fetchJson<V2RemoteRunSummary>("/api/v2/executor-runs", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(input)
  });
}

export async function getV2ExecutorRun(apiKey: string, runId: string) {
  return fetchJson<V2RemoteRunRecord>(`/api/v2/executor-runs/${encodeURIComponent(runId)}`, { headers: authHeaders(apiKey) });
}

export async function getV2ExecutorRunResult(apiKey: string, runId: string) {
  return fetchJson<V2RemoteRunResult>(`/api/v2/executor-runs/${encodeURIComponent(runId)}/result`, { headers: authHeaders(apiKey) });
}

async function downloadV2File(apiKey: string, path: string, filename: string) {
  const response = await fetch(path, { headers: authHeaders(apiKey) });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }
  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
