---
name: dev-selftest-pipeline
description: Use when Claude Code needs to run LocalToolHub dev_selftest through MCP: commit and push local code, sync an allowlisted git repo/ref, build, deploy a Docker cluster, run tests, poll queued runs, diagnose failures from run evidence, generate a report, and optionally clean up the Docker environment.
---

# Dev Self-Test Pipeline

Use this skill from a Claude Code client connected to a configured LocalToolHub MCP Server. The
Server only exposes controlled MCP tools and resources; it does not run this workflow, load this
skill, or accept free-form shell.

Before calling the remote self-test tools:

1. Make any requested code changes locally.
2. Do not run local compile, build, unit-test, integration-test, Docker, or cluster checks unless
   the user explicitly asks for them and the local OS/toolchain is known to match the target.
   Windows clients commonly cannot build the Linux target; remote MCP `build` is the source of
   truth.
3. Read MCP resource `logagent://dev_selftest/config`; use only the repo/ref/profile ids and
   profile details returned there.
4. Commit and push the branch/ref that LocalToolHub is configured to allow.
5. If the needed repo/ref is not in `logagent://dev_selftest/config`, stop and ask the user whether
   to update the Server allowlist. Only after explicit consent, call
   `logagent.dev_selftest.allowlist.update` with `confirmedUserConsent:true`, then reread the
   config resource.
6. If the needed Docker build/test profile is absent or wrong, stop and ask the user whether to
   update Server profiles. Only after explicit consent, call
   `logagent.dev_selftest.profiles.upsert` with `confirmedUserConsent:true`, then reread the
   config resource.
7. Immediately call `sync_workspace`, then rely on remote `build`/`deploy`/`run_tests` results for
   feedback. If a remote step fails, read `logagent.runs.result`, call
   `logagent.dev_selftest.diagnose` for the failed `devselftest_*` run, then use the returned
   evidence/category before changing code or asking to clean up.
   For externally-created cloud DB instances, skip `deploy` and pass only non-secret
   `testParams` such as `caseName`, `instanceId`, and `endpoint` to `run_tests`; ToolHub injects
   them as Docker env vars and does not own cloud instance lifecycle.
8. Confirm MCP connectivity with `initialize` and `tools/list`.

Then run the MCP workflow in `references/workflow.md`. Read that file before executing the
pipeline or diagnosing a failed step.

Important ID rule:

- `devselftest_*` is the persistent dev_selftest workspace id returned by
  `logagent.dev_selftest.sync_workspace`. Pass it to `build`, `deploy`, `run_tests`, and
  `report`; pass it to `cleanup` only after reporting when the user or workflow explicitly wants
  to release the Docker compose resources.
- `task_*` is a queued Tool Runner id returned by `runMode:"queued"`. Use it only with
  `logagent.runs.get` and `logagent.runs.result`.

Never upload source archives for this workflow. Source enters LocalToolHub only through an
allowlisted git repo/ref after the local client has pushed the change.

Never SSH to the Server or scan a local `prd_assistant` checkout to discover Server config. Never
force-push to an old allowlisted branch just to satisfy the allowlist unless the user explicitly
asks for that operation.
