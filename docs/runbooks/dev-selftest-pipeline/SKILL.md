---
name: Dev Self-Test Pipeline
description: Runbook for the converged dev_selftest workflow: sync source, build, deploy a local docker cluster, run inline-docker tests, poll, and report through MCP.
---

Use this runbook when a remote MCP client needs to drive the Linux LocalToolHub
dev_selftest loop. Every step is a catalog tool sharing the same execution boundary;
the server never accepts free-form shell or runs an agent loop.

## Prerequisites

- `dev_selftest.enabled: true` in the server config.
- At least one configured build profile, docker cluster profile, and test suite.
- Absolute, allowlisted command/binary/compose paths, plus an allowlisted git repo/ref.
- Docker access for the server process.
- `mcp.enabled: true`; remote clients normally connect to `POST /api/mcp` over an SSH tunnel.
- The Windows-side client commits and pushes first; ToolHub only clones or pulls from git.

## State Model

A dev_selftest run is a persistent workspace:

```text
data/dev_selftest/runs/{runId}/
  source/
  artifacts/
  logs/
  tool_results/
  progress.json
  report.md
  report.json
```

`sync_workspace` creates the run and returns `runId`. Carry that `runId` to every later
tool call. Each step appends or replaces its entry in `progress.json` and writes a step
`result.json`.

## Pipeline

| Step | Tool | Key params |
|---|---|---|
| 1. sync | `logagent.dev_selftest.sync_workspace` | `{label, gitRepo, gitRef}`. Git source must match config allowlist. New runs clone; existing run workspaces pull fast-forward updates. |
| 2. build | `logagent.dev_selftest.build` | `{runId, buildProfile}`. Runs the configured build command and collects `artifact_globs`. |
| 3. deploy | `logagent.dev_selftest.deploy` | `{runId, profile}`. Runs `docker compose -p <project> -f <compose> up -d` and the declared health check. |
| 4. run tests | `logagent.dev_selftest.run_tests` | `{runId, testSuite}`. A suite with `docker` runs through the inline Docker runner; otherwise it uses the local stub `argv`. |
| 5. poll | `logagent.runs.get` | `{runId}` for a queued call. Side-effect-free. |
| 6. result | `logagent.runs.result` | `{runId}` for the structured result of a succeeded queued call. Side-effect-free. |
| 7. report | `logagent.dev_selftest.report` | `{runId}`. Writes `report.md` and `report.json`. |

## Queued Calls

`tools/call` accepts `runMode: "sync" | "queued"` (default `sync`). Use `queued` for slow
builds or tests. A queued call returns `{runId, status:"QUEUED"}` immediately; poll with
`logagent.runs.get` until `SUCCEEDED`, then call `logagent.runs.result`.

## Inline Docker Tests

For a suite with `docker`, `run_tests` builds a Docker target from config and executes:

```text
docker run --rm --network <network> [--workdir <dir>] [-v host:container:mode] \
  [-e KEY=value] <image> <argv>
```

`argv` and timeout come from `suite.command` referencing `remote_execution.commands`, or
from `suite.argv` when no command template is used. System env
(`DEVSELFTEST_HOST`, `DEVSELFTEST_PORT`, `DEVSELFTEST_RUN_DIR`, `DEVSELFTEST_SOURCE_DIR`,
`DEVSELFTEST_ARTIFACTS_DIR`, `DEVSELFTEST_PROJECT_NAME`) is injected with final priority.

## Failure Handling

A failed step marks the dev_selftest run `FAILED` and records the error in `progress.json`.
Later steps can still run, and `report` will include `failedSteps`.

## Removed Paths

Do not use or document these as current behavior: SSH/SCP executor, managed executor
records, `/api/executors`, `/api/executor-runs`, `suite.executor`, `ssh_binary_replace`,
Huawei package sync, GeminiDB create instance, or server-loaded skills.

## Notes

- Tool params may be sent either nested under `params` or as top-level MCP arguments.
- The default openGemini demo lives in `deploy/devselftest/opengemini/`.
- The runbook is a local authoring reference for MCP clients; the server no longer loads
  `skills/` or runbooks.
