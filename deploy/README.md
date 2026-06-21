# LogAgent Runtime Deploy

This directory contains the runtime deployment assets for a local LogAgent server.

## Files

```text
deploy/
  README.md
  .env.example
  install-deps.sh
  logagent.example.yaml
  logagent-v2ctl.sh
  logagentctl.sh
  rebuild-v2-install.sh
  rebuild-install.sh
```

Runtime files stay one level above this directory:

```text
$LOGAGENT_APP_DIR/
  bin/logagent-server
  server-v2/.venv/
  bin/tools/
    influxql-analyzer
    flux_query_analyzer
    opengemini-storage-analyzer
    influxdb_storage_analyzer
  data/
    uploads/
    sessions/
    session_workspaces/
    tasks/
    workspaces/
    cases/              # legacy Case JSON migration/rollback source
    case_imports/
    executors/          # WebUI-managed ECS SSH executor records
    memory/
      memory.sqlite     # Memory SQLite index, currently memoryType=case
  data-v2/
    logagent.sqlite     # V2 SQLite state
    artifacts/
    tmp/
    skills/
  webui/out/
  logagent-server.pid
  logagent-server.log
  logagent-v2.pid
  logagent-v2.log
```

## Environment

Load or export the required environment variables before starting the server:

```bash
cd /path/to/runtime/deploy
cp .env.example .env
```

Edit `.env`. `logagentctl.sh` and `rebuild-install.sh` load `$HOME/.bashrc` on a best-effort basis and then load `.env` when present; you can also load `.env` manually:

```bash
set -a
source .env
set +a
```

Required variables:

- `LOGAGENT_APP_DIR`: runtime directory, parent of `deploy/`.
- `LOGAGENT_SRC_DIR`: source repository directory used by `rebuild-install.sh`.
- `LOGAGENT_NATIVE_API_KEY`: API key accepted by Server.
- `LOGAGENT_LLM_BASE_URL`: OpenAI-compatible LLM base URL.
- `LOGAGENT_LLM_API_KEY`: LLM API key.
- `LOGAGENT_LLM_MODEL`: model name.

Optional variables:

- `LOGAGENT_CONFIG`: defaults to `$LOGAGENT_APP_DIR/deploy/logagent.yaml`.
- `LOGAGENT_SERVER_BIN`: defaults to `$LOGAGENT_APP_DIR/bin/logagent-server`.
- `LOGAGENT_HEALTH_URL`: defaults to `http://127.0.0.1:50992/health`.
- `LOGAGENT_PID_FILE`: defaults to `$LOGAGENT_APP_DIR/logagent-server.pid`.
- `LOGAGENT_LOG_FILE`: defaults to `$LOGAGENT_APP_DIR/logagent-server.log`.
- `LOGAGENT_V2_APP_DIR`: V2 runtime directory, defaults to `LOGAGENT_APP_DIR`.
- `LOGAGENT_V2_API_KEY`: V2 bearer token, defaults to `LOGAGENT_NATIVE_API_KEY` when unset.
- `LOGAGENT_V2_HOST`: V2 bind host, defaults to `127.0.0.1`.
- `LOGAGENT_V2_PORT`: V2 bind port, defaults to `50993`.
- `LOGAGENT_V2_DATA_DIR`: V2 SQLite/artifact directory, defaults to `$LOGAGENT_APP_DIR/data-v2`.
- `LOGAGENT_V2_WEBUI_DIR`: V2 static WebUI directory, defaults to `$LOGAGENT_APP_DIR/webui/out`.
- `LOGAGENT_V2_VENV_DIR`: V2 virtualenv directory, defaults to `$LOGAGENT_APP_DIR/server-v2/.venv`.
- `LOGAGENT_V2_PID_FILE`: defaults to `$LOGAGENT_APP_DIR/logagent-v2.pid`.
- `LOGAGENT_V2_LOG_FILE`: defaults to `$LOGAGENT_APP_DIR/logagent-v2.log`.
- `LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS`: V2 start health wait timeout, defaults to `30`.
- `LOGAGENT_V2_DISCOVER_PROCESS`: optional global process discovery for lost pid files, defaults to `0`.
- `LOGAGENT_V2_FETCH_ENABLED`: optional V2 Fetch endpoint execution switch, disabled by default.
- `LOGAGENT_V2_FETCH_ALLOWED_HOSTS`: comma-separated exact host or host:port allowlist for V2 Fetch execution.
- `LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES`: defaults to `1048576`, limiting saved endpoint bodies and runtime body overrides.
- `LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES`: defaults to `1048576`, limiting stored Fetch response bodies/previews.
- `LOGAGENT_V2_FETCH_SECRET_KEY`: optional Fernet-compatible 32-byte urlsafe base64 key required when saving sensitive Fetch credentials.
- `LOGAGENT_V2_TOOLS_DIR`: optional custom directory for V2 source-built
  analyzer binaries. If unset, V2 auto-discovers the standard analyzer
  filenames from `$LOGAGENT_V2_APP_DIR/bin/tools` or `$LOGAGENT_APP_DIR/bin/tools`.
- `LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER`,
  `LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER`,
  `LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER`, and
  `LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER`: optional explicit source-built
  analyzer executable path overrides, usually unnecessary because a full
  `rebuild-v2-install.sh` now builds analyzers into the runtime tool directory.
  Rust/V1 aliases
  `LOGAGENT_TOOL_INFLUXQL_ANALYZER`, `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER`,
  `LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER`, and
  `LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER` are also accepted for migration;
  V2-specific names take precedence.
- `LOGAGENT_V2_PPROF_ENABLED` and `LOGAGENT_V2_PPROF_GO_COMMAND`: optional
  built-in `pprof_analyzer` configuration.
- `LOGAGENT_V2_REMOTE_EXECUTION_ENABLED`, `LOGAGENT_V2_REMOTE_SSH_COMMAND`,
  `LOGAGENT_V2_REMOTE_CONNECT_TIMEOUT_SECONDS`,
  `LOGAGENT_V2_REMOTE_COMMAND_TIMEOUT_SECONDS`,
  `LOGAGENT_V2_REMOTE_MAX_OUTPUT_BYTES`,
  `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY`, and
  `LOGAGENT_V2_REMOTE_COMMANDS_JSON`: optional V2 Remote Executor SSH boundary
  and whitelisted command template configuration. Remote execution is enabled
  by default with the low-risk `smoke_ls_root` template; set
  `LOGAGENT_V2_REMOTE_EXECUTION_ENABLED=0` in runtimes where SSH should never
  run.
- `LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED` plus `LOGAGENT_V2_HUAWEI_OBS_*`
  and `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN`: optional built-in Huawei OBS + GaussDB
  package sync configuration.
- `LOGAGENT_EMBEDDING_API_KEY`: reserved for future embedding/vector recall. The sample config keeps `embedding.enabled=false`, so it is not required today.
- `LOGAGENT_CLAUDE_CODE_PATH`: required by the default `logagent.yaml`. Set it to the absolute Claude Code CLI path, usually the output of `which claude`.
- `LOGAGENT_SUBMODULE_BASE_URL`: optional internal Git namespace for all source-built analyzer submodules.
- `LOGAGENT_SUBMODULE_INFLUXQL_URL`, `LOGAGENT_SUBMODULE_FLUX_URL`, `LOGAGENT_SUBMODULE_OPENGEMINI_URL`, `LOGAGENT_SUBMODULE_INFLUXDB_URL`: optional per-repository clone URLs. These override the committed GitHub defaults through local Git submodule config only and must not change the parent repository `origin`.

## Dependencies

Running an already built `logagent-server` binary does not require installing SQLite separately; the deploy binary is built with bundled SQLite support.

Building from source with `rebuild-install.sh` needs:

- Rust toolchain (`cargo`)
- Node.js and npm
- git and curl
- Go toolchain (`go`) for source-referenced diagnostic tools
- OpenSSH client (`ssh`) when `remote_execution.enabled=true`
- C/C++ build tools and pkg-config

Quick install on macOS or common Linux distributions:

```bash
./install-deps.sh
```

Preview without changing the host:

```bash
./install-deps.sh --dry-run
```

InfluxQL, Flux, openGemini storage, and InfluxDB storage analyzers are built from `third_party/` submodules during `rebuild-install.sh` and installed to `$LOGAGENT_APP_DIR/bin/tools/`. `pprof_analyzer` uses the configured Go executable at runtime. Remote Executor uses the configured system `ssh` binary and the Server process user's SSH config/agent/keys; LogAgent does not store SSH private keys.

In environments that cannot reach GitHub, set the submodule URL variables in `.env` before running `rebuild-install.sh`. The rebuild path calls `scripts/build-tools.sh`, which first writes these URLs to local Git submodule config and then runs `git submodule update --init --recursive` as needed. If a submodule directory exists but has not been initialized as its own Git worktree yet, the helper skips `remote set-url origin` for that directory so the parent repository remote stays intact. You can also apply the same override manually from the source checkout:

```bash
export LOGAGENT_SUBMODULE_BASE_URL="ssh://git@gitlab.internal/zhiwangdu"
./scripts/configure-tool-submodules.sh
git submodule update --init --recursive third_party/flux
```

## Configure

`logagent.yaml` is the active config used by `logagentctl.sh`. It uses `${LOGAGENT_APP_DIR}` for `storage.data_dir`; the Server expands this value at startup.

To reset from the sample:

```bash
cp logagent.example.yaml logagent.yaml
```

The sample config includes an `embedding` block with `enabled: false`, a `remote_execution` block with the low-risk `smoke_ls_root` template, a `claude_code` block, and `mcp.transport=stdio`. `LOGAGENT_CLAUDE_CODE_PATH` points directly to the `claude` binary; the Server invokes it with `--print --output-format json --json-schema ... --mcp-config ... --strict-mcp-config`. Memory currently uses local SQLite FTS/BM25 recall and writes the index to `data/memory/memory.sqlite`; legacy Case JSON files in `data/cases/` are kept as migration and rollback source.

## Build And Install

Build from source and replace the runtime binary:

```bash
./rebuild-install.sh
```

Useful variants:

```bash
./rebuild-install.sh --server-only
./rebuild-install.sh --no-restart
```

The script builds `logagent-server`, builds source-referenced diagnostic tools into `$LOGAGENT_APP_DIR/bin/tools/`, creates the expected runtime data directories including `data/memory/`, replaces `$LOGAGENT_APP_DIR/bin/logagent-server`, syncs `webui/out`, and restarts only if the server was already running. It does not delete or migrate runtime data.

`rebuild-install.sh` loads `$HOME/.cargo/env` when present, so Rust installed through rustup is available in non-interactive SSH shells. `logagentctl.sh` also loads `$HOME/.bashrc`, so runtime-only environment variables such as `LOGAGENT_CLAUDE_CODE_PATH` may live there or in deploy `.env`.

When developing from the Mac workspace, `scripts/build-all.sh` also runs `scripts/auto-deploy-lan.sh` after the local Server and WebUI build completes. The helper only runs on macOS, pings `192.168.31.128`, and when reachable SSHes to `duzhiwang@192.168.31.128`, runs `git pull --ff-only` in the remote source tree, then runs the remote runtime `deploy/rebuild-install.sh` and `logagentctl.sh start/status`.

V2 uses a Python virtualenv instead of a Rust binary. Build/install V2 from the
same source checkout and sync the WebUI static build:

```bash
./rebuild-v2-install.sh
```

Useful variants:

```bash
./rebuild-v2-install.sh --server-only
./rebuild-v2-install.sh --with-tools
./rebuild-v2-install.sh --skip-tools
./rebuild-v2-install.sh --tools-only --only-tool influxql
./rebuild-v2-install.sh --tools-only --only-tool influxql_analyzer
./rebuild-v2-install.sh --no-restart
```

The script creates `$LOGAGENT_V2_VENV_DIR`, installs `server-v2` with pip,
initializes the V2 SQLite database under `$LOGAGENT_V2_DATA_DIR`, runs the WebUI
build unless `--server-only` is set, syncs `webui/out` to
`$LOGAGENT_V2_WEBUI_DIR`, builds source-referenced analyzer tools into
`$LOGAGENT_APP_DIR/bin/tools` by default, and restarts V2 only if it was
already running. `--skip-tools` keeps the older fast path when a deployment
must avoid submodule clone/compile work; `--tools-only` skips server install,
DB init, and WebUI sync for fast analyzer rebuilds. `logagent-v2ctl.sh` exports
`LOGAGENT_V2_APP_DIR`, so a later V2 start auto-registers the standard analyzer
filenames from `$LOGAGENT_APP_DIR/bin/tools` unless explicit
`LOGAGENT_V2_TOOL_*` overrides or Rust/V1 `LOGAGENT_TOOL_*` aliases are set;
V2-specific names take precedence. `rebuild-v2-install.sh` also loads
`$HOME/.cargo/env` when present so Flux analyzer builds can find rustup-managed
`cargo` in non-interactive SSH shells.

For local development from the source checkout, `scripts/v2-local.sh` provides
the same fast V2 build/start/stop/status/logs loop without copying `deploy/`:

```bash
./scripts/v2-local.sh build
./scripts/v2-local.sh start
./scripts/v2-local.sh status
./scripts/v2-local.sh stop
```

It defaults to `server-v2/.venv`, `/tmp/logagent-v2-local`, port `50993`, and
`target/tools`. Use `--with-tools` or `--only-tool <name>` when source-built
analyzers need to be rebuilt. Single-tool rebuild accepts both short names
`influxql|flux|opengemini|influxdb` and V2 tool IDs such as
`influxql_analyzer`, `flux_query_analyzer`, `opengemini_storage_analyzer`, and
`influxdb_storage_analyzer`.

Useful overrides:

```bash
LOGAGENT_LAN_AUTO_DEPLOY=0 ./scripts/build-all.sh
LOGAGENT_LAN_REMOTE_ADDR=192.168.31.128 ./scripts/build-all.sh
LOGAGENT_LAN_REMOTE_HOST=duzhiwang@192.168.31.128 ./scripts/build-all.sh
LOGAGENT_LAN_REMOTE_DEPLOY_DIR=/home/duzhiwang/workspace/data/prd_assistant/deploy ./scripts/build-all.sh
```

## Start And Stop

```bash
./logagentctl.sh start
./logagentctl.sh status
./logagentctl.sh logs
./logagentctl.sh restart
./logagentctl.sh stop
```

V2 has equivalent controls:

```bash
./logagent-v2ctl.sh --help
./logagent-v2ctl.sh start
./logagent-v2ctl.sh status
./logagent-v2ctl.sh logs
./logagent-v2ctl.sh smoke-tools --only-tool flux_query_analyzer
./logagent-v2ctl.sh restart
./logagent-v2ctl.sh stop
```

`logagent-v2ctl.sh` is scoped to the configured V2 pid file by default, so
multiple runtime directories do not accidentally control each other's V2
processes. `help`, `--help`, and `-h` print usage and exit successfully.
`start` and `restart` wait for the configured V2 health URL to return success.
If the process exits or the health check times out, the script removes the
stale pid file and returns a non-zero status. `smoke-tools` delegates to
`$LOGAGENT_SRC_DIR/scripts/smoke-source-built-analyzers.sh` and accepts the same
short analyzer names and V2 tool IDs as `rebuild-v2-install.sh --only-tool`.

Check the UI after startup:

```text
http://127.0.0.1:50992/
```

Check the V2 UI after startup:

```text
http://127.0.0.1:50993/
```

The WebUI opens on `Log Analysis`; top navigation is `Log Analysis`, `Memory`, `System Context`, `Tools`.
