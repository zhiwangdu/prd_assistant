# LogAgent V2 Server Spec

## Goal

V2 is a clean-room small-team implementation of LogAgent. It favors a simple
single-machine deployment over distributed infrastructure:

- Python + FastAPI for the API.
- LangGraph-oriented Agent runtime.
- SQLite WAL for durable state.
- Local filesystem artifacts for large evidence.
- DB-backed jobs instead of Redis.

V2 does not need to be compatible with the current Rust Server API or artifact
layout. The stable product goal remains evidence-backed diagnosis with an
auditable agent boundary.

## Product Model

- `Workspace`: top-level diagnosis container.
- `Run`: one Agent execution inside a Workspace.
- `TimelineEvent`: append-only product event stream.
- `Evidence`: typed fact or background item.
- `Artifact`: large file tracked by DB metadata and content hash.
- `Upload`: user-provided file attached to a Workspace.
- `Action`: Agent-requested operation that may require approval.
- `Job`: persistent background work item.

## Current Implementation

Implemented in this slice:

- FastAPI app and public `GET /health`.
- Bearer auth for `/api/v2/*`.
- SQLite schema creation with WAL.
- Workspace creation/list/read.
- Upload storage as local artifacts.
- Run creation and queued `run_analysis` job.
- Inline DB-backed worker.
- Initial evidence pipeline for uploaded text files and supported archives.
- Node log package preprocessing for
  `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz`; supported log
  directories are classified into stable `extracted/<node>/<timestamp>/<group>`
  paths, and gzip-rotated files are decoded by magic bytes.
- Materialized `tool_inputs/index.json` generation for node package tsdb
  InfluxQL query lines. Generated entries are compatible with the V1
  `ToolInputEntry` shape and include V2 artifact ids for local execution.
- `manifest.json` and `grep_results.json` artifact generation.
- Stub Agent runtime that records initial question evidence, consumes the
  initial evidence pipeline, and returns a low-confidence evidence summary.
- Timeline events for workspace, upload, run, and evidence lifecycle.
- Artifact download.
- Evidence listing for a run.
- Read-only MCP placeholder with `initialize`, `resources/list`,
  `resources/read`, `tools/list`, and `tools/call logagent.list_tools`.
- Task MCP endpoint with `summary`, `evidence`, `manifest`, and `grep_results`
  resources.
- Task MCP `logagent.search_logs`, which creates follow-up `log_search`
  evidence and stable `log_searches/<search_id>.json#matches/<index>` refs.
- Task MCP `logagent.get_log_slice`, which reads bounded context from a current
  Workspace text path and persists `log_slice` evidence.
- Minimal configured Tool Runner. Tools are loaded from
  `LOGAGENT_V2_TOOLS_JSON`, listed through `/api/v2/tools`, and runnable through
  task MCP `logagent.run_domain_tool`. Tools with `{input_file}` arguments
  consume matching materialized tool inputs before execution, then fall back to
  manifest file patterns and initial grep keyword matches. Generic JSON stdout
  and InfluxQL analyzer report/compare stdout are normalized into
  `summary/findings`.
- Fetch endpoint foundation. Endpoints are stored in SQLite, listed and managed
  through protected HTTP APIs, importable from DevTools bash cURL, exposed as a
  built-in `/api/v2/tools` descriptor, and executable through task MCP
  `logagent.fetch` only when enabled and allowlisted.
- Waiting-state foundation through task MCP `logagent.request_user_input` and
  `logagent.request_approval`; pending actions are persisted and user
  message/approval APIs can requeue the run.
- Final answer schema normalization and evidence ref validation. A run can only
  be marked `succeeded` after final refs point to current-run, final-allowed
  log search, log slice, or tool finding evidence.
- Metadata foundation with direct JSON/YAML/openGemini content import,
  allowlisted URL fetch, preview/confirm draft workflow, SQLite snapshot
  storage, field/tag type query APIs, per-run `metadata_context`
  auto-selection, readonly MCP tools, and task MCP background slices.
- Case Memory foundation with manual Case creation, succeeded-run Case
  confirmation, SQLite FTS5/BM25 recall, edit/disable API, readonly MCP search,
  and task MCP background case context.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit or auto-matched Workspace skill selection, per-run
  `system_context` artifact, readonly MCP Skill tools, and task MCP reference
  artifacts.
- `skills.zip` export for the current Skill registry, with regular files only,
  root manifest, and symlink skipping.
- `tools.zip` export for enabled configured subprocess tools, with packaged
  executable files, shell wrappers, config examples, and skip reasons for tools
  that cannot be packaged.

Not yet implemented:

- LangGraph provider integration.
- V1-compatible analyzer materialized `tool_inputs/index.json` generation beyond
  node-package InfluxQL JSONL, per-tool params schema, and full multi-round
  model reasoning after resume.
- Encrypted Fetch credential sets, WebUI Fetch management, and WebUI cutover.
- WebUI System Context cutover.
- Case import drafts, embedding/vector recall, and WebUI Memory management.
- WebUI V2 cutover.

## API

Protected endpoints use:

```text
Authorization: Bearer <api-key>
```

Current V2 endpoints:

```http
GET  /health
POST /api/v2/workspaces
GET  /api/v2/workspaces
GET  /api/v2/workspaces/:workspace_id
POST /api/v2/workspaces/:workspace_id/uploads
POST /api/v2/workspaces/:workspace_id/runs
GET  /api/v2/runs/:run_id
GET  /api/v2/runs/:run_id/timeline
GET  /api/v2/runs/:run_id/evidence
POST /api/v2/runs/:run_id/messages
POST /api/v2/actions/:action_id/decisions
GET  /api/v2/evidence/:evidence_id
GET  /api/v2/artifacts/:artifact_id
GET  /api/v2/tools
GET  /api/v2/exports/skills.zip
GET  /api/v2/exports/tools.zip
GET  /api/v2/metadata/instances
GET  /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/instances/:instance_id/snapshot
DELETE /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/imports
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/preview
POST /api/v2/metadata/imports/fetch/preview
POST /api/v2/metadata/imports/:import_id/confirm
POST /api/v2/metadata/imports/fetch
POST /api/v2/metadata/imports
POST /api/v2/metadata/field-types
POST /api/v2/metadata/tag-fields
POST /api/v2/cases
POST /api/v2/runs/:run_id/case
GET  /api/v2/cases
GET  /api/v2/cases/:case_id
PATCH /api/v2/cases/:case_id
GET  /api/v2/skills
GET  /api/v2/skills/:skill_id
POST /api/v2/skills/imports
POST /api/v2/skills/preview
POST /api/v2/fetch/imports/preview
POST /api/v2/fetch/imports
GET  /api/v2/fetch/endpoints
POST /api/v2/fetch/endpoints
GET  /api/v2/fetch/endpoints/:endpoint_id
PATCH /api/v2/fetch/endpoints/:endpoint_id
DELETE /api/v2/fetch/endpoints/:endpoint_id
POST /api/v2/runs/:run_id/fetch/:endpoint_id
POST /api/v2/mcp/readonly
POST /api/v2/mcp/task/:run_id
```

## Storage

Default data layout:

```text
/tmp/logagent-v2/
  logagent.sqlite
  artifacts/
    <workspace_id>/
      <artifact_file_id>/
        <filename>
  tmp/
```

SQLite tables:

- `workspaces`
- `runs`
- `timeline_events`
- `artifacts`
- `uploads`
- `evidence_items`
- `actions`
- `jobs`
- `metadata_instances`
- `metadata_imports`
- `cases`
- `fetch_endpoints`

The database stores state and bounded previews. Large payloads live in artifact
files and are referenced by `relative_path`, `sha256`, and size.

## Initial Evidence Pipeline

Run execution currently performs:

```text
Workspace uploads
  -> safe archive scan / text file collection
  -> optional node-package InfluxQL JSONL tool_inputs materialization
  -> manifest.json artifact
  -> bounded keyword grep
  -> grep_results.json artifact
  -> manifest and log_search evidence
  -> low-confidence stub final answer
```

Supported archive formats are `.zip`, `.tar`, `.tar.gz`, and `.tgz`. Archive
members are never extracted by path. V2 normalizes member names and rejects
absolute paths, `..` traversal, empty paths, and other unsafe names. Non-file
members and symlinks are skipped. Text files are bounded by
`LOGAGENT_V2_MAX_TEXT_FILE_BYTES`, aggregate scanned bytes by
`LOGAGENT_V2_MAX_ARCHIVE_BYTES`, and initial matches by
`LOGAGENT_V2_MAX_GREP_MATCHES`.

Initial grep refs use:

```text
grep_results.json#matches/<index>
```

These refs are current-task evidence. Manifest evidence is background and not
final evidence.

Node log packages named
`<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` or `.tgz` are a
special tar scan mode. Archive members can live below a wrapper directory; V2
searches normalized path components for supported log roots:

```text
var/chroot/gemini/log/tsdb
var/chroot/gemini/log/stream
home/Ruby/log
```

Files under those roots are accepted regardless of suffix, decoded as gzip when
the bytes start with gzip magic, and exposed in manifest/search paths as:

```text
extracted/<nodeId>/<timestamp>/tsdb/<relative-file>
extracted/<nodeId>/<timestamp>/stream/<relative-file>
extracted/<nodeId>/<timestamp>/agent/<relative-file>
```

Each manifest file entry records `originalPath`, `logGroup`, and `nodePackage`
metadata. A matching node package with no supported log directories is rejected
with a clear error so an empty manifest is not treated as a successful import.

For node package `tsdb` logs, V2 extracts JSON lines with a string `query`,
`sql`, or `statement` field and raw lines that look like InfluxQL statements.
Those records are written to `influxql_analyzer` JSONL artifacts. The
corresponding `tool_inputs/index.json` artifact uses entries like:

```json
{
  "path": "tool_inputs/influxql_analyzer/<node>/<timestamp>.jsonl",
  "inputKind": "influxql_jsonl",
  "scope": "package",
  "toolIds": ["influxql_analyzer"],
  "nodeId": "node-a",
  "instanceId": "inst-a",
  "packageTimestamp": "20260617130000",
  "logGroup": "tsdb",
  "sourceFiles": ["extracted/node-a/20260617130000/tsdb/query.log"],
  "recordCount": 1,
  "artifactId": "artfile_...",
  "artifactRelativePath": "artifacts/..."
}
```

Follow-up task MCP searches use:

```text
log_searches/<search_id>.json#matches/<index>
```

Each follow-up search persists a `log_search` evidence item and a JSON artifact.

Log slice refs use:

```text
log_slices/<slice_id>.json#lines
```

`logagent.get_log_slice` only reads paths that are available from the current
Workspace's uploaded text files or supported archive members.

Configured Tool Runner execution:

```text
LOGAGENT_V2_TOOLS_JSON
  -> /api/v2/tools descriptor
  -> MCP logagent.run_domain_tool { toolId }
  -> optional materialized tool input selection
  -> fixed absolute command + fixed args with {input_file} substitution
  -> tool_result artifact/evidence
```

The model cannot submit executable paths, shell snippets, dynamic argv, or
environment overrides.

Tool stdout is parsed as JSON when possible. Generic JSON output supports
`summary` / `message` / `title`, `findings` / `issues` / `diagnostics`, and
finding fields `severity` / `level` / `status`, `file` / `path` / `filename`,
`line` / `lineNumber` / `startLine`, and `message` / `summary` /
`description` / `detail` / `title` / `cause`.

InfluxQL analyzer report stdout is specially adapted. Summary includes
`total_records`, `records_in_window`, `total_statements`, `parse_error_count`,
and `special_rules`. Findings include special rule hits, parse errors,
realtime classification, and notable fingerprints. InfluxQL compare report
stdout is also adapted: `statement_delta`, `qps_delta`, `batch_a`, and
`batch_b` go into summary, while new/removed/changed fingerprints and
`rule_deltas` become findings.

`GET /api/v2/exports/tools.zip` exports only enabled configured subprocess
tools from `LOGAGENT_V2_TOOLS_JSON`. Built-in tools are not packaged. The
archive contains:

```text
README.md
tools-manifest.json
bin/<toolId>/<executable>
wrappers/<toolId>.sh
config/examples/<toolId>.yaml
```

Executable commands must resolve to absolute, regular, executable files to be
packaged. Tools that cannot be packaged remain in `tools-manifest.json` with
`skipped=true` and `skipReason`; disabled tools are omitted. The export does
not include API keys, Fetch endpoint secrets, environment values, uploads,
artifacts, or workspace data.

When a tool arg contains `{input_file}`, V2 selects entries from the current
run's latest `tool_input_index` evidence whose `toolIds` contain the requested
tool id. The placeholder is replaced with the resolved artifact path. Each input
creates a stable action id derived from tool id plus virtual input path, and the
evidence ref prefix becomes:

```text
tool_results/<tool_id>_<input_hash>/result.json#findings/
```

If no materialized input matches, V2 falls back to current-run text files.
Manifest paths matching `match.filePatterns` are selected first. If capacity
remains, initial `grep_results.json` matches whose line text contains one of
`match.keywords` select additional files. Fallback files are materialized as
run-local `logagent.v2.tool_input.text_file.v1` artifacts and exposed to tools
as virtual `extracted/<manifest path>` inputs. Selection is de-duplicated,
bounded by `maxInputFiles`, and preserves materialized-input priority.
Multi-input MCP responses keep `result/evidence` for the primary execution and
add `results[]` and `evidenceItems[]`.

## Fetch Endpoints

V2 Fetch endpoints are stored in SQLite table `fetch_endpoints` with name,
method, URL, headers, optional body, enabled flag, and timestamps. The public
API returns redacted endpoint previews; raw headers and bodies are only used by
the server-side executor.

Endpoints can be created directly or imported from DevTools bash cURL commands:

```text
POST /api/v2/fetch/imports/preview
POST /api/v2/fetch/imports
```

The cURL importer supports request method, headers, body, cookies,
`--compressed`, `--head`, and `--location`. It rejects unsupported flags such as
form uploads, proxy, cert, file, or resolver options rather than widening the
network or filesystem boundary. Import previews redact sensitive query,
header, and JSON/form body fields and return detected sensitive field
locations. Encrypted credential sets are not implemented in V2 yet, so saved
endpoints still use the existing endpoint storage path.

Fetch execution is disabled by default. To execute endpoints, set:

```text
LOGAGENT_V2_FETCH_ENABLED=1
LOGAGENT_V2_FETCH_ALLOWED_HOSTS=127.0.0.1,example.internal:8080
LOGAGENT_V2_FETCH_MAX_REDIRECTS=5
```

Only `http` and `https` URLs are supported. The requested host or host:port must
exactly match the allowlist. Controlled headers such as `Host`,
`Content-Length`, `Transfer-Encoding`, and `Connection` are rejected when
endpoints are saved. Sensitive headers, query parameters, and JSON/form body fields
containing token/secret/password/api key style names are redacted from API, MCP,
and artifact previews.

Redirects are followed manually up to `LOGAGENT_V2_FETCH_MAX_REDIRECTS`. Every
redirect target is revalidated with the same scheme/host allowlist before the
next request is sent. Sensitive headers such as Authorization, Cookie, and
X-Api-Key are stripped when a redirect crosses origin. Each response artifact
records `finalUrl`, `redirectCount`, and redacted redirect hops.

Task MCP exposes:

```text
logagent.list_fetch_endpoints
logagent.fetch { endpointId }
```

`logagent.fetch` writes a `fetch_result` artifact/evidence item. Network errors
produce a failed Fetch result rather than crashing the run. HTTP 4xx/5xx
responses are stored as responses.

Fetch response evidence refs use:

```text
tool_results/<fetch_action_id>/result.json#response
```

Final-answer validation accepts these refs only for current-run,
`final_allowed=true`, `kind=fetch_result` evidence whose artifact contains a
real `response` object. The readonly MCP endpoint may list the built-in Fetch
catalog descriptor, but it does not expose or run `logagent.fetch`.

## Metadata

V2 stores Metadata import drafts in SQLite table `metadata_imports` and
confirmed snapshots in `metadata_instances`. Instance rows are keyed by
`instance_id` and contain `template_type`, optional `remark`, normalized
`snapshot_json`, and original `raw_json`.

The product import flow is preview then confirm:

```text
POST /api/v2/metadata/imports/preview
POST /api/v2/metadata/imports/fetch/preview
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/:import_id/confirm
```

Preview parses and normalizes content into a draft with status `previewed` and
returns summary counts. It does not mutate `metadata_instances`. Confirm upserts
the draft snapshot into `metadata_instances` and marks the draft `confirmed`.
`POST /api/v2/metadata/imports` remains as a direct immediate import shortcut.

URL fetch import uses the same default-off Fetch boundary. It requires
`LOGAGENT_V2_FETCH_ENABLED=1`, an exact host or host:port match in
`LOGAGENT_V2_FETCH_ALLOWED_HOSTS`, and the shared Fetch timeout/response-size
limits. V2 uses GET only, rejects redirects, redacts sensitive query parameters
in draft `sourceUrl`, and then runs the fetched content through the same
normalization and preview/confirm path.

Current direct import request:

```json
{
  "instanceId": "prod-og-1",
  "templateType": "opengemini",
  "content": "{...}",
  "remark": "optional display name"
}
```

`templateType=json` normalizes generic `instance` / `cluster` / `nodes` /
`databases` content. `templateType=yaml` uses PyYAML. `templateType=opengemini`
normalizes `MetaNodes`, `DataNodes`, `SqlNodes`, `Databases`, retention
policies, measurements, and schema field types. Field type mapping follows the
existing openGemini labels:

```text
0 Unknown
1 Integer
2 Unsigned
3 Float
4 String
5 Boolean
6 Tag
7 Unknown
```

Readonly MCP resources/tools expose imported instance lists, snapshots, field
type lookups, and tag-field lookups. Task MCP exposes the same tools and writes
results as `metadata_slice` evidence with `final_allowed=false`; Metadata is
background context and cannot be cited by final answers as root-cause evidence.

Run startup writes `metadata_context.json` as background evidence and exposes it
through task MCP resource `logagent-v2://run/<run_id>/metadata_context`. The
context has schema version 1, a selection summary, and bounded
`metadata_instance` resources. If exactly one instance exists, V2 includes it as
`default_single`; if multiple exist, V2 scores instance id, remark, product,
environment, cluster, node, database, retention policy, measurement, and field
names against the Workspace question and mode. Selected resources include only
bounded topology/schema outlines; full snapshots remain behind
`logagent.get_metadata_snapshot` and detailed field/tag queries remain behind
their dedicated tools. The context artifact and all metadata slices use
`final_allowed=false`.

## Case Memory

V2 Case Memory stores confirmed Case schema v2 records in SQLite table `cases`.
Each row contains `source_type`, optional `task_id`, `enabled`, full
`record_json`, and a denormalized `searchable_text` field for local keyword
recall.

Supported sources:

- `manual`: created through `POST /api/v2/cases`; requires `title`, `symptom`,
  `rootCause`, and `solution`.
- `task`: created through `POST /api/v2/runs/:run_id/case`; the run must be
  `succeeded` and have a final answer. Repeated confirmation of one run returns
  the existing task Case instead of creating duplicates.

Search is dependency-light and local: V2 maintains a SQLite FTS5 table beside
`cases` and ranks query matches with `bm25`. The indexed text covers `title`,
`symptom`, `rootCause`, `solution`, product/version/environment, instance/node,
and evidence refs. If FTS5 is unavailable, V2 falls back to token-overlap
scoring. Disabled cases are excluded by default and can be included with
`includeDisabled=true`.

Readonly MCP exposes `logagent.search_cases` and `logagent.get_case`. Task MCP
exposes the same tools and writes results as `case_context` evidence with
`final_allowed=false`. Historical Cases are background references; final answers
still need current-task evidence refs.

## Skills And System Context

V2 Skills are Codex-compatible filesystem directories under
`LOGAGENT_V2_DATA_DIR/skills`. Each Skill requires `SKILL.md` with `name` and
`description` frontmatter; optional `logagent.json` defines display metadata,
`includeByDefault`, priority, and declared references.

The import API writes:

```text
skills/<skillId>/SKILL.md
skills/<skillId>/logagent.json
```

with a conservative default manifest. Workspaces can store explicit `skillIds`.
When a run starts, V2 writes `system_context.json` as a background artifact with
schema v2 resources:

```text
kind=diagnostic_skill
skillId
selectionReason
matchScore
revision
summary
content
references[]
```

If no explicit `skillIds` are set, V2 includes Skills whose manifest has
`includeByDefault=true` and auto-matches Skills whose `keywords`, `products`,
`toolIds`, `domainAdapters`, name, display name, Skill id, or description match
the Workspace question or mode. Each resource records `selectionReason` as
`explicit`, `default`, or `auto`, plus a numeric `matchScore`.

Readonly MCP exposes `logagent.list_skills`, `logagent.get_skill`,
`logagent.get_skill_reference`, and `logagent.preview_system_context` against
the current registry. Task MCP exposes the same tools, but
`logagent.get_skill_reference` is constrained to Skills and references captured
in the current run's `system_context` snapshot and persists `skill_reference`
evidence with `final_allowed=false`.

`GET /api/v2/exports/skills.zip` builds a current registry snapshot. The archive
contains each Skill directory's regular files under `<skillId>/...` plus a root
`manifest.json` with `schemaVersion`, `generatedAt`, Skill ids, display names,
revisions, source paths, and exported file metadata. Symlinks and symlinked
directories are skipped; archive paths must remain relative and cannot contain
`..`.

## Final Answer Validation

Final answers must be JSON objects with a non-empty `summary`, string arrays for
`symptoms`, `nextChecks`, `fixSuggestions`, and `missingInformation`,
`likelyRootCauses[]` objects with non-empty `cause`, and `confidence` set to
`low`, `medium`, or `high`. Scalar strings for the simple array fields are
normalized to one-item arrays.

The validator collects top-level `evidenceRefs` and
`likelyRootCauses[].evidenceRefs`, then verifies every ref against evidence rows
visible to the current run where `final_allowed=true`.

Accepted ref formats:

```text
grep_results.json#matches/<index>
log_searches/<search_id>.json#matches/<index>
log_slices/<slice_id>.json#lines
tool_results/<tool_id>/result.json#findings/<index>
```

The referenced artifact must exist and the match/finding index must be in
range. Background context such as `manifest.json`, `system_context.json`,
metadata slices, case context, and diagnostic skill references must stay
readable context and cannot be cited as final root-cause evidence.

## Waiting States

Task MCP can now request:

```text
logagent.request_user_input
logagent.request_approval
```

Both calls create an `actions` row and append timeline events. User input moves
the run to `waiting_for_user`; approval moves it to `waiting_for_approval`.
`POST /api/v2/runs/:run_id/messages` and
`POST /api/v2/actions/:action_id/decisions` requeue waiting runs into the
SQLite job queue. The current Agent runtime is still a stub, so resumed runs do
not yet perform true multi-round model reasoning.

## Security

- API key is read from `LOGAGENT_V2_API_KEY`.
- Artifact paths are resolved relative to `data_dir` and rejected if they
  escape it.
- Upload filenames are basename-normalized and character-filtered.
- Archive entries are scanned in memory and rejected if they contain absolute
  paths or traversal components.
- Tools execute only through configured whitelist descriptors, with absolute
  command paths, fixed args, timeout, and bounded stdout/stderr.
- Agent final answers are rejected before success if they cite missing,
  out-of-range, unsupported, or background-only refs.

## Acceptance

The current slice is accepted when:

- `python3 -m compileall logagent_v2` passes.
- `PYTHONPATH=. python3 -m unittest discover tests` passes.
- A Workspace can be created, an upload stored, a Run queued, and the inline
  worker can complete the stub Agent result.
