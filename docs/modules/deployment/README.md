# Deployment

Deployment 面向个人本地或单台内网机器部署。

## 目标

- Rust binary 可复制。
- WebUI static 可复制。
- source-built tools 可复制。
- data 目录独立，不随重建删除。
- dev_selftest 部署模板可按需生成 openGemini 集群或 InfluxDB 单机配置。

## 非目标

- Kubernetes。
- 多租户权限。
- 集中式数据库。

## dev_selftest 配置模板

- `deploy/probe-opengemini-config.sh` 生成 openGemini 3 meta + 3(sql+store) Docker demo 配置。
- `deploy/probe-influxdb-config.sh` 生成 InfluxDB OSS v1 单机 Docker demo 配置，默认构建 `master-1.x` 的 `influxd`。
