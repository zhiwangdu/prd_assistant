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

## 调查循环图

```mermaid
flowchart TD
    Start["任务已持久化<br/>QUEUED"] --> Initial["基础采集 / 解压 / 初始搜索"]
    Initial --> Running["RUNNING<br/>加载 analysis state 与 evidence"]
    Running --> Decide["Analysis Agent + LLM Gateway<br/>生成结构化决策"]

    Decide --> Search["search_logs"]
    Decide --> Tool["run_tool"]
    Decide --> Code["collect_code_evidence"]
    Decide --> Env["collect_environment"]
    Decide --> Ask["ask_user"]
    Decide --> Final["final_answer"]

    Search --> Validate["Server 校验<br/>schema / 白名单 / 预算 / 幂等"]
    Tool --> Validate
    Code --> Validate
    Env --> Approval{"需要批准？"}

    Validate --> Execute["执行安全只读动作"]
    Execute --> Persist["持久化 action 结果与事件"]
    Persist --> Budget{"预算和终止条件"}

    Approval -->|"是"| WaitingApproval["WAITING_FOR_APPROVAL"]
    Approval -->|"否"| Validate
    WaitingApproval -->|"批准"| Validate
    WaitingApproval -->|"拒绝及原因"| Persist

    Ask --> WaitingUser["WAITING_FOR_USER"]
    WaitingUser -->|"用户补充消息"| Persist

    Budget -->|"继续"| Running
    Budget -->|"耗尽 / 重复 / 证据不足"| Limited["生成带不确定性的结果"]
    Limited --> Final

    Final --> Result["写入 result.json / result.md"]
    Result --> Success["SUCCEEDED"]

    Initial -->|"不可恢复系统错误"| Failed["FAILED"]
    Validate -->|"不可恢复系统错误"| Failed
```

状态和阶段分离：

- 稳定状态：`QUEUED`、`RUNNING`、`WAITING_FOR_USER`、`WAITING_FOR_APPROVAL`、`SUCCEEDED`、`FAILED`。
- 执行阶段：`COLLECT`、`EXTRACT`、`SEARCH_LOGS`、`RUN_TOOL`、`COLLECT_CODE`、`PLAN_ANALYSIS`、`EXECUTE_ACTION`、`GENERATE_RESULT` 等。
- 预算耗尽或证据不足属于可解释的分析终止，通常生成低置信度结果并进入 `SUCCEEDED`；只有不可恢复系统错误进入 `FAILED`。

## 当前已实现

- Chrome Extension 识别下载完成并调用 Native Agent。
- Native Agent 接收本地导入请求，校验路径、后缀和大小，上传 Server。
- Server 支持 multipart 上传、分片上传、任务创建、任务产物读取。
- Server 持久化任务并在后台执行，支持重启恢复。
- Upload session 持久化并支持重启续传。
- Metadata 接入 task context，写入 `metadata_context.json` 并进入 LLM Prompt。
- Executor 按持久化 phase 调度并从中断阶段恢复，公共 Action/Evidence 契约已落地。
- Tool Runner MVP 支持白名单工具配置、规则版多输入 `run_tool` action、`RUN_TOOL` phase、`tool_results` artifact 和 JSON stdout summary/findings 解析；真实 `influxql-analyzer` Report stdout 已适配为结构化 findings 并通过本地 smoke。
- Analysis State Store MVP 已写入 `analysis_state.json` / `analysis_events.jsonl`，并提供 `GET /api/tasks/:task_id/analysis` 读取当前快照和事件流；`PLAN_ANALYSIS` 真实 LLM 调用会记录 callId、attempt 和 schema retry 事件。
- Analysis Agent 已支持 `ask_user` 进入 `WAITING_FOR_USER`，通过 `POST /api/tasks/:task_id/messages` 接收回答后恢复同一任务。
- Analysis Agent 已支持 `collect_environment` 进入 `WAITING_FOR_APPROVAL`，通过 `POST /api/tasks/:task_id/actions/:action_id/decision` 批准或拒绝后恢复；当前批准后生成 mock `environment_evidence`，真实 SSH/SCP 采集后续接入。
- Log Analyzer 支持 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz`、`.tar`。
- LLM Gateway 支持 stub 和 OpenAI-compatible Chat Completions，基于 manifest/grep/metadata/tool evidence 单次生成结构化结果，并已通过 `PLAN_ANALYSIS` 接入多轮 ActionDecision / FinalAnswer 决策、预算和重复 fingerprint 防护。
- WEBUI 使用 React + Vite，支持上传、任务证据、Task execution loop 摘要、单次 LLM 结果、顶部 LLM debug 开关、完整 Metadata 拓扑、Diagnostics 和 Raw JSON。

## 待实现能力

- 接入真实 `flux_query_analyzer` 工具路径和规则。
- 扩展 `influxql_analyzer` compare mode delta 字段映射。
- 根据用户输入的软件版本切换代码仓分支并收集证据。
- 测试环境通过 SSH/SCP 采集日志和运行环境信息。
- Analysis Agent 更完整的用户追问/审批策略、真实环境采集执行器和恢复幂等审计。
- LLM Gateway 扩展为多轮 action/final-answer 协议、用量审计和有限重试。
- Case Store 沉淀和召回历史 Case。

## 全局验收

- 本地 `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- WEBUI 能完成上传、创建任务、读取证据。
- API 受 API Key 保护，密钥不写入日志或产物。
- 压缩包解压不能逃逸 workspace。
- Agent 动作必须经过 schema、白名单、预算和审批校验。
- 任务能从 `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 接收输入并恢复。
- 后续每个功能变更必须同步更新对应模块 `README.md` 和 `SPEC.md`。
