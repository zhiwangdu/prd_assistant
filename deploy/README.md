# LogAgent Runtime Deploy

This directory contains the runtime deployment assets for a local LogAgent server.

## Files

```text
deploy/
  README.md
  .env.example
  logagent.example.yaml
  logagentctl.sh
  rebuild-install.sh
```

Runtime files stay one level above this directory:

```text
$LOGAGENT_APP_DIR/
  bin/logagent-server
  data/
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

## Configure

`logagent.yaml` is the active config used by `logagentctl.sh`. It uses `${LOGAGENT_APP_DIR}` for `storage.data_dir`; the Server expands this value at startup.

To reset from the sample:

```bash
cp logagent.example.yaml logagent.yaml
```

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

The script builds `logagent-server`, replaces `$LOGAGENT_APP_DIR/bin/logagent-server`, syncs `webui/out`, and restarts only if the server was already running.

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
