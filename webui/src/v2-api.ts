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
  event_type: string;
  payload: Record<string, unknown>;
  created_at: string;
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
  artifacts: V2RunArtifacts;
  resources: Record<string, unknown | null>;
  result: V2RunResult | null;
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
