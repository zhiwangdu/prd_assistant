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
