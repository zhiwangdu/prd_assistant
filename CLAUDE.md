# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

LogAgent is a **local Tool/MCP Workbench**: a single-binary Rust server that hosts a web admin UI, runs a catalog of controlled tools, stores run/artifact history, and exposes an MCP server so external clients (Claude Code, Codex, Cursor, OpenCode) can call the same tools. It is **not** a general-purpose agent and does **not** use Claude Code / an LLM as its default backend — those are optional automation only.

The repo is mid-pivot on branch `rewrite/local-toolhub-rust` (base `main`). Old session/task/analysis-agent code is present as a **migration source only**, not the target architecture. New work lands in tools / runs / artifacts / MCP semantics. Read root `README.md`, `SPEC.md`, `AGENTS.md`, and the relevant component `README.md`/`SPEC.md` before starting any work.

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
scripts/start-local.sh [--llm|--stub] [--foreground]   # stub→:50992, llm→:50994
scripts/build-all.sh            # server + webui + lan deploy
```

Analyzer smoke tests against real `third_party/` binaries: `scripts/smoke-*.sh`.

## Environment

- `LOGAGENT_NATIVE_API_KEY` — required API key (the `Authorization: Bearer` token). Fallback when no `auth.api_keys` are configured.
- `LOGAGENT_FETCH_SECRET_KEY` — base64 of 32 bytes, required only when `fetch.enabled: true`.
- HuaweiCloud package sync, remote SSH, etc. each pull their own secrets from env **only when their subsystem is `enabled: true`** — disabled subsystems do not require their env vars.

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
- `http/` — Axum handlers, one file per resource (`tools`, `fetch`, `executors`, `metadata`, `cases`, `skills`, `settings`, `sessions`, `tasks`, `uploads`, `mcp_readonly`, …). `http/mod.rs::router` is the single route table; everything under `/api/*` is behind the `require_api_key` middleware except `/health`.
- `services/` — business logic: `tool_runner` (allowlisted external binary exec with timeout/output limits), `fetch`, `remote_execution` (SSH/SCP via templated commands only), `metadata`, `log_analyzer`, `skill_registry`. (The legacy `llm_gateway`/`agent_backend`/`domain_adapters`/`agent_contracts` analysis-agent modules were removed in Phase 5.)
- `stores/` — persistence: JSON files per record + SQLite (`rusqlite` bundled, e.g. `memory.sqlite`). No Postgres/Redis/ES. `pipeline/executor.rs` runs async tasks with a concurrency cap.
- `support/` — `config.rs` (the large config loader/resolver), `auth.rs` (bearer-token middleware), `error.rs` (`AppError` → HTTP), `fs_utils.rs` (logical path safety), `id.rs`.
- `domain/` — shared `contracts` and `models` types.
- `mcp_server.rs` — the task-free MCP server (stdio JSON-RPC via `mcp-serve`, also exposed at `POST /api/mcp`). Reuses the same `ToolRunner`, registry, allowlists, and artifact store as the HTTP path — MCP tool calls and WebUI tool runs share one execution boundary. (`http/mcp_readonly.rs` provides the read-only `/api/mcp/readonly` preview.)

**Tool Runner** — tools are external binaries configured in the `tools:` map (name → path/args/timeout/limits/match-patterns). Paths must be absolute. `{input_file}` is substituted in args. Source-built analyzers live in `third_party/` git submodules (influxql, flux, openGemini, influxdb) and are built to `bin/tools/` by `scripts/build-tools.sh`.

**WebUI (`webui/`)** — Tools-first nav (`Tools | Runs | Metadata | Fetch | Executors | MCP | Cases | SystemContext | Settings`, default Tools): `ToolsView`, `RunsView`, `FetchView`, `McpView`, `ExecutorsView`, `MetadataDashboard`, `CasesView`, `SystemContextView`, `SettingsView`. The legacy `OperationsView` (Analyze) is demoted out of nav pending removal. Vite config rewrites `/api` to the server. All artifact downloads must carry the `Authorization` header; sensitive fields render masked.

**Native Agent (`native-agent/`)** — optional localhost-only (`127.0.0.1:17321`) import bridge: Chrome extension hands it a local file path, it validates `allowed_dirs`/`allowed_suffixes`/size, then uploads to the server. It never holds the server API key in plaintext.

**Data layout** — everything under one `storage.data_dir` (default `./data/logagent`): `uploads/`, `workspaces/`, `tasks/`, `sessions/`, `cases/`, `memory/`, `executors/`, `metadata/`, `fetch/`, `system_context/`. Artifact paths exposed externally are logical IDs, never raw local paths.

## Security boundaries (enforced in code, not just docs)

- Every execution surface has an **allowlist**: tools (configured paths only), fetch (explicit `allowed_hosts`, default off), remote execution (templated commands only — no free shell), code evidence (read-only repo search).
- Secrets come only from env vars; `Debug` impls for settings with secrets redact them. Never log, persist, or export API keys, cookies, or `Authorization` headers (exports/skills/tools zips are scrubbed).
- Path safety: tool/fetch/executor inputs are validated against traversal; artifact references are logical.

## Repo conventions (from AGENTS.md)

- **After any change, update the touched component's `README.md` + `SPEC.md` AND root `PROGRESS.md`** (behavior changes, verification results, next steps). This is enforced by convention across the repo.
- **The user requires auto `commit` and `push` after each implementation/modification** (unless explicitly told otherwise). Commit only relevant files — never `.idea/`, temp review inputs, secrets, runtime `data/`, build caches, or `third_party/` generated artifacts.
- After Rust changes run at least `cargo fmt --check` + `cargo check` (+ `cargo test` when tests cover the change). After WebUI changes run `npm run lint` + `npm run typecheck` + `npm run build`.
- `third_party/` are upstream submodule sources; do not rewrite their READMEs. Skills (`skills/<name>/SKILL.md` + `logagent.json`) are loaded by `SkillRegistry`.
- API direction: prefer `/api/tools*`, `/api/runs*`, `/api/artifacts*`, `/api/mcp*`, `/api/settings*`. Legacy `/api/sessions*`, `/api/tasks*` are migration-compat only — don't add new features there.
