# GeminiDB Influx API request fields

Reference for the six GeminiDB Influx tools. These fields are taken from the
HuaweiCloud GeminiDB API v3 instance-management docs.

Docs:

- API v3 instance management index: <https://support.huaweicloud.com/api-nosql/topic_300000002.html>
- Create instance: <https://support.huaweicloud.com/api-nosql/nosql_05_0014.html>
- Delete instance: <https://support.huaweicloud.com/api-nosql/nosql_05_0015.html>
- List/query instances: <https://support.huaweicloud.com/api-nosql/nosql_05_0016.html>
- Rename instance: <https://support.huaweicloud.com/api-nosql/nosql_05_0102.html>
- Toggle SSL: <https://support.huaweicloud.com/api-nosql/nosql_05_0107.html>
- Restart instance or node: <https://support.huaweicloud.com/api-nosql/nosql_05_0108.html>

Auth: `X-Auth-Token` header. Base path: `{endpoint}/v3/{project_id}/instances`.

## create_instance — `POST /v3/{pid}/instances`

Canonical tool params are camelCase, then mapped to the documented snake_case body:

- `name` *(required)* — instance name, 4..64 bytes.
- `datastore` *(required object)* — must include `type: "influxdb"`; documented
  Influx values include `version: "1.8"` or `"1.7"` and `storage_engine: "rocksDB"`.
- `region` *(required)*.
- `availabilityZone` *(required)* -> `availability_zone`.
- `vpcId` *(required)* -> `vpc_id`.
- `subnetId` *(required)* -> `subnet_id`.
- `securityGroupId` *(required)* -> `security_group_id`.
- `password` *(required)* — stored result redacts this value.
- `mode` *(required)* — Influx values: `Cluster`, `CloudNativeCluster`,
  `EnhancedCluster`, `InfluxdbSingle`.
- `flavor` *(required array)* — elements use documented fields `num`, `size`,
  `storage`, `spec_code`.
- Optional mapped fields: `productType`, `diskEncryptionId`, `configurationId`,
  `backupStrategy`, `enterpriseProjectId`, `sslOption` (`"0"` or `"1"`),
  `chargeInfo`, `dedicatedResourceId`, `port`, `restoreInfo`,
  `availabilityZoneDetail`.

Advanced escape hatch: pass exact raw documented JSON as `body`. If `body` is present,
the tool sends that object instead of building from structured params, but still validates
the documented required create fields, `body.datastore.type == "influxdb"`, and a
non-empty `body.flavor` array.

## delete_instance — `DELETE /v3/{pid}/instances/{instanceId}`

No body. `instanceId` path param. Response contains `job_id` on accepted deletion.

## list_instances — `GET /v3/{pid}/instances`

The tool is scoped to GeminiDB Influx and defaults query `datastore_type=influxdb`.
Optional filters: `id`, `name`, `mode`, `vpcId` -> `vpc_id`,
`subnetId` -> `subnet_id`, `offset`, `limit`.

`limit` is documented as `1..100`; omitted by HuaweiCloud means 100. `offset` defaults
to 0 when omitted. `id` and `name` support the documented `*prefix` fuzzy matching form.

## rename_instance — `PUT /v3/{pid}/instances/{instanceId}/name`

Body built by the tool from params: `{"name": "<new name>"}`. `name` must be 4..64 bytes.
Successful response has HTTP 204 and no response body.

## toggle_ssl — `POST /v3/{pid}/instances/{instanceId}/ssl-option`

Required param: `sslOption`, either `on` or `off`. Body sent by the tool:

```json
{ "ssl_option": "on" }
```

or:

```json
{ "ssl_option": "off" }
```

Response contains `job_id` on accepted switch.

## restart_instance — `POST /v3/{pid}/instances/{instanceId}/restart`

Omit `nodeId` to restart the whole instance; the tool sends no request body. If `nodeId`
is present, the tool sends:

```json
{ "node_id": "<node_id>" }
```

HuaweiCloud's current docs note `node_id` is only supported for GeminiDB Redis
cloud-native cluster node restart. For GeminiDB Influx, use whole-instance restart unless
the service docs change.
