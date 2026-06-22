# Deployment Spec

## Runtime Layout

```text
bin/logagent-local
bin/tools/
webui/out/
data/
deploy/logagent.yaml
```

## Acceptance

- `logagentctl.sh start/status/stop` 可用。
- `rebuild-install.sh` 不删除 data。
- 无 LLM 配置时部署可用。
