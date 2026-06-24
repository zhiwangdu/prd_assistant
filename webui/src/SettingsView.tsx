import { Cable, KeyRound } from "lucide-react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./components/ui";

type Props = { apiKey: string };

export function SettingsView({ apiKey }: Props) {
  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <CardTitle>API Key</CardTitle>
          <CardDescription>在顶部输入框填写 API Key，所有请求会带 Authorization: Bearer 头。</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <KeyRound className="h-4 w-4" />
            <span>{apiKey.trim() ? `已设置（${apiKey.trim().slice(0, 4)}…）` : "未设置"}</span>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>MCP 接入</CardTitle>
          <CardDescription>外部 MCP 客户端（Claude Code / Codex / Cursor / OpenCode）接入方式见 MCP 页面。</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Cable className="h-4 w-4" />
            <span>POST /api/mcp（streamable-http）或 <code>logagent-server mcp-serve</code>（stdio）</span>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Skills</CardTitle>
          <CardDescription>诊断 runbook 不再由 server 托管；作为本地 Claude Code skill 使用，调用 server 的 MCP 工具。</CardDescription>
        </CardHeader>
      </Card>
    </div>
  );
}
