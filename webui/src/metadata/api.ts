import type { MetadataSnapshotResponse } from "./types";

export async function fetchSnapshot(url: string, apiKey: string) {
  return fetchJson<MetadataSnapshotResponse>("/api/metadata/snapshots/fetch", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ url, templateType: "opengemini", filename: "opengemini-getdata.json" })
  });
}

export async function fetchStoredCluster(clusterId: string, apiKey: string): Promise<MetadataSnapshotResponse> {
  const [clusterResponse, nodesResponse] = await Promise.all([
    fetchJson<{ cluster: MetadataSnapshotResponse["cluster"] }>(`/api/metadata/clusters/${encodeURIComponent(clusterId)}`, {
      headers: authHeaders(apiKey)
    }),
    fetchJson<{ nodes: MetadataSnapshotResponse["nodes"] }>(`/api/metadata/clusters/${encodeURIComponent(clusterId)}/nodes`, {
      headers: authHeaders(apiKey)
    })
  ]);
  return { cluster: clusterResponse.cluster, nodes: nodesResponse.nodes };
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

export async function previewImport(url: string, apiKey: string) {
  return fetchJson<ImportPreview>("/api/metadata/imports/fetch", {
    method: "POST",
    headers: jsonHeaders(apiKey),
    body: JSON.stringify({ url, templateType: "opengemini", filename: "opengemini-getdata.json" })
  });
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
