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
  data/
    uploads/
    sessions/
    session_workspaces/
    tasks/
    workspaces/
    cases/              # legacy Case JSON migration/rollback source
    case_imports/
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

Edit `.env`. `logagentctl.sh` and `rebuild-install.sh` load it automatically when present; you can also load it manually:

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

## Dependencies

Running an already built `logagent-server` binary does not require installing SQLite separately; the deploy binary is built with bundled SQLite support.

Building from source with `rebuild-install.sh` needs:

- Rust toolchain (`cargo`)
- Node.js and npm
- git and curl
- C/C++ build tools and pkg-config

Quick install on macOS or common Linux distributions:

```bash
./install-deps.sh
```

Preview without changing the host:

```bash
./install-deps.sh --dry-run
```

Optional diagnostic tools are not installed by this script. Install and configure `go` only if enabling `pprof_analyzer`; install `influxql_analyzer` or `flux_query_analyzer` separately and set their configured paths when enabling those tools.

## Configure

`logagent.yaml` is the active config used by `logagentctl.sh`. It uses `${LOGAGENT_APP_DIR}` for `storage.data_dir`; the Server expands this value at startup.

To reset from the sample:

```bash
cp logagent.example.yaml logagent.yaml
```

The sample config includes an `embedding` block with `enabled: false`. Memory currently uses local SQLite FTS/BM25 recall and writes the index to `data/memory/memory.sqlite`; legacy Case JSON files in `data/cases/` are kept as migration and rollback source.

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

The script builds `logagent-server`, creates the expected runtime data directories including `data/memory/`, replaces `$LOGAGENT_APP_DIR/bin/logagent-server`, syncs `webui/out`, and restarts only if the server was already running. It does not delete or migrate runtime data.

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
