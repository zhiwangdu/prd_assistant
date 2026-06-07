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
- `server.max_concurrent_tasks`
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
- `llm.provider`
- `llm.base_url_env`
- `llm.api_key_env`
- `llm.model_env`
- `llm.model`
- `llm.request_timeout_seconds`
- `llm.max_input_chars`
- `llm.max_output_tokens`
- `tools.<name>.enabled`
- `tools.<name>.path`
- `tools.<name>.timeout_seconds`
- `tools.<name>.max_output_bytes`
- `tools.<name>.args`
- `tools.<name>.match.file_patterns`
- `tools.<name>.match.keywords`

待扩展：

- product/version 到代码仓 ref 映射
- SSH/SCP 测试环境节点
- metadata store 路径和模板导入限制；当前 store 使用 `storage.data_dir/metadata`，模板支持 YAML/JSON/openGemini `/getdata`
- LLM 多轮重试、用量和 request id 审计
- Analysis Agent 轮数、调用、动作、重复动作、追问和运行时间预算
- action 审批策略
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
- `server.max_concurrent_tasks` 默认 2，并发下限为 1。
- `llm.provider` 默认 `stub`；真实 Provider 缺少 URL 或 API Key 环境变量时启动失败。
- `llm.model_env` 配置后优先于 `llm.model`；对应环境变量缺失或模型名为空时启动失败。
- 启用的 tool path 必须是绝对路径；非法工具名或相对路径启动失败。
- 用户输入不能覆盖 tool path 或自由 argv。
- 预算字段必须大于零且有上限；未知 action 类型启动失败。
- 用户输入不能修改预算、白名单和审批策略。
- README 和 SPEC 在配置字段变更时同步更新。
