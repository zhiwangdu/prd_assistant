# Batch InfluxQL analysis result

`result.json` from `logagent.batch_influxql_analysis` has this shape:

```json
{
  "schemaVersion": 1,
  "toolId": "logagent.batch_influxql_analysis",
  "actionId": "act_tool_batch_influxql_<task_id>",
  "status": "OK | PARTIAL | FAILED",
  "preprocessSummary": {
    "uploads": 0,
    "extractedFiles": 0,
    "influxqlInputs": 0,
    "nodes": 0
  },
  "analyzedInputs": 0,
  "failedCount": 0,
  "findings": [
    {
      "inputFile": "tool_inputs/influxql_analyzer/<node>/<timestamp>.jsonl",
      "nodeId": "<node>",
      "instanceId": "<instance>",
      "packageTimestamp": "<ts>",
      "artifactPath": "<workspace-relative artifact path>",
      "summary": "<analyzer summary>",
      "result": { "/* raw influxql_analyzer JSON output */": "..." }
    }
  ],
  "warnings": ["..."],
  "durationMs": 0,
  "createdAt": "<iso8601>"
}
```

## Field notes

- **`status`** — `OK` when every materialized input was analyzed; `PARTIAL` when one or more inputs failed but at least one succeeded; `FAILED` when there are no findings (no InfluxQL queries materialized, or every input failed).
- **`preprocessSummary`** — counts from the unpack/preprocess phase. `influxqlInputs` is the number of materialized JSONL files (one per node/timestamp with queries); `nodes` is the distinct node count among them.
- **`findings[]`** — one entry per analyzed input. The `result` object is the verbatim JSON produced by the `influxql_analyzer` binary for that input (see the `influxql-analysis` skill for finding-severity interpretation). Cite `findings/<index>` plus `inputFile`/`nodeId` for evidence.
- **`warnings[]`** — includes: packages that yielded no InfluxQL queries, per-input analyzer failures (with the failing `inputFile`), and a notice when more than 200 inputs were truncated.
- **`artifactPath`** — workspace-relative path to the per-input analyzer artifact; the aggregate result is at `tool_results/<actionId>/result.json`.
