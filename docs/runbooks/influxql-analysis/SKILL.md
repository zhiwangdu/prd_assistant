---
name: InfluxQL Analysis
description: Interpret InfluxQL analyzer findings and decide when to run query analysis tools.
---

Use this skill when logs, user questions, or tool results mention InfluxQL, SQL-like query text, slow query behavior, unsupported syntax, or query planner/engine failures.

Operational guidance:
- Prefer current log evidence and `tool_results/<action_id>/result.json#findings/<index>` for final root cause citations.
- Use `influxql_analyzer` only through the LogAgent Tool Runner or MCP domain tool boundary.
- Treat analyzer output as structured findings, not as proof by itself unless the finding is present in task tool artifacts.
- When analyzer input is JSONL, each line should contain one query record with a query string and optional metadata such as file, line, and timestamp.

Read the declared reference for finding interpretation details when needed.
