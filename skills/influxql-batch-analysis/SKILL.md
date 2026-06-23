---
name: InfluxQL Batch Log Analysis
description: Runbook for batch-analyzing node log packages end-to-end with the InfluxQL analyzer (upload -> unpack/preprocess -> analyze).
---

Use this skill when you need to analyze InfluxQL query behavior across one or more node log packages in a single batch run, rather than inspecting one query file at a time.

## Flow

1. **Upload + preprocess**: feed the node log packages (`.tar.gz` / `.tgz` / `.tar`) as inputs to the `logagent.batch_influxql_analysis` tool. The tool unpacks the archives, normalizes rotated logs, extracts lines that look like InfluxQL queries, and materializes one JSONL record per query under `tool_inputs/influxql_analyzer/<node>/<timestamp>.jsonl`.
2. **Analyze**: the `influxql_analyzer` binary runs once per materialized JSONL input, using the analyzer's configured args (e.g. `-detail-limit`). Findings are aggregated into a single result.
3. **Read**: inspect the combined `result.json` — preprocess summary, per-input findings, warnings, and an overall status.

The batch tool drives the configured `influxql_analyzer` and bypasses its per-run `max_input_files` cap by iterating inputs internally (with a 200-input safety cap). No params are required.

## Input expectations

The batch tool reuses the shared log-package preprocessor, so uploads must follow its package contract (same as `logagent.preprocess_log_package`):

- **Package filename**: `<package_id>_<instance_id>_<node_id>_<YYYY>_<MM>_<DD>_<HH>_<MM>_<SS>_<micros>_logs.tar.gz` — `node_id`, `instance_id`, and `package_timestamp` are parsed from the name and attached to each finding. IDs are alphanumeric; the timestamp is 7 numeric fields. Other names are accepted as uploads but yield no parsed node/timestamp metadata.
- **Log paths inside the tar**: files must live under `var/chroot/gemini/log/tsdb`, `var/chroot/gemini/log/stream`, or `home/Ruby/log` (classified into the `tsdb`, `stream`, or `agent` log group). Files outside these prefixes are ignored.
- **Query extraction**: each log line is scanned for an InfluxQL query. A line is recognized if it is a JSON object with a `query`/`sql`/`stmt`/`statement` field (e.g. `{"timestamp":"...","query":"select * from cpu limit 100000"}`), or a `query="..."`/`sql="..."` key=value line. Free-text lines with a query embedded in prose are not extracted.
- Packages with no extractable InfluxQL queries produce no findings (`status: FAILED`, warning) — expected for non-query logs.

## Prerequisites

- `influxql_analyzer` must be configured and enabled: build the binary with `scripts/build-tools.sh --only influxql` and set `enabled: true` (path via `LOGAGENT_INFLUXQL_ANALYZER_PATH` or the default `bin/tools/` location). The batch tool is disabled in the catalog until this is true.

## Reading results

- `result.json#preprocessSummary` — `uploads`, `extractedFiles`, `influxqlInputs` (materialized query files), `nodes`.
- `result.json#findings[]` — one entry per analyzed input: `inputFile`, `nodeId`, `instanceId`, `packageTimestamp`, `artifactPath`, `summary`, and the raw analyzer `result`. Cite `findings/<index>` together with `inputFile`/`nodeId` for root-cause evidence.
- `result.json#warnings[]` — packages with no InfluxQL queries, per-input analyzer failures, or batches exceeding the input cap.
- `result.json#status` — `OK` (all inputs analyzed), `PARTIAL` (some inputs failed), `FAILED` (no findings).

For interpreting individual finding severity and the JSONL input shape, see the `influxql-analysis` skill and `references/batch-result.md`.
