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

- Each upload is a tarball of node logs (e.g. `<node>_logs.tar.gz`). Loose log files are not accepted — package them first.
- The preprocessor extracts InfluxQL queries from log lines (`select`, `show series`, `show measurements`, …). Packages with no InfluxQL queries produce no findings (status `FAILED`, warning) — that is expected for non-query logs.

## Prerequisites

- `influxql_analyzer` must be configured and enabled: build the binary with `scripts/build-tools.sh --only influxql` and set `enabled: true` (path via `LOGAGENT_INFLUXQL_ANALYZER_PATH` or the default `bin/tools/` location). The batch tool is disabled in the catalog until this is true.

## Reading results

- `result.json#preprocessSummary` — `uploads`, `extractedFiles`, `influxqlInputs` (materialized query files), `nodes`.
- `result.json#findings[]` — one entry per analyzed input: `inputFile`, `nodeId`, `instanceId`, `packageTimestamp`, `artifactPath`, `summary`, and the raw analyzer `result`. Cite `findings/<index>` together with `inputFile`/`nodeId` for root-cause evidence.
- `result.json#warnings[]` — packages with no InfluxQL queries, per-input analyzer failures, or batches exceeding the input cap.
- `result.json#status` — `OK` (all inputs analyzed), `PARTIAL` (some inputs failed), `FAILED` (no findings).

For interpreting individual finding severity and the JSONL input shape, see the `influxql-analysis` skill and `references/batch-result.md`.
