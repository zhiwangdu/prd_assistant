# Skills

`skills/` is the repository distribution directory for client-installed Claude Code skills.
These skills teach an external MCP client how to orchestrate LocalToolHub tools, but they are
not loaded, indexed, served, or installed by the Rust Server.

## Boundary

- Skills are copied or symlinked by the user into the local Claude Code skills directory.
- LocalToolHub Server does not scan `skills/`, expose a skill registry, or provide skill
  download/install APIs.
- Runtime integration goes only through MCP `initialize`, `resources/list`, `resources/read`,
  `tools/list`, and `tools/call`. dev_selftest workflows must read
  `logagent://dev_selftest/config` before choosing repo/ref/profile ids.
- Skill content must not contain secrets, local tokens, generated run data, or machine-specific
  workspaces.

## Layout

Each skill directory must be self-contained:

```text
skills/<skill-id>/
  SKILL.md
  references/
    *.md
```

`SKILL.md` contains concise trigger and workflow guidance. Detailed schemas, result shapes, and
long procedures belong in `references/`. New top-level skills do not use legacy `logagent.json`
manifests; those belonged to the removed server-side skill registry.

## Available Skills

| Skill | Purpose |
|-------|---------|
| `dev-selftest-pipeline/` | Claude Code orchestration for `logagent.dev_selftest.*`: discover the allowlist and profile details from MCP, commit/push local code, skip local builds by default, sync the allowlisted git ref, request user consent before allowlist updates, use remote build/deploy/tests or externally-created cloud targets with non-secret `testParams`, poll queued runs, diagnose failed steps from bounded MCP evidence, generate a report, and optionally call cleanup after reporting. |

## Maintenance

When a server MCP tool schema or behavior changes, update the relevant skill, this README,
[`SPEC.md`](./SPEC.md), the affected component README/SPEC, and root
[`PROGRESS.md`](../PROGRESS.md) in the same change.
