import { Download } from "lucide-react";
import { useState } from "react";
import { Button, Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui";
import { authHeaders } from "./metadata/api";

type Props = { apiKey: string };

export function SettingsView({ apiKey }: Props) {
  const [status, setStatus] = useState("Ready");

  async function downloadExport(path: string, filename: string) {
    if (!apiKey.trim()) {
      setStatus("请先填写 API Key");
      return;
    }
    setStatus(`Downloading ${filename}...`);
    try {
      const response = await fetch(path, { headers: authHeaders(apiKey) });
      if (!response.ok) {
        const text = await response.text();
        throw new Error(`HTTP ${response.status}: ${text}`);
      }
      const blob = await response.blob();
      const objectUrl = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = objectUrl;
      link.download = filename;
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(objectUrl);
      setStatus(`${filename} downloaded`);
    } catch (reason) {
      setStatus(formatError(reason));
    }
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <CardTitle>Export bundles</CardTitle>
          <CardDescription>MCP 连接配置已统一移到 MCP 页面；这里仅保留 Skills/Tools 导出。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 lg:grid-cols-3">
            <SettingMetric label="MCP config" value="See MCP page" />
            <SettingMetric label="Skills export" value="/api/exports/skills.zip" />
            <SettingMetric label="Tools export" value="/api/exports/tools.zip" />
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => void downloadExport("/api/exports/skills.zip", "skills.zip")}>
              <Download className="mr-2 h-4 w-4" />
              Skills ZIP
            </Button>
            <Button variant="outline" onClick={() => void downloadExport("/api/exports/tools.zip", "tools.zip")}>
              <Download className="mr-2 h-4 w-4" />
              Tools ZIP
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">{status}</p>
        </CardContent>
      </Card>
    </div>
  );
}

function SettingMetric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-lg border border-border bg-white p-3">
      <p className="text-xs uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="mt-1 break-all text-sm font-semibold">{value}</p>
    </div>
  );
}

function formatError(reason: unknown) {
  return reason instanceof Error ? reason.stack ?? reason.message : String(reason);
}
