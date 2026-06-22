# Deployment

Deployment 面向个人本地或单台内网机器部署。

## 目标

- Rust binary 可复制。
- WebUI static 可复制。
- source-built tools 可复制。
- data 目录独立，不随重建删除。

## 非目标

- Kubernetes。
- 多租户权限。
- 集中式数据库。
