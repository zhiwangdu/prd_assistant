# LLM Gateway

## 定位

LLM Gateway 是 Server 内部的受限模型辅助模块，当前用于 Case import、task alias 生成和少量兼容恢复路径。Log Analysis 的主要调查循环由 Analysis Orchestrator + Agent Provider Runtime 承担，不由 LLM Gateway 决定状态流转。

LLM Gateway 负责：

- OpenAI-compatible 等普通 LLM provider 适配。
- Case import、alias 和兼容恢复路径的 prompt 组装。
- token/字符预算下的输入裁剪。
- 结构化 JSON schema 校验和有限解析修正重试。
- 返回 provider usage、request id 和错误分类供审计。

LLM Gateway 不负责：

- 保存 Session/Run 状态。
- 调用 Tool Runner、Fetch、Code Evidence、Remote Executor 或 SSH/SCP。
- 调用 Claude Code CLI 或管理 task MCP。
- 决定 WAITING/SUCCEEDED/FAILED 状态。
- 保存模型隐藏思维链。

## 当前实现

已实现：

- `stub` provider。
- OpenAI-compatible `/chat/completions` provider。
- `binary` provider 兼容分支，固定调用 `<binary_path> run <prompt>`。
- Case import 结构化解析和用户补全循环。
- 成功 run 的短 alias 生成；失败时使用 Server fallback。
- FinalAnswer parser 和 evidence ref 规范化，供兼容恢复路径复用。
- provider HTTP、鉴权、限流、输入过大、超时、5xx、transport、decode 和 schema 错误分类。
- 进程内 `/api/debug/llm` response content debug 开关，默认关闭。

V2 Agent Provider Runtime 另行支持 `stub`、`openai_compatible`、`binary` 和可选 `claude_code`。这些 provider 的 run 调查产物由 Analysis Orchestrator 管理，LLM Gateway 只复用结构化解析/校验能力，不拥有 provider loop。

## Settings API

受保护诊断接口：

- `GET /api/v2/settings/llm`
- `GET /api/v2/settings/llm/models`
- `POST /api/v2/settings/llm/chat`
- `GET /api/v2/settings/agent-backends`
- `POST /api/v2/settings/agent-backends/:backend_id/test`
- `GET /api/v2/settings/domain-adapters`

`/api/v2/settings/agent-backends` 返回当前 Agent provider runtime 摘要；当 provider 是 `claude_code` 时展示 Claude Code path/profile 诊断，否则展示当前 provider 的配置状态。

## Evidence Ref 规范化

兼容路径会接受并规范化以下引用：

- `session_text_input.json#question`
- `grep_results.json#matches/<index>`
- `log_searches/<search_id>.json#matches/<index>`
- `tool_results/<action_id>/result.json#findings/<index>`
- `case_context.json#cases/<index>`

支持可唯一映射的别名：

- 原始日志行号或行号范围。
- `matches/<index>`、`matches/<start>-<end>`。
- `#<start>-#<end>`。
- `case_<id>` 或“历史案例 case_<id>”。

System Context、Diagnostic Skill reference 和 Metadata slice 不能作为最终根因 evidence ref。

## 配置示例

OpenAI-compatible：

```yaml
llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model_env: "LOGAGENT_LLM_MODEL"
  max_input_chars: 60000
  max_output_tokens: 4096
  request_timeout_seconds: 120
```

Binary：

```yaml
llm:
  provider: "binary"
  binary_path_env: "LOGAGENT_LLM_BINARY_PATH"
  model: "binary-reserved"
  binary_max_output_bytes: 1048576
  request_timeout_seconds: 120
```

V2 Agent provider 的等价环境变量在 [Agent Provider Runtime](../agent-backends/README.md) 中维护。

## 安全边界

- 不执行 action。
- 不接收或记录 API Key、Cookie、SSH key 或完整 headers。
- binary provider 只能调用配置的绝对路径和固定 argv。
- response debug 只打印模型 response content，不打印 prompt、headers 或密钥。
- Prompt 中的用户文本、日志、Case 和 System Context 都是不可信输入，不能改变 schema、白名单或审批策略。
