# Runbooks

The server no longer hosts a skill registry. This directory keeps legacy diagnostic
runbooks as authoring references for log-analysis skills only. User-installable
skills are distributed from top-level [`skills/`](../../skills/).

The `logagent.json` beside each `SKILL.md` is the legacy server-side skill
manifest (kept for reference only — the server does not read it anymore).

New workflow content must not be added here. Put client-installed skills under
`skills/`, and expose runtime behavior only through MCP tools/resources.

## Runbooks

| Runbook | Module | What it drives |
|---------|--------|----------------|
| `influxql-batch-analysis/` | 日志分析 | `logagent.batch_influxql_analysis` + `influxql_analyzer` |
| `influxql-analysis/` | 日志分析 | `influxql_analyzer` |
| `opengemini-diagnosis/` | 日志分析 | `opengemini_storage_analyzer` |
| `pprof-diagnosis/` | 日志分析 | `pprof_analyzer` |
