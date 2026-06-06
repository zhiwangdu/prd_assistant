# Development Progress

Last updated: 2026-06-06

## Status Summary

LogAgent MVP has a working upload-to-evidence loop and a documented Analysis Agent architecture.

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
  - `GET /api/metadata/instances/:instance_id`
  - `GET /api/metadata/clusters/:cluster_id`
  - `GET /api/metadata/clusters/:cluster_id/nodes`
  - `POST /api/metadata/snapshots/fetch`
  - `POST /api/metadata/imports`
  - `POST /api/metadata/imports/fetch`
  - `GET /api/metadata/imports/:import_id/preview`
  - `POST /api/metadata/imports/:import_id/confirm`
- Uses API Key middleware for protected APIs.
- Statically serves Vite output from `webui/out`.
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

- React + Vite + TypeScript + Tailwind CSS app under `webui/`.
- Uses shadcn/ui composition primitives and React Flow.
- `npm run build` writes `webui/out`.
- Served by Server at `/` from `webui/out`.
- Supports:
  - health check
  - fixed top-bar API Key input
  - one or more file uploads
  - chunked upload for large files
  - task creation with `uploadIds`
  - artifact display
  - localStorage recent task list
  - Metadata query
  - Metadata YAML/JSON import preview and confirmation
  - Metadata openGemini `/getdata` URL fetch preview
  - Metadata cluster view for `PtView` partition state and `Databases` schema/RP/shard summary
  - Metadata Overview, Nodes, Partitions, Topology, Databases, Schemas, Diagnostics, and Raw JSON
  - complete Shard, IndexGroup, Index, and MstVersions logical/physical table views
  - topology follows DataNode -> Database/PT -> ShardGroup -> Shard -> IndexGroup -> Index
  - DataNode container lanes, topology filters, abnormal highlighting, missing-owner lanes, and entity detail panel

### Metadata

- Implemented as Server internal Rust module.
- Uses local JSON files under `storage.data_dir/metadata`.
- Supports:
  - instance lookup
  - cluster lookup
  - cluster node listing
  - JSON/YAML import preview
  - openGemini `/getdata` snapshot normalization
  - openGemini `PtView` normalization into cluster `partitionViews`
  - openGemini `Databases` normalization into database/RP/measurement/shard summaries
  - server-side metadata URL fetch
  - import confirmation and persistence
- CSV import remains reserved but not implemented.
- Raw openGemini snapshots are preserved.
- Shard and Index owners are modeled as PT IDs and resolved through PtView to DataNodes.

### Documentation

- Root `README.md` and `SPEC.md` exist.
- Root `PROGRESS.md` records development status and must be updated after file changes.
- Every component has a `README.md` and `SPEC.md`.
- `AGENTS.md` records development conventions and current context.
- `AGENT.md` has been renamed to `AGENTS.md`.
- `metadata/` module has planning docs for instance, cluster, node, and template import management.
- Added independent `analysis-agent/` documentation for task-scoped context, multi-round investigation, user questions, action approvals, budgets, termination, and recovery.
- Repositioned `llm-agent/` as an LLM Gateway instead of the task orchestrator.
- Unified task planning around stable states plus execution phases:
  - `QUEUED`
  - `RUNNING`
  - `WAITING_FOR_USER`
  - `WAITING_FOR_APPROVAL`
  - `SUCCEEDED`
  - `FAILED`
- Defined Agent actions: `search_logs`, `run_tool`, `collect_code_evidence`, `collect_environment`, `ask_user`, and `final_answer`.
- Defined planned analysis APIs for state reads, user messages, and action approval decisions.
- Documented that safe read-only actions may run automatically while SSH/SCP collection requires approval by default.
- Documented that hidden chain-of-thought is not stored; only auditable rationale summaries and evidence references are persisted.
- Added Mermaid diagrams for the planned component architecture, execution trust boundary, Analysis Agent investigation loop, waiting states, and recovery path.

## Verified

Recent checks run successfully:

```bash
cargo fmt --check
cargo check
cargo test
npm run lint
npm run typecheck
npm run build
```

Recent HTTP smoke checks:

- `GET /health`
- `GET /`
- batch upload with `/api/uploads/batch`
- task creation with `uploadIds`
- artifact read with `/api/tasks/:task_id/artifacts`
- manifest paths include package-name prefixes for batch analysis
- metadata YAML import preview and confirm
- metadata instance and cluster query
- metadata `http://127.0.0.1:8091/getdata` parsing plan implemented for openGemini snapshot
- metadata server-side fetch from `http://127.0.0.1:8091/getdata`, confirm, cluster query, node query, and instance query
- metadata unit test covers openGemini `PtView` owner/status and `Databases` RP/schema/shard summary parsing
- live `127.0.0.1:8091/getdata` smoke test verified PT 0 -> DataNode 2, Shard/Index 1, and `testmst -> testmst_0000`

## Planned Next

1. Persist Server task list and task state machine so WEBUI does not depend only on localStorage.
2. Connect Metadata to task creation and write `metadata_context.json`.
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
6. Implement Analysis Agent state/events, action executor, user questions, approvals, budgets, idempotency, and restart recovery.
7. Implement LLM Gateway structured action/final-answer decisions.
8. Implement Case Store save and recall from manually confirmed final results.

## Documentation Verification

For the Analysis Agent architecture update:

- Reviewed all component README/SPEC documents.
- Updated root architecture, original `plan.md`, interfaces, Server, WebUI, config, security, testing, deployment, evidence providers, Case Store, roadmap, and `AGENTS.md`.
- No application code or runtime configuration was changed, so Rust and WebUI build checks were not required.
- Added and syntax-reviewed the root Mermaid architecture and investigation-loop diagrams.

## Maintenance Rule

Every completed file change must update this progress document when it changes project status, behavior, APIs, module scope, verification, or next-step priorities.

When changing a component, also update that component's `README.md` and `SPEC.md`.
