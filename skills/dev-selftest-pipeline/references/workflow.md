# Dev Self-Test Workflow

## Table Of Contents

- Boundary
- Prerequisites
- Client-Side Code Phase
- MCP Tool Order
- Queued Calls And IDs
- Step Parameters
- Result Shapes
- Failure Handling
- Removed Paths

## Boundary

LocalToolHub Server provides MCP tools/resources and controlled execution only. It does not own a
workflow engine, local skill registry, server-hosted skill content, or runbook compatibility API.
Claude Code owns the workflow: edit locally, commit, push, then drive Server tools through MCP.

## Prerequisites

- `mcp.enabled: true` on the Server.
- `dev_selftest.enabled: true` on the Server.
- At least one configured build profile, Docker cluster profile, and test suite.
- Absolute allowlisted command, binary, compose, repository, and ref values.
- Docker access for the Server process.
- The MCP client can call `tools/list` and sees:
  - `logagent.dev_selftest.sync_workspace`
  - `logagent.dev_selftest.build`
  - `logagent.dev_selftest.deploy`
  - `logagent.dev_selftest.run_tests`
  - `logagent.dev_selftest.report`
  - `logagent.runs.get`
  - `logagent.runs.result`

## Client-Side Code Phase

Before `sync_workspace`, Claude Code should finish local source work:

```bash
git status --short
# run focused checks when practical
git add <changed-files>
git commit -m "<type>: <summary>"
git push
```

Do not continue to remote self-test if the relevant branch/ref has not been pushed. The Server
will clone or pull from git; it will not receive local uncommitted changes.

## MCP Tool Order

Run the steps in this order:

| Step | Tool | Use |
|------|------|-----|
| 1 | `logagent.dev_selftest.sync_workspace` | Create or update the persistent dev_selftest workspace from an allowlisted git repo/ref. |
| 2 | `logagent.dev_selftest.build` | Run a configured build profile and collect declared artifacts. |
| 3 | `logagent.dev_selftest.deploy` | Start the configured Docker cluster and run its health check. |
| 4 | `logagent.dev_selftest.run_tests` | Run a configured test suite, usually through inline Docker. |
| 5 | `logagent.dev_selftest.report` | Generate `report.md` and `report.json` from recorded step evidence. |

Recommended flow:

1. Call `sync_workspace` synchronously and capture its returned `devselftest_*` id.
2. Use `runMode:"queued"` for slow `build`, `deploy`, or `run_tests` calls.
3. Poll queued calls with `logagent.runs.get`.
4. Read final queued output with `logagent.runs.result`.
5. Continue passing the original `devselftest_*` id to subsequent dev_selftest tools.

## Queued Calls And IDs

There are two different id families:

| Prefix | Meaning | Where to use |
|--------|---------|--------------|
| `devselftest_*` | Persistent dev_selftest workspace/run id. | Pass to `build`, `deploy`, `run_tests`, and `report`. |
| `task_*` | Queued Tool Runner execution id. | Poll with `logagent.runs.get` and read with `logagent.runs.result` only. |

If `runMode:"queued"` returns:

```json
{
  "runId": "task_...",
  "status": "QUEUED"
}
```

that `runId` is a polling id, not the dev_selftest workspace id. After the task succeeds,
`logagent.runs.result` returns the underlying tool result, which contains the `devselftest_*`
workspace id again.

## Step Parameters

### sync_workspace

Use an allowlisted git repo/ref. Prefer synchronous mode so the workspace id is immediately clear.

```json
{
  "name": "logagent.dev_selftest.sync_workspace",
  "arguments": {
    "label": "pr-123",
    "gitRepo": "https://github.com/openGemini/openGemini.git",
    "gitRef": "main"
  }
}
```

Result:

```json
{
  "runId": "devselftest_...",
  "sourceRef": "git:https://github.com/openGemini/openGemini.git@main",
  "status": "OK"
}
```

To pull a newer pushed commit into the same workspace, call `sync_workspace` again with the same
`devselftest_*` in `runId`.

### build

```json
{
  "name": "logagent.dev_selftest.build",
  "arguments": {
    "runId": "devselftest_...",
    "buildProfile": "opengemini",
    "runMode": "queued"
  }
}
```

### deploy

```json
{
  "name": "logagent.dev_selftest.deploy",
  "arguments": {
    "runId": "devselftest_...",
    "profile": "opengemini_cluster",
    "runMode": "queued"
  }
}
```

### run_tests

```json
{
  "name": "logagent.dev_selftest.run_tests",
  "arguments": {
    "runId": "devselftest_...",
    "testSuite": "opengemini_smoke",
    "runMode": "queued"
  }
}
```

### report

```json
{
  "name": "logagent.dev_selftest.report",
  "arguments": {
    "runId": "devselftest_..."
  }
}
```

## Result Shapes

### progress.json

```json
{
  "schemaVersion": 1,
  "runId": "devselftest_...",
  "steps": [
    { "step": "sync_workspace", "status": "OK", "durationMs": 12, "error": null },
    { "step": "build", "status": "OK", "durationMs": 345 },
    { "step": "deploy", "status": "OK" },
    { "step": "run_tests", "status": "OK" },
    { "step": "report", "status": "OK" }
  ]
}
```

### report

```json
{
  "runId": "devselftest_...",
  "status": "SUCCEEDED",
  "reportPath": "report.md",
  "failedSteps": []
}
```

## Failure Handling

- If local changes are not pushed, stop and push before remote self-test.
- If a queued `task_*` fails, call `logagent.runs.result` for stdout/stderr artifact refs and the
  structured error.
- A failed dev_selftest step records evidence in `progress.json` and marks the dev_selftest run
  failed.
- `report` remains callable after failures and should be used to summarize failed steps.
- Docker health check failures do not trigger automatic rollback. Clean up the compose project
  manually when needed:

```bash
docker compose -p devselftest_<runId>_<profile> down
```

## Removed Paths

Do not use or describe these as current behavior:

- Server-side workflow API.
- Server-loaded skills or skill registry.
- Legacy `docs/runbooks/dev-selftest-pipeline/` entry.
- Source tarball upload for dev_selftest.
- SSH/SCP executor or managed executor records.
- `/api/executors` or `/api/executor-runs`.
- `suite.executor`, Huawei package sync, GeminiDB create instance, or custom agent loops.
