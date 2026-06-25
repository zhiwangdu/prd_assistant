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
server.public_base_url
server.max_concurrent_tasks
storage.max_upload_bytes
storage.max_chunk_bytes
log_analyzer.max_matches
log_analyzer.keywords
tools.<analyzer>.enabled
tools.<analyzer>.path | tools.<analyzer>.path_env
tools.<analyzer>.args
tools.<analyzer>.match
remote_execution.commands
dev_selftest.enabled
dev_selftest.git
dev_selftest.builds
dev_selftest.docker.clusters
dev_selftest.test_suites
mcp.allowed_origins
```

## Acceptance

- 缺少 LLM/Agent/Fetch/Executor/Metadata/Case/Skills 配置时 Server 可启动。
- 缺少 required secret env 时启动失败并指出变量名。
- 配置样例不包含密钥原文。
- `dev_selftest.enabled=false` 时，未填写的 docker binary / compose path 不阻断启动。
- `dev_selftest.enabled=true` 时，build/docker/test 路径必须绝对且来自 allowlist；`sync_workspace` 只接受 allowlisted git repo/ref。
- `remote_execution.commands` 只作为 dev_selftest `test_suites.*.command` 的 argv/timeout 模板。
