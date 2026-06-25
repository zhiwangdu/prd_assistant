# Interfaces Spec

## HTTP

Core endpoints:

```text
/health
/
/api/uploads
/api/uploads/:upload_id
/api/tools
/api/tools/:tool_id
/api/tools/:tool_id/runs
/api/runs
/api/runs/:run_id
/api/runs/:run_id/result
/api/runs/:run_id/artifacts
/api/artifacts/:artifact_id
/api/mcp
```

`/api/mcp` respects `mcp.enabled`; when disabled it returns a JSON-RPC error instead of listing resources or tools.

Removed interfaces must stay absent from the current API surface:

```text
/api/fetch
/api/metadata
/api/cases
/api/skills
/api/executors
/api/executor-runs
```

## MCP

Must support:

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

Current resources:

```text
logagent://runs/recent
logagent://tools/catalog
```

Current tool groups:

```text
logagent.preprocess_log_package
logagent.batch_influxql_analysis
configured log analyzers
logagent.dev_selftest.*
logagent.runs.get
logagent.runs.result
```

## Acceptance

- MCP tools/list equals WebUI catalog.
- HTTP and MCP errors are structured and do not leak secrets.
- MCP resources/list returns only current resources.
- Removed API paths are not documented as supported behavior.
