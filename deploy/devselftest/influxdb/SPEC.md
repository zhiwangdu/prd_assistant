# InfluxDB dev_selftest Spec

## Goal

Provide a LocalToolHub dev_selftest fixture for OSS InfluxDB v1 single-node
validation. The fixture must build only the single-node `influxd` server,
deploy exactly one containerized server, and run a bounded HTTP API smoke test.

## Requirements

- Source enters only through `logagent.dev_selftest.sync_workspace` from an
  allowlisted git repo/ref.
- Default repo/ref is `ssh://git@github.com/zhiwangdu/influxdb.git` +
  `master-1.x`.
- Build output is `build/influxd`; no clustered or enterprise binaries are
  built.
- Default build profile is Docker-backed so the produced binary targets Linux
  even when the LocalToolHub Server runs on macOS with Docker Desktop.
- Runtime deploy uses `docker compose` with a single service, no privileged
  mode, no host filesystem mounts except the run source build directory and
  checked-in config/entrypoint files.
- Smoke test uses the InfluxDB v1 HTTP API over `DEVSELFTEST_HOST` /
  `DEVSELFTEST_PORT` and does not require network access from the test
  container.
- Cleanup remains the standard dev_selftest `docker compose down` step and does
  not delete run evidence.

## Non-Goals

- No InfluxDB clustering or enterprise mode.
- No auth, TLS, backup/restore, or upgrade testing in the default smoke fixture.
- No server-side workflow orchestration; Claude Code or another MCP client still
  owns step ordering.

## Acceptance

- Generated config exposes build profile `influxdb`, deploy profile
  `influxdb_single`, and test suite `influxdb_smoke` through
  `logagent://dev_selftest/config`.
- Full workflow succeeds:
  `sync_workspace -> build -> deploy -> run_tests -> report`.
