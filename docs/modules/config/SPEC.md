# Config Spec

## Required Keys

```yaml
server.bind
storage.data_dir
auth.api_keys[].value_env
mcp.enabled
```

## Optional Keys

```yaml
tools.directories
fetch.enabled
fetch.allowed_hosts
executors.enabled
code_evidence.repos
skills.roots
cases.enabled
llm.enabled
```

## Acceptance

- 缺少 LLM/Claude 配置时 Server 可启动。
- 缺少 required secret env 时启动失败并指出变量名。
- 配置样例不包含密钥原文。
