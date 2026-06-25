import { AlertCircle, Cable, GitBranch, KeyRound, RefreshCw, Save } from "lucide-react";
import { useEffect, useState } from "react";
import { authHeaders, fetchJson, jsonHeaders } from "./metadata/api";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input } from "./components/ui";

type Props = { apiKey: string };

type DevSelftestGitRepo = {
  url: string;
  refs: string[];
};

type DevSelftestConfigSummary = {
  schemaVersion: number;
  devSelftestEnabled: boolean;
  gitEnabled: boolean;
  gitRepos: DevSelftestGitRepo[];
  defaultGitRepo?: string | null;
  defaultGitRef?: string | null;
  buildProfiles: string[];
  dockerProfiles: string[];
  testSuites: string[];
};

type AllowlistUpdateResponse = {
  updated: boolean;
  summary: DevSelftestConfigSummary;
};

export function SettingsView({ apiKey }: Props) {
  const [summary, setSummary] = useState<DevSelftestConfigSummary | null>(null);
  const [repoUrl, setRepoUrl] = useState("");
  const [gitRef, setGitRef] = useState("");
  const [loadingAllowlist, setLoadingAllowlist] = useState(false);
  const [savingAllowlist, setSavingAllowlist] = useState(false);
  const [allowlistError, setAllowlistError] = useState<string | null>(null);
  const [allowlistMessage, setAllowlistMessage] = useState<string | null>(null);

  const loadAllowlist = async () => {
    if (!apiKey.trim()) {
      setSummary(null);
      return;
    }
    setLoadingAllowlist(true);
    setAllowlistError(null);
    try {
      const next = await fetchJson<DevSelftestConfigSummary>("/api/settings/dev-selftest/git-allowlist", {
        headers: authHeaders(apiKey)
      });
      setSummary(next);
      setRepoUrl(next.defaultGitRepo ?? "");
      setGitRef(next.defaultGitRef ?? "");
    } catch (err) {
      setAllowlistError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingAllowlist(false);
    }
  };

  useEffect(() => {
    void loadAllowlist();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apiKey]);

  const saveAllowlist = async () => {
    if (!apiKey.trim()) {
      setAllowlistError("请先填写 API Key。");
      return;
    }
    if (!repoUrl.trim() || !gitRef.trim()) {
      setAllowlistError("Repo URL 和 Git ref 都不能为空。");
      return;
    }
    setSavingAllowlist(true);
    setAllowlistError(null);
    setAllowlistMessage(null);
    try {
      const response = await fetchJson<AllowlistUpdateResponse>("/api/settings/dev-selftest/git-allowlist", {
        method: "PUT",
        headers: jsonHeaders(apiKey),
        body: JSON.stringify({
          repoUrl: repoUrl.trim(),
          gitRef: gitRef.trim(),
          setDefault: true,
          confirmedUserConsent: true,
          reason: "WebUI Settings update"
        })
      });
      setSummary(response.summary);
      setRepoUrl(response.summary.defaultGitRepo ?? repoUrl.trim());
      setGitRef(response.summary.defaultGitRef ?? gitRef.trim());
      setAllowlistMessage(response.updated ? "已保存并设为默认。" : "配置已验证，无需变更。");
    } catch (err) {
      setAllowlistError(err instanceof Error ? err.message : String(err));
    } finally {
      setSavingAllowlist(false);
    }
  };

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

      <Card>
        <CardHeader>
          <CardTitle>Dev Self-Test Git Allowlist</CardTitle>
          <CardDescription>查看 MCP 暴露的 repo/ref/profile，并追加新的 repo/ref 作为默认推荐项。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {!apiKey.trim() ? (
            <div className="flex items-center gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800">
              <AlertCircle className="h-4 w-4" />
              <span>请先填写 API Key 后再读取或保存 allowlist。</span>
            </div>
          ) : (
            <>
              <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                <GitBranch className="h-4 w-4" />
                <span>默认：</span>
                <Badge variant="outline">{summary?.defaultGitRepo ?? "未配置 repo"}</Badge>
                <Badge variant="secondary">{summary?.defaultGitRef ?? "未配置 ref"}</Badge>
                {summary && !summary.devSelftestEnabled ? <Badge variant="warning">dev_selftest disabled</Badge> : null}
                {summary && !summary.gitEnabled ? <Badge variant="warning">git disabled</Badge> : null}
              </div>

              <div className="grid gap-3 lg:grid-cols-[1fr_220px_auto_auto] lg:items-end">
                <label className="space-y-1 text-sm">
                  <span className="font-medium">Repo URL</span>
                  <Input value={repoUrl} onChange={(event) => setRepoUrl(event.target.value)} placeholder="ssh://git@github.com/org/repo.git" />
                </label>
                <label className="space-y-1 text-sm">
                  <span className="font-medium">Git ref</span>
                  <Input value={gitRef} onChange={(event) => setGitRef(event.target.value)} placeholder="feature/branch" />
                </label>
                <Button variant="outline" onClick={loadAllowlist} disabled={loadingAllowlist || savingAllowlist}>
                  <RefreshCw className="mr-2 h-4 w-4" />刷新
                </Button>
                <Button onClick={saveAllowlist} disabled={loadingAllowlist || savingAllowlist}>
                  <Save className="mr-2 h-4 w-4" />保存
                </Button>
              </div>

              {allowlistError ? <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">{allowlistError}</div> : null}
              {allowlistMessage ? <div className="rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-700">{allowlistMessage}</div> : null}

              <div className="grid gap-3 lg:grid-cols-3">
                <ProfileList title="Build profiles" items={summary?.buildProfiles ?? []} />
                <ProfileList title="Docker profiles" items={summary?.dockerProfiles ?? []} />
                <ProfileList title="Test suites" items={summary?.testSuites ?? []} />
              </div>

              <div className="space-y-2">
                <div className="text-sm font-medium">Allowlisted repo/ref</div>
                {summary?.gitRepos.length ? (
                  <div className="space-y-2">
                    {summary.gitRepos.map((repo) => (
                      <div key={repo.url} className="rounded-md border border-border px-3 py-2">
                        <div className="break-all text-sm font-medium">{repo.url}</div>
                        <div className="mt-2 flex flex-wrap gap-1.5">
                          {repo.refs.map((ref) => <Badge key={`${repo.url}:${ref}`} variant="secondary">{ref}</Badge>)}
                        </div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="rounded-md border border-dashed border-border px-3 py-5 text-center text-sm text-muted-foreground">
                    暂无 allowlisted repo/ref。
                  </div>
                )}
              </div>
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function ProfileList({ title, items }: { title: string; items: string[] }) {
  return (
    <div className="rounded-md border border-border px-3 py-2">
      <div className="text-xs font-medium uppercase text-muted-foreground">{title}</div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {items.length ? items.map((item) => <Badge key={item} variant="outline">{item}</Badge>) : <span className="text-sm text-muted-foreground">未配置</span>}
      </div>
    </div>
  );
}
