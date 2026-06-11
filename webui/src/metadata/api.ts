import type { MetadataInstanceSummary, MetadataSnapshotResponse } from "./types";

export async function fetchSnapshot(url: string, instanceId: string, remark: string, apiKey: string) {
  return fetchJson<MetadataSnapshotResponse>("/api/metadata/snapshots/fetch", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(metadataFetchBody(url, instanceId, remark))
  });
}

export async function fetchImportedInstances(apiKey: string) {
  return fetchJson<{ instances: MetadataInstanceSummary[] }>("/api/metadata/instances", {
    headers: authHeaders(apiKey)
  });
}

export async function fetchStoredInstance(instanceId: string, apiKey: string): Promise<MetadataSnapshotResponse> {
  return fetchJson<MetadataSnapshotResponse>(`/api/metadata/instances/${encodeURIComponent(instanceId)}/snapshot`, {
    headers: authHeaders(apiKey)
  });
}

export type ImportPreview = {
  importId: string;
  summary: {
    instances: number;
    clusters: number;
    nodes: number;
    databases: number;
    partitionViews: number;
    warnings: number;
    errors: number;
  };
};

export type MetadataTemplateImport = {
  templateType: "json";
  filename?: string;
  instanceId?: string;
  remark?: string;
  content: string;
};

export async function previewImport(url: string, instanceId: string, remark: string, apiKey: string) {
  return fetchJson<ImportPreview>("/api/metadata/imports/fetch", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify(metadataFetchBody(url, instanceId, remark))
  });
}

export async function previewTemplateImport(request: MetadataTemplateImport, apiKey: string) {
  const trimmedRemark = request.remark?.trim();
  const trimmedInstanceId = request.instanceId?.trim();
  return fetchJson<ImportPreview>("/api/metadata/imports", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({
      templateType: request.templateType,
      filename: request.filename,
      content: request.content,
      ...(trimmedInstanceId ? { instanceId: trimmedInstanceId } : {}),
      ...(trimmedRemark ? { remark: trimmedRemark } : {})
    })
  });
}

function metadataFetchBody(url: string, instanceId: string, remark: string) {
  const trimmedRemark = remark.trim();
  return {
    url,
    instanceId,
    ...(trimmedRemark ? { remark: trimmedRemark } : {}),
    templateType: "opengemini",
    filename: "opengemini-getdata.json"
  };
}

export async function confirmImport(importId: string, apiKey: string) {
  return fetchJson<{ applied: boolean }>(`/api/metadata/imports/${encodeURIComponent(importId)}/confirm`, {
    method: "POST",
    headers: authHeaders(apiKey)
  });
}

export async function fetchJson<T>(url: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(url, options);
  const text = await response.text();
  const body = text ? JSON.parse(text) : {};
  if (!response.ok) throw new Error(body.error || `HTTP ${response.status}`);
  return body as T;
}

export function authHeaders(apiKey: string): HeadersInit {
  return { Authorization: `Bearer ${apiKey.trim()}` };
}

export function jsonHeaders(apiKey: string): HeadersInit {
  return { ...authHeaders(apiKey), "Content-Type": "application/json" };
}
