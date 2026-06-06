# LogAgent MVP Spec

## 目标

LogAgent 把用户问题、日志包或测试环境采集结果转换成可审计证据链，由 Analysis Agent 在受限预算内多轮识别信息缺口、请求补充证据并输出结构化故障分析。

第一阶段目标是跑通：

```text
Chrome 下载或 WEBUI 上传
  -> Native Agent 或 Server 上传接口
  -> Server workspace
  -> 解压与 manifest
  -> grep 证据
  -> WEBUI 查看证据
```

## 技术原则

新实现优先使用 Rust，语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

已有编译工具可复用，不强制重写。外部工具统一通过白名单配置和 Tool Runner 调用。

## 模块边界

| 模块 | Spec |
|------|------|
| Chrome Extension | [chrome-extension/SPEC.md](./chrome-extension/SPEC.md) |
| Native Agent | [native-agent/SPEC.md](./native-agent/SPEC.md) |
| Server | [server/SPEC.md](./server/SPEC.md) |
| Log Analyzer | [log-analyzer/SPEC.md](./log-analyzer/SPEC.md) |
| Tool Runner | [tool-runner/SPEC.md](./tool-runner/SPEC.md) |
| Code Evidence | [code-evidence/SPEC.md](./code-evidence/SPEC.md) |
| Environment Collector | [environment-collector/SPEC.md](./environment-collector/SPEC.md) |
| Metadata | [metadata/SPEC.md](./metadata/SPEC.md) |
| Analysis Agent | [analysis-agent/SPEC.md](./analysis-agent/SPEC.md) |
| LLM Gateway | [llm-agent/SPEC.md](./llm-agent/SPEC.md) |
| Case Store | [case-store/SPEC.md](./case-store/SPEC.md) |
| WebUI | [webui/SPEC.md](./webui/SPEC.md) |
| Config | [config/SPEC.md](./config/SPEC.md) |
| Interfaces | [interfaces/SPEC.md](./interfaces/SPEC.md) |
| Deployment | [deployment/SPEC.md](./deployment/SPEC.md) |
| Security | [security/SPEC.md](./security/SPEC.md) |
| Testing | [testing/SPEC.md](./testing/SPEC.md) |
| Roadmap | [roadmap/SPEC.md](./roadmap/SPEC.md) |

## 核心数据流

上传来源：

```text
Chrome Extension -> Native Agent -> Server upload API -> Task pipeline
WEBUI -> Server upload API -> Task pipeline
```

测试环境来源：

```text
WEBUI/Server task -> Environment Collector -> Server workspace -> Task pipeline
```

证据处理：

```text
raw file -> extracted files -> initial evidence
  -> Analysis Agent context
  -> action -> Server validation/execution -> new evidence
  -> ask user / request approval / next round
  -> final result
```

Analysis Agent 使用任务级持久化上下文：

```text
analysis_state.json
analysis_events.jsonl
result.json
result.md
```

模型只通过 LLM Gateway 返回结构化 action 或最终答案候选。Server 是日志搜索、工具、代码检索和远程采集的唯一执行者。

## 当前已实现

- Chrome Extension 识别下载完成并调用 Native Agent。
- Native Agent 接收本地导入请求，校验路径、后缀和大小，上传 Server。
- Server 支持 multipart 上传、分片上传、任务创建、任务产物读取。
- Log Analyzer 支持 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz`、`.tar`。
- WEBUI 使用 React + Vite，支持上传、任务证据、完整 Metadata 拓扑、Diagnostics 和 Raw JSON。

## 待实现能力

- 持久化任务列表和状态机。
- Metadata 接入 task context 并写入 `metadata_context.json`。
- Tool Runner 调用 `flux_query_analyzer`、`influxql_analyzer` 等已有工具。
- 根据用户输入的软件版本切换代码仓分支并收集证据。
- 测试环境通过 SSH/SCP 采集日志和运行环境信息。
- Analysis Agent 持久化上下文，执行多轮调查、用户追问、动作审批和预算终止。
- LLM Gateway 组织 Prompt、裁剪证据、调用模型并解析结构化决策。
- Case Store 沉淀和召回历史 Case。

## 全局验收

- 本地 `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- WEBUI 能完成上传、创建任务、读取证据。
- API 受 API Key 保护，密钥不写入日志或产物。
- 压缩包解压不能逃逸 workspace。
- Agent 动作必须经过 schema、白名单、预算和审批校验。
- 任务能从 `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 接收输入并恢复。
- 后续每个功能变更必须同步更新对应模块 `README.md` 和 `SPEC.md`。
