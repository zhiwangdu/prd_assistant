---
name: Dev Self-Test Pipeline
description: Runbook for the dev self-test workflow — sync source, build, deploy (docker_cluster in P1), run tests, poll, and report — driven from a remote MCP client (Claude Code on Windows) against the Linux LocalToolHub server.
---

Use this skill to run a development self-test loop: edit code on Windows, sync it to the
Linux server, build, deploy to a local docker cluster, run a test suite, poll the run, and
generate a report — all through MCP tool calls. Every step is a catalog tool sharing one
execution boundary; the server never runs a free shell or an agent loop.

## Prerequisites

- `dev_selftest.enabled: true` in the server config, with at least one `builds` profile,
  one `docker.clusters` profile, and one `test_suites` profile configured. All commands,
  the docker binary, and compose file paths must be absolute and allowlisted in config.
- `mcp.enabled: true`. Connect from Windows via the streamable-http endpoint
  `POST /api/mcp` (recommended over an SSH tunnel; direct HTTPS needs TLS + API key +
  `mcp.allowed_origins`).
- The five `logagent.dev_selftest.*` tools are greyed out until `dev_selftest.enabled`.

## State model

A **run** is a persistent workspace `data/dev_selftest/runs/{runId}/` (with `source/`,
`artifacts/`, `logs/`, `progress.json`, `report.md`, `report.json`) plus an index record.
`sync_workspace` creates the run and returns `runId`; **carry that `runId` to every later
tool call**. Each tool appends a step to `progress.json` and writes its own `result.json`.

## The pipeline (P1: docker path)

| Step | Tool | Key params |
|---|---|---|
| 1. sync | `logagent.dev_selftest.sync_workspace` | `{label}`, plus one source: `{uploadId}` (a source tarball) **or** `{gitRepo, gitRef}` (must be in the configured allowlist) **or** omit for an empty stub source. Returns `{runId, status, sourceRef}`. |
| 2. build | `logagent.dev_selftest.build` | `{runId, buildProfile}`. Runs the configured build command in `source/{working_dir}`, collects `artifact_globs` into `artifacts/`. Returns `{status, exitCode, artifacts}`. |
| 3. deploy | `logagent.dev_selftest.deploy` | `{runId, profile}`. P1: `docker_cluster` — `docker compose -p <project> -f <compose> up -d` + declared health check. Records the deploy target. Returns `{status, projectName, deployTarget}`. |
| 4. run tests | `logagent.dev_selftest.run_tests` | `{runId, testSuite}`. A suite with a `docker` block dispatches through the executor docker runner (`docker run --rm --network host <image> <argv>`); a suite without one uses the P1 local stub (runs `argv` on the server host). Either way `DEVSELFTEST_HOST`/`DEVSELFTEST_PORT` are injected as system env (final priority). Returns `{status, exitCode, executor, stdoutPath, stderrPath}`. |
| 5. poll | `logagent.runs.get` | `{runId}` — the `runId` returned by any `runMode:"queued"` call (e.g. `run_tests`). Side-effect-free: creates no run record. Returns `{status, phase, resultAvailable}`. |
| 6. result | `logagent.runs.result` | `{runId}` — reads the structured result of a successful run. Side-effect-free. |
| 7. report | `logagent.dev_selftest.report` | `{runId}`. Aggregates `progress.json` + step evidence into `report.md` + `report.json`. Returns `{status, reportPath, failedSteps, steps}`. |

### Sync vs queued (`runMode`)

`tools/call` accepts an optional `runMode: "sync" | "queued"` (default `sync`). Short steps
(sync/build/deploy/report) run inline. For a long test suite, call `run_tests` with
`arguments: {runId, testSuite, runMode: "queued"}` — it returns `{runId, status: "QUEUED"}`
immediately; poll with `logagent.runs.get` until `status: "SUCCEEDED"`, then
`logagent.runs.result` for the structured outcome. **One run per queued call — no child
runs.**

## Reading results

Each `logagent.dev_selftest.*` call writes a `result.json` with `status` (`OK` / `FAILED` /
`SUCCEEDED`), `runId`, `durationMs`, `error`, and step-specific fields. The run workspace
holds the durable evidence: `progress.json` (step ledger), `logs/*.stdout.txt` /
`*.stderr.txt`, `artifacts/`, and `report.md` / `report.json`.

## Failure handling

A failed step marks the run `FAILED` and records the error in `progress.json`; subsequent
steps still run (so you can still call `report`), but `report` overall status is `FAILED`
with `failedSteps` listed. In P1, a failed docker health check does **not** roll back
(rollback lands in P2 for the SSH binary-replace path).

## Roadmap (later phases)

- **P2 docker slice (done)**: the executor execution engine is extracted into a reusable
  `run_executor_command` supporting `Ssh` and `Docker` targets; `run_tests` dispatches a
  `docker` test suite through it as an ephemeral `docker run --rm` container (replacing the
  P1 local stub for docker suites). System env (`DEVSELFTEST_HOST/PORT` + run dirs) is
  injected with final priority; the docker target (image/network/volumes/env) is
  config-allowlisted and validated.
- **P2 still deferred**: parameterized executor command templates (`{var}` + small JSON
  Schema); Docker executor 纳管 (executor record docker kind + `/api/executors` CRUD +
  run history); `ssh_binary_replace` deploy profile + controlled SCP (deploy the built
  binary to a GeminiDB Influx node, swap + restart, health check, rollback).
- **P3** `package_create_instance` deploy profile: publish the binary via Huawei OBS package
  sync, then `logagent.geminidb.create_instance` + poll-until-ready.

Until the deferred P2/P3 pieces land, only the `docker_cluster` deploy profile is available,
and the docker test target is declared inline in the test suite config (not via an executor
record).

## Notes

- **MCP arguments**: tool params may be sent either nested under `params`
  (`{params: {runId, buildProfile}}`) or as top-level arguments per the tool's
  `inputSchema` (`{runId, buildProfile}`) — both work. `runMode`/`uploadIds` are
  always top-level. `logagent.runs.get`/`runs.result` take `runId` top-level.
- **Docker path validated**: the `docker_cluster` profile has been run end-to-end
  against a real **openGemini** 3 meta + 3 (sql+store) cluster (sync→build→deploy→
  run_tests→report, all SUCCEEDED). The default demo cluster artifacts (compose /
  config template / entrypoints / build script) are in the repo under
  `deploy/devselftest/opengemini/` — see its README. They are intranet-configurable
  via server-process env: `OG_BASE_IMAGE` (image name / registry mirror),
  `GOPROXY`/`GOSUMDB` (Go module source), and `dev_selftest.git.repos` (openGemini
  source mirror). Key requirements for an openGemini-style cluster compose:
  **static IPs** (the DB's raft uses the bind-address string as the node ID, so
  hostnames break leader election), a recent base image (e.g. `ubuntu:24.04` for
  libstdc++), and sequential startup gating in the entrypoint (meta → store → sql;
  `depends_on` only orders, it does not wait for readiness).
