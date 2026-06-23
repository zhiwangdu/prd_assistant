# GeminiDB Influx API request fields

Reference for the request bodies/params accepted by the six GeminiDB Influx tools. The
`create` / `toggle_ssl` / `restart` tools forward `body` verbatim, so supply the exact
documented JSON. **Field names below are best-known from the HuaweiCloud NoSQL API family;
verify against the live docs before relying on them in production.**

Doc index: <https://support.huaweicloud.com/api-nosql/topic_300000002.html>
Creating an instance: <https://support.huaweicloud.com/api-nosql/nosql_05_0007.html>

Auth: `X-Auth-Token` header. Base path: `{endpoint}/v3/{project_id}/instances`.

## create_instance — `POST /v3/{pid}/instances`

`body` (forwarded verbatim). Best-known fields:

- `name` *(string, required)* — instance name, 4–64 chars, `^[a-zA-Z0-9._-]+$`-ish.
- `datastore` *(object, required)* — `{"type": "influxdb", "version": "1.7"}`.
- `engine` *(string)* — e.g. `influxdb`.
- `mode` *(string, required)* — `Cluster` or `Single`.
- `flavor_ref` *(string, required)* — spec code, e.g. `gemini.cassandra.xlarge.4`.
- `volume` *(object, required)* — `{"size": <GB>}`.
- `region` *(string, required)* — e.g. `cn-north-4`.
- `availability_zone` *(string, required)* — e.g. `cn-north-4a`.
- `vpc_id` *(string, required)*.
- `subnet_id` *(string, required)*.
- `security_group_id` *(string, required)*.
- `password` *(string, required)* — the tool redacts this in the stored `request.body`.
- `backup_strategy` *(object, optional)* — `{"start_time": "00:00-01:00", "keep_days": 7}`.
- `charging_mode` *(string, optional)* — `postPaid` / `prePaid`.

Response: `{"id": "<instance_id>", "job_id": "<job_id>"}`.

## delete_instance — `DELETE /v3/{pid}/instances/{instanceId}`

No body. `instanceId` path param. Response: `{"job_id": "<job_id>"}`.

## list_instances — `GET /v3/{pid}/instances`

Optional query filters (all optional): `id`, `name`, `mode`, `datastore_type`, `vpc_id`,
`subnet_id`, `offset`, `limit`. Pass `id` to fetch a single instance's details.

Response: `{"instances": [ { "id", "name", "status", "mode", "datastore": {...}, "engine", "flavor_ref", "volume": {...}, ... } ], "total_count": N}`.

## rename_instance — `PUT /v3/{pid}/instances/{instanceId}/name`

Body built by the tool from params: `{"name": "<new name>"}`. `instanceId` + `name` required.

## toggle_ssl — `PUT /v3/{pid}/instances/{instanceId}/ssl`

`body` (forwarded verbatim). Best-known field (verify): `{"ssl": true}` or `{"ssl": false}`.
If the live doc uses a different key (e.g. `ssl_option`), pass that exact shape in `body`.

## restart_instance — `POST /v3/{pid}/instances/{instanceId}/restart`

Optional `body` (forwarded verbatim). Omit `body` (or pass `{}`) to restart the whole
instance. To restart a single node, pass the documented node-targeting fields, e.g.
`{"node_id": "<node_id>"}` (verify the exact field against the live doc).

Response: `{"job_id": "<job_id>"}`.

## Verification note

The GeminiDB NoSQL API path/method conventions above (`/v3/{project_id}/instances...`) and
`X-Auth-Token` auth are stable across the NoSQL service family. The exact body field names
for SSL toggle and node restart are the least certain — because the `body` is forwarded
verbatim, you can always supply the exact documented shape without waiting for a tool update.
