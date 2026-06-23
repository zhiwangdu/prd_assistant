---
name: GeminiDB Influx Instance Management
description: Runbook for managing HuaweiCloud GeminiDB Influx (NoSQL) instances via the six built-in tools — create, delete, list, rename, toggle SSL, restart.
---

Use this skill when you need to manage the lifecycle of HuaweiCloud GeminiDB Influx
(NoSQL) instances: create, delete, list/query, rename, toggle SSL, or restart an
instance or node. All six tools share one HTTP client and one configuration block.

## Prerequisites

- `huawei_cloud.gemini_db.enabled: true` in the server config, with:
  - `endpoint` — the NoSQL API base URL for the target region (e.g. `https://nosql.cn-north-4.myhuaweicloud.com`).
  - `project_id` (or `project_id_env`) — the target project.
  - `auth_token_env` — an env var holding a valid IAM `X-Auth-Token`.
- The six tools are **disabled and greyed out in the catalog** until this block is
  enabled. The auth token is read from env only and is never accepted via tool params.

## Endpoint is dynamically configurable

Every tool accepts optional `endpoint` and `projectId` params that **override** the
configured defaults for that single run. Use this to target a different region or
project without editing config or restarting the server. The `X-Auth-Token` is always
taken from the configured env var regardless of the override.

## The six tools

Base path: `{endpoint}/v3/{projectId}/instances`. Auth: `X-Auth-Token` header.

| Tool | Method & path | Key params |
|---|---|---|
| `logagent.geminidb.create_instance` | `POST /v3/{pid}/instances` | documented create fields: `name`, `datastore`, `region`, `availabilityZone`, `vpcId`, `subnetId`, `securityGroupId`, `password`, `mode`, `flavor`; optional `body` advanced escape hatch |
| `logagent.geminidb.delete_instance` | `DELETE /v3/{pid}/instances/{instanceId}` | `instanceId` |
| `logagent.geminidb.list_instances` | `GET /v3/{pid}/instances?datastore_type=influxdb&id=&name=&mode=&vpc_id=&subnet_id=&offset=&limit=` | filters optional; the tool defaults `datastore_type=influxdb`; `id` fetches one instance |
| `logagent.geminidb.rename_instance` | `PUT /v3/{pid}/instances/{instanceId}/name` | `instanceId`, `name` (sent as `{"name": name}`) |
| `logagent.geminidb.toggle_ssl` | `POST /v3/{pid}/instances/{instanceId}/ssl-option` | `instanceId`, `sslOption` (`on` or `off`, sent as `{"ssl_option": ...}`) |
| `logagent.geminidb.restart_instance` | `POST /v3/{pid}/instances/{instanceId}/restart` | `instanceId`, optional `nodeId` (sent as `{"node_id": ...}`); omit `nodeId` to restart the whole instance with no body |

`create_instance` maps camelCase tool params to the documented snake_case request body.
It also accepts an advanced `body` object for exact raw create payloads, but the default
tool template uses the documented fields. `toggle_ssl` and `restart_instance` build their
documented request bodies from `sslOption` and `nodeId`; they no longer use guessed
`ssl` booleans or raw body templates.

`instanceId` is validated to contain only letters, digits, `_`, `-` (path-safe).

## Reading results

Each run writes `result.json`:
- `status` — `OK` (HTTP 2xx) or `FAILED`.
- `http` — `method`, `path`, `url`, `ok`, `statusCode`.
- `request.body` — the request body **with sensitive fields redacted** (`password`,
  `secret`, `token`, `ak`/`sk` keys → `<redacted>`).
- `response` — `statusCode`, `body` (truncated to 64 KiB), `truncated`.
- `endpoint` — resolved `baseUrl`, `projectId`, `region`.
- `credentialMetadata.authTokenEnv` — the env var name the token came from (never the token).
- `timings.totalMs`, `warnings[]`, `error`.

Cite `http.statusCode` + `response.body` for API evidence, and `request.body` (redacted)
for what was sent.

## Notes

- These tools perform real management actions on cloud resources; treat create/delete/
  restart as high-risk. Prefer `list_instances` (read-only) to confirm state first.
- The `X-Auth-Token` is injected by the server from the configured env var; it never
  appears in params, logs, or `result.json`.
