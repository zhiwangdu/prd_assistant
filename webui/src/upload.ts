import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";

const CHUNK_BYTES = 512 * 1024;

export type UploadResponse = { uploadId: string; filename: string; size: number };

export async function uploadFile(file: File, apiKey: string, onProgress: (value: number) => void) {
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
