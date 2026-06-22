export function startPolling(callback: () => void, intervalMs: number) {
  const timer = window.setInterval(callback, intervalMs);
  return () => window.clearInterval(timer);
}
