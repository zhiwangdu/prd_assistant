# LogAgent V2 Runtime Deploy

This directory contains the runtime deployment assets for the Python/FastAPI
V2 server.

## Files

```text
deploy/
  README.md
  .env.example
  install-deps.sh
  logagent-v2ctl.sh
  rebuild-v2-install.sh
```

Runtime files stay one level above this directory:

```text
$LOGAGENT_APP_DIR/
  server-v2/.venv/
  bin/tools/
    influxql-analyzer
    flux_query_analyzer
    opengemini-storage-analyzer
    influxdb_storage_analyzer
  data-v2/
    logagent.sqlite
    artifacts/
    tmp/
    skills/
  webui/out/
  logagent-v2.pid
  logagent-v2.log
```

## Environment

```bash
cd /path/to/runtime/deploy
cp .env.example .env
```

Edit `.env`. `logagent-v2ctl.sh` and `rebuild-v2-install.sh` load
`$HOME/.bashrc` on a best-effort basis and then load `.env` when present.

Required variables:

- `LOGAGENT_APP_DIR`: runtime directory, parent of `deploy/`.
- `LOGAGENT_SRC_DIR`: source repository directory used by `rebuild-v2-install.sh`.
- `LOGAGENT_NATIVE_API_KEY`: shared API key; V2 uses it when `LOGAGENT_V2_API_KEY` is unset.
- `LOGAGENT_LLM_BASE_URL`, `LOGAGENT_LLM_API_KEY`, `LOGAGENT_LLM_MODEL`: optional
  OpenAI-compatible provider settings when V2 is not using the stub provider.

Common V2 overrides:

- `LOGAGENT_V2_HOST`, `LOGAGENT_V2_PORT`
- `LOGAGENT_V2_DATA_DIR`
- `LOGAGENT_V2_WEBUI_DIR`
- `LOGAGENT_V2_VENV_DIR`
- `LOGAGENT_V2_PID_FILE`
- `LOGAGENT_V2_LOG_FILE`
- `LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS`
- `LOGAGENT_V2_FETCH_*`
- `LOGAGENT_V2_REMOTE_*`
- `LOGAGENT_V2_TOOL_*_ANALYZER`
- `LOGAGENT_SUBMODULE_*_URL`

## Dependencies

```bash
./install-deps.sh
```

`server-v2` itself runs in a Python virtualenv and uses SQLite from Python's
standard library. Building from source also needs Node.js/npm for WebUI and Go
for source-built analyzers. `cargo` is only needed when building Flux or
InfluxDB analyzers, because those builds compile local `third_party/flux`
Rust sources.

## Build And Install

```bash
./rebuild-v2-install.sh
```

Useful variants:

```bash
./rebuild-v2-install.sh --server-only
./rebuild-v2-install.sh --skip-tools
./rebuild-v2-install.sh --tools-only --only-tool influxql
./rebuild-v2-install.sh --tools-only --only-tool flux_query_analyzer
./rebuild-v2-install.sh --no-restart
```

The script creates `$LOGAGENT_V2_VENV_DIR`, installs `server-v2` with pip,
initializes the V2 SQLite database under `$LOGAGENT_V2_DATA_DIR`, builds and
syncs `webui/out` to `$LOGAGENT_V2_WEBUI_DIR` unless `--server-only` is set,
and builds source-referenced analyzer tools into `$LOGAGENT_APP_DIR/bin/tools`
by default. Use `--skip-tools` when a deployment must avoid submodule clone or
compile work.

## Control

```bash
./logagent-v2ctl.sh start
./logagent-v2ctl.sh status
./logagent-v2ctl.sh smoke-tools
./logagent-v2ctl.sh logs
./logagent-v2ctl.sh restart
./logagent-v2ctl.sh stop
```

`status` checks `/health` and, when reachable, queries `/api/v2/tools` with the
configured API key to report whether the four source-built analyzers are
registered and runnable.
