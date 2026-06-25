---
name: dev-selftest-pipeline
description: Use when Claude Code needs to run LocalToolHub dev_selftest through MCP: commit and push local code, sync an allowlisted git repo/ref, build, deploy a Docker cluster, run tests, poll queued runs, and generate a report.
---

# Dev Self-Test Pipeline

Use this skill from a Claude Code client connected to a configured LocalToolHub MCP Server. The
Server only exposes controlled MCP tools and resources; it does not run this workflow, load this
skill, or accept free-form shell.

Before calling the remote self-test tools:

1. Make any requested code changes locally.
2. Run focused local checks when practical.
3. Commit and push the branch/ref that LocalToolHub is configured to allow.
4. Confirm MCP connectivity with `initialize` and `tools/list`.

Then run the MCP workflow in `references/workflow.md`. Read that file before executing the
pipeline or diagnosing a failed step.

Important ID rule:

- `devselftest_*` is the persistent dev_selftest workspace id returned by
  `logagent.dev_selftest.sync_workspace`. Pass it to `build`, `deploy`, `run_tests`, and
  `report`.
- `task_*` is a queued Tool Runner id returned by `runMode:"queued"`. Use it only with
  `logagent.runs.get` and `logagent.runs.result`.

Never upload source archives for this workflow. Source enters LocalToolHub only through an
allowlisted git repo/ref after the local client has pushed the change.
