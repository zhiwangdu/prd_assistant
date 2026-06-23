# dev_selftest openGemini docker cluster (default demo)

A 3 meta + 3 (sql+store) openGemini cluster brought up by the `logagent.dev_selftest.deploy`
tool's `docker_cluster` profile. This is the validated default demo for the dev_selftest
Docker path (sync → build → deploy → run_tests → report, all SUCCEEDED).

The cluster artifacts live here in the repo. The openGemini **source** and **binaries**
are NOT here: the pipeline syncs the source (`dev_selftest.git.repos`) and builds the
binaries into the run's `source/build/`, which the compose mounts via
`${DEVSELFTEST_SOURCE_DIR}/build`.

## Files

- `build-opengemini.sh` — go1.26 compat (`go mod edit -go=1.26` + upgrade `bytedance/sonic`)
  + `go build` of `ts-meta/ts-store/ts-sql`. Referenced by a `dev_selftest.builds` profile.
- `docker-compose.yml` — 6 services (meta-1/2/3, sqlstore-1/2/3), static IPs, one shared
  config template. Referenced by a `dev_selftest.docker.clusters` profile.
- `config/openGemini.conf.template` — upstream openGemini config with `{{addr}}` /
  `{{id}}` / `{{meta_addr_1..3}}` placeholders; each container's entrypoint substitutes
  its own `OG_ADDR/OG_ID` + the shared `OG_META_*`.
- `entrypoint-meta.sh` / `entrypoint-sqlstore.sh` — per-node config substitution + startup
  gating (meta → store → sql; `depends_on` only orders, the entrypoint waits for readiness).

## Wire it into a server config

See `examples/server-dev-selftest.yaml` (the openGemini demo). Key pieces:

```yaml
dev_selftest:
  enabled: true
  git:
    enabled: true
    binary: "/usr/bin/git"
    repos:
      - { url: "https://github.com/openGemini/openGemini.git", refs: ["main"] }
  builds:
    opengemini:
      command: ["<repo>/deploy/devselftest/opengemini/build-opengemini.sh"]
      working_dir: ""                                  # source/ is the openGemini root
      artifact_globs: ["build/ts-meta", "build/ts-store", "build/ts-sql"]
  docker:
    binary: "/usr/bin/docker"
    clusters:
      opengemini_cluster:
        compose_file: "<repo>/deploy/devselftest/opengemini/docker-compose.yml"
        exposed_port: 8086
        health_check: { cmd: ["curl", "-sf", "http://127.0.0.1:8086/query?q=SHOW+DATABASES"], timeout_seconds: 180 }
  test_suites:
    opengemini_smoke:
      argv: ["curl", "-sf", "http://127.0.0.1:8086/query?q=SHOW+DATABASES"]
```

`<repo>` = absolute path to this repo (dev_selftest requires absolute paths). The server
process must have docker access (be in the `docker` group, or start via `sg docker -c`).

## Intranet / air-gapped overrides

All via environment variables on the **server process** (inherited by the deploy/build
children — no code change):

- **Image** (`镜像名`): `OG_BASE_IMAGE=<registry>/ubuntu:24.04` (default `ubuntu:24.04`).
  The compose uses `image: ${OG_BASE_IMAGE:-ubuntu:24.04}`.
- **Go module source** (`源`): `GOPROXY=https://goproxy.intranet,direct` (default
  `https://goproxy.cn,direct`) and, if your proxy can't reach `sum.golang.org`,
  `GOSUMDB=off`. The build script respects both.
- **openGemini source** (`源`): set `dev_selftest.git.repos` to your internal git mirror
  (the allowlist entry).

Example: `OG_BASE_IMAGE=registry.intranet:5000/ubuntu:24.04 GOPROXY=https://goproxy.intranet,direct GOSUMDB=off sg docker -c 'logagent-server --config ...'`

## Why static IPs / ubuntu:24.04 / gating (gotchas)

- **Static IPs**: openGemini meta uses `rpc-bind-address` as the raft Server ID; hostnames
  resolve to a different IP string → hashicorp raft reports "not part of a stable
  configuration" and never elects a leader. Static IPs (matching the official
  `install_cluster.sh` 127.0.0.1/2/3) fix it.
- **ubuntu:24.04**: 22.04's libstdc++ lacks `GLIBCXX_3.4.32` (the binary needs it).
- **Startup gating**: meta must be ready before store, store before sql. `depends_on` only
  orders container start; the entrypoint polls ports (meta:8091, store:8400) before
  proceeding. ts-store binds `store-ingest-addr` (the container IP:8400), so the store
  check uses `OG_ADDR`, not 127.0.0.1.

## Manual standalone test (outside the pipeline)

```bash
DEVSELFTEST_SOURCE_DIR=/path/to/openGemini-repo \
  docker compose -p ogtest -f deploy/devselftest/opengemini/docker-compose.yml up -d
# after ~10s:
curl -s "http://127.0.0.1:8086/query?q=SHOW+DATABASES"
docker compose -p ogtest -f deploy/devselftest/opengemini/docker-compose.yml down
```
