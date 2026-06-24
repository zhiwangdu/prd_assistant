# Runbooks (local Claude Code skill references)

The server no longer hosts a skill registry. Diagnostic runbooks live here as
**authoring references** for local Claude Code skills: copy a `SKILL.md` into
your local Claude skills directory and adapt it. The runbooks tell Claude which
server MCP tools to call (the server exposes the same tools over
`POST /api/mcp` / `logagent-server mcp-serve`).

The `logagent.json` beside each `SKILL.md` is the legacy server-side skill
manifest (kept for reference only — the server does not read it anymore).

## Runbooks

| Runbook | Module | What it drives |
|---------|--------|----------------|
| `dev-selftest-pipeline/` | dev_selftest | `logagent.dev_selftest.*` (sync → build → deploy → run_tests → report) |
| `influxql-batch-analysis/` | 日志分析 | `logagent.batch_influxql_analysis` + `influxql_analyzer` |
| `influxql-analysis/` | 日志分析 | `influxql_analyzer` |
| `opengemini-diagnosis/` | 日志分析 | `opengemini_storage_analyzer` |
| `pprof-diagnosis/` | 日志分析 | `pprof_analyzer` |
