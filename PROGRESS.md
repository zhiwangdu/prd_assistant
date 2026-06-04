# Development Progress

Last updated: 2026-06-04

## Status Summary

LogAgent MVP has a working upload-to-evidence loop and a documented module plan.

Current runnable loop:

```text
Chrome Extension or WEBUI
  -> Native Agent or Server upload API
  -> Server task workspace
  -> archive extraction / manifest
  -> simple grep evidence
  -> WEBUI artifact display
```

## Implemented

### Chrome Extension

- Manifest V3 extension exists under `chrome-extension/`.
- Watches Chrome download completion.
- Filters by configured URL prefixes and file suffixes.
- Shows a notification and calls Native Agent `POST /imports`.
- Options page supports `agentBaseUrl`, `urlPrefixes`, and `fileSuffixes`.

### Native Agent

- Rust/Axum local agent exists under `native-agent/`.
- Supports:
  - `GET /health`
  - `POST /imports`
- Validates:
  - allowed local directories
  - allowed suffixes
  - max upload size
- Uploads small files via multipart.
- Uploads large files via chunked upload.
- Calls Server task creation after upload.

### Server

- Rust/Axum server exists under `server/`.
- Supports:
  - `GET /health`
  - `POST /api/uploads`
  - `POST /api/uploads/batch`
  - `POST /api/uploads/init`
  - `POST /api/uploads/:upload_id/chunks?offset=<bytes>`
  - `POST /api/uploads/:upload_id/complete`
  - `POST /api/tasks`
  - `GET /api/tasks/:task_id/artifacts`
- Uses API Key middleware for protected APIs.
- Statically serves `webui/`.
- Creates Server-owned task IDs and workspaces.

### Upload And Workspace

- Server owns `upload_id`, `task_id`, and workspace location.
- Single task can now reference one upload or many uploads.
- Batch task workspace layout:

```text
workspaces/task_xxx/
  raw/
    upl_xxx/<filename>
  extracted/
    <package_name>/
  manifest.json
  grep_results.json
```

- Batch task manifest includes:
  - `uploadId` for backward compatibility
  - `uploadIds`
  - `uploads`
  - `files`

### Log Analyzer

- Implemented as Server internal Rust module.
- Supports:
  - `.log`
  - `.txt`
  - `.zip`
  - `.tar`
  - `.tar.gz`
  - `.tgz`
- `.tar.gz` / `.tgz` extraction falls back to plain `.tar` if gzip tar extraction fails.
- Archive extraction uses safe path joining to prevent workspace escape.
- Produces `manifest.json` and `grep_results.json`.

### WEBUI

- Static HTML/CSS/JS under `webui/`.
- Served by Server at `/`.
- Supports:
  - health check
  - API Key input
  - one or more file uploads
  - chunked upload for large files
  - task creation with `uploadIds`
  - artifact display
  - localStorage recent task list

### Documentation

- Root `README.md` and `SPEC.md` exist.
- Root `PROGRESS.md` records development status and must be updated after file changes.
- Every component has a `README.md` and `SPEC.md`.
- `AGENT.md` records development conventions and current context.
- `metadata/` module has planning docs for instance, cluster, node, and template import management.

## Verified

Recent checks run successfully:

```bash
cargo fmt --check
cargo check
cargo test
node --check webui/app.js
```

Recent HTTP smoke checks:

- `GET /health`
- `GET /`
- batch upload with `/api/uploads/batch`
- task creation with `uploadIds`
- artifact read with `/api/tasks/:task_id/artifacts`
- manifest paths include package-name prefixes for batch analysis

## Planned Next

1. Persist Server task list and task state machine so WEBUI does not depend only on localStorage.
2. Implement Metadata Store:
   - instance ID metadata
   - cluster records
   - node records
   - template import preview and confirmation
   - WEBUI metadata page
3. Implement Tool Runner for existing compiled tools:
   - `flux_query_analyzer`
   - `influxql_analyzer`
4. Implement Code Evidence:
   - map product/version to branch/tag/ref
   - prepare read-only worktree/cache
   - collect code file/line evidence
5. Implement Environment Collector:
   - SSH/SCP test environment collection
   - whitelist nodes, paths, and commands
6. Implement LLM Agent structured analysis.
7. Implement Case Store save and recall.

## Maintenance Rule

Every completed file change must update this progress document when it changes project status, behavior, APIs, module scope, verification, or next-step priorities.

When changing a component, also update that component's `README.md` and `SPEC.md`.
