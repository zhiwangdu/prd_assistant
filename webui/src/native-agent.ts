const NATIVE_AGENT_BASE_URL = "http://127.0.0.1:17321";

export async function setNativeCurrentSession(sessionId: string) {
  const response = await fetch(`${NATIVE_AGENT_BASE_URL}/workspace/current`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ sessionId })
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
}
