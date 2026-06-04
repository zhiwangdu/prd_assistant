# LogAgent MVP Spec

## 目标

LogAgent 把日志包或测试环境采集结果转换成可审计证据链，并结合外部工具、对应版本代码和历史 Case 输出结构化故障分析。

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
| LLM Agent | [llm-agent/SPEC.md](./llm-agent/SPEC.md) |
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
raw file -> extracted files -> manifest.json -> grep_results.json -> tool/code/env evidence -> LLM result
```

## 当前已实现

- Chrome Extension 识别下载完成并调用 Native Agent。
- Native Agent 接收本地导入请求，校验路径、后缀和大小，上传 Server。
- Server 支持 multipart 上传、分片上传、任务创建、任务产物读取。
- Log Analyzer 支持 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz`、`.tar`。
- WEBUI 支持手动上传、分片上传、创建任务、展示 manifest 和 grep 结果。

## 待实现能力

- 持久化任务列表和状态机。
- Tool Runner 调用 `flux_query_analyzer`、`influxql_analyzer` 等已有工具。
- 根据用户输入的软件版本切换代码仓分支并收集证据。
- 测试环境通过 SSH/SCP 采集日志和运行环境信息。
- LLM Agent 组织证据、调用模型并输出结构化结论。
- Case Store 沉淀和召回历史 Case。

## 全局验收

- 本地 `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- WEBUI 能完成上传、创建任务、读取证据。
- API 受 API Key 保护，密钥不写入日志或产物。
- 压缩包解压不能逃逸 workspace。
- 后续每个功能变更必须同步更新对应模块 `README.md` 和 `SPEC.md`。
