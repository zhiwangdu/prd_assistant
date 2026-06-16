# Development Progress

Last updated: 2026-06-17

## 2026-06-17 V2 Fetch Credential Sets

- Added `cryptography` dependency and `LOGAGENT_V2_FETCH_SECRET_KEY` for Fernet-encrypted Fetch credential sets.
- Added SQLite-backed `fetch_credential_sets` with one encrypted credential set per sensitive Fetch endpoint.
- Fetch endpoint create/import/update now stores only redacted URL/header/body material in `fetch_endpoints` when sensitive query, header, or body fields are detected.
- Sensitive endpoint writes require a valid secret key before the endpoint row is created or updated; non-sensitive endpoints continue to work without a key.
- Fetch execution hydrates the full request material from the encrypted credential set, while API, MCP, and result artifacts continue to expose redacted values.
- Added regression coverage for missing-key rejection, encrypted storage, credential hydration, execution with restored secrets, and redacted request artifacts.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `server-v2/pyproject.toml`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Case Import Drafts

- Added SQLite-backed V2 `case_imports` drafts for Memory import preview/confirm flow.
- Added deterministic Case draft parsing for JSON fields and plain text sections such as `Title`, `Symptom`, `Root Cause`, `Solution`, `Product`, `Instance ID`, and `Evidence Refs`.
- Added protected APIs: `GET /api/v2/cases/imports`, `GET /api/v2/cases/imports/:import_id`, `POST /api/v2/cases/imports/preview`, and `POST /api/v2/cases/imports/:import_id/confirm`.
- Preview stores the source text, parsed draft, and validation errors without writing to `cases`; confirm may apply overrides and only then creates a manual Case through the existing FTS-indexed path.
- Confirming an already confirmed import returns the existing Case instead of creating duplicates.
- Added regression coverage for preview parsing, confirm/search, idempotent confirm, incomplete draft rejection, and override completion.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Metadata Task Context

- Added V2 run-start `metadata_context.json` artifact generation.
- Metadata context auto-selects up to three imported instances from the Workspace question/mode; a single imported instance is included as `default_single`, while multiple instances require question matches on instance, remark, node, database, measurement, or field terms.
- The context stores bounded topology/schema outlines only; full snapshots and field/tag details remain available through existing Metadata MCP tools.
- Task MCP now exposes `logagent-v2://run/:run_id/metadata_context` through `resources/list` and `resources/read`.
- Metadata context evidence is background-only with `final_allowed=false`.
- Added regression coverage for auto-selection, MCP resource exposure, outline contents, and final evidence exclusion.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Fetch cURL Import

- Added V2 DevTools bash cURL import helpers for Fetch endpoints.
- Added protected APIs `POST /api/v2/fetch/imports/preview` and `POST /api/v2/fetch/imports`.
- Supported cURL flags are limited to method, headers, data/body, cookie, compressed, HEAD, and location; unsupported flags such as form upload are rejected.
- Import previews redact sensitive query, header, and JSON/form body fields and report detected sensitive fields.
- V2 Fetch endpoint methods now accept `HEAD`; controlled headers now include `Transfer-Encoding`.
- Added regression coverage for cURL parsing, redaction, sensitive field detection, direct endpoint creation data, HEAD import, unsupported flag rejection, and controlled header rejection.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Case FTS Recall

- Added a local SQLite FTS5 `cases_fts` index for Case Memory, synchronized on create/update and backfilled during store initialization.
- `Store.search_cases` now prefers FTS5 `MATCH` with `bm25` ranking when a query is present, while retaining token-overlap fallback if FTS5 is unavailable.
- Search results include `searchBackend` (`fts5`, `keyword`, or `recent`) and a score.
- Added regression coverage for FTS-backed ranking and index updates after case edits.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Skill Auto Matching

- Added V2 automatic Skill selection when a Workspace has no explicit `skillIds`.
- Skill matching now considers `keywords`, `products`, `toolIds`, `domainAdapters`, Skill id, name, display name, description, Workspace question, and mode.
- `includeByDefault` Skills are still included; matched resources record `selectionReason` (`explicit`, `default`, or `auto`) and `matchScore` in `system_context.json`.
- Imported Skill default manifests now include an empty `keywords` list.
- Added regression coverage for question-driven Skill selection and non-matching Skill exclusion.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Fetch Redirect Revalidation

- Added `LOGAGENT_V2_FETCH_MAX_REDIRECTS` with default 5 for V2 Fetch execution.
- Fetch now follows redirects manually and revalidates every redirect target against the same `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` allowlist before sending the next request.
- Cross-origin redirects strip sensitive headers such as Authorization, Cookie, X-Api-Key, and X-Auth-Token.
- Fetch response artifacts now include redacted `finalUrl`, `redirectCount`, and redirect hop summaries.
- Added regression coverage for allowlisted redirect success and disallowed redirect failure.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 InfluxQL Tool Output Adapter

- Added V2 Tool Runner stdout normalization for InfluxQL analyzer report and compare-report JSON.
- Generic JSON stdout parsing now accepts `summary`/`message`/`title`, `findings`/`issues`/`diagnostics`, string findings, and alternate finding fields such as `level`, `path`, `lineNumber`, and `description`.
- InfluxQL report output now produces summaries for record/window/statement/parse-error counts plus special rule counts, and findings for special rules, parse errors, realtime classification, and notable fingerprints.
- InfluxQL compare output now produces summaries for statement/QPS deltas and batch stats, plus findings for new/removed/changed fingerprints and rule deltas.
- Added regression coverage for InfluxQL report and compare-report parsing.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Tool Runner Fallback Inputs

- Added V2 Tool Runner fallback input matching for configured tools whose args contain `{input_file}`.
- Materialized `tool_inputs/index.json` entries still take priority; when none match, V2 selects current-run text files by `match.filePatterns`, then supplements with initial `grep_results.json` line matches containing `match.keywords`.
- Fallback inputs are persisted as `logagent.v2.tool_input.text_file.v1` artifacts and exposed to tools as virtual `extracted/<manifest path>` inputs.
- Multi-input task MCP `logagent.run_domain_tool` responses now preserve the primary `result/evidence` fields and also return `results[]` and `evidenceItems[]`.
- Added regression coverage for pattern-first and grep-keyword fallback selection, multi-input execution, stable distinct action ids, and evidence creation.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Tools Zip Export

- Added V2 `GET /api/v2/exports/tools.zip` for exporting enabled configured subprocess tools.
- Tool exports now include `README.md`, `tools-manifest.json`, packaged binaries under `bin/<toolId>/`, shell wrappers under `wrappers/`, and config examples under `config/examples/`; packaged binaries and wrappers carry executable zip mode metadata.
- Missing, relative, non-file, or non-executable configured tools are retained in the manifest with `skipped=true`; disabled tools and built-in tools are omitted.
- The export does not include API keys, Fetch endpoint credentials, runtime environment values, uploads, artifacts, or workspace data.
- Added regression coverage for executable packaging, skipped missing tools, config examples, manifest fields, and disabled-tool omission.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Skills Zip Export

- Added V2 `GET /api/v2/exports/skills.zip` for exporting the current filesystem Skill registry.
- Added `server-v2/logagent_v2/exports.py` to package each Skill directory's regular files, preserve relative paths, and write a root `manifest.json` with Skill revision and file metadata.
- Export path validation rejects absolute/parent paths, and symlinks or symlinked directories are skipped so exports cannot include files outside the Skill registry.
- Added regression coverage for regular Skill files, reference files, manifest generation, and symlink skipping.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Metadata URL Fetch

- Added allowlisted Metadata URL fetch for preview and direct import, reusing `LOGAGENT_V2_FETCH_ENABLED`, `LOGAGENT_V2_FETCH_ALLOWED_HOSTS`, timeout, and response-size limits.
- Added `POST /api/v2/metadata/imports/fetch/preview` to GET a metadata URL, parse/normalize the response, and create a preview draft without mutating confirmed instances.
- Added `POST /api/v2/metadata/imports/fetch` as a direct fetch-and-confirm shortcut.
- Metadata import drafts now track a redacted `sourceUrl`; sensitive query parameters are not exposed in previews.
- Redirects are rejected in this V2 slice; HTTP errors and allowlist failures are returned as controlled errors.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Metadata Preview Confirm

- Added SQLite-backed V2 `metadata_imports` drafts with `previewed` and `confirmed` statuses.
- Added Metadata preview flow that parses and normalizes JSON/YAML/openGemini content into a draft without mutating `metadata_instances`.
- Added Metadata confirm flow that upserts the draft snapshot into `metadata_instances` and marks the draft confirmed.
- Added protected APIs: `GET /api/v2/metadata/imports`, `GET /api/v2/metadata/imports/:import_id`, `POST /api/v2/metadata/imports/preview`, and `POST /api/v2/metadata/imports/:import_id/confirm`.
- Kept `POST /api/v2/metadata/imports` as the direct immediate import shortcut.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Materialized Tool Inputs

- Added V2 `ToolDefinition` support for `maxInputFiles` and `match` metadata in `LOGAGENT_V2_TOOLS_JSON`.
- Added node-package `tsdb` InfluxQL query materialization during initial evidence indexing. V2 now writes `influxql_analyzer` JSONL artifacts and a `tool_inputs/index.json` artifact with V1-compatible `ToolInputEntry` fields plus V2 artifact ids.
- Manifest artifacts now include `toolInputsPath=tool_inputs/index.json` and `toolInputCount` when materialized inputs exist.
- Added `tool_input_index` background evidence so task MCP Tool Runner calls can find run-local materialized inputs without exposing them as final root-cause evidence.
- Configured tools whose args contain `{input_file}` now select matching materialized inputs by `toolIds`, substitute the local artifact path, and generate stable per-input action ids for `tool_results/<tool_id>_<input_hash>/result.json#findings/<index>` refs.
- Existing no-input configured tools keep the previous action id behavior and final ref shape.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Fetch Endpoint Foundation

- Added V2 Fetch endpoint settings: default disabled execution, explicit host/host:port allowlist, request timeout, and bounded response preview size.
- Added SQLite-backed Fetch endpoint storage with create/list/read/update/delete helpers and redacted public endpoint previews.
- Added protected Fetch APIs: `GET/POST /api/v2/fetch/endpoints`, `GET/PATCH/DELETE /api/v2/fetch/endpoints/:endpoint_id`, and `POST /api/v2/runs/:run_id/fetch/:endpoint_id`.
- Added built-in Fetch catalog descriptor to `/api/v2/tools` and readonly MCP catalog output.
- Added task MCP tools `logagent.list_fetch_endpoints` and `logagent.fetch`; execution writes `fetch_result` artifacts/evidence and returns controlled refs in the form `tool_results/<fetch_action_id>/result.json#response`.
- Final-answer validation now accepts Fetch `#response` refs only when they point to current-run, final-allowed `fetch_result` evidence with a real response object.
- Readonly MCP remains non-executing for Fetch; `tools/call logagent.fetch` is rejected there.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Skill-backed System Context Foundation

- Added V2 filesystem Skill registry under `LOGAGENT_V2_DATA_DIR/skills` with Codex-compatible `SKILL.md`, optional `logagent.json`, revision hashing, declared references, and safe reference path validation.
- Added Skill APIs: `GET /api/v2/skills`, `GET /api/v2/skills/:skill_id`, `POST /api/v2/skills/imports`, and side-effect-free `POST /api/v2/skills/preview`.
- Workspaces now persist explicit `skillIds`; run execution writes a `system_context` background artifact containing selected diagnostic Skills, bounded content, revisions, and reference indexes.
- Added readonly MCP and task MCP Skill tools: `logagent.list_skills`, `logagent.get_skill`, `logagent.get_skill_reference`, and `logagent.preview_system_context`.
- Task MCP Skill reference reads are constrained to the run snapshot and persist `skill_reference` background evidence with `final_allowed=false`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Case Memory Foundation

- Added SQLite-backed V2 Case Memory table and Case schema v2 records.
- Added manual Case creation, succeeded-run Case confirmation with duplicate prevention, keyword recall, get-by-id, edit, and enable/disable support.
- Added V2 Case APIs: `POST /api/v2/cases`, `POST /api/v2/runs/:run_id/case`, `GET /api/v2/cases`, `GET /api/v2/cases/:case_id`, and `PATCH /api/v2/cases/:case_id`.
- Added readonly MCP and task MCP Case tools: `logagent.search_cases` and `logagent.get_case`.
- Task MCP Case calls now persist `case_context` background evidence with `final_allowed=false`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Metadata Foundation

- Added SQLite-backed V2 `metadata_instances` storage for imported raw snapshots and normalized Metadata snapshots.
- Added direct Metadata import for JSON, YAML via PyYAML, and openGemini-style content. The openGemini path normalizes nodes, databases, retention policies, measurements, and field type labels.
- Added V2 Metadata APIs for instance list/detail/snapshot/delete plus field type and tag field queries.
- Added readonly MCP and task MCP Metadata tools: `logagent.list_metadata_instances`, `logagent.get_metadata_snapshot`, `logagent.get_metadata_field_types`, and `logagent.get_metadata_tag_fields`.
- Task MCP Metadata calls now persist `metadata_slice` background evidence with `final_allowed=false`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Node Log Package Preprocessor

- Added V2 preprocessing for node log packages named `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` or `.tgz`.
- Tar scanning now classifies wrapped `var/chroot/gemini/log/tsdb`, `var/chroot/gemini/log/stream`, and `home/Ruby/log` members into stable `extracted/<nodeId>/<timestamp>/<group>/...` paths.
- Node package rotated files are accepted by directory membership instead of suffix, and gzip content is decoded by magic bytes before manifest and grep indexing.
- Matching node packages with no supported log directories now fail clearly instead of producing an empty successful manifest.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Final Evidence Validation

- Added V2 final answer normalization and validation before a run can be marked `succeeded`.
- Final answers now require a non-empty summary, normalized string arrays, structured likely root causes, and `confidence=low|medium|high`.
- Final evidence refs are validated against current-run `final_allowed` evidence artifacts. Supported refs are initial grep matches, follow-up log search matches, log slices, and configured tool findings.
- Background context such as `manifest.json` is explicitly rejected as final root-cause evidence.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Waiting Actions

- Added persistent V2 action helpers for pending user input and approval requests.
- Added task MCP `logagent.request_user_input` and `logagent.request_approval`; calls persist actions and move the run into `waiting_for_user` or `waiting_for_approval`.
- Updated message and action decision APIs so a waiting run can be requeued into the SQLite job queue after user input or approve/reject.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Minimal Tool Runner

- Added V2 configured Tool Runner foundation. Tools can be loaded from `LOGAGENT_V2_TOOLS_JSON`, listed through `/api/v2/tools`, and invoked through task MCP `logagent.run_domain_tool`.
- Tool execution is whitelist-only: models provide `toolId` only; executable path and argv come from Server configuration, run without shell, with timeout and bounded stdout/stderr.
- Tool results are written as JSON artifacts and `tool_result` evidence. JSON stdout with `summary` and `findings` is parsed into structured result fields.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Task MCP Log Slice

- Added task MCP `logagent.get_log_slice` for bounded context around a current Workspace text path and line number.
- Log slices are persisted as `log_slice` evidence with refs in the form `log_slices/<slice_id>.json#lines`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Task MCP Log Search

- Added V2 task MCP endpoint `POST /api/v2/mcp/task/:run_id`.
- Task MCP now supports `initialize`, `resources/list`, `resources/read`, `tools/list`, and `tools/call`.
- Exposed run-scoped `summary`, `evidence`, `manifest`, and `grep_results` resources over MCP.
- Implemented `logagent.search_logs` follow-up search for current Workspace uploads. Each search creates a `log_search` evidence item and returns stable refs in the form `log_searches/<search_id>.json#matches/<index>`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Initial Evidence Pipeline

- Continued the `rewrite/logagent-v2` clean-room migration toward Rust Server feature parity.
- Added V2 initial evidence indexing for Workspace uploads. Runs now collect supported plain text logs and scan `.zip`, `.tar`, `.tar.gz`, and `.tgz` packages with bounded file counts, bytes, and grep matches.
- Added archive safety checks that reject absolute paths, `..` traversal, empty paths, and unsafe member names; symlinks and non-file archive members are skipped.
- V2 run execution now writes `manifest.json` and `grep_results.json` artifacts, records `manifest` and `log_search` evidence rows, exposes `GET /api/v2/runs/:run_id/evidence`, and lets the current stub final answer cite `grep_results.json#matches/<index>` when initial matches exist.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-16 LogAgent V2 Clean-room Scaffold

- Created the `rewrite/logagent-v2` branch from `main` to keep Rust V1 stable while evaluating a small-team Python Agent stack.
- Added `server-v2/` as a clean-room FastAPI/LangGraph-oriented implementation. The first slice uses SQLite WAL and local artifacts instead of PostgreSQL/Redis, matching the small-team deployment decision.
- Implemented the V2 foundation: settings, API key auth, SQLite schema, DB-backed job queue, local artifact writes, Workspace/Run/Upload/Evidence/Timeline models, inline worker, stub Agent runtime, artifact download, and read-only MCP discovery placeholder.
- Added `server-v2/README.md` and `server-v2/SPEC.md`, and updated root README/SPEC to document the V2 exception to the Rust-first rule.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-16 Stable MCP Log Search Evidence

- Investigated `sess_1781622184222_1` / `task_1781622204486_5`: the uploaded openGemini node packages were extracted successfully and the real analyzer found query issues, but Claude Code later issued broad MCP searches such as `memory`, `at `, and Java exception names. The previous `logagent.search_logs` response returned only counts and `grep_results.json#matches/*` refs, so the model misread benign `lib/memory/memory.go` and ordinary English/Go log lines as JVM/OOM/NPE evidence.
- Changed task MCP `logagent.search_logs` to write each subsequent search to stable `log_searches/logsearch_*.json` artifacts, return matched line text, `keywordCounts`, `unmatchedKeywords`, and `log_searches/...#matches/<index>` refs, and stop overwriting the initial `grep_results.json`.
- Extended Claude Code / LLM final evidence validation to accept stable log search refs and reject unknown log search artifacts or out-of-range matches. Updated the Claude startup prompt to require inspecting `matches[].text` and to forbid inferring exception types or technology stacks from `totalMatches` alone.
- Updated root, Server, Log Analyzer, Analysis Agent, and LLM Gateway docs/specs.
- Verification passed: `cargo fmt`; focused tests for MCP stable search artifacts, LLM log search evidence refs, and Claude evidence rejection.

## 2026-06-16 Claude Code Session Timeout

- Increased the default and recommended `claude_code.max_session_seconds` from 120 seconds to 600 seconds so `PLAN_ANALYSIS` has enough time for Claude Code MCP evidence reads and tool calls on larger log packages.
- Updated example configs, deploy sample config, Agent Backend docs, Config docs, and the active 50992 runtime config.
- Verification passed: `cargo fmt --check`, `cargo check`, and `cargo test -p logagent-server support::config -- --nocapture`.

## 2026-06-16 Analyze Start With Pending Uploads

- Fixed Analyze Session run creation when a user selected log files and clicked `Start analysis` without first clicking the separate upload button. WebUI now uploads pending files, attaches the completed upload IDs to the selected Session, and only then creates the Session task run.
- Root cause observed in `sess_1781620388058_1`: the latest task snapshot had `uploadIds=[]`, `manifest.files=0`, and `analysis_package.task.uploadIds=[]`, so Claude Code correctly treated it as a question-only task and asked for more data.
- Updated WebUI README/SPEC to document that `Start analysis` auto-attaches pending selected files before creating the run.
- Verification passed: `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and the rebuilt static `webui/out` was synced to the running 50992 runtime directory for manual retest.

## 2026-06-16 Node Package Directory Entry Fix

- Fixed node log package extraction for tar archives that include `./` or other directory entries. The preprocessor now skips directory entries before path normalization and only classifies ordinary files, matching tarballs produced by `tar -czf package.tar.gz .`.
- Updated the node log package regression fixture to include an archive root directory entry, covering the real extraction failure observed during openGemini rotation upload testing.
- Tool Runner now treats matching materialized `tool_inputs` as exclusive automatic inputs; raw manifest/grep fallback is only used when no materialized input exists, preventing `influxql_analyzer` from being run against raw rotated `.log.gz` files after the preprocessor has generated JSONL.
- Documentation now states that top-level wrapper directories and directory entries are supported for node log packages.
- Verification covered the real openGemini rotation flow: deployment configs were set to `max-size=1m`, `max-num=8`, `max-age=7`, `compress-enabled=true`; tsbs load/query traffic generated rotated gzip logs; a node package with wrapper directory, `./` tar entry, active logs, and rotated gzip logs uploaded successfully; LogAgent extracted 16 files into node log groups, generated a 2,247-line `influxql_analyzer` JSONL input, and the real analyzer finished with `status=OK`.
- Local checks passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::log_analyzer -- --nocapture`, `cargo test -p logagent-server services::tool_runner -- --nocapture`, and `git diff --check`.

## 2026-06-16 Session Tarball Extraction Fix

- Fixed node log package preprocessing for Session tasks when uploaded `.tar.gz` archives contain a top-level wrapper directory. The path classifier now searches normalized archive components for `var/chroot/gemini/log/{tsdb,stream}` and `home/Ruby/log` instead of requiring those prefixes at archive root.
- Node log packages that match `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` but contain no supported log directories now fail EXTRACT with a clear error instead of producing an empty successful manifest.
- Added pipeline regression coverage for ordinary `.tar.gz` Session task extraction and wrapped node log package extraction, plus Log Analyzer coverage for unsupported-node-package failure.
- Updated root, Server, and Log Analyzer docs/specs.

## 2026-06-16 Analyze Session Create/Delete

- Fixed the Analyze “新建 Session” path by aligning newly created `AnalysisSessionRecord` values with the Session Store's supported schema version. The previous handler created `schemaVersion=2` while the store validator only accepted `schemaVersion=1`, causing `POST /api/sessions` to fail.
- Added protected `DELETE /api/sessions/:session_id`. It rejects running/waiting Sessions and Sessions with unfinished tasks, then removes only the Session JSON and Session timeline workspace.
- Preserved evidence by design: deleting a Session does not delete associated upload payloads, task records, task workspaces, result artifacts, Cases, or Memory entries.
- Added Analyze Session history delete controls with confirmation. Deleting the selected Session clears the draft/details, run list, timeline, artifacts and active task state before refreshing the Session list.
- Updated root, Server and WebUI docs/specs for Session deletion semantics and the create-session schema fix.

## 2026-06-16 Log Package Preprocessor

- Added Server-native preprocessing for node log packages named `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz`. Matching uploads now expand to `extracted/<nodeId>/<timestamp>/{tsdb,stream,agent}/` and record package/node metadata in `manifest.json`.
- Log rotation is handled by directory membership, not filename suffix. All ordinary files under the three supported log directories are kept; gzip content is detected by magic bytes so rotated files with arbitrary names can be searched and sliced.
- Generated analyzer-ready materialized inputs under `tool_inputs/`, including a shared `tool_inputs/index.json`, generic `log_text` JSONL, and `influxql_analyzer` JSONL containing only lines where a query can be extracted.
- Tool Runner selection now prefers `tool_inputs/index.json` entries for the current toolId before falling back to manifest file patterns and grep keywords. Manual configured tool runs can explicitly use safe `tool_inputs/...` paths.
- Added built-in runnable catalog tool `logagent.preprocess_log_package` so the same preprocessing can be run from WebUI Tools as a `tool_run` task and viewed as a JSON result artifact.
- Updated root, Server, WebUI, Log Analyzer, Tool Runner, Interfaces, and Security docs/specs.
- Verification passed: `cargo fmt --check`, `cargo check`, focused Log Analyzer tests, focused pipeline preprocessing test, focused Tool Runner materialized-input test, `cargo test -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`.

## 2026-06-16 Huawei OBS + GaussDB Package Sync Tool

- Added disabled-by-default `huawei_cloud.package_sync` Server config with OBS endpoint/bucket/object prefix, OBS credential env vars, optional security token env var, GaussDB host/port/database/user/password env var, `sslmode=disable`, and per-step timeout. Disabled configs do not read credential env vars; enabled configs fail startup on missing or empty secrets.
- Added built-in Tools catalog descriptor `logagent.huawei_cloud_package_sync` with `source=built_in`, `backend=huawei_cloud_package_sync`, `minFiles=maxFiles=1`, `exportable=false`, `editable=false`, and `runnable` tied to config.
- Added Huawei package sync execution service. Manual `tool_run` requires one completed upload, streams that raw snapshot file to Huawei OBS with signed PUT, executes user-provided GaussDB update SQL, performs OBS HEAD, executes GaussDB query SQL, and writes `tool_results/<action_id>/result.json`.
- Result artifacts include OBS PUT/HEAD status, object key/URL, GaussDB affected rows, bounded query rows, step timings, failed step/error, and credential environment variable names. They intentionally omit raw SQL and OBS/GaussDB secret values.
- Added object key validation, config validation tests, parameter validation tests, fake-client success result tests, and Tools API descriptor regression coverage.
- Updated root, Server, WebUI, Tool Runner, Interfaces, Security, Config docs/specs plus deploy and example configs for the disabled Huawei package sync section.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, `cargo test -p logagent-server huawei -- --nocapture`, `cargo test -p logagent-server http::tools::tests::pprof_tool_run_reuses_uploads_tasks_and_result_api -- --nocapture`, `cargo test -p logagent-server -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`.

## 2026-06-16 Fetch Endpoint MVP

- Added Server-managed Fetch endpoints behind a new `fetch` config section. Fetch is disabled by default; enabling it requires a 32-byte base64 secret key from `fetch.secret_key_env` and an explicit `fetch.allowed_hosts` HTTP/HTTPS allowlist.
- Added DevTools bash cURL import preview and endpoint CRUD APIs under `/api/fetch/*`. Import detects Authorization, Cookie, and token/api_key/secret/password/session-style query/body fields, stores sensitive values in an AES-256-GCM credential set under `storage.data_dir/fetch`, and returns only redacted previews.
- Added built-in runnable tool descriptor `logagent.fetch`, manual Fetch runs as `taskKind=tool_run`, bounded reqwest execution with request/response size limits, timeout, redirect revalidation, cross-host Authorization/Cookie stripping, controlled-header rejection, and response artifacts at `tool_results/<action_id>/result.json` plus `response_body.bin`.
- Added task MCP tools `logagent.list_fetch_endpoints` and `logagent.fetch`; the latter returns bounded response previews and final-answer evidence refs in the controlled format `tool_results/<action_id>/result.json#response`.
- Kept the personal read-only HTTP MCP boundary read-only: it may expose the `logagent.fetch` catalog descriptor through `logagent://tools/catalog` / `logagent.list_tools`, but `tools/call logagent.fetch` is rejected.
- Updated LLM final evidence validation so `#response` refs are accepted only for current-task actions whose result artifact is real and `tool=logagent.fetch`; unknown actions and non-Fetch actions are rejected.
- Added WebUI `Tools / Fetch` for cURL paste, redacted preview, save, enable/disable, delete, manual run, recent run polling, and response artifact viewing.
- Updated root, Server, WebUI, Tool Runner, Interfaces, Security, Config docs/specs, added `examples/server-fetch.yaml`, and synced this progress log.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server fetch -- --nocapture`, `cargo test -p logagent-server readonly_mcp_exposes_resources_and_tools_without_task_access -- --nocapture`, `cargo test -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`.

## 2026-06-16 Submodule Remote Guard

- Fixed `scripts/configure-tool-submodules.sh` so custom source-built analyzer submodule URLs can no longer rewrite the parent repository `origin`.
- Root cause: when a `third_party/<tool>` directory existed but the submodule was not initialized, `git -C <tool> rev-parse --git-dir` discovered the parent repository, so `remote set-url origin <submodule-url>` targeted the parent checkout.
- The script now only updates an existing submodule `origin` when that directory is its own initialized Git worktree; otherwise it only writes `submodule.<path>.url` in the parent repo local config for future `git submodule update`.
- Updated deployment and Tool Runner docs/specs to state that submodule URL overrides must not mutate `.gitmodules` or the top-level `origin`.
- Verification passed: shell syntax checks for `scripts/configure-tool-submodules.sh` and `scripts/build-tools.sh`, `configure-tool-submodules.sh --help`, `build-tools.sh --help`, temp-repo regression for an uninitialized submodule directory preserving parent `origin`, temp-repo regression for an initialized submodule updating only the child `origin`, and `git diff --check`.

## 2026-06-16 Metadata Compact Schema Type Codes

- openGemini raw JSON import now accepts both `Measurement.Schema` shapes: compact `{ "field": <typeCode> }` values and object `{ "field": { "Typ": <typeCode>, "EndTime": ... } }` values.
- Schema normalization resolves each field by first parsing the compact direct type code, including numeric strings, then falling back to object aliases `Typ` / `Type` / `type` / `typ`; `EndTime` remains available for object fields.
- The openGemini type-code mapping is unchanged, so `logagent.get_metadata_field_types`, `logagent.get_metadata_tag_fields`, and WebUI Schemas continue to display the same `Unknown/Integer/Unsigned/Float/String/Boolean/Tag/Unknown` labels.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::metadata -- --test-threads=1`, and `cargo test -p logagent-server -- --test-threads=1`.

## 2026-06-16 Analyze Language Preference

- Added WebUI language configuration for the Analyze workflow. The default is Simplified Chinese, with an English switch in the top bar; the selected language is saved in browser localStorage.
- Analyze now localizes fixed UI labels, status/phase/confidence badges, common timeline event labels, waiting prompts, approval controls, result headings, Case confirmation controls, and artifact panel labels. Technical terms such as `Session`, `Case`, `Claude Code`, `MCP`, `Metadata`, `Tool Runner`, `grep`, `artifact`, `evidence ref`, JSON keys, paths, and product names remain unchanged where translation would reduce precision.
- Added Server `analysisLanguage` persistence to Sessions and log-analysis tasks. Existing persisted JSON defaults to `zh-CN`; new task runs snapshot the Session language into `TaskRecord`, `analysis_state.json`, task MCP summary, and `analysis_package.json`.
- Claude Code startup prompts now include response-language instructions: `zh-CN` asks for Simplified Chinese natural-language fields, while `en-US` asks for English. Protocol names, paths, JSON keys, tool names, product names, and evidence refs remain raw.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`. Plain parallel `cargo test` exposed existing concurrency-sensitive fake CLI/tool timeout failures, so the single-thread run is the final test signal for this change.

## 2026-06-16 Source-built Domain Analyzer Tools

- Unified the four source-built analyzer submodules on Go 1.26: InfluxQL analyzer, Flux query analyzer, openGemini storage analyzer, and InfluxDB storage analyzer now have matching module/build/CI or developer documentation baselines. InfluxDB already had `go 1.26`; its Dockerfile and storage-analyzer README now match that baseline.
- Verification for the Go 1.26 bump passed for `go test ./...` in `third_party/influxql`, `scripts/build-tools.sh --only influxql`, `--only opengemini`, `--only influxdb`, Go module parsing for all updated modules, shell syntax checks, and `git diff --check`. A focused Flux Go package test (`go -C third_party/flux test ./stdlib/strings`) could not complete in this local environment because `libflux` pkg-config metadata (`flux.pc`) is not installed.
- Added `third_party/` submodules for source-referenced tools: `influxql` branch `influxql-analyzer` at `d76501446edb0520dddf88951c985dc62dc671de`, `flux` branch `feature/query-stats` at pushed commit `182ee355aff98074feb952ae92e6eb561bc72076`, `openGemini` branch `openGemini-tools` at pushed commit `9fb4d6feafcc9990504b3112182184dc41b1ea76`, and `influxdb` branch `influxdb-tools` at pushed commit `20ecb454036c40730236c174dee1067e3c6f5760`.
- Added `scripts/configure-tool-submodules.sh` and wired `scripts/build-tools.sh` to honor `LOGAGENT_SUBMODULE_BASE_URL` plus per-tool `LOGAGENT_SUBMODULE_*_URL` clone URL overrides before initializing analyzer submodules, so intranet deployments can use mirrored repositories without editing `.gitmodules`.
- `scripts/smoke-flux-query-analyzer.sh` now uses `scripts/build-tools.sh --only flux`, matching the other real-tool smoke scripts and inheriting the same submodule URL override behavior.
- Verification passed for the submodule override follow-up: shell syntax checks for the changed scripts, `configure-tool-submodules.sh --help`, `build-tools.sh --help`, temporary per-tool and `--base-url` overrides with local Git config/remote restoration, `scripts/build-tools.sh --only flux --output-dir target/tools`, full `scripts/build-tools.sh --output-dir target/tools`, `scripts/smoke-flux-query-analyzer.sh`, and `git diff --check`.
- Flux `query_stats` now supports LogAgent-friendly bounded stdout JSON via `--format json`, input/error caps, and real `--fail-on-parse-error`; stdout includes `summary/findings/topQueries/parseErrors`.
- openGemini now has `opengemini-storage-analyzer` for read-only TSSP and TSI mergeset inspection; InfluxDB 1.x now has `influxdb_storage_analyzer` for read-only TSM, TSI, and `_series` inspection.
- Added `scripts/build-tools.sh` to build all source-referenced tools into `target/tools/`, `$LOGAGENT_WORK_DIR/bin/tools/`, or deploy runtime `$LOGAGENT_APP_DIR/bin/tools/`; `scripts/build-server.sh` and `deploy/rebuild-install.sh` invoke it automatically.
- Server tool fixed `path` now supports `${ENV}` expansion, so deploy/runtime configs can point directly at `${LOGAGENT_APP_DIR}/bin/tools/...` without extra path env vars.
- Added dedicated example configs and smoke scripts for Flux, InfluxQL, openGemini storage, and InfluxDB storage analyzers; `scripts/smoke-product-loop.sh` now builds and uses the repo InfluxQL analyzer instead of `/usr/bin/influxql-analyzer`.
- Updated root, Server, Tool Runner, Domain Adapter, Config, Deployment, and Testing docs/specs for source-built analyzers, storage analyzer tool IDs, config paths, and verification flow.

## 2026-06-15 Metadata Tag Fields Tool

- Added built-in metadata tool `logagent.get_metadata_tag_fields` for Tools API, read-only HTTP MCP, and task stdio MCP.
- The tool requires `instanceId`, `database`, and `measurement`, accepts optional `retentionPolicy`, and intentionally does not support `field`; omitted RP uses the DB default RP exactly like `logagent.get_metadata_field_types`.
- Returned JSON reuses the field-types response contract but filters `fields` to Tag entries (`typ=6` / `typeLabel=Tag`), keeps `missingFields=[]`, and sets `finalEvidenceAllowed=false`.
- Manual no-upload Tools runs now persist `tool_run` results for the tag fields tool, and task MCP calls write background artifacts at `metadata_slices/tag_fields_<stable_id>.json` with audit entries in `mcp_calls.jsonl`.
- Updated Server, Metadata, Tool Runner, and root specs/docs to distinguish arbitrary field type lookup from tag-only field lookup.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, focused metadata/task-MCP/read-only-MCP/Tools API tests, `cargo test -p logagent-server -- --test-threads=1` after rerunning one transient Remote Executor test, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build`.

## 2026-06-15 Manual Tools JSON Template Run

- `/api/tools` descriptors now include `paramsTemplate`; WebUI Tools uses it to prefill editable JSON params for every runnable catalog tool.
- Configured command tools are now runnable from the Tools page when enabled. Manual runs prepare `extracted/`, `manifest.json`, and `grep_results.json`, then execute the configured whitelist args through the existing ToolRunner. Users can leave `inputFiles=[]` for match-rule selection or provide safe `extracted/...` paths.
- Built-in metadata tools remain read-only, non-editable, and non-exportable, but are now runnable from Tools without uploads. Results are persisted as `tool_run` artifacts and shown as JSON.
- `pprof_analyzer` keeps its specialized parsed top-table result display; other tool results use a generic JSON viewer.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, focused `http::tools`, `http::exports`, and `http::mcp_readonly` tests, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, `git diff --check`, and LAN auto deploy to `duzhiwang@192.168.31.128`.

## 2026-06-15 Built-in Tool Catalog Registration

- Registered the built-in metadata tools in the shared Tools catalog: `logagent.list_metadata_instances`, `logagent.get_metadata_snapshot`, and `logagent.get_metadata_field_types`.
- Extended Tool descriptors with `source`, `tags`, `readOnly`, `editable`, `exportable`, and `runnable`; configured tools use `source=configured`, while built-ins use `source=built_in`, are read-only, cannot be edited, and cannot be exported.
- `/api/tools`, read-only MCP `logagent://tools/catalog`, and `logagent.list_tools` now use the same descriptor view. The catalog only shows real configured tools plus built-ins; an unconfigured pprof template no longer appears as a fake configured tool.
- `tools.zip` remains limited to enabled configured executable tools, and tests assert built-in metadata tools are excluded from `tools-manifest.json`.
- WebUI Tools / Tool plugins now displays configured vs built-in tags, read-only status, exportability, and input schema; only `runnable=true` tools show the upload/run form.
- Updated Server, WebUI, and Tool Runner docs/specs for the catalog fields, built-in metadata tools, and export/run restrictions.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, focused `http::tools`, `http::exports`, and `http::mcp_readonly` tests, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, `git diff --check`, and local `/api/tools` sanity check with `examples/server-test.yaml`.

## 2026-06-15 LAN Auto Deploy

- Added `scripts/auto-deploy-lan.sh` and wired it into `scripts/build-all.sh`.
- On macOS, after local Server and WebUI builds complete, the helper pings `192.168.31.128`; when reachable it SSHes to `duzhiwang@192.168.31.128`, runs `git pull --ff-only` in the remote source tree, then runs the remote runtime `deploy/rebuild-install.sh` plus `logagentctl.sh start/status`.
- The helper can be disabled with `LOGAGENT_LAN_AUTO_DEPLOY=0` and supports overrides for remote address, SSH host, and runtime deploy directory.
- `deploy/rebuild-install.sh` and the LAN auto-deploy SSH payload now load `$HOME/.cargo/env` when present so rustup-installed cargo is available in non-interactive SSH shells.
- Runtime deploy scripts now best-effort load `$HOME/.bashrc` before deploy `.env`, matching the LAN server where LogAgent runtime environment variables live in `/home/duzhiwang/.bashrc`.
- Deployment docs/specs were updated with the LAN auto-deploy behavior.
- Verification passed: `bash -n scripts/auto-deploy-lan.sh`, `bash -n scripts/build-all.sh`, `LOGAGENT_LAN_AUTO_DEPLOY=0 scripts/auto-deploy-lan.sh`, and `git diff --check`.

## 2026-06-15 Metadata Raw Refresh And Delete

- Stopped local `logagent-server` processes, including the existing screen-managed local service, before changing the code.
- Added protected metadata instance maintenance APIs: `POST /api/metadata/instances/:instance_id/refresh` rebuilds the stored openGemini snapshot from its saved `rawSnapshot`, and `DELETE /api/metadata/instances/:instance_id` removes the instance plus its non-shared cluster/node records.
- Metadata import confirmation now treats `instanceId` as the overwrite boundary. Re-importing the same instance replaces the old snapshot and clears stale cluster/node records instead of leaving residual topology behind.
- System Context / Metadata WebUI now has a `Raw JSON 刷新` button and per-instance delete controls. Successful refresh/delete updates the imported list and right-side snapshot state.
- Updated Server, WebUI, Metadata module, and root specs/readmes for Raw JSON refresh, deletion, and duplicate InstanceID overwrite behavior.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::metadata -- --test-threads=1`, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-15 Metadata Schema Field Type Display Follow-up

- Fixed the Metadata Schemas type renderer to resolve field type codes from `typ`, `type`, `Typ`, or `Type` and coerce numeric strings before mapping labels.
- Updated the openGemini display mapping to match `FieldTypeName`: `0 Unknown`, `1 Integer`, `2 Unsigned`, `3 Float`, `4 String`, `5 Boolean`, `6 Tag`, and `7 Unknown`.
- Metadata JSON template import now accepts `Typ`, `Type`, and `type` aliases for field type codes so newly imported schemas do not persist empty field types.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::metadata -- --test-threads=1`, `cargo test -p logagent-server mcp -- --test-threads=1`, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-15 Metadata Field Type Lookup Tool

- Added built-in MCP tool `logagent.get_metadata_field_types` for task stdio MCP and read-only HTTP MCP.
- The tool requires `instanceId`, `database`, and `measurement`; `retentionPolicy` is optional and defaults to the DB default RP, while `field` may be omitted, a single string, or an array of field names.
- Returned field entries include raw `typ`, mapped `typeLabel`, and `endTime`; the openGemini mapping is `0 Unknown`, `1 Integer`, `2 Unsigned`, `3 Float`, `4 String`, `5 Boolean`, `6 Tag`, and `7 Unknown`.
- Task MCP lookups write `metadata_slices/field_types_<stable_id>.json`, audit the call in `mcp_calls.jsonl`, and remain background context only.
- Verification passed: `cargo fmt --check`, `cargo check`, focused metadata/MCP/read-only MCP tests, `cargo test -p logagent-server -- --test-threads=1`, and `git diff --check`.

## 2026-06-15 Metadata Schema Field Type Mapping Fix

- Fixed Metadata Schemas field type display by routing the table through a dedicated openGemini field type mapping helper.
- The WebUI mapping is now explicit for `Field_Type_Unknown=0`, `Field_Type_Int=1`, `Field_Type_UInt=2`, `Field_Type_Float=3`, `Field_Type_String=4`, `Field_Type_Boolean=5`, `Field_Type_Tag=6`, and `Field_Type_Last=7`.
- Updated WebUI and Metadata docs/specs with the exact enum-to-label mapping.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build`.

## 2026-06-15 System Context Skill Import

- Added protected `POST /api/skills/imports` for Markdown Diagnostic Skill imports. The API writes `<skillId>/SKILL.md` and default `logagent.json` under the first configured `skills.roots`, reloads the Skill Registry snapshot, and returns the imported Skill detail.
- Skill Registry now uses a reloadable in-memory snapshot so Skills list/detail, Analyze Skill resolve, read-only MCP, and `skills.zip` use imported Skills without restarting Server. Reload failure rolls back the newly created import directory and keeps the old snapshot.
- System Context / Skills WebUI now has an Import control beside Refresh. Users can choose `.md/.markdown` files or paste Markdown, frontmatter can prefill name/description, and successful imports refresh the list, select the new Skill, and close the form.
- Updated Server, WebUI, Skills and System Context docs/specs with the import API, storage location, generated manifest defaults, v1 limitations, and UI behavior.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::skill_registry -- --test-threads=1`, `cargo test -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-15 WAITING_FOR_USER Finalize Button Fix

- Fixed the Analyze task execution `WAITING_FOR_USER` finalize path: the parent callback now preserves the child component's `resumeMode: "finalize"` argument instead of dropping it and falling back to `continue`.
- The finalize button label now matches the product wording `没有更多信息，直接生成最终结果`, and waiting interaction buttons explicitly use non-submit button semantics.
- Verification passed: `cd webui && npm run lint`, `npm run typecheck`, and `npm run build`.

## Status Summary

LogAgent MVP is now framed as a diagnostic evidence workbench and Claude Code domain enhancement layer. The self-built analysis action loop is no longer the running analysis path; `PLAN_ANALYSIS` prepares LogAgent evidence/MCP artifacts, starts or resumes Claude Code, and persists Claude session outcomes, MCP calls, waiting state and final evidence.

Current runnable loop:

```text
Chrome Extension, WEBUI upload, or WEBUI question-only Session
  -> optional Native Agent or Server upload API
  -> persisted QUEUED task and raw/text snapshot
  -> bounded background extraction / manifest
  -> simple grep evidence
  -> optional rule-based Tool Runner evidence
  -> analysis_state.json / analysis_events.jsonl audit snapshot
  -> analysis_package.json / claude_prompt.md / claude_mcp_config.json
  -> Claude Code CLI --print --output-format json --json-schema --mcp-config with short stdin prompt
  -> Claude reads full evidence package through task MCP analysis_package resource
  -> LogAgent MCP resources/tools for logs, slices, tools, Metadata, Cases, prompts and approvals
  -> claude_session.json / mcp_calls.jsonl / agent_response.json
  -> completed final answer, WAITING_FOR_USER, WAITING_FOR_APPROVAL, or PLAN_ANALYSIS failure
  -> persisted result and WEBUI display
  -> optional human confirmation or LLM-assisted text import into local Memory-backed Case Store
```

Current manual Tools loop:

```text
WEBUI Tools
  -> upload pprof profile through existing upload API
  -> create persisted taskKind=tool_run task
  -> background RUN_TOOL execution
  -> go tool pprof top/tree/raw artifacts
  -> /api/tools/runs polling and result display
```

Current Remote Executor loop:

```text
WEBUI Tools / Executors
  -> create or edit ECS executor records in Server JSON store
  -> select a remote_execution command template
  -> create taskKind=remote_command_run
  -> background EXECUTE_REMOTE_COMMAND phase calls system ssh with BatchMode=yes
  -> remote_command/result.json, stdout.txt, stderr.txt
  -> /api/executor-runs polling and result display
```

## Implemented

### Remote Executor Framework

- Added `remote_execution` Server config with system `ssh` path, host key policy, connect/command timeouts, output cap, and command templates. Empty command config defaults to `smoke_ls_root`, which runs `ls -la /root`.
- Added WebUI-managed ECS executor records under `storage.data_dir/executors`, plus protected APIs for executor CRUD, command template discovery, remote run creation, run polling, and result reads.
- Added `taskKind=remote_command_run`, `source=remote_executor`, and `EXECUTE_REMOTE_COMMAND` phase. Remote command runs reuse TaskStore, workspaces, background executor recovery, and write `remote_command/{result.json,stdout.txt,stderr.txt}`.
- Added `Tools / Executors` WebUI subpage for executor create/edit/disable, template selection, SSH run creation, run history polling, stdout/stderr preview, and artifact paths. The UI does not expose free-form shell command input.
- Updated Server, WebUI, Environment Collector, Config, Testing, Deploy, root docs/specs, and example configs. Full Environment Collector SCP collection and Analysis Agent approval mapping remain follow-up work.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1` (native 1 + server 129 tests), `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and direct low-risk SSH smoke `ssh root@112.74.50.120 ls -la /root`.

### Architecture Review

- Added `docs/architecture_review.md` with a current real architecture diagram, module-by-module implemented capability inventory, README/SPEC/roadmap gap analysis, and the top 10 extension-blocking architecture issues.
- The review cites concrete code evidence by file and function, including the task executor, task/stdout MCP tools, final evidence validation, Metadata fetch/store behavior, Native Agent local boundary, Tool Runner/Tools split, and WebUI evidence navigation.
- Reorganized the path from the current state to a complete MVP loop into P0/P1/P2 tasks, marking which tasks are suitable for Codex Implement mode and which require human environment/configuration input.
- Added a suggested 2-week development rhythm focused first on evidence contract stability, action budget guardrails, failed artifact observability, tool platform cleanup, security hardening, Code Evidence, and Environment Collector.
- Verification: docs-only change; `git diff --check` passed.

### Claude CLI Prompt Delivery

- Changed Claude Code session runner prompt delivery from a full prompt argv argument to a short `claude_prompt.md` artifact piped through stdin.
- Stopped inlining `analysis_package.json` into the Claude startup prompt. Claude now reads the full task evidence package through the task-scoped stdio MCP `analysis_package` resource.
- Added `promptDelivery` metadata to `claude_session.json` and `agent_response.json`, recording `mode=stdin_file`, `promptPath=claude_prompt.md`, prompt byte size and `largeContextVia=mcp_resource`.
- Added task MCP `analysis_package` resource backed by workspace `analysis_package.json`; existing permission profiles remain unchanged, so `diagnose` still does not need native file `Read`.
- Kept Claude CLI as the production backend for this change. Claude Agent SDK remains a later adapter PoC option for SDK streaming/hooks/session runtime needs; it is not required to solve oversized prompt delivery.
- Verification: `cargo fmt --check`, `cargo check`, focused `cargo test -p logagent-server agent_backend -- --test-threads=1`, focused MCP resource test, `cargo test -p logagent-server -- --test-threads=1`, and workspace `cargo test -- --test-threads=1` pass.

### Dual Entry: WebUI Analyze + Read-only HTTP MCP

- Renamed the primary WebUI navigation entry from `Log Analysis` to `Analyze`; the underlying workflow remains Session-first and continues to use the Server machine's Claude Code, task-scoped stdio MCP config and Server local workspace.
- Added protected read-only HTTP MCP at `POST /api/mcp/readonly` for personal local Claude Code knowledge access. It supports `initialize`, `resources/list`, `resources/read`, `tools/list` and `tools/call`.
- Read-only MCP resources now cover Skills, Metadata instances/snapshots, recent Cases, Tools catalog and Domain Adapters. Read-only tools cover case search/get, skill list/get/reference, System Context preview, metadata list/snapshot, tool catalog and domain adapter list.
- The read-only MCP is deliberately separated from `logagent-server mcp --task-id ...`: it does not read task workspaces, start or resume Sessions, upload files, request approval, run Tool Runner, SSH/SCP, or mutate Case/Metadata/Skills/System Context.
- Added protected `GET /api/exports/skills.zip`; it packages all indexed Skill ordinary files, keeps relative Skill directory structure, writes `manifest.json`, and skips symlinks.
- Added protected `GET /api/exports/tools.zip`; it packages enabled executable tool binaries for the Server OS/arch, wrappers, sanitized config examples and `tools-manifest.json`; missing, non-file, non-executable or unreadable tools are marked skipped without failing the download.
- Added Settings `Personal Claude Code` block showing the read-only MCP URL, Authorization header hint, Claude Code HTTP MCP config example, and authenticated download buttons for Skills and Tools packages. It intentionally does not install, bootstrap or write local Claude Code configuration.
- Updated root, Server, WebUI, Skills, System Context, Tool Runner, Metadata, Case Store, Domain Adapter, Interfaces, Deployment and Security docs/specs.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass. A default parallel `cargo test -p logagent-server` run hit existing timeout-style failures, while the same server suite passed with `--test-threads=1`. Vite HTTP smoke returned `/`; in-app Browser smoke could not run because the `iab` browser was unavailable in this session.

### Skill-backed System Context

- Reworked System Context semantics from editable long-lived Prompt Pack/Runbook/Architecture resources into task-level Skill-backed background snapshots.
- Added Server `SkillRegistry` with `skills` config (`enabled`, `roots`, `max_skill_chars`, `max_reference_chars`), defaulting to repository `skills/`.
- Added Codex-compatible initial Skills: `opengemini-diagnosis`, `influxql-analysis`, and `pprof-diagnosis`, each with `SKILL.md`, optional `logagent.json`, and declared `references/`.
- Added protected Skills APIs: `GET /api/skills`, `GET /api/skills/:skill_id`, and `POST /api/skills/preview`.
- Added `AnalysisSessionRecord.skillIds`; `systemContextIds` remains deserializable for old Sessions but no longer injects old non-Metadata resources into new tasks.
- Log Analysis task creation now writes `system_context.json` schema v2 containing selected/auto-matched `diagnostic_skill` items plus Metadata adapter summaries.
- Added MCP `logagent.get_skill_reference`, restricted to references declared by Skills already snapshotted in the current task; reads write `skill_references/<stable_id>.json` background artifacts.
- Final result evidence validation now explicitly rejects `system_context.json`, `diagnostic_skill`, and `skill_references/*` as root-cause evidence refs.
- WebUI `System Context` page now has `Skills` and `Metadata` tabs; Log Analysis draft uses a Skill picker backed by `/api/skills` and persists `skillIds`.
- Updated root, Server, WebUI, System Context, Config, and new Skills docs, plus example configs.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass. Added focused Rust coverage for task Skill snapshots and MCP Skill reference artifacts. Local smoke with `examples/server-test.yaml` returned HTTP 200 for `/`, listed the three initial Skills from `/api/skills`, and previewed selected plus auto-matched Skills via `/api/skills/preview`.

### Server Observability

- Added default Server tracing when `RUST_LOG` is unset: `logagent_server=info,tower_http=info`, with logs written to stderr so MCP stdio stdout remains JSON-RPC only.
- Configured HTTP trace logging for request/response/failure summaries and changed `AppError` logging so 4xx/409 responses are warnings while 5xx responses are errors.
- Added lifecycle logs for uploads, Sessions, Tasks, Tool runs, Case imports, Metadata imports, System Context writes, Executor phase transitions, waiting-state resumes, Claude Code sessions, MCP calls, Tool Runner execution/reuse, and pprof analyzer runs.
- Added focused English comments around stderr logging for MCP safety, error-level mapping, persisted phase recovery, MCP waiting marker handoff, and Tool Runner idempotent artifact reuse.
- Updated Server README/SPEC with logging defaults, level policy, covered events, and sensitive-data exclusions.
- Verification: `cargo fmt --check`, `git diff --check`, `cargo check -p logagent-server`, and `cargo test -p logagent-server` pass.

### Claude Code MCP Session Runner

- Replaced the running `PLAN_ANALYSIS` action-loop path with Claude Code session orchestration. The executor now writes `analysis_package.json` and `claude_mcp_config.json`, invokes Claude Code, validates completed final evidence refs, and persists waiting user/approval requests from MCP markers.
- Added `claude_code` config, `mcp` config, `analysisMode=diagnose|code_investigation|fix`, mode-specific permission profiles, and the `logagent-server mcp --config <path> --task-id <task_id> --mode <mode>` stdio subcommand.
- Added LogAgent MCP resources and tools for task/artifact context, manifest/grep, metadata, system context, case context, tool results, log search, log slices, domain tool execution, case recall, metadata topology, user input requests and approval requests. MCP calls append to `mcp_calls.jsonl` and evidence-producing tools write workspace artifacts.
- Redefined `agent_response.json` as a Claude Code session response with `runtimeStatus`, `claudeSessionId`, `analysisMode`, `permissionProfile`, `structuredOutput`, usage/cost, MCP call path, native tool policy, duration, error and stdout preview. Added `claude_session.json` for resume metadata.
- Removed `agent_request.json` from the task artifacts API and WebUI artifact model. The old AgentDecision parser helpers are test-gated in LLM Gateway; normal runtime validation uses final-answer evidence validation directly.
- Updated sample/deploy configs to use `LOGAGENT_CLAUDE_CODE_PATH`, `claude_code`, and `mcp.transport=stdio`; updated root, Server, WebUI, deploy and module README/SPEC docs for the Claude Code MCP architecture.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, and `npm --prefix webui run build` pass.

### Agent Contract Artifacts

- Added `claude_agent_sdk` as an Agent Backend type with `agent_sdk_adapter` execution mode; enabled external adapters still require an absolute configured command path, and Settings diagnostics only inspect the path without invoking the adapter.
- Log Analysis `PLAN_ANALYSIS` now refreshes `analysis_package.json`, `agent_request.json`, and `agent_response.json` in the task workspace before each internal decision call. The package freezes task input, manifest, grep, Metadata, System Context, Case context, Tool results, analysis state summary, and Server execution boundaries.
- `agent_response.json` is currently an explicit `not_invoked` placeholder: the production decision loop still uses `internal_llm`, and the contract artifacts prepare the next Claude Agent SDK adapter PoC without changing the running analysis path.
- Task artifacts API now returns optional `analysisPackage`, `agentRequest`, and `agentResponse`; WebUI successful task details show an Agent contract panel with backend, execution mode, runtime status, and artifact paths.
- Updated sample and deploy configs with disabled `claude_agent_sdk` entries and optional `LOGAGENT_AGENT_CLAUDE_SDK_PATH`.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, and `npm --prefix webui run build` pass.

### Agent Backend And Domain Adapter Direction

- Reframed LogAgent as a diagnostic evidence workbench that can call mature agent backends instead of trying to replace Codex, Claude Code or OpenCode with a fully self-built general agent loop.
- Added Server `agent_backends` config with default `internal_llm` and reserved `claude_agent_sdk`, `codex_cli`, `claude_code_cli` and `opencode_cli` backend types.
- Added protected Settings APIs:
  - `GET /api/settings/agent-backends`
  - `POST /api/settings/agent-backends/:backend_id/test`
  - `GET /api/settings/domain-adapters`
- Agent backend diagnostics are first-stage dry-run checks: `internal_llm` returns ready; external adapters validate configured command paths but do not execute the adapter.
- Added an in-process Domain Adapter registry with active `opengemini_influxdb` and skeleton `cassandra` / `rocksdb` adapters.
- Extended WebUI Settings to show LLM diagnostics, Agent Backend summaries/dry-run results, and Domain Adapter status.
- Added Agent Backends and Domain Adapters module docs, and updated root, Server, WebUI, config, interfaces, security, analysis, LLM Gateway, Tool Runner, Metadata, Code Evidence, Environment Collector, System Context and Roadmap docs for the new direction.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test` (106 server tests plus native-agent test pass), `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass. Local Vite dev server started successfully, but in-app Browser visual smoke could not run because no browser instance was available.

### Settings LLM Diagnostics

- Added protected Server Settings APIs for LLM diagnostics: configuration summary, model list test and simple chat message test.
- OpenAI-compatible model list diagnostics call the provider `/models` endpoint; stub and binary providers report the configured model.
- Diagnostic model/chat calls return `{ok,result,error}` so WebUI can print provider HTTP, auth, rate-limit, network, timeout or decode exceptions directly.
- Added a WebUI `Settings` top navigation page with LLM configuration summary, model list fetch test, simple message send test and raw success/error output panels.
- Updated WebUI, Server and LLM Gateway docs/specs for the new Settings page and APIs.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, local Vite HTTP smoke, and stub LLM Settings API smoke pass; in-app Browser was unavailable for visual verification.

### Metadata Page Usability Refinements

- Raw JSON now renders as a lazy expandable tree instead of stringifying and rendering the full snapshot on initial page load.
- Imported Instances can collapse into a narrow rail and expand back without losing selection.
- Metadata Explorer now combines the former topology and database detail entry points into `Node / DBPT / Shards` and `DB / RP / Shards / Indexes` views.
- Schemas now defaults to the first non-`_internal` database and its first retention policy, keeps RP options scoped to the selected DB, and renders field type codes as names.
- Nodes now display MetaNode status as `none` and map Data/SQL node status codes to `none`, `alive`, `leaving`, `left`, and `failed`.
- Verification: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and local Vite HTTP smoke pass; in-app Browser was unavailable for visual verification.

### Sticky Table Headers

- Metadata reusable tables now use bounded local scrolling with sticky headers, so Nodes, Partitions, Explorer shard/index rows and Schemas field rows keep column meanings visible while scrolling.
- Tools pprof top table now uses the same sticky-header scrolling behavior.
- Updated WebUI and Metadata module docs/specs for the long-table browsing behavior.
- Verification: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and local Vite HTTP smoke pass; in-app Browser was unavailable for visual verification.

### Metadata Cascading Views

- Removed the WebUI Metadata graph rendering path and the `@xyflow/react` dependency; Topology is now rendered as a cascade instead of a React Flow graph.
- Topology now expands as `Database -> DataNode -> DBPT -> Shards`, with each Shard row showing RP, ShardGroup, time range, Owners, IndexID and Index status information.
- Databases now expands as `Database -> RP -> ShardGroup/IndexGroup -> Shard/Index`, so large clusters no longer render every detail table at once.
- Schemas now requires Database, RP, Measurement or field filtering before results render, preventing all schemas from being laid out by default.
- Updated WebUI and Metadata module docs/specs for the cascading topology, database and schema views.
- Verification: `cd webui && npm run typecheck`, `cd webui && npm run lint`, `cd webui && npm run build`, and `git diff --check` pass.

### Metadata Topology Explorer

- WebUI Metadata Topology no longer builds a full graph by default. It now derives a topology index and starts with an abnormal-first PT overview table.
- The overview groups by DataNode / Database / PT and shows ShardGroup, Shard, IndexGroup, Index, diagnostic count, owner and time range summaries.
- The right-side details panel supports aggregate PT details.
- Updated WebUI and Metadata module docs/specs for the overview-first topology behavior.
- Verification: `cd webui && npm run typecheck`, `cd webui && npm run lint`, `cd webui && npm run build`, and `git diff --check` pass; in-app Browser was unavailable for visual verification.

### Metadata Import UX

- WebUI Metadata tab now exposes three explicit import paths in the management panel: realtime openGemini `/getdata` URL loading, `.json` file upload, and manual JSON text paste.
- Realtime loading keeps the existing readonly snapshot behavior and still requires a user-provided InstanceID for openGemini metadata.
- JSON file and manual JSON text imports call `/api/metadata/imports` with `templateType=json`, then require preview and confirmation before writing to the Metadata Store.
- Full Metadata JSON templates can be previewed without an InstanceID; raw openGemini JSON still requires InstanceID so the Server can normalize it into the instance-keyed topology model.
- Updated WebUI and Metadata module docs/specs for the three import modes.
- Verification: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check` pass.

### Memory-backed Case Store

- Added Server `MemoryStore` backed by `storage.data_dir/memory/memory.sqlite` with `memory_items`, `memory_chunks`, and FTS5 `memory_chunks_fts`.
- Converted `CaseStore` into a compatibility facade over Memory while preserving existing `/api/cases*`, `CaseRecord`, `CaseSearchHit`, `case_context.json`, and `case_context.json#cases/<index>` evidence refs.
- Server startup now idempotently imports legacy `storage.data_dir/cases/*.json` by `caseId` into SQLite. Legacy JSON files remain untouched as migration/rollback source, and create/update still syncs JSON.
- Case recall now filters `memoryType=case`, active status, and enabled state, then merges SQLite FTS/BM25 scores with existing keyword-overlap scoring. If FTS is unavailable, recall falls back to keyword overlap.
- Added disabled embedding config shape (`embedding.enabled/provider/model/api_key_env/store`) for future vector retrieval without changing API contracts.
- WebUI now defaults to `Log Analysis` and top navigation order is `Log Analysis`, `Memory`, `System Context`, `Tools`; the old top-level Metadata path is removed and Metadata remains reachable inside System Context.
- The former Cases page is presented as `Memory` while keeping Case terminology inside confirmed fault-case forms.
- Added Memory module docs and updated Server/WebUI/Case Store docs for storage, migration, recall, and UI naming.
- Verification so far: focused `cargo test -p logagent-server stores::case_store -- --nocapture` and `cargo check -p logagent-server` pass.

### Deploy Template Refresh

- Updated `deploy/logagent.example.yaml` with the default disabled `embedding` block so runtime deployments match the current Server config shape.
- Added optional `LOGAGENT_EMBEDDING_API_KEY` documentation to `deploy/.env.example`.
- Added `deploy/install-deps.sh` to install common source rebuild dependencies on macOS/Homebrew and common Linux package managers; SQLite remains bundled with the Server binary and is not installed separately.
- Updated `deploy/logagentctl.sh` and `deploy/rebuild-install.sh` to pre-create expected runtime data directories, including `data/memory`, `data/cases`, and `data/case_imports`, without deleting existing data.
- Updated deploy README and deployment module docs to document `data/memory/memory.sqlite`, legacy Case JSON rollback files, and the current WebUI navigation.

### Legacy System Context (superseded by Skill-backed System Context)

- Added Server `SystemContextStore`, persisted under `storage.data_dir/system_context/resources`, for versioned Prompt Pack, Architecture Doc, Runbook, Glossary, Tool Capability and Knowledge Note resources.
- Added protected System Context API: list/create/get/patch resources, add/patch/activate versions, and prompt preview.
- Metadata is now included under System Context through a read-only `metadata_instance` adapter while existing `/api/metadata/*` APIs and topology models remain unchanged.
- `AnalysisSessionRecord` now persists `systemContextIds`; Log Analysis task creation resolves explicit Session selections, enabled matching resources and Metadata adapter summaries into `workspaces/<task_id>/system_context.json`.
- `TaskRecord` and artifacts responses now expose `systemContextPath` / `systemContext`; Session timeline records `system_context_recorded`.
- LLM Gateway now injects System Context as background reference for both final-result generation and action decisions; System Context cannot be used as final result evidence refs.
- WebUI now has a top-level `System Context` page with resource library, version management, Prompt preview, Architecture Mermaid source preview and embedded Metadata tab; Log Analysis Session draft can select System Context resources and displays the frozen snapshot after a run.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass.

This behavior is retained only for legacy data/API compatibility. New tasks and the current WebUI use the Skill-backed System Context flow documented above.

### Runtime Deploy Template

- Added repository `deploy/` runtime template copied from `/home/duzhiwang/workspace/data/prd_assistant/deploy`: README, `.env.example`, `logagent.example.yaml`, `logagentctl.sh`, and `rebuild-install.sh`.
- Real runtime secrets/config remain excluded: `deploy/.env` and `deploy/logagent.yaml` are ignored.
- Deployment scripts now auto-load same-directory `.env`; `logagentctl.sh` starts Server detached so non-interactive script execution can keep the process alive.
- Verification: `bash -n deploy/logagentctl.sh`, `bash -n deploy/rebuild-install.sh`, and `git diff --check` pass.

### Session-first Log Analysis

- Log Analysis 公开入口从一次性 task 改为可恢复 Session。
- 新增 Server `AnalysisSessionStore`，持久化到 `storage.data_dir/sessions/<session_id>.json`，Session events 追加到 `storage.data_dir/session_workspaces/<session_id>/session_events.jsonl`。
- 新增受保护 Session API：`POST/GET /api/sessions`、`GET/PATCH /api/sessions/:session_id`、`POST /api/sessions/:session_id/uploads`、`DELETE /api/sessions/:session_id/uploads/:upload_id`、`POST /api/sessions/:session_id/tasks`、`GET /api/sessions/:session_id/timeline`。
- `TaskRecord` schema 增加 `sessionId`；`log_analysis` task 必须绑定 Session，`tool_run` 不绑定 Session。
- 每次从 Session 启动分析都会创建新的 `log_analysis` task workspace 快照，Session 记录 `taskIds`、`activeTaskId` 和状态。
- Task 状态进入 running、waiting、succeeded、failed 时会同步 Session status，并追加 timeline event。
- Task 创建时继续固化 `metadata_context.json` 和 `case_context.json`，同时向 Session timeline 记录 Metadata summary 和 Case recall count。
- WebUI `Log analysis` 页面改为 Session-first：左侧 Session history，右侧 Session draft editor、upload attach、Start analysis、runs panel 和 unified Evidence timeline；草稿字段 debounce PATCH 到 Server。
- WebUI 选择 Session 时 best-effort 调用本机 Native Agent `PUT /workspace/current` 设置活动 Session，失败只提示不阻断 WebUI 上传。
- Native Agent 新增 `native_agent.state_path`，默认 `~/.logagent/native-agent-state.json`，并提供 `GET/PUT/DELETE /workspace/current`。
- Native Agent `POST /imports` 上传后附加到活动 Session；没有活动 Session 时自动创建 `Native import <filename>` Session 并设为活动；返回 `{uploadId, sessionId, taskId:null, url}`。
- Chrome Extension 成功通知改为 `LogAgent session updated`。
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass after implementation.

### Text-only Log Analysis

- Log Analysis Session 现在支持不上传日志直接分析，只填写 Session 问题即可点击 `Start analysis`。
- Server `POST /api/sessions/:session_id/tasks` 和兼容 `POST /api/tasks` 在绑定 `sessionId` 后允许 `uploadIds=[]`；有上传时仍校验上传存在且为 `COMPLETE`。
- Text-only task 持久化 `uploadIds=[]`、`inputs=[]`，创建空 `raw/` 和 `extracted/`，并生成 `session_text_input.json`、`manifest.json` / `grep_results.json`。其中 `manifest.uploads`、`manifest.files` 和 grep `matches` 为空。
- Session timeline 会记录 `text_input_recorded`，用于区分只来自问题文本的 run；`session_text_input.json#question` 可作为最终结果 evidence ref。
- WebUI `Log analysis` 的 `Start analysis` 不再依赖已附加上传，Session draft 中标明 uploads optional；成功 artifacts 会展示 Session text input 并支持该 evidence ref 跳转。
- Verification: focused Rust regressions, `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass.

### Log Analysis Collapse UX

- WebUI `Log analysis` 的 `Session draft` 现在支持展开/收起；新建空 Session 默认展开，已有 run 的 Session 默认收起，点击 `Start analysis` 创建 run 后会自动收起。
- Session draft 收起态展示 title、question、source URL、Metadata 绑定、upload/run 数量和 session 状态摘要，避免运行中占用主要视野。
- Unified Evidence Timeline 现在支持展开/收起；运行中 run 默认展开，切换到历史终态 run 或当前 run 到达 `SUCCEEDED` / `FAILED` 后自动收起。
- Timeline 收起态只展示最终结果 summary、confidence、失败 phase/message，或运行中的当前状态和最近事件；用户仍可手动展开查看完整 timeline。
- Verification: `npm run lint`, `npm run typecheck`, and `npm run build` pass in `webui/`.

### Task Alias Naming

- 成功的 Log Analysis task 现在持久化 `alias` 字段；新写入的 Log Analysis task 使用 schemaVersion 7，tool_run 使用 schemaVersion 6，旧 task 缺少 alias 时仍可读取。
- Task alias 在最终结果写入后由 LLM Gateway 静默生成，输入为用户问题、最终结果、manifest 和 Metadata 摘要；命名调用不写入 `analysis_events.jsonl`，也不追加 Session timeline event。
- alias schema 错误会重试一次；Provider 或 schema 最终失败时 Server 使用最终 summary/question 生成短标题，避免命名失败影响 task 成功状态。
- WebUI Runs、timeline 收起态和 Case 确认区优先展示 alias；没有 alias 时用状态/时间回退，不再把裸 `task_...` 当主要显示名称。
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass.

### Case Evidence Ref Normalization

- 修复线上 Session `sess_1781100427508_1` 中 `task_1781103906266_1` 的 `PLAN_ANALYSIS` 失败原因：模型把历史 Case 输出为 `历史案例 case_1781027802189_1`，旧校验无法映射该 evidence ref。
- LLM Gateway 现在在 Prompt 中给历史 Case 标注 `case_context.json#cases/<index>`，并把模型输出的 `case_<id>` 或“历史案例 case_<id>”规范化为当前 task `case_context.json` 中的 canonical ref。
- 最终结果允许引用 `case_context.json#cases/<index>`；未知 Case、缺失 case context 或越界 index 仍会拒绝。
- WebUI 现在支持点击 `case_context.json#cases/<index>` 跳转到对应 Case context 条目。
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass; deployed to `/home/duzhiwang/workspace/data/prd_assistant`, rebuilt Server/WebUI, restarted with deployment env, and `logagentctl.sh status` returned `{"status":"ok"}`.

### WebUI Naming

- Renamed the top bar product title from `LogAgent Metadata Console` to `LogAgent Analysis Workbench`.
- Updated the subtitle to describe the broader WebUI scope: evidence, metadata, tools, and case workflow.
- Verification: `npm run lint`, `npm run typecheck`, and `npm run build` from `webui/` all pass.

### Repository Structure

- Root directory now keeps only runnable components and engineering support directories: `server/`, `native-agent/`, `chrome-extension/`, `webui/`, `examples/`, `scripts/`, and `testing/`.
- Former planning-only capability directories were moved under `docs/modules/`, including Log Analyzer, Tool Runner, Metadata, Analysis Agent, LLM Gateway, Case Store, Code Evidence, Environment Collector, Config, Interfaces, Security, Deployment, and Roadmap.
- Server remains a single Rust crate. Internal code is now organized by layer:
  - `http/` for routes and handlers.
  - `domain/` for shared DTOs and Action/Evidence contracts.
  - `stores/` for JSON-backed persistence.
  - `services/` for Log Analyzer, Tool Runner, Metadata, LLM Gateway, and Tools plugin implementations.
  - `pipeline/` for task phase handlers and the recoverable executor.
  - `support/` for config, auth, errors, IDs, and path safety.
- The refactor is structure-only: HTTP API paths, config keys, task/upload/case JSON schema, and workspace artifact paths remain unchanged.
- Verification: `cargo fmt --check`, `cargo check`, and `cargo test` all pass after the module move.

### Runtime Scripts

- Added `scripts/init-workdir.sh`, `scripts/build-server.sh`, `scripts/build-webui.sh`, `scripts/build-all.sh`, and `scripts/server-service.sh`.
- All new workdir scripts require `LOGAGENT_WORK_DIR`; if it is unset or empty they fail before writing runtime files.
- `init-workdir.sh` creates `bin/`, `config/`, `data/`, `logs/`, `run/`, and `webui/`, then writes `config/server.yaml` with `storage.data_dir` under the work directory.
- `build-server.sh` compiles the release Server and installs it to `$LOGAGENT_WORK_DIR/bin/logagent-server`.
- `build-webui.sh` runs the WebUI production build and syncs `webui/out` to `$LOGAGENT_WORK_DIR/webui/out`.
- `server-service.sh` manages start, stop, restart, status, and logs using `$LOGAGENT_WORK_DIR/run/logagent-server.pid` and `$LOGAGENT_WORK_DIR/logs/logagent-server.log`.
- Script validation: `bash -n` passes for the new scripts; every new entry script fails when `LOGAGENT_WORK_DIR` is missing; initialization smoke created the expected runtime tree; `build-server.sh` installed the release Server binary; `build-webui.sh` built and copied WebUI static output. `server-service.sh start` reached the Server startup step, but this Codex sandbox rejects local port binding (`PermissionError: [Errno 1] Operation not permitted` from a minimal Python socket bind), so live health-check startup cannot be completed in this environment.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass.

### Documentation Cleanup

- Removed obsolete root `plan.md`. Its early full-plan content duplicated current README/SPEC and `docs/modules/*` documents while retaining outdated module-directory and implementation assumptions.
- Root README now points readers to the maintained documentation set instead of the deleted historical draft.

### Case Store Import

- Reworked the top-level WebUI `Cases` page from direct manual entry to an LLM-assisted import workflow.
- Users can paste long Case text or upload UTF-8 text-like files (`.txt/.md/.log/.json/.yaml/.yml/.csv`); PDF/DOCX parsing is intentionally out of scope for this first pass.
- Server now persists Case import drafts under `storage.data_dir/case_imports/` and exposes `POST /api/cases/imports`, `GET/PATCH /api/cases/imports/:draft_id`, `POST /api/cases/imports/:draft_id/messages`, and `POST /api/cases/imports/:draft_id/confirm`.
- LLM Gateway now has a Case extraction call for stub, OpenAI-compatible, and binary providers. It returns `structuredCase`, `missingFields`, `assistantQuestion`, and `readyToConfirm`.
- Missing `title`, `symptom`, `rootCause`, or `solution` blocks confirmation; users can answer follow-up questions or edit the structured draft before saving.
- Confirmation still creates a normal `sourceType=manual` Case, so existing search, detail edit, enable/disable, and task recall behavior remain unchanged.
- Verification: `cargo test -p logagent-server`, `npm --prefix webui run typecheck`, `npm --prefix webui run lint`, and `npm --prefix webui run build` pass.

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
  - `GET /api/tasks`
  - `GET /api/tasks/:task_id`
  - `GET /api/tasks/:task_id/artifacts`
  - `GET /api/tasks/:task_id/result`
  - `POST /api/tasks/:task_id/messages`
  - `POST /api/tasks/:task_id/actions/:action_id/decision`
  - `POST /api/tasks/:task_id/case`
  - `POST /api/cases`
  - `POST /api/cases/imports`
  - `GET /api/cases/imports/:draft_id`
  - `PATCH /api/cases/imports/:draft_id`
  - `POST /api/cases/imports/:draft_id/messages`
  - `POST /api/cases/imports/:draft_id/confirm`
  - `GET /api/cases`
  - `GET /api/cases/:case_id`
  - `PATCH /api/cases/:case_id`
  - `GET /api/metadata/instances`
  - `GET /api/metadata/instances/:instance_id`
  - `GET /api/metadata/instances/:instance_id/snapshot`
  - `GET /api/metadata/clusters/:cluster_id`
  - `GET /api/metadata/clusters/:cluster_id/nodes`
  - `POST /api/metadata/snapshots/fetch`
  - `POST /api/metadata/imports`
  - `POST /api/metadata/imports/fetch`
  - `GET /api/metadata/imports/:import_id/preview`
  - `POST /api/metadata/imports/:import_id/confirm`
  - `GET /api/tools`
  - `GET /api/tools/:tool_id`
  - `POST /api/tools/:tool_id/runs`
  - `GET /api/tools/runs`
  - `GET /api/tools/runs/:task_id`
  - `GET /api/tools/runs/:task_id/result`
  - `GET /api/tools/runs/:task_id/artifacts`
- Uses API Key middleware for protected APIs.
- Statically serves Vite output from `webui/out`.
- Creates Server-owned task IDs and workspaces.
- Persists each task as `storage.data_dir/tasks/<task_id>.json` with atomic replacement.
- Returns `202 Accepted` after raw snapshot creation and runs tasks in the background.
- Limits concurrent tasks with `server.max_concurrent_tasks` (default 2).
- Recovers `QUEUED` and interrupted `RUNNING` tasks after restart; successful and failed tasks remain terminal.
- Uses a phase-driven Executor dispatcher instead of a hard-coded linear executor.
- Preserves the interrupted phase across restart, increments attempts on resume, and reruns only that idempotent phase.
- Rejects stale phase advancement and inconsistent persisted `RUNNING`/`SUCCEEDED` state.
- Defines shared `TaskContext`, Action, EvidenceArtifact, and EvidenceProvider contracts for Tool Runner and later evidence modules.
- Runs optional rule-based Tool Runner actions during `RUN_TOOL` and exposes `toolResults` in artifacts.
- Runs `PLAN_ANALYSIS` after rule-based tools and consumes bounded multi-round LLM `action | final_answer` decisions.
- Executes `search_logs` action by rebuilding `grep_results.json` with model-provided keywords, then continues to the next analysis round.
- Executes LLM-selected `run_tool` action through the same whitelist Tool Runner channel, then continues to the next analysis round.
- Enters `WAITING_FOR_USER` for `ask_user`, persists `pendingUserPrompts`, accepts `POST /api/tasks/:task_id/messages`, records the user message, and resumes the same task from `PLAN_ANALYSIS`.
- Enters `WAITING_FOR_APPROVAL` for `collect_environment` / `REQUIRES_APPROVAL`, persists `pendingApprovals`, accepts approval or rejection through `POST /api/tasks/:task_id/actions/:action_id/decision`, and resumes the same task from `PLAN_ANALYSIS`.
- Approved environment collection currently writes mock `environment_evidence/<action_id>/result.json`; real SSH/SCP execution remains planned for Environment Collector.
- Successful tasks can now be manually confirmed into the local Case Store through `POST /api/tasks/:task_id/case`.
- Manual Cases can now be created through `POST /api/cases` without binding to a task.
- Case Store records now use schema v2 with `sourceType=task|manual`; task Cases require `taskId/sourceResultPath`, while manual Cases forbid those fields. Development-stage startup intentionally rejects old v1 Case JSON instead of migrating it.
- Case Store records are persisted as JSON under `storage.data_dir/cases`, loaded at startup, searchable through `GET /api/cases`, and can be updated or disabled through `PATCH /api/cases/:case_id`.
- Case search now indexes InstanceID, NodeID, and evidence refs in addition to title, symptom, root cause, solution, product, version, and environment.
- New tasks now recall up to 5 enabled Cases by question, persist `case_context.json`, expose `caseContext` in artifacts, and include historical Case references in the LLM prompt as non-authoritative context.
- Metadata now uses user-provided `instanceId` as the user-facing unique key. openGemini imports require an explicit InstanceID, preserve the raw openGemini `ClusterID` as `sourceClusterId`, expose an imported Instance list, and serve stored topology snapshots by InstanceID. Legacy cluster endpoints remain for compatibility.
- Metadata instances now support an optional `remark` display name. openGemini fetch/import requests accept it, the store persists it, WebUI shows it beside InstanceID, truncates it in the list, and includes it in Overview.
- Persists `final_answer` decisions directly as `result.json` / `result.md`.
- Stops repeated action fingerprints and exhausted analysis budgets with a low-confidence final result instead of an infinite loop.
- Rejects artifact reads before success with `409` and the current task status.
- Runs one LLM result generation phase after grep and persists `result.json` / `result.md`.
- `GENERATE_RESULT` now reads `tool_results/*/result.json` and passes Tool Runner summary/findings into LLM Gateway as citeable evidence.
- Persists Analysis State Store MVP files, `analysis_state.json` and `analysis_events.jsonl`, and serves them through `GET /api/tasks/:task_id/analysis`.
- Records LLM call lifecycle events for `PLAN_ANALYSIS` action decisions, including `llm_call_started`, `llm_call_completed`, `llm_call_schema_retry`, `llmcall_*` callId, attempt, model, and schema error details.
- Exposes `/api/debug/llm` for runtime LLM response-content logging control; the flag is in-memory, defaults off, and does not print prompts or API keys.
- `scripts/start-local.sh` background mode now disowns the server job after `nohup`, so local quick-start remains available after non-interactive zsh exits.
- Runtime deployment assets are now organized under `$LOGAGENT_APP_DIR/deploy`, including deploy steps, env sample, config sample, `logagentctl.sh`, and `rebuild-install.sh`. The scripts use `LOGAGENT_APP_DIR` and `LOGAGENT_SRC_DIR`, replace `$LOGAGENT_APP_DIR/bin/logagent-server`, sync WebUI static files, and restart only when the server was already running.
- Tools API now supports a separate `taskKind=tool_run` path for user-triggered tools. Tool runs reuse upload records, raw workspace snapshots, TaskStore, background executor, status polling, and `tool_results`, while `/api/tasks` remains scoped to log analysis tasks.
- `pprof_analyzer` is the first Tools plugin. It is configured through `tools.pprof_analyzer`, expects the path to a Go executable, runs `go tool pprof -top/-tree/-raw` with `PPROF_TMPDIR` inside the task workspace, optionally attempts SVG output, and writes a structured result with profile type, total, top functions, warnings, and artifact paths.
- Added `examples/server-pprof-tool.yaml` for pprof Tools smoke with `LOGAGENT_TOOL_PPROF_GO`.

### Upload And Workspace

- Server owns `upload_id`, `task_id`, and workspace location.
- Persists each upload as `storage.data_dir/uploads/<upload_id>.json` with atomic replacement.
- Restores completed and in-progress uploads after restart.
- Tracks `UPLOADING` / `COMPLETE`, expected size, received size, payload path, and timestamps.
- Flushes small-file and batch multipart payloads before persisting `COMPLETE` upload records.
- Enforces sequential chunk offsets and exact expected size at completion; incomplete uploads cannot create tasks.
- Reconciles an interrupted `UPLOADING` record from the payload file length on startup.
- Corrupt records, unsafe paths, missing payloads, and inconsistent completed sizes fail startup; orphan upload directories are only warned.
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

### Tool Runner

- Implemented as a Server internal Rust module.
- Reads `tools` whitelist configuration.
- Validates enabled tool paths are absolute.
- Supports `tools.<name>.path_env` for environment-provided tool paths; disabled tools do not read their env vars.
- Supports `tools.<name>.max_input_files` to bound automatic per-tool input selection; default is 1.
- Generates rule-based `run_tool` actions from manifest file patterns first, then grep keywords as fallback candidates.
- Uses stable action ids derived from tool name and input file hash, so one tool can process multiple files in the same task without result directory collisions.
- Executes configured tools through `tokio::process::Command` without shell string concatenation.
- Supports timeout, stdout/stderr capture, output truncation, non-zero exit recording, spawn failure recording, and idempotent result reuse.
- Parses JSON stdout into structured `summary` and `findings`; non-JSON stdout keeps the old fallback summary and does not fail the task.
- Adapts real `influxql-analyzer` Report stdout into LogAgent `summary` and structured findings, including `large_limit`, `no_time_filter`, `group_by_high_cardinality_risk`, `meta_query`, parse errors, realtime classification, and query fingerprint statistics.
- `ToolRunRecord` schema version 2 adds `findings[]` with optional `severity`, `file`, `line` and required `message`.
- Writes:

```text
tool_results/<action_id>/
  result.json
  stdout.txt
  stderr.txt
```

- `GET /api/tasks/:task_id/artifacts` returns `toolResults`.
- WebUI displays tool result status, exit code, duration, summary, structured findings, stdout path, and stderr path.
- Tool findings can be cited by final LLM results as `tool_results/<action_id>/result.json#findings/<index>`.
- Manual Tools runs now also write under `tool_results/<action_id>/`; `pprof_analyzer` results are exposed through `/api/tools/runs/:task_id/result` rather than the log-analysis artifact endpoint.
- Added `examples/server-tools.yaml` with `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER` and `LOGAGENT_TOOL_INFLUXQL_ANALYZER` templates for real tool smoke tests.
- Added `examples/server-influxql-tool.yaml` for single-tool real InfluxQL smoke; it now uses `/usr/bin/influxql-analyzer` directly.
- Local Tool Runner smoke on port 50998 used `examples/server-tools.yaml` with both tool env vars pointed at `/bin/echo`; a batch `.flux` + `.sql` task `task_1780845768676_3` reached `SUCCEEDED` and returned OK tool results for both configured analyzers.
- Real InfluxQL Tool Runner smoke on port 50999 previously used a local analyzer path; current local setup exposes the analyzer as `/usr/bin/influxql-analyzer`, a symlink to `/home/duzhiwang/workspace/influxql/influxql-analyzer`. Analyzer docs and code live under `/home/duzhiwang/workspace/influxql`.
- The prior real InfluxQL smoke task `task_1780932701757_2` reached `SUCCEEDED` and artifacts contained `toolResults[0].findings` for `no_time_filter`, `large_limit`, `group_by_high_cardinality_risk`, `has_wildcard`, and `meta_query`.
- InfluxQL CompareReport stdout mapping now includes batch summaries, statement/QPS deltas, fingerprint count/QPS A->B deltas, rule count/QPS A->B deltas, rules, normalized query, and severity classification.
- Local search did not find `flux_query_analyzer` or `flux-query-analyzer`; real Flux smoke remains blocked until the binary is installed.

### WEBUI

- React + Vite + TypeScript + Tailwind CSS app under `webui/`.
- Uses shadcn/ui composition primitives.
- `npm run build` writes `webui/out`.
- Served by Server at `/` from `webui/out`.
- Supports:
  - health check
  - fixed top-bar API Key input
  - top-level Tools page
  - one or more file uploads
  - chunked upload for large files
  - task creation with `uploadIds`
  - artifact display
  - Server-backed recent task list and task detail polling
  - separate upload and task execution progress
  - persisted task recovery after page refresh
  - failed phase/message display and historical artifact selection
  - manual tool run status polling and result loading
  - Tool Runner result display
  - pprof analyzer upload, sample index/node count/SVG controls, top function table, and artifact path display
  - user question input and structured LLM result display
  - live Task execution loop summary from `/api/tasks/:task_id/analysis`
  - `WAITING_FOR_USER` prompt answer form
  - `WAITING_FOR_APPROVAL` action approval/rejection form
  - top-bar LLM debug switch backed by `/api/debug/llm`
  - successful task confirmation into Case Store with editable title/symptom/root cause/solution
  - Case Store schema v2 source display, keyword search, and disabling cases from the Log analysis view
  - top-level Cases page for Case Store search, manual entry, detail editing, evidence ref maintenance, and enable/disable state changes
  - grep evidence reference navigation
  - Metadata query
  - imported Metadata Instance list
  - stored Metadata snapshot loading by InstanceID
  - Metadata YAML/JSON import preview and confirmation
  - Metadata openGemini `/getdata` URL fetch preview with explicit InstanceID
  - Metadata Instance view for `PtView` partition state and `Databases` schema/RP/shard summary
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
  - optional instance `remark` persistence and summary listing
  - openGemini `PtView` normalization into cluster `partitionViews`
  - openGemini `Databases` normalization into database/RP/measurement/shard summaries
  - server-side metadata URL fetch
  - import confirmation and persistence
- CSV import remains reserved but not implemented.
- Raw openGemini snapshots are preserved.
- Shard and Index owners are modeled as PT IDs and resolved through PtView to DataNodes.
- Task creation accepts optional instance, cluster, and node IDs, resolves related IDs, and rejects unknown or conflicting relationships.
- Persists an immutable normalized task snapshot as `metadata_context.json` without duplicating the raw Metadata snapshot.
- LLM prompts include bounded product, version, environment, node, database, and partition summaries.

### LLM Gateway

- Implemented as a Server-internal Rust module.
- Supports deterministic `stub` and OpenAI-compatible Chat Completions.
- Supports a reserved `binary` provider branch that invokes a configured absolute-path executable as `<binary_path> run <prompt>` through argv arrays, parses stdout with the same JSON/schema/evidence validation as the HTTP provider, and enforces timeout plus `binary_max_output_bytes`. Current verification uses a mock binary, so no real model executable is required in this environment.
- Supports `llm.model_env` for environment-provided model names while retaining static `llm.model` compatibility.
- Accepts pure JSON, whole-response JSON Markdown fences, and natural-language responses containing exactly one top-level JSON object.
- Builds a bounded prompt from question, manifest summary, and indexed grep matches.
- Adds bounded Tool Runner summary/findings to the prompt after grep evidence; stdout/stderr raw output is not sent.
- Validates result schema, confidence, and task-local grep evidence references.
- Validates task-local Tool Runner finding evidence references.
- Provides ActionDecision / FinalAnswer dual-mode schema and parser for the multi-round action loop.
- `PLAN_ANALYSIS` now calls the dual-mode action decision entrypoint until final answer, budget exhaustion, or repeated fingerprint termination.
- ActionDecision currently accepts `search_logs`, `run_tool`, `ask_user`, `collect_environment`, and `final_answer`; `collect_environment` requires approval and `collect_code_evidence` remains closed.
- If a real model returns a bare final-result JSON during `PLAN_ANALYSIS` without the outer `type` field, or returns common nested final-answer wrappers such as `final_answer.result.result`, `answer`, or `finalAnswer`, LLM Gateway wraps it as `final_answer` and still validates evidence refs.
- Action decision parsing/schema failures in `PLAN_ANALYSIS` now get one corrective retry with the latest schema error, so a first response missing top-level `type` no longer fails the task immediately.
- Normalizes traceable LLM evidence ref aliases, including raw log line ranges such as `12-14`, index ranges such as `#0-#7`, and `matches/<start>-<end>`, into canonical `grep_results.json#matches/<index>` refs.
- Normalizes real-model schema drift for string root causes with embedded evidence refs and single-string list fields.
- Retries final-result parsing/schema failures once with a corrective schema prompt and returns latest/previous parse errors if both attempts fail.
- Provider or schema failure in action decision moves the task to `FAILED / PLAN_ANALYSIS`; final result generation failures still move the task to `FAILED / GENERATE_RESULT`.

### Analysis Agent

- Analysis State Store MVP is implemented as a Server internal module.
- Current pipeline records analysis initialization, manifest evidence, grep evidence, Tool Runner action/evidence, model decision, final result, and failure events.
- Workspaces now include `analysis_state.json` and append-only `analysis_events.jsonl`.
- `GET /api/tasks/:task_id/analysis` returns the current state snapshot and event list.
- Multi-round Action Loop MVP is enabled through `PLAN_ANALYSIS` for `search_logs`, `run_tool`, and `final_answer`.
- User questions, approvals, token/runtime budgets, and richer state facts/hypotheses remain planned.

### Local startup

- Added `scripts/start-local.sh` for one-command local Server startup.
- Defaults to the real OpenAI-compatible configuration on port 50994 and supports `--stub` and `--foreground`.
- Builds the WebUI only when `webui/out/index.html` is missing, builds the Rust Server, writes PID/log files under `/tmp`, and waits for the health endpoint without exposing secrets.
- Shell syntax/help checks passed; a real-LLM foreground launch reached `http://127.0.0.1:50994/health` with a healthy response and an active listener.

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

Task, upload, and LLM verification:

- 92 Rust tests pass.
- Upload Store tests cover persistence/reload, interrupted progress reconciliation, strict chunk offsets, completion size, and corrupt JSON.
- Upload API tests cover single and batch multipart upload flush-before-persist behavior.
- Task API rejects `UPLOADING` records until completion.
- Metadata context tests cover node/instance/cluster derivation, conflict rejection, workspace persistence, artifacts, prompt inclusion, and rerun preservation.
- Isolated HTTP smoke on port 50997 created a task with only `nodeId`, derived its instance/cluster IDs, reached `SUCCEEDED`, and returned the immutable Metadata artifact without `rawSnapshot`.
- Isolated HTTP restart smoke on port 50996 uploaded 6/12 bytes, restarted the Server, resumed from persisted offset 6, completed at 12 bytes, and created a task that reached `SUCCEEDED`.
- Task Store reload, corruption failure, reverse chronological listing, terminal-state protection, and interrupted task recovery.
- Executor recovery tests resume directly from `SEARCH_LOGS` and `GENERATE_RESULT`; Action/Evidence serialization and safe relative artifact paths are covered.
- Tool Runner, LLM, and Analysis State tests cover config validation, analysis budget defaults, `max_input_files`, rule-based multi-input selection, stable action ids, fake tool execution, JSON stdout summary/findings parsing, non-JSON fallback, timeout evidence, idempotent reuse, dispatcher `RUN_TOOL`, multi-round `PLAN_ANALYSIS`, repeated fingerprint termination, artifacts API `toolResults`, `/analysis` API, LLM prompt inclusion of tool findings, ActionDecision / FinalAnswer parsing, bare final-result JSON and nested final-answer wrapper normalization, and tool finding evidence ref validation.
- Case Store tests cover local JSON persistence, task final-result confirmation, keyword recall, duplicate task confirmation protection, and disabling cases from default recall.
- LLM Gateway tests cover Case context prompt inclusion and the task API test verifies recalled Case context appears in artifacts.
- Tool Runner tests cover enhanced InfluxQL CompareReport delta summary/findings.
- Pipeline rerun removes stale derived files and rebuilds evidence from raw snapshots.
- Task API covers `202`, list/detail, `404`, and artifacts `409`.
- Stub task execution reaches `SUCCEEDED`, writes result files, and serves the result API.
- Prompt truncation, Chat Completions parsing, Provider error classification, evidence refs, and evidence ref alias normalization are tested.
- Task API tests use per-process atomic temp roots so concurrent test cleanup cannot remove another task workspace.
- LLM model configuration tests cover static values, `model_env` precedence, and missing or empty environment values.
- Chat Completions parsing tests cover pure JSON, JSON code fences, natural-language wrappers around a single JSON object, and rejection of multiple JSON objects.
- LLM Gateway now normalizes real-model string root causes with embedded `evidenceRefs`, including `matches/<index>` and `matches/<start>-<end>` aliases, into canonical result objects.
- LLM Gateway now normalizes single-string list fields such as `missingInformation: "..."` into one-item arrays, matching the observed cluster metadata real-model response.
- LLM Gateway now retries final-result parsing/schema failures once with a corrective schema prompt and returns latest/previous parse errors when both attempts fail.
- LLM Gateway now normalizes observed real-model `PLAN_ANALYSIS` final-answer wrapper variants, including nested `result.result` and `action.decision.type=final_answer` with a result in `input`, into true `FinalAnswer` decisions while preserving strict final-result schema checks.
- LLM Gateway now applies the same bounded schema-correction pattern to action decisions that final-result generation already used; after two invalid action decision responses, the final error includes latest and previous parse reasons.
- LLM Gateway now supports a runtime debug switch for printing raw model response content to Server stderr when manually enabled.
- LLM Gateway now assigns `llmcall_*` callIds to real `PLAN_ANALYSIS` action-decision calls and propagates the callId into started/completed/schema retry events and final provider/schema errors.
- WebUI Task execution now shows live analysis loop revision, budget counters, recent events, LLM callId/attempt/schema retry details, model decisions, actions, artifacts, and evidence refs.
- WebUI top bar now includes an LLM debug switch that controls the Server-side response logging flag.
- WebUI Log analysis now shows a Case confirmation panel for successful tasks and a local Case search/disable panel.
- WebUI successful task artifacts now show task-local Case context captured at task creation.
- Added `scripts/smoke-product-loop.sh` for repeatable local product-loop smoke with real `/usr/bin/influxql-analyzer`, Case save, and Case recall.
- Upload API tests now use per-process atomic temp roots so concurrent cleanup cannot remove another upload payload.
- Real OpenAI-compatible smoke on port 50994 with clusterId `8343121086559132311` completed task `task_1780843631402_1` as `SUCCEEDED` after the LLM retry/error-detail change.
- LLM request failure is verified to persist `FAILED / GENERATE_RESULT`.
- Isolated HTTP smoke on port 50993 verified upload, `202 QUEUED`, polling to `SUCCEEDED`, persisted list/detail, `attempts=1`, and artifact reads.
- Isolated stub LLM HTTP smoke on port 50995 verified question persistence, `GENERATE_RESULT`, `result.json` / `result.md`, result API, and grep evidence references.
- Real OpenAI-compatible smoke on port 50994 reached the configured `deepseek-v4-flash` model and completed task `task_1780762062871_3` as `SUCCEEDED`.
- The successful real-model result persisted `result.json` / `result.md`, returned through the result API, and cited both task-local grep matches.
- Two preceding real-model attempts returned content that failed the strict result JSON parser, while an equivalent direct request and the third task returned valid JSON. This confirms the end-to-end protocol but leaves output-format stability, JSON response-format enforcement, and bounded schema retry as follow-up work.
- After evidence ref alias normalization, two real-model smoke tasks on port 50994 still failed earlier at strict JSON parsing (`LLM content is not valid result JSON`), so the exact `12-14` normalization path is covered by unit tests rather than real-model completion.

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
- isolated Tool Runner smoke on port 50998 configured `/bin/echo` as a fake tool, uploaded sample.log, completed a task as `SUCCEEDED`, and returned `toolResults[0].status=OK`

Current product-loop Case Store slice verification:

- `cargo fmt --check`
- `cargo check`
- `cargo test` (84 Rust tests pass)
- `npm run lint`
- `npm run typecheck`
- `npm run build`
- `scripts/smoke-product-loop.sh` passed with upload `upl_1780939967830_1`, task `task_1780939967836_2`, case `case_1780939968869_3`, and recall task `task_1780939968874_4`.

## 2026-06-11 Claude SDK Backend Runtime Switch

- Log Analysis `PLAN_ANALYSIS` no longer uses the self-built `internal_llm` agent loop. The executor now refreshes `analysis_package.json` and `agent_request.json`, invokes the configured `claude_agent_sdk` adapter, validates the returned `AgentDecision`, and writes a real `agent_response.json`.
- `agent_response.json` now records `runtimeStatus`, raw decision, `normalizedDecision`, usage/cost, duration, and errors. Adapter failures, non-JSON stdout, illegal actions, or invalid evidence refs fail the task without fallback.
- Config defaults now require `claude_agent_sdk`; missing `LOGAGENT_AGENT_CLAUDE_SDK_PATH` or a configured absolute `command_path` fails startup. `internal_llm` is no longer a supported agent backend type.
- Tool capability metadata is now included in `analysis_package.json` so Claude can request `run_tool` using Server-managed whitelist execution and canonical finding refs.
- WebUI now labels results as `Agent analysis` and shows a `Claude Code backend` panel with real backend status, usage/cost, duration, errors, and artifact paths instead of the old contract-only panel.
- Added mock Claude SDK adapter tests for final answers, `run_tool`, `ask_user`, invalid evidence refs, non-zero adapter exits, and executor multi-round search/budget behavior.
- Updated root, Server, WebUI, Agent Backends, Analysis Agent, LLM Gateway, Config, System Context, Interfaces, Roadmap, Tool Runner, and Testing docs to remove the old `internal_llm` default/runtime language.
- Verification for this slice passed: `cargo fmt --check`, `cargo check`, `cargo test` (native 1 + server 113 tests), `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, and `npm --prefix webui run build`.

- Fixed direct Claude Code CLI execution for `claude_agent_sdk`: when `LOGAGENT_AGENT_CLAUDE_SDK_PATH` points to a `claude` / `claude.exe` binary, Server now invokes `claude --print --output-format json --json-schema ... --tools "" --no-session-persistence <prompt>` instead of the custom adapter-only `run --request ...` protocol. Non-`claude` command paths continue to use the custom LogAgent adapter protocol.
- Agent response parsing now accepts Claude CLI JSON envelopes by parsing the `result` field, records `total_cost_usd` as `cost.usd`, and still rejects failed CLI envelopes, invalid decisions, and invalid evidence refs without fallback.
- Added a mock direct `claude` CLI unit test to ensure `--request` is not passed to the native CLI and Claude envelope output normalizes to `final_answer`.
- Updated deploy templates so `claude_agent_sdk` is the default backend and `.env.example` documents `LOGAGENT_AGENT_CLAUDE_SDK_PATH` as the absolute `claude` CLI path. The local runtime `/home/duzhiwang/workspace/data/prd_assistant/deploy/logagent.yaml` was also updated with an explicit `agent_backends` block.
- Updated `deploy/logagentctl.sh` to load runtime `.env` with auto-export semantics so background Server processes receive `LOGAGENT_AGENT_CLAUDE_SDK_PATH` and other configured environment variables. The local runtime `logagentctl.sh` and `.env` were adjusted, the runtime server binary was rebuilt/installed, and `/health` returned `{"status":"ok"}` after a normal control-script start.
- Verification for the Claude CLI backend fix passed: `cargo fmt --check`, `cargo check`, `cargo test` (native 1 + server 114 tests), `bash -n deploy/logagentctl.sh deploy/rebuild-install.sh /home/duzhiwang/workspace/data/prd_assistant/deploy/logagentctl.sh`, and repeated runtime `/health` checks through `logagentctl.sh status`.

- Fixed Claude CLI structured output parsing after real `PLAN_ANALYSIS` returned a valid Claude envelope whose user-visible `result` was `"Done."` and whose actual schema-constrained decision was in `structured_output`. Agent Backend parsing now checks `structured_output` / `structuredOutput` before `result`, and failed parse responses include a truncated `rawStdoutPreview` plus the full error chain for future diagnosis.
- Verification for the structured output parser fix passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::agent_backend`, `cargo test` (native 1 + server 114 tests), runtime `rebuild-install.sh --server-only --no-restart`, and `logagentctl.sh status` returning `/health` `{"status":"ok"}`.

## 2026-06-12 Metadata On-Demand Loading

- `analysis_package.json` no longer embeds the full `metadataContext` payload. The Claude evidence package now writes `evidence.metadataContextOutline` with selected instance/cluster/node ids, product/version/environment, section counts, and `logagent.query_metadata` discovery info.
- The full `metadata_context.json` remains in the task workspace and continues to be returned by the successful task artifacts API for WebUI and compatibility.
- Task stdio MCP `resources/read metadata_context` and compatibility tool `logagent.get_metadata_topology` now return the outline instead of full metadata topology.
- Added task MCP tool `logagent.query_metadata` with `section`, `database`, `retentionPolicy`, `measurement`, `nodeId`, `ownerNodeId`, `ptId`, `shardId`, `indexId`, `limit`, and `cursor` arguments. It returns bounded `items`, `total`, `nextCursor`, `truncated`, `backgroundRef`, writes `metadata_slices/<stable_id>.json`, and records calls in `mcp_calls.jsonl`.
- Metadata slices remain background context and are not accepted as final evidence refs.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server metadata`, `cargo test -p logagent-server mcp`, and `cargo test -- --test-threads=1` (native 1 + server 128 tests).

## 2026-06-12 Claude MCP Permission Allowlist Fix

- Root cause for repeated "LogAgent MCP tools are being denied because permission mode is don't ask" prompts: the Server launched Claude Code in `dontAsk` mode with `--tools ""` and no `--allowedTools` entry for the task MCP server. LogAgent user approvals cannot change Claude CLI's tool allowlist, so answering the prompt did not unblock MCP calls.
- Every Claude Code permission profile now automatically includes `mcp__logagent__*` in `allowedTools`, while `diagnose` still keeps native built-in tools disabled through `--tools ""` and disallowed `Bash/Edit/Write/Read/Grep`.
- The Claude startup prompt now tells the model that LogAgent MCP read tools are pre-authorized and should be used directly; `request_approval` remains reserved for approval-gated actions such as remote environment collection.
- Verification passed: `cargo fmt --check`, `cargo check`, focused LogAgent Server config and Claude Code session tests, and `cargo test --quiet -- --test-threads=1` (native 1 + server 128 tests).

## 2026-06-12 Waiting Prompt Finalize Control

- Added a `resumeMode` field to task user messages. The default `continue` keeps the existing answer-and-resume behavior; `finalize` records that the user has no more information and wants a final result from current evidence.
- `analysis_package.json` now exposes `analysisState.finalizeRequested`, and the Claude startup prompt instructs Claude to return `completed` instead of asking the user again when that flag is true.
- WebUI `Task execution` now shows a secondary `没有更多信息，生成最终结果` button in `WAITING_FOR_USER`, sending the finalize mode with a bounded default message when the answer box is empty.
- Verification passed: `cargo fmt --check`, `cargo check --quiet`, focused `cargo test -p logagent-server task_message_resumes_waiting_for_user_task -- --test-threads=1`, `cargo test --quiet -- --test-threads=1` (native 1 + server 128 tests), `npm run lint`, `npm run typecheck`, and `npm run build`.

## Planned Next

1. Complete the current product loop around the existing upload, Metadata, Tool Runner, Analysis Agent, and WebUI flow:
   - stable task creation and polling
   - user question and approval interactions
   - evidence display and navigation
   - broader final result confirmation polish after the Case Store MVP
   - repeatable local smoke with `/usr/bin/influxql-analyzer`
2. Install/connect and smoke-test real `flux_query_analyzer`; run a real `influxql_analyzer` compare-mode smoke and tune delta mapping if needed.
3. Extend Case Store with embedding recall and a formal Analysis Agent evidence bundle after the product loop is stable.
4. Implement Code Evidence after the product loop is stable:
   - map product/version to branch/tag/ref
   - prepare read-only worktree/cache
   - collect code file/line evidence
5. Replace mock approved environment evidence with the real Environment Collector SSH/SCP executor after Code Evidence.

## Documentation Verification

For the Analysis Agent architecture update:

- Reviewed all component README/SPEC documents.
- Updated root architecture, interfaces, Server, WebUI, config, security, testing, deployment, evidence providers, Case Store, roadmap, and `AGENTS.md`.
- No application code or runtime configuration was changed, so Rust and WebUI build checks were not required.
- Added and syntax-reviewed the root Mermaid architecture and investigation-loop diagrams.

## Maintenance Rule

Every completed file change must update this progress document when it changes project status, behavior, APIs, module scope, verification, or next-step priorities.

When changing a component, also update that component's `README.md` and `SPEC.md`.
