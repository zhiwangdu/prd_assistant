# Server Spec

## 目标

Rust Server 是 LocalToolHub 的唯一执行边界。它接收 WebUI 和 MCP 请求，校验参数和权限，执行受控工具，保存 artifact，并返回结构化结果。

## 职责边界

必须提供：

- 本机 HTTP API 和静态 WebUI。
- Tool Catalog、Tool Runner 和内置工具。
- Run History、Artifact Store 和 result API。
- Metadata、Fetch、Executor、Code Evidence、Log Analyzer、Skills/System Context、Case Notes。
- MCP Server，支持外部客户端读取资源和调用工具。
- 配置、密钥引用、allowlist、timeout、预算、路径安全和审计。

不得默认提供：

- Claude Code 作为 Server 后端。
- 自研 Agent 决策循环。
- 任意 shell 命令输入。
- 任意远程主机访问。
- 任意本地路径读取。

## Run 模型

统一 run 字段：

```text
runId
toolId or operation
status
phase
inputSummary
resultArtifactId
stdoutArtifactId
stderrArtifactId
supportArtifacts
createdAt
updatedAt
error
```

状态：

```text
QUEUED
RUNNING
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
CANCELLED
```

`WAITING_FOR_APPROVAL` 仅用于远程采集、SCP、大范围文件读取等高风险动作。普通工具运行不进入用户追问循环。

## Tool Catalog

每个工具 descriptor 必须包含：

```text
toolId
displayName
description
source: built_in | configured | source_built
backend
runnable
readOnly
manualOnly
acceptedSuffixes
minFiles / maxFiles
paramsSchema
paramsTemplate
outputViews
timeoutSeconds
unavailableReason
```

WebUI、HTTP API 和 MCP `tools/list` 必须共享同一 catalog。

## Tool Runner

执行要求：

- 只运行 catalog 中 enabled/runnable 的工具。
- 参数必须通过 schema 校验。
- 输入文件必须来自当前 upload/run/artifact 或明确 allowlist。
- stdout/stderr 必须保存为 artifact。
- 结构化 JSON stdout 优先解析为 result。
- 非零退出、timeout 和 parser error 必须写入 result/error，不吞掉原始输出。
- 密钥和敏感 header 必须脱敏。

## MCP

MCP endpoint 可以先使用 HTTP JSON-RPC，后续可增加 stdio。MCP tools 必须复用 Tool Runner 和各能力模块，不得另开执行通道。
`mcp.enabled=false` 时 HTTP `/api/mcp` 和 stdio `mcp-serve` 必须都拒绝服务。

最低方法：

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

## 配置

配置必须支持环境变量展开。secret 只允许通过 env 引用。

关键配置：

```yaml
server.bind
storage.data_dir
auth.api_keys[].value_env
tools.directories
fetch.enabled
fetch.allowed_hosts
executors.enabled
code_evidence.repos
mcp.enabled
```

## 验收标准

- `/health` 无鉴权可用。
- `/` 返回 WebUI 静态页面。
- `/api/tools` 与 MCP tools/list 一致。
- `mcp.enabled=false` 时 `/api/mcp` 返回 JSON-RPC error，`mcp-serve` 启动失败。
- 手动 tool run 生成 result/stdout/stderr artifacts。
- Fetch/Executor/Code Evidence 默认关闭或受 allowlist 限制。
- 旧 Agent/Analyze 路径不再是新开发主线。
- `cargo fmt --check`、`cargo check`、相关 `cargo test` 通过。
