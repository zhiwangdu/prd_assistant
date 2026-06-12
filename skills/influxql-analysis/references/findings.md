# InfluxQL Analyzer Findings

Use analyzer findings as task evidence only when they are present under `tool_results/<action_id>/result.json#findings/<index>`.

Common finding interpretation:

- `error` or `high`: query shape is likely invalid, unsafe, or known to trigger a failure mode.
- `warning` or `medium`: query may be valid but should be reviewed for missing time bounds, broad scans, unsupported functions, or expensive predicates.
- `info` or `low`: context that can guide next checks but usually should not be the sole root cause.

For JSONL input, preserve source file and line when possible. For compare mode, cite only concrete delta findings that appear in tool output.
