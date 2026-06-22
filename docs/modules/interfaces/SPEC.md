# Interfaces Spec

## HTTP

Core endpoints:

```text
/api/tools
/api/runs
/api/artifacts
/api/metadata
/api/fetch
/api/executors
/api/mcp
/api/settings
```

`/api/mcp` respects `mcp.enabled`; when disabled it returns a JSON-RPC error instead of listing resources or tools.

## MCP

Must support:

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

## Acceptance

- MCP tools/list equals WebUI catalog.
- HTTP and MCP errors are structured and do not leak secrets.
