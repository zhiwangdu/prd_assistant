# Dev Self-Test Workflow

## Table Of Contents

- Boundary
- Prerequisites
- Config Discovery And Allowlist Updates
- Client-Side Code Phase
- Remote-First Build Loop
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
  - `logagent.dev_selftest.allowlist.update`
  - `logagent.dev_selftest.profiles.upsert`
  - `logagent.dev_selftest.sync_workspace`
  - `logagent.dev_selftest.build`
  - `logagent.dev_selftest.deploy`
  - `logagent.dev_selftest.run_tests`
  - `logagent.dev_selftest.report`
  - `logagent.dev_selftest.cleanup`
  - `logagent.dev_selftest.diagnose`
  - `logagent.runs.get`
  - `logagent.runs.result`
- The MCP client can call `resources/read` for `logagent://dev_selftest/config`.

## Config Discovery And Allowlist Updates

Before selecting any repo/ref/profile, read:

```json
{
  "method": "resources/read",
  "params": { "uri": "logagent://dev_selftest/config" }
}
```

Use the returned `gitRepos`, `defaultGitRepo`, `defaultGitRef`, `buildProfiles`,
`dockerProfiles`, `testSuites`, `dockerProfileDetails`, `buildProfileDetails`, and
`testSuiteDetails` as the source of truth. Do not infer values from a local checkout, SSH into the
Server, or read Server config files directly.

If the needed repo/ref is not present:

1. Stop the workflow.
2. Ask the user whether they approve updating the Server dev_selftest git allowlist.
3. Only after explicit approval, call:

```json
{
  "name": "logagent.dev_selftest.allowlist.update",
  "arguments": {
    "repoUrl": "https://github.com/openGemini/openGemini.git",
    "gitRef": "feature/branch",
    "setDefault": true,
    "confirmedUserConsent": true,
    "reason": "User approved dev_selftest for this branch"
  }
}
```

The Server verifies URL/ref safety and `git ls-remote` reachability, writes the config file, then
updates the runtime allowlist. After a successful update, reread `logagent://dev_selftest/config`
and continue using the returned values. Existing `devselftest_*` workspaces are not modified by
the allowlist update.

If the needed Docker build/test profile is absent or wrong:

1. Stop the workflow.
2. Ask the user whether they approve updating Server dev_selftest profiles.
3. Only after explicit approval, call:

```json
{
  "name": "logagent.dev_selftest.profiles.upsert",
  "arguments": {
    "kind": "build",
    "id": "opengemini_ci",
    "image": "registry.local/localtoolhub/opengemini-builder:latest",
    "argv": ["/usr/local/bin/build-selftest"],
    "timeoutSeconds": 1800,
    "network": "host",
    "workdir": "/workspace/source",
    "artifactGlobs": ["build/ts-meta", "build/ts-store", "build/ts-sql"],
    "confirmedUserConsent": true,
    "reason": "User approved Docker build profile for this branch"
  }
}
```

The Server validates the profile id, Docker target, and argv, writes the config file, then updates
the runtime profile registry. After a successful update, reread `logagent://dev_selftest/config`
and continue using the returned profile ids. The upsert does not run build/test, and queued
build/test tasks keep the profile snapshot selected when they were created.

## Client-Side Code Phase

Before `sync_workspace`, Claude Code should finish local source work:

```bash
git status --short
git add <changed-files>
git commit -m "<type>: <summary>"
git push
```

Do not continue to remote self-test if the relevant branch/ref has not been pushed. The Server
will clone or pull from git; it will not receive local uncommitted changes.

Do not run local compile, build, unit-test, integration-test, Docker, or cluster checks by
default. The client may be Windows or otherwise missing the Linux target toolchain. Local checks
are limited to source inspection and git hygiene unless the user explicitly asks for a local
check and the environment is known to match the target. Dependency-management commands that are
part of the requested edit are allowed, but they do not replace the remote build.

## Remote-First Build Loop

Use the MCP Server as the build/test authority:

1. Edit locally.
2. Commit and push.
3. Read `logagent://dev_selftest/config` and confirm the pushed repo/ref is allowlisted.
4. Call `logagent.dev_selftest.sync_workspace` for the pushed repo/ref.
5. Call remote `logagent.dev_selftest.build`.
6. If `build` fails, read `logagent.runs.result`, call `logagent.dev_selftest.diagnose`, then fix
   locally using the returned evidence/category.
7. Commit and push the fix.
8. Call `sync_workspace` again, passing the same `devselftest_*` in `runId` when continuing an
   existing workspace.
9. Retry remote `build`.

Only proceed to `deploy`, `run_tests`, and `report` after remote `build` returns `status:"OK"`.
Do not try to reproduce the build locally unless the user explicitly requests that separate
diagnostic step.

## MCP Tool Order

Run the steps in this order:

| Step | Tool | Use |
|------|------|-----|
| 1 | `logagent.dev_selftest.sync_workspace` | Create or update the persistent dev_selftest workspace from an allowlisted git repo/ref. |
| 2 | `logagent.dev_selftest.build` | Run a configured host or Docker build profile and collect declared artifacts. |
| 3 | `logagent.dev_selftest.deploy` | Start the configured Docker cluster and run its health check. |
| 4 | `logagent.dev_selftest.run_tests` | Run a configured test suite, usually through inline Docker; cloud-instance flows may skip deploy and pass non-secret `testParams`. |
| 5 | `logagent.dev_selftest.report` | Generate `report.md` and `report.json` from recorded step evidence. |
| 6 | `logagent.dev_selftest.cleanup` | Optional after report: run `docker compose down` for the run's configured Docker project while preserving run evidence. |
| Diagnose | `logagent.dev_selftest.diagnose` | Read bounded run evidence and allowlisted read-only Docker probes to classify failures before asking for cleanup or code changes. |

Recommended flow:

1. Call `sync_workspace` immediately after commit/push and capture its returned `devselftest_*` id.
2. Use `runMode:"queued"` for slow `build`, `deploy`, or `run_tests` calls.
3. Poll queued calls with `logagent.runs.get`.
4. Read final queued output with `logagent.runs.result`.
5. If a step result has `status:"FAILED"`, call `logagent.dev_selftest.diagnose` with the original
   `devselftest_*` id before deciding whether to fix source, inspect more evidence, or request
   cleanup.
6. Continue passing the original `devselftest_*` id to subsequent dev_selftest tools.
7. After `report`, call `cleanup` only when the user or workflow wants to release Docker compose resources. Failed runs should usually keep the environment for inspection unless diagnose recommends cleanup and the user agrees.

## Queued Calls And IDs

There are two different id families:

| Prefix | Meaning | Where to use |
|--------|---------|--------------|
| `devselftest_*` | Persistent dev_selftest workspace/run id. | Pass to `build`, `deploy`, `run_tests`, `report`, and optional `cleanup`. |
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
The allowed values come from `logagent://dev_selftest/config`; if the desired ref is absent, use
the allowlist update flow above before calling this tool.

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

For a cloud DB instance created by an external/internal skill, skip `deploy` and pass only
non-secret runtime identifiers through `testParams`. ToolHub injects them into the Docker test
runner as `DEVSELFTEST_PARAM_*`; values are visible in the host-side `docker run --env` argv, so
never pass credentials here.

```json
{
  "name": "logagent.dev_selftest.run_tests",
  "arguments": {
    "runId": "devselftest_...",
    "testSuite": "cloud_opengemini_case",
    "testParams": {
      "caseName": "opengemini_rw_smoke",
      "instanceId": "local-demo",
      "endpoint": "http://127.0.0.1:8086"
    },
    "runMode": "queued"
  }
}
```

Do not use `DevSelftestDeployTarget::Instance` for this path. The external/internal skill owns
cloud instance lifecycle and cleanup; ToolHub only runs the Dockerized test framework and records
evidence.

### report

```json
{
  "name": "logagent.dev_selftest.report",
  "arguments": {
    "runId": "devselftest_..."
  }
}
```

### cleanup

Cleanup is optional and should normally run after `report`. It releases only the Docker compose
resources for the run and keeps `source/`, `artifacts/`, `logs/`, `progress.json`, `report.md`,
and `report.json` for audit.

```json
{
  "name": "logagent.dev_selftest.cleanup",
  "arguments": {
    "runId": "devselftest_..."
  }
}
```

If the run was not deployed yet, pass an explicit configured docker profile:

```json
{
  "name": "logagent.dev_selftest.cleanup",
  "arguments": {
    "runId": "devselftest_...",
    "profile": "opengemini_cluster"
  }
}
```

### diagnose

Use after any failed step. Omit `step` to let the server select the first failed non-cleanup step.
The tool reads only the run workspace evidence and fixed read-only Docker probes derived from the
configured profile; it never cleans up or restarts containers.

```json
{
  "name": "logagent.dev_selftest.diagnose",
  "arguments": {
    "runId": "devselftest_...",
    "taskRunId": "task_...",
    "includeDockerProbes": true
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
    { "step": "report", "status": "OK" },
    { "step": "cleanup", "status": "OK" }
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

### cleanup

```json
{
  "runId": "devselftest_...",
  "profile": "opengemini_cluster",
  "projectName": "devselftest_<runId>_opengemini_cluster",
  "status": "OK",
  "stdoutPath": "logs/cleanup.stdout.txt",
  "stderrPath": "logs/cleanup.stderr.txt"
}
```

### diagnose

```json
{
  "runId": "devselftest_...",
  "status": "OK",
  "diagnosedStep": "deploy",
  "category": "port_conflict",
  "confidence": "high",
  "summary": "deploy failed because Docker reports an exposed port is already allocated.",
  "evidence": [
    { "path": "logs/deploy.stderr.txt", "exists": true, "truncated": false, "text": "..." }
  ],
  "dockerProbes": [
    { "name": "docker_ps_port", "status": "OK", "stdout": "..." }
  ],
  "recommendedActions": [
    {
      "kind": "cleanup",
      "tool": "logagent.dev_selftest.cleanup",
      "arguments": { "runId": "devselftest_...", "profile": "opengemini_cluster" }
    }
  ]
}
```

## Failure Handling

- If local changes are not pushed, stop and push before remote self-test.
- If the desired repo/ref is absent from `logagent://dev_selftest/config`, stop and ask for user
  consent before calling `logagent.dev_selftest.allowlist.update`.
- If local build/test would be useful but the environment is Windows or otherwise not target
  equivalent, skip it and use remote `build` as the feedback loop.
- If remote `build` fails, do not switch to local compile by default. Read the remote result and
  evidence, make a local source fix, commit/push it, call `sync_workspace` again, and retry
  remote `build`.
- If a queued `task_*` returns a failed dev_selftest result, call `logagent.runs.result`, then call
  `logagent.dev_selftest.diagnose` for the persistent `devselftest_*` run. Use the diagnosis before
  asking the user to run cleanup or before changing code.
- A failed dev_selftest step records evidence in `progress.json` and marks the dev_selftest run
  failed.
- `report` remains callable after failures and should be used to summarize failed steps.
- Docker health check failures do not trigger automatic rollback. Keep the environment for
  inspection by default; after `report`, call `logagent.dev_selftest.cleanup` when cleanup is
  explicitly desired.
- Cleanup failures are recorded as cleanup evidence, but they do not change the report's core
  self-test verdict. Retry cleanup with the same `devselftest_*` id if needed.

## Removed Paths

Do not use or describe these as current behavior:

- Server-side workflow API.
- Server-loaded skills or skill registry.
- Legacy `docs/runbooks/dev-selftest-pipeline/` entry.
- Source tarball upload for dev_selftest.
- SSH/SCP executor or managed executor records.
- `/api/executors` or `/api/executor-runs`.
- `suite.executor`, Huawei package sync, GeminiDB create instance, or custom agent loops.
- SSHing to the Server or scanning local `prd_assistant` config to discover allowlist values.
- Force-pushing to an old allowlisted branch to avoid updating the allowlist, unless the user
  explicitly asks for that operation.
