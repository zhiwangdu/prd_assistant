# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

LogAgent is a **two-module local Tool/MCP Workbench**: a single-binary Rust server that hosts a web admin UI, runs the **dev_selftest** pipeline (Linux docker build/deploy/test) and the **log analysis** toolchain (preprocess + analyzers), stores run/artifact history, and exposes an MCP server so external clients (Claude Code, Codex, Cursor, OpenCode) can call the same tools. It is **not** a general-purpose agent and does **not** use Claude Code / an LLM as its default backend.

Convergence work is on branch `converge/two-modules` (from `rewrite/local-toolhub-rust`, base `main`): fetch / gemini_db / huawei_package_sync / metadata / cases / system_context / skills / SSH-SCP executor / 纳管 executor modules have been removed; `remote_execution` is gutted to a docker runner reused by dev_selftest. Read root `README.md`, `SPEC.md`, `AGENTS.md`, and the relevant component `README.md`/`SPEC.md` before starting any work.

## Commands

Rust (run from repo root; workspace has two crates: `logagent-server`, `logagent-native-agent`):

```bash
cargo fmt --check
cargo check
cargo test                                  # all tests
cargo test -p logagent-server               # server crate only
cargo test -p logagent-server config::tests # single module / test name
cargo run -p logagent-server -- --config examples/logagent.yaml
```

WebUI (`webui/`, React + Vite + TS + Tailwind):

```bash
cd webui && npm install
cd webui && npm run dev          # vite dev server
cd webui && npm run lint         # eslint, --max-warnings 0
cd webui && npm run typecheck    # tsc -b
cd webui && npm run build        # outputs webui/out (served by the Rust server)
```

Local run helpers (build WebUI if missing, build server, start, health-check):

```bash
scripts/start-local.sh [--foreground]   # 127.0.0.1:50992, examples/server-test.yaml
scripts/build-all.sh                    # server + webui + lan deploy
```

Analyzer smoke tests against real `third_party/` binaries: `scripts/smoke-*.sh`.

## Environment

- `LOGAGENT_NATIVE_API_KEY` — required API key (the `Authorization: Bearer` token). Fallback when no `auth.api_keys` are configured.
- `dev_selftest` pulls its git/docker/build secrets from env only when `dev_selftest.enabled: true`.

LogAgent is LLM-free by default; no `LOGAGENT_CLAUDE_CODE_PATH` or `LOGAGENT_LLM_*` env vars are required to start.

Config is YAML (`examples/*.yaml`). `${ENV}` placeholders in config values are expanded from the environment (see `expand_env_vars_with` in `support/config.rs`); secrets are never written into config files.

## Architecture

```
Browser WebUI / External MCP client / Chrome Ext → Native Agent
  -> Rust server (Axum)
       auth middleware -> http handlers -> services -> stores -> local data/
```

**Server (`server/src/`)** — layered:

- `main.rs` — parses config, creates `AppState`, mounts `http::router` + a `ServeDir` fallback to `webui/out`. Also has a `mcp-serve` subcommand: `logagent-server mcp-serve` speaks JSON-RPC over stdio for external MCP clients (task-free; logs forced to stderr).
- `app.rs` — `AppState` is the god-object holding every store and service, constructed from `AppConfig`. `recover_tasks()` re-enqueues incomplete tasks on startup.
- `http/` — Axum handlers, one file per resource (`tools`, `runs`, `artifacts`, `uploads`). `http/mod.rs::router` is the single route table; everything under `/api/*` is behind the `require_api_key` middleware except `/health`.
- `services/` — business logic: `tool_runner` (allowlisted external binary exec with timeout/output limits), `remote_execution` (docker runner only — `run_executor_command` + `ExecutorTarget::Docker` + `command_template`, reused by dev_selftest), `log_analyzer` (preprocess + grep), `tools` (catalog dispatcher), `dev_selftest` (sync/build/deploy/run_tests/report pipeline).
- `stores/` — persistence: JSON files per record. No Postgres/Redis/ES. `pipeline/executor.rs` runs async tasks with a concurrency cap.
- `support/` — `config.rs` (the config loader/resolver), `auth.rs` (bearer-token middleware), `error.rs` (`AppError` → HTTP), `fs_utils.rs` (logical path safety), `id.rs`, `docker_target.rs` (DockerTargetSpec + validation, shared by dev_selftest).
- `domain/` — shared `contracts` and `models` types.
- `mcp_server.rs` — the task-free MCP server (stdio JSON-RPC via `mcp-serve`, also exposed at `POST /api/mcp`). Reuses the same `ToolRunner`, registry, allowlists, and artifact store as the HTTP path — MCP tool calls and WebUI tool runs share one execution boundary. Resources are `logagent://runs/recent` + `logagent://tools/catalog`.

**Tool Runner** — tools are external binaries configured in the `tools:` map (name → path/args/timeout/limits/match-patterns). Paths must be absolute. `{input_file}` is substituted in args. Source-built analyzers live in `third_party/` git submodules (influxql, flux, openGemini, influxdb) and are built to `bin/tools/` by `scripts/build-tools.sh`.

**WebUI (`webui/`)** — Tools-first nav with English-only top-level tabs: `Tools (with sub-item Runs History) | MCP | Settings`, default Tools. Views: `ToolsView`, `RunsView`, `McpView`, `SettingsView`. Vite config rewrites `/api` to the server. All artifact downloads must carry the `Authorization` header; sensitive fields render masked.

**Native Agent (`native-agent/`)** — optional localhost-only (`127.0.0.1:17321`) import bridge: Chrome extension hands it a local file path, it validates `allowed_dirs`/`allowed_suffixes`/size, then uploads to the server. It never holds the server API key in plaintext.

**Data layout** — everything under one `storage.data_dir` (default `./data/logagent`): `uploads/`, `workspaces/`, `tasks/`, `dev_selftest/`. Artifact paths exposed externally are logical IDs, never raw local paths.

## Security boundaries (enforced in code, not just docs)

- Every execution surface has an **allowlist**: tools (configured paths only), dev_selftest (configured git repos / build profiles / docker clusters / command templates only — no free shell).
- Secrets come only from env vars; `Debug` impls for settings with secrets redact them. Never log, persist, or export API keys, cookies, or `Authorization` headers.
- Path safety: tool/dev_selftest inputs are validated against traversal; artifact references are logical.

## Repo conventions (from AGENTS.md)

- **After any change, update the touched component's `README.md` + `SPEC.md` AND root `PROGRESS.md`** (behavior changes, verification results, next steps). This is enforced by convention across the repo.
- **The user requires auto `commit` and `push` after each implementation/modification** (unless explicitly told otherwise). Commit only relevant files — never `.idea/`, temp review inputs, secrets, runtime `data/`, build caches, or `third_party/` generated artifacts.
- After Rust changes run at least `cargo fmt --check` + `cargo check` (+ `cargo test` when tests cover the change). After WebUI changes run `npm run lint` + `npm run typecheck` + `npm run build`.
- `third_party/` are upstream submodule sources; do not rewrite their READMEs. Diagnostic runbooks live in `docs/runbooks/` as local Claude Code skill references (the server no longer loads skills).
- API direction: prefer `/api/tools*`, `/api/runs*`, `/api/artifacts*`, `/api/mcp*`. Legacy `/api/sessions*`, `/api/tasks*` are migration-compat only — don't add new features there.
