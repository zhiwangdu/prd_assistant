# dev_selftest InfluxDB single-node demo

This directory provides a single-node InfluxDB OSS v1 dev_selftest target. It
uses the same LocalToolHub pipeline shape as the openGemini demo:

```text
sync_workspace -> build -> deploy -> run_tests -> report -> cleanup(optional)
```

InfluxDB OSS is single-node only in this branch, so the deployment profile starts
one `influxd` container and exposes the v1 HTTP API on port `8086`.

## Files

- `build-influxdb.sh` - builds only the `influxd` server binary from the synced
  InfluxDB source checkout. The recommended profile runs it in a Linux
  `golang:1.26-bookworm` Docker build target.
- `docker-compose.yml` - one `influxdb` service using `ubuntu:24.04`, mounting
  `${DEVSELFTEST_SOURCE_DIR}/build/influxd`.
- `config/influxdb.conf` - minimal v1 config for a single-node, auth-disabled
  smoke environment.
- `entrypoint.sh` - validates the mounted `influxd` binary and starts it with the
  bundled config.
- `tests/smoke.sh` - InfluxDB v1 API smoke test: `SHOW DATABASES`, create DB,
  write line protocol, then query it back.

## Wire It Into A Server Config

Use the probe script to write a machine-local config with absolute paths:

```bash
deploy/probe-influxdb-config.sh --print
```

Defaults:

- repo: `ssh://git@github.com/zhiwangdu/influxdb.git`
- ref: `master-1.x`
- builder image: `golang:1.26-bookworm` (the build script installs
  `pkg-config/curl` with `apt-get` when missing and uses rustup for Rust 1.83)
- runtime image: `ubuntu:24.04`
- test image: `alpine:3.20`

The user-facing repo form `git@github.com:zhiwangdu/influxdb.git` is normalized
to `ssh://git@github.com/zhiwangdu/influxdb.git` in generated Server config
because LocalToolHub's git allowlist requires an explicit URL scheme.

Core config shape:

```yaml
remote_execution:
  commands:
    influxdb_smoke:
      enabled: true
      argv: ["sh", "/tests/smoke.sh"]
      timeout_seconds: 180

dev_selftest:
  enabled: true
  git:
    repos:
      - url: "ssh://git@github.com/zhiwangdu/influxdb.git"
        refs: ["master-1.x"]
  builds:
    influxdb:
      argv: ["bash", "/scripts/build-influxdb.sh"]
      artifact_globs: ["build/influxd"]
      docker:
        image: "golang:1.26-bookworm"
        network: "host"
        workdir: "/workspace/source"
        volumes:
          - "<repo>/deploy/devselftest/influxdb:/scripts:ro"
  docker:
    clusters:
      influxdb_single:
        compose_file: "<repo>/deploy/devselftest/influxdb/docker-compose.yml"
        exposed_port: 8086
        health_check:
          cmd: ["curl", "-sf", "http://127.0.0.1:8086/query?q=SHOW+DATABASES"]
  test_suites:
    influxdb_smoke:
      command: influxdb_smoke
      docker:
        image: "alpine:3.20"
        network: "host"
        volumes:
          - "<repo>/deploy/devselftest/influxdb/tests:/tests:ro"
```

## Intranet Overrides

All overrides are environment variables on the Server process or probe script:

- `INFLUXDB_BUILDER_IMAGE=<registry>/golang:1.26-bookworm`
- `INFLUXDB_INSTALL_BUILD_DEPS=0` if your builder image already includes
  `rustc`/`cargo` >= 1.83 and `pkg-config` and must not run installers.
- `INFLUXDB_RUST_TOOLCHAIN=1.83.0` to pin or override the Rust toolchain used
  for Flux `libflux`.
- `INFLUXDB_BASE_IMAGE=<registry>/ubuntu:24.04`
- `DEVSELFTEST_TEST_IMAGE=<registry>/alpine:3.20`
- `GOPROXY=https://goproxy.intranet,direct`
- `GOSUMDB=off`
- `LOGAGENT_INFLUXDB_REPO_URL=ssh://git@github.com/zhiwangdu/influxdb.git`
- `LOGAGENT_INFLUXDB_REF=master-1.x`

The compose host port is driven by `DEVSELFTEST_PORT`, which the Server injects
from `dev_selftest.docker.clusters.*.exposed_port`. When using the probe script,
prefer `--db-port` so the profile, health check, and compose interpolation stay
aligned.

## Manual Standalone Test

After `build/influxd` exists under an InfluxDB source checkout:

```bash
DEVSELFTEST_SOURCE_DIR=/path/to/influxdb DEVSELFTEST_PORT=8086 \
  docker compose -p influxdbtest -f deploy/devselftest/influxdb/docker-compose.yml up -d

curl -s "http://127.0.0.1:8086/query?q=SHOW+DATABASES"

docker compose -p influxdbtest -f deploy/devselftest/influxdb/docker-compose.yml down
```
