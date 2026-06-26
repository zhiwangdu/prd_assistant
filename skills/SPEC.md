# Skills Spec

## Goal

The top-level `skills/` tree distributes optional Claude Code skills for users who want a local
client to drive LocalToolHub MCP tools. Skills live on the client side and encode workflow
knowledge that should not become Server logic.

## Non-Goals

- No Server-side skill registry.
- No skill install, update, search, or download API.
- No workflow engine, agent loop, or runbook compatibility endpoint in Server.
- No legacy `logagent.json` manifest for newly distributed skills.
- No secrets or generated runtime artifacts in skill files.

## Distribution Contract

- A skill is a directory under `skills/<skill-id>/`.
- `skill-id` uses lowercase letters, digits, and hyphens.
- `SKILL.md` is required and must include YAML frontmatter with `name` and `description`.
- `references/` is optional and contains detailed workflow or schema documentation that the
  client loads only when needed.
- Skills may reference LocalToolHub MCP tool names, but they must not assume direct filesystem or
  private API access to Server internals.

## Runtime Contract

The Server contract available to a skill is MCP only:

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

Current HTTP and stdio transports are:

```text
POST /api/mcp
logagent-server mcp-serve
```

Skills may ask the client to call `tools/list` before execution and then call only published
tools with schema-valid arguments. Long-running calls must use `runMode:"queued"` and poll with
`logagent.runs.get` / `logagent.runs.result`; those platform tools read run records and do not
create extra tool runs. dev_selftest skills must read `logagent://dev_selftest/config` through
`resources/read` before selecting repo/ref/profile ids.

## Dev Self-Test Skill

`skills/dev-selftest-pipeline/` is the canonical client workflow for development self-test:

- Claude Code edits code locally, commits, and pushes; it must not run local compile/build/test
  steps by default because the client may be Windows or otherwise lack the Linux target toolchain.
- LocalToolHub Server pulls only allowlisted git repo/ref values through
  `logagent.dev_selftest.sync_workspace`; the skill must use the MCP config resource as the
  source of truth for allowed values.
- If the user asks for a repo/ref not present in `logagent://dev_selftest/config`, the skill must
  stop, ask for explicit consent, call `logagent.dev_selftest.allowlist.update` with
  `confirmedUserConsent:true` only after consent, then reread the config resource before
  continuing.
- Remote `build` is the first build authority. On any remote step failure, the client reads
  `logagent.runs.result` and calls `logagent.dev_selftest.diagnose` for the persistent
  `devselftest_*` run before deciding whether to fix source, inspect Docker evidence, or ask for
  cleanup. Source fixes are committed/pushed, followed by `sync_workspace` and retry.
- The client carries the returned `devselftest_*` workspace id through `build`, `deploy`,
  `run_tests`, `report`, and the optional post-report `cleanup`.
- Environment cleanup is explicit and optional: call `logagent.dev_selftest.cleanup` after
  `report` only when the user or workflow wants to release the Docker compose resources. Cleanup
  keeps source, logs, artifacts, progress, and reports for audit.
- Queued execution returns `task_*` ids for polling only; a `task_*` id must not be passed as the
  dev_selftest workspace id.
- The skill must not SSH to the Server to read config, scan a local `prd_assistant` checkout for
  Server config, or force-push an old allowlisted branch just to satisfy the allowlist unless the
  user explicitly asks for that operation.

## Acceptance

- `skills/README.md` and this spec describe every top-level skill.
- Skill references match the current MCP tool names and parameter shapes.
- Removing or changing a Server tool updates affected skills in the same commit.
- `git diff --check` passes for skill-only changes.
