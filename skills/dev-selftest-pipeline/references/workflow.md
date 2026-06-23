# Dev self-test workflow and result shapes (P1)

P1 implements the **docker self-test closed loop**. All commands/binaries/compose paths
come from the `dev_selftest` config allowlist; tool params only select profile ids and carry
a `runId`. Source ingest is tarball upload or allowlisted git; SFTP is not supported.

## Config (representative)

```yaml
dev_selftest:
  enabled: true
  build_timeout_seconds: 600
  max_output_bytes: 8388608
  git:
    enabled: false
    binary: "/usr/bin/git"
    repos:
      - { url: "https://example/influxql-analyzer.git", refs: ["main", "release/*"] }
  builds:
    influxql_analyzer:
      command: ["cargo", "build", "--release"]
      working_dir: "source/influxql-analyzer"
      artifact_globs: ["target/release/influxql-analyzer"]
  docker:
    binary: "/usr/bin/docker"
    clusters:
      local_cluster:
        compose_file: "/opt/dev_selftest/docker-compose.yml"
        exposed_port: 8086
        health_check:
          cmd: ["curl", "-sf", "http://127.0.0.1:8086/health"]
          timeout_seconds: 60
  test_suites:
    influxql_smoke:
      argv: ["/opt/testfw/bin/runner", "--host", "127.0.0.1", "--port", "8086"]
      timeout_seconds: 300
```

## Step-by-step (MCP `tools/call`)

### 1. sync_workspace
```
{ "name": "logagent.dev_selftest.sync_workspace",
  "arguments": { "label": "pr-123", "uploadId": "upl_..." } }
```
- Source options (one): `uploadId` (source tarball `.tar.gz/.tar/.zip` unpacked into
  `source/`), `gitRepo`+`gitRef` (allowlisted; `git clone --depth 1 --branch`), or omit
  (empty stub source).
- Omit `runId` to create a new run; pass `runId` to reuse an existing one.
- Result: `{ runId, sourceRef, status: "OK"|"FAILED", error, durationMs }`.

### 2. build
```
{ "name": "logagent.dev_selftest.build",
  "arguments": { "runId": "devselftest_...", "buildProfile": "influxql_analyzer" } }
```
- Runs the profile `command` with cwd `source/{working_dir}`; captures
  `logs/build.stdout.txt` / `build.stderr.txt`; copies `artifact_globs` matches into
  `artifacts/`.
- Result: `{ runId, buildProfile, status, exitCode, artifacts: [...], error, durationMs }`.

### 3. deploy (docker_cluster)
```
{ "name": "logagent.dev_selftest.deploy",
  "arguments": { "runId": "...", "profile": "local_cluster" } }
```
- Runs `<docker> compose -p devselftest_<runId>_<cluster> -f <compose> up -d`, then the
  declared `health_check.cmd` until it succeeds or `timeout_seconds` elapses.
- Result: `{ runId, profile, projectName, status, exitCode, deployTarget: {kind:"docker", cluster, exposed_port}, error, durationMs }`.
- P1: no rollback on health-check failure.

### 4. run_tests (stub in P1)
```
{ "name": "logagent.dev_selftest.run_tests",
  "arguments": { "runId": "...", "testSuite": "influxql_smoke", "runMode": "queued" } }
```
- P1 stub: runs the suite `argv` locally (cwd = run workspace) with `suite.env` plus
  `DEVSELFTEST_HOST`/`DEVSELFTEST_PORT` from the docker target. The real executor-dispatched
  test framework lands in P2.
- `runMode: "queued"` returns `{ runId, status: "QUEUED", url }` immediately; poll with
  `logagent.runs.get`. `runMode: "sync"` (default) runs inline.
- Result: `{ runId, testSuite, status, exitCode, stdoutPath: "logs/tests.stdout.txt", stderrPath, error, durationMs }`.

### 5. poll + result (platform tools, no run record created)
```
{ "name": "logagent.runs.get",      "arguments": { "runId": "task_..." } }
{ "name": "logagent.runs.result",   "arguments": { "runId": "task_..." } }
```
- `runs.get`: `{ runId, status, phase, toolId, error, resultAvailable }`.
- `runs.result`: `{ runId, taskKind, toolId, resultPath, result: <step result.json> }` (only
  when `status: "SUCCEEDED"`).

### 6. report
```
{ "name": "logagent.dev_selftest.report",
  "arguments": { "runId": "..." } }
```
- Reads `progress.json`, writes `report.md` + `report.json`.
- Result: `{ runId, status: "SUCCEEDED"|"FAILED", reportPath: "report.md", failedSteps: [...], steps: [...] }`.

## progress.json shape

```json
{
  "schemaVersion": 1,
  "runId": "devselftest_...",
  "steps": [
    { "step": "sync_workspace", "status": "OK", "durationMs": 12, "error": null, "evidenceRefs": ["source/"], "startedAt": "..." },
    { "step": "build", "status": "OK", "durationMs": 345, "evidenceRefs": ["artifacts/influxql-analyzer"] },
    { "step": "deploy", "status": "OK", "evidenceRefs": ["logs/deploy.stdout.txt", "logs/deploy.stderr.txt"] },
    { "step": "run_tests", "status": "OK", "evidenceRefs": ["logs/tests.stdout.txt", "logs/tests.stderr.txt"] },
    { "step": "report", "status": "OK", "evidenceRefs": ["report.md", "report.json"] }
  ]
}
```

Re-running a step replaces its prior entry (no duplicates). A `FAILED` step marks the run
`FAILED`; `report` overall status is `FAILED` with `failedSteps` listed.
