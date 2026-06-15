# LogAgent Runtime Deploy

This directory contains the runtime deployment assets for a local LogAgent server.

## Files

```text
deploy/
  README.md
  .env.example
  install-deps.sh
  logagent.example.yaml
  logagentctl.sh
  rebuild-install.sh
```

Runtime files stay one level above this directory:

```text
$LOGAGENT_APP_DIR/
  bin/logagent-server
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
  webui/out/
  logagent-server.pid
  logagent-server.log
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
- `LOGAGENT_EMBEDDING_API_KEY`: reserved for future embedding/vector recall. The sample config keeps `embedding.enabled=false`, so it is not required today.
- `LOGAGENT_CLAUDE_CODE_PATH`: required by the default `logagent.yaml`. Set it to the absolute Claude Code CLI path, usually the output of `which claude`.
- `LOGAGENT_SUBMODULE_BASE_URL`: optional internal Git namespace for all source-built analyzer submodules.
- `LOGAGENT_SUBMODULE_INFLUXQL_URL`, `LOGAGENT_SUBMODULE_FLUX_URL`, `LOGAGENT_SUBMODULE_OPENGEMINI_URL`, `LOGAGENT_SUBMODULE_INFLUXDB_URL`: optional per-repository clone URLs. These override the committed GitHub defaults through local Git config only.

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

In environments that cannot reach GitHub, set the submodule URL variables in `.env` before running `rebuild-install.sh`. The rebuild path calls `scripts/build-tools.sh`, which first writes these URLs to local `.git/config` and then runs `git submodule update --init --recursive` as needed. You can also apply the same override manually from the source checkout:

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

Check the UI after startup:

```text
http://127.0.0.1:50992/
```

The WebUI opens on `Log Analysis`; top navigation is `Log Analysis`, `Memory`, `System Context`, `Tools`.
