# Config Spec

## 目标

统一使用 `logagent.yaml` 描述 Server、Native Agent、存储、安全和模块配置。

## 当前状态

Server 和 Native Agent 已读取部分配置。示例文件：

- `examples/logagent.yaml`
- `examples/server-test.yaml`
- `examples/native-agent-remote-50992.yaml`

## 配置范围

当前已用：

- `server.bind`
- `server.public_base_url`
- `native_agent.bind`
- `native_agent.server_base_url`
- `native_agent.api_key_env`
- `native_agent.allowed_dirs`
- `native_agent.allowed_suffixes`
- `native_agent.request_timeout_seconds`
- `native_agent.upload_chunk_bytes`
- `storage.data_dir`
- `storage.max_upload_bytes`
- `storage.max_chunk_bytes`
- `auth.api_keys`
- `log_analyzer.keywords`
- `log_analyzer.max_matches`

待扩展：

- tool runner 白名单
- product/version 到代码仓 ref 映射
- SSH/SCP 测试环境节点
- LLM provider 和模型
- Case Store 存储路径

## 密钥规则

配置文件只保存环境变量名，不直接保存密钥值。

```yaml
auth:
  api_keys:
    - name: "native-agent"
      value_env: "LOGAGENT_NATIVE_API_KEY"
```

## 验收标准

- 缺少必要密钥环境变量时启动失败。
- 配置有默认值，但示例文件必须展示推荐值。
- README 和 SPEC 在配置字段变更时同步更新。
