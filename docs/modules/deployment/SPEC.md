# Deployment Spec

## Runtime Layout

```text
bin/logagent-server
bin/tools/
webui/out/
data/
data/uploads/
data/workspaces/
data/tasks/
data/dev_selftest/
deploy/logagent.yaml
```

## Acceptance

- `logagentctl.sh start/status/stop` 可用。
- `rebuild-install.sh` 不删除 data。
- 无 LLM/Agent/Fetch/Executor/Metadata/Case/Skills 配置时部署可用。
- dev_selftest 默认关闭时，占位 Docker 配置不阻断启动。
- 日志 analyzer 二进制缺失时，工具可出现在 catalog 但保持不可运行。
