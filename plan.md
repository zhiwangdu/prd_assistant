# LogAgent MVP 方案

## 1. 项目目标

个人主导、业余时间开发一个可落地的日志分析助手 MVP。加入版本感知代码证据、测试环境采集、统一配置和测试策略后，第一版建议控制在 5~8 周。

核心链路：

用户问题 + 日志来源（浏览器下载 / 手动上传 / 测试环境采集）-> 初始证据 -> Analysis Agent 多轮调查 -> LLM Gateway 结构化决策 -> 人工确认 -> 沉淀 Case。

MVP 不做企业级日志平台，不引入 Elasticsearch/OpenSearch、CMDB、监控接入、通用远程运维、复杂权限体系和 Multi-Agent 编排。

## 2. 技术选型原则

能用 Rust 实现的模块优先使用 Rust。整体语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

默认建议：

- Native Agent 使用 Rust 实现。
- Server/API 使用 Rust Web 框架实现，例如 Axum、Actix Web 或 Poem。
- Log Analyzer、Tool Runner、Code Evidence、Environment Collector 优先使用 Rust。
- 已有 C/C++ 编译工具直接复用，通过 Tool Runner 调用。
- Go/Python/Java 只作为已有工具、历史代码或生态强依赖时的备选。
- MVP 部署形态优先采用单一 Rust binary + 内部 crate/module 拆分，避免一开始拆成多个服务。

## 3. MVP 边界

第一版只解决一个明确问题：把日志包或测试环境采集结果整理成可供 LLM 使用的高质量证据，并结合软件版本对应的代码实现，输出结构化故障分析结果。

必须包含：

- Chrome 插件：识别日志下载，触发上传流程
- Native Agent：本地接收文件、上传服务端、创建分析任务，优先 Rust 实现
- Server：任务管理、状态流转、模块编排和 API，优先 Rust 实现
- rg 检索器：从大日志中提取错误摘要和关键上下文
- Tool Runner：按配置调用已有诊断工具，例如 `flux_query_analyzer`、`influxql_analyzer`
- Code Evidence：根据用户输入的软件版本定位对应代码分支，检索实际代码形成证据链
- Environment Collector：测试环境下通过 SSH/SCP 从目标节点收集日志、配置和诊断信息
- Analysis Agent：维护任务级上下文，多轮识别证据缺口、请求动作、追问用户并决定终止
- LLM Gateway：Provider 适配、Prompt、证据裁剪、模型调用和结构化响应解析
- Case 库：人工确认后沉淀经验，后续任务召回相似 Case
- WebUI：任务列表、任务详情、Case 库
- Config：单一 `logagent.yaml` 管理所有模块配置
- Interfaces：定义模块边界、Rust trait 和状态机
- Testing：fixture、集成测试和 LLM stub

暂不做：

- 企业统一登录和复杂 RBAC
- 多租户隔离
- 实时日志流式接入
- 分布式任务调度
- 自动执行修复动作
- 复杂日志聚类和完整根因图谱

## 4. 总体架构

```text
Source A: Chrome Extension / Manual Upload
  |
  | 1A. 下载日志 / 上传日志包
  v
Native Agent (Rust, localhost HTTP) / Web Upload
  |
  | 2A. 创建任务
  v
Server

Source B: Test Environment
  |
  | 1B. SSH/SCP 采集日志、配置、诊断命令输出
  v
Server

Server
  |
  | 3. 编排任务状态和各执行模块
  v
Initial Evidence Modules
  |
  | 4. rg 摘要 / 工具结果 / 代码证据 / 环境证据 / 相似 Case
  v
Analysis Agent
  |
  | 5. 通过 LLM Gateway 生成结构化动作或最终答案
  | 6. Server 校验并执行安全动作，或等待用户/审批
  | 7. 新证据回填并继续下一轮
  v
WebUI + Case Store
```

评审后调整：

- Chrome 插件不直接调用本地进程，改为调用 Native Agent 暴露的 `localhost` HTTP 服务。
- Cookie 不传给 Native Agent。优先让浏览器完成下载，Native Agent 只处理已下载文件，降低 session token 泄露风险。
- pgvector 不作为第一版硬依赖。MVP 可以先用本地文件保存 embedding，并在服务端用余弦相似度召回；后续再迁移到 PostgreSQL + pgvector。
- 日志“聚类”第一版不做复杂算法，先用正则归一化 + 计数排序跑通。
- 外部工具调用第一版只做白名单配置和同步执行，不做复杂插件市场、远程执行和自动安装。
- 代码证据第一版只做本地已有代码仓的只读检索，不做自动拉取陌生仓库、不做自动改代码。
- 测试环境采集第一版只做白名单节点和白名单命令，不做通用远程运维平台。
- LLM 提供商、embedding 模型、token 预算、API Key、关键词、环境采集并发都必须配置化。
- Case 基础功能前移到第一阶段，至少支持确认、存储、embedding 和 Top 5 召回。
- 自主调查使用独立 Analysis Agent；`llm-agent/` 保留目录但定位为 LLM Gateway。
- MVP 只保存任务级上下文，不引入 Multi-Agent 或用户级长期记忆。
- 安全只读动作可自动执行，SSH/SCP 远程采集默认需要用户批准。

## 5. 模块设计

### 5.1 Chrome Extension

职责：

- 使用 `chrome.downloads.onChanged` 监听下载完成事件
- 匹配日志下载 URL 或文件后缀
- 弹出确认：是否交给 LogAgent 分析
- 下载完成后把本地文件路径、文件名、来源 URL 发送给 Native Agent

匹配规则示例：

```js
const URL_PREFIXES = [
  "https://xxx/download/",
  "https://logs.xxx.com/export/"
]

const FILE_SUFFIXES = [
  ".log",
  ".txt",
  ".zip",
  ".tar.gz",
  ".tgz",
  ".tar"
]
```

第一版推荐流程：

1. 浏览器正常下载文件。
2. 插件通过 `chrome.downloads.onChanged` 识别 `state.current === "complete"`。
3. 用户确认上传。
4. 插件调用 `http://127.0.0.1:<port>/imports`。

这样可以避免插件把 Cookie、Referer、Authorization 等敏感信息转交给本地进程。

### 5.2 Native Agent

技术选型：Rust。

原因：

- 单文件部署
- 跨平台方便
- 静态类型和内存安全适合本地常驻 Agent
- 进程调用、文件校验、HTTP 上传等能力成熟
- 后续可以扩展本地文件扫描、SSH 诊断、离线模式

职责：

- 启动本地 HTTP Server
- 接收 Chrome 插件提交的本地文件路径或文件元信息
- 校验文件大小、后缀、路径合法性
- 上传日志包到服务端
- 创建分析任务
- 返回任务 URL 给插件或自动打开 WebUI

接口建议：

```http
POST /imports
Content-Type: application/json

{
  "filePath": "/Users/xxx/Downloads/redis.tar.gz",
  "filename": "redis.tar.gz",
  "sourceUrl": "https://logs.xxx.com/export/123"
}
```

Native Agent 上传服务端：

```http
POST /api/uploads
Authorization: Bearer <api_key>
```

上传成功后创建任务：

```http
POST /api/tasks
Authorization: Bearer <api_key>

{
  "uploadId": "upl_123"
}
```

返回：

```json
{
  "taskId": "task_456",
  "url": "http://logagent/tasks/task_456"
}
```

### 5.3 Server

技术选型：Rust 优先。

可选框架：

- Axum
- Actix Web
- Poem

如果团队已有大量 Python 资产，FastAPI 可以作为备选；但新实现默认不优先选择 Python。

职责：

- 上传管理
- 任务状态流转
- 编排 Log Analyzer、Tool Runner、Code Evidence、Environment Collector、Analysis Agent 和 LLM Gateway
- 管理模块输出和任务失败原因
- 持久化分析状态、事件、预算、待回答问题和待审批动作
- 校验并执行结构化 Agent action
- Case 存储和召回
- 提供 WebUI API

任务状态：

```text
QUEUED
RUNNING
WAITING_FOR_USER
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
```

执行阶段独立记录，例如 `EXTRACT`、`SEARCH_LOGS`、`RUN_TOOL`、`COLLECT_CODE`、`PLAN_ANALYSIS`、`EXECUTE_ACTION` 和 `GENERATE_RESULT`。

职责边界：

- Server 只负责编排和状态管理。
- Log Analyzer 执行解压、manifest、rg 检索和错误摘要。
- Tool Runner 执行外部工具。
- Code Evidence 执行代码检索。
- Environment Collector 执行 SSH/SCP 采集。
- Analysis Agent 管理调查策略、上下文、动作和终止条件。
- LLM Gateway 执行证据裁剪、Prompt 组装、模型调用和响应校验。
- Server 是唯一动作执行者。

目录建议：

```text
/data/logagent
  uploads/
  workspaces/
  tasks/
  cases/
```

每个任务一个 workspace：

```text
/data/logagent/workspaces/task_456
  raw/
  extracted/
  manifest.json
  error_summary.json
  contexts.jsonl
  tool_results/
  analysis_state.json
  analysis_events.jsonl
  result.json
  result.md
```

### 5.4 Tool Runner

Agent 需要支持调用已有编译好的分析工具，用这些工具补充领域证据。例如：

- `flux_query_analyzer`
- `influxql_analyzer`

MVP 不让 LLM 任意执行命令，而是由服务端维护工具白名单。初始规则、用户显式选择或 Analysis Agent 的 `run_tool` action 都只能由 Server 映射到配置工具和参数模板，结果作为结构化证据回填同一任务。

工具配置示例：

```yaml
tools:
  flux_query_analyzer:
    enabled: true
    path: /opt/logagent/tools/flux_query_analyzer
    timeout_seconds: 30
    input_mode: file
    match:
      file_patterns:
        - "*.flux"
        - "*.log"
      keywords:
        - "flux"
        - "query"
        - "planner"
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"

  influxql_analyzer:
    enabled: true
    path: /opt/logagent/tools/influxql_analyzer
    timeout_seconds: 30
    input_mode: file
    match:
      file_patterns:
        - "*.sql"
        - "*.log"
      keywords:
        - "influxql"
        - "select"
        - "show series"
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"
```

工具执行原则：

- 只允许调用配置文件中声明的工具。
- 工具路径必须是绝对路径，启动时检查是否存在和可执行。
- 参数只允许使用预定义占位符，例如 `{input_file}`、`{workspace}`、`{task_id}`。
- 每次执行必须设置 timeout。
- stdout、stderr、exit code、耗时都要保存。
- 工具失败不应导致整个任务失败，除非该工具被标记为必需。

工具结果落盘：

```text
tool_results/
  flux_query_analyzer.json
  influxql_analyzer.json
```

统一结果结构：

```json
{
  "tool": "flux_query_analyzer",
  "status": "OK",
  "exitCode": 0,
  "durationMs": 1234,
  "summary": "发现 2 个可能导致慢查询的 range/filter 顺序问题",
  "findings": [
    {
      "severity": "medium",
      "file": "query.log",
      "line": 120,
      "message": "filter 下推失败，可能导致扫描数据量过大"
    }
  ],
  "rawOutputPath": "tool_results/flux_query_analyzer.raw.json"
}
```

如果已有工具输出不是 JSON，MVP 可以先保存原始文本，再由服务端做一层简单包装：

```json
{
  "tool": "influxql_analyzer",
  "status": "OK",
  "rawText": "..."
}
```

### 5.5 Code Evidence

用户输入软件版本后，Agent 需要能定位对应代码分支，并结合实际代码继续给出证据链。

典型输入：

```json
{
  "product": "influxdb",
  "version": "3.0.2",
  "question": "为什么这个 Flux 查询在该版本上变慢？"
}
```

MVP 推荐维护一份版本到代码仓引用的配置，而不是让 Agent 猜分支。

配置示例：

```yaml
code_repos:
  influxdb:
    repo_path: /data/repos/influxdb
    default_ref: main
    version_refs:
      "3.0.2": "v3.0.2"
      "3.0.1": "v3.0.1"
      "2.7.8": "v2.7.8"
    search_roots:
      - query/
      - storage/
      - influxql/
      - flux/

  redis:
    repo_path: /data/repos/redis
    default_ref: unstable
    version_refs:
      "7.2.5": "7.2.5"
      "7.0.15": "7.0.15"
    search_roots:
      - src/
```

代码定位流程：

1. 用户在任务中填写 `product` 和 `version`。
2. 服务端根据 `version_refs` 找到 tag 或 branch。
3. 使用 `git worktree` 或只读 checkout 准备对应版本代码目录。
4. 从日志、工具输出、用户问题中提取关键词。
5. 使用 `rg` 或 `git grep` 在指定 `search_roots` 检索。
6. 抽取命中的函数、文件、上下文行，生成 `code_evidence.json`。
7. 将代码证据回填 Analysis Agent evidence bundle。

关键词提取策略：

- 优先使用工具结果中的 `symbol`、`rule`、`operator`、`error_code`。
- 其次使用日志上下文中的函数名、错误码、模块名。
- 再使用用户问题中的领域词，例如 `query`、`planner`、`compaction`、`write`。
- 每个任务最多生成 20 个代码检索关键词。
- 每个关键词最多保留 Top 10 命中。

代码工作区建议：

```text
/data/logagent/code_worktrees/
  influxdb/
    v3.0.2/
    v3.0.1/
```

worktree 清理策略：

- 按 repo + ref 复用 worktree。
- 每个 repo 默认最多保留 5 个 worktree。
- 超过上限时删除最近最少使用的 worktree。
- 正在被任务使用的 worktree 不删除。
- 启动时扫描孤儿 worktree 并记录告警。

代码证据结构：

```json
{
  "product": "influxdb",
  "version": "3.0.2",
  "ref": "v3.0.2",
  "repoPath": "/data/repos/influxdb",
  "findings": [
    {
      "file": "query/planner/rules.go",
      "line": 214,
      "symbol": "PushDownFilterRule",
      "reason": "日志中出现 filter pushdown failed，与该规则相关",
      "snippet": "..."
    }
  ]
}
```

实现边界：

- 代码仓由管理员预先配置和同步。
- 任务执行时只允许切到配置中允许的 ref。
- 第一版只做代码检索和证据引用，不做自动 diff 分析。
- 后续可以增加“当前版本 vs 相邻版本”的 commit/diff 对比，用于定位回归。

### 5.6 Environment Collector

在测试环境中，可以跳过浏览器下载和本地上传，直接通过 SSH/SCP 从目标节点收集信息。

适用场景：

- 测试集群复现问题
- CI 或压测环境自动诊断
- 已知目标节点 IP，可以直接拉日志、配置、运行状态

MVP 不做任意 SSH 命令执行，而是通过环境配置定义节点、文件路径和允许执行的诊断命令。

配置示例：

```yaml
environments:
  test-influxdb-cluster:
    ssh_user: test
    ssh_key_path: /data/logagent/keys/test_cluster.pem
    nodes:
      - name: meta-1
        host: 10.0.1.11
        roles: ["meta"]
      - name: data-1
        host: 10.0.1.21
        roles: ["data"]
    collect:
      files:
        - /var/log/influxdb/*.log
        - /etc/influxdb/config.toml
      commands:
        - name: process
          argv: ["ps", "-ef"]
        - name: disk
          argv: ["df", "-h"]
        - name: ports
          argv: ["ss", "-lntp"]

environment_collector:
  max_parallel_nodes: 4
  connect_timeout_seconds: 10
  command_timeout_seconds: 30
  retries: 1
```

连接策略：

- 多节点默认并行采集，最大并发由 `max_parallel_nodes` 控制。
- 单节点内先拉文件，再执行诊断命令。
- 单个节点失败不直接失败整个任务，但会写入 `environment_evidence.json`。
- SSH 连接失败允许按配置重试。
- 命令超时后终止并保留 stderr/timeout 信息。

任务输入示例：

```json
{
  "source": "environment",
  "environment": "test-influxdb-cluster",
  "product": "influxdb",
  "version": "3.0.2",
  "question": "压测时写入延迟突然升高，帮我分析原因"
}
```

采集流程：

1. 用户选择测试环境和目标节点范围。
2. 服务端根据配置建立 SSH 连接。
3. 用 SCP 拉取白名单路径下的日志和配置。
4. 执行白名单诊断命令。
5. 将采集结果保存到任务 workspace。
6. 将结果交给 Analysis Agent；后续可按信息缺口继续请求日志、工具、代码或环境证据。

环境采集目录：

```text
/data/logagent/workspaces/task_456
  collected/
    meta-1/
      files/
      commands/
        process.txt
        disk.txt
        ports.txt
    data-1/
      files/
      commands/
```

环境证据结构：

```json
{
  "environment": "test-influxdb-cluster",
  "nodes": [
    {
      "name": "data-1",
      "host": "10.0.1.21",
      "filesCollected": 8,
      "commands": [
        {
          "name": "disk",
          "status": "OK",
          "summary": "/data 使用率 96%"
        }
      ]
    }
  ]
}
```

## 6. 数据设计

MVP 以 5 张核心表为主，但任务表需要记录来源、产品和版本；代码证据、工具结果、环境采集结果优先落 workspace 文件。Case embedding 可以先落文件，后续再迁移 pgvector。

### upload

| 字段 | 说明 |
|------|------|
| id | 上传 ID |
| filename | 原始文件名 |
| size | 文件大小 |
| path | 服务端存储路径 |
| source_url | 来源 URL，可为空 |
| created_at | 创建时间 |

### task

| 字段 | 说明 |
|------|------|
| id | 任务 ID |
| upload_id | 上传 ID |
| status | 任务状态 |
| source | 任务来源：upload / environment |
| product | 软件产品，例如 influxdb / redis |
| version | 用户输入的软件版本 |
| question | 用户问题 |
| summary | 简要结论 |
| created_at | 创建时间 |
| updated_at | 更新时间 |

### task_result

| 字段 | 说明 |
|------|------|
| task_id | 任务 ID |
| markdown | Analysis Agent 最终输出 |
| evidence_json | 关键证据 |
| confidence | 置信度 |

### workspace evidence

以下证据文件第一版不一定单独建表，先统一落在任务 workspace：

| 文件 | 说明 |
|------|------|
| `manifest.json` | 日志和采集文件清单 |
| `error_summary.json` | rg 错误模式摘要 |
| `contexts.jsonl` | 关键日志上下文 |
| `tool_results/*.json` | 外部工具结果 |
| `code_evidence.json` | 对应版本代码证据 |
| `environment_evidence.json` | 测试环境采集摘要 |
| `analysis_state.json` | 当前事实、假设、缺口、待处理请求和预算 |
| `analysis_events.jsonl` | 用户、模型决策、动作、审批和状态事件 |
| `result.json` | 最终结构化结果 |

### case

| 字段 | 说明 |
|------|------|
| id | Case ID |
| title | 标题 |
| symptom | 现象 |
| root_cause | 根因 |
| solution | 解决方案 |
| confirmed | 是否人工确认 |
| created_at | 创建时间 |

### task_case

| 字段 | 说明 |
|------|------|
| task_id | 任务 ID |
| case_id | Case ID |
| score | 相似度 |

## 7. 日志处理流程

### 7.1 解压和 manifest

支持格式：

- `.log`
- `.txt`
- `.zip`
- `.tar.gz`
- `.tgz`
- `.tar`

上传后统一进入 workspace。压缩包解压后生成 `manifest.json`：

```json
{
  "files": [
    {
      "path": "redis.log",
      "size": 2147483648,
      "modifiedAt": "2026-05-30T10:00:00Z"
    }
  ]
}
```

MVP 建议限制：

- 单文件默认上限 2GB
- 单任务默认最多 20 个日志文件
- 超限时任务标记为 `FAILED`，提示用户拆分或调整配置

### 7.2 rg 检索

不要直接把完整日志喂给 LLM。先用 `rg` 做压缩和证据提取。

`rg` 是系统依赖，启动时需要检查 `rg_path` 是否存在并可执行。关键词必须配置化。

```yaml
log_analyzer:
  rg_path: "rg"
  context_lines: 50
  keywords:
    - error
    - exception
    - timeout
    - fail
    - failed
    - panic
    - fatal
    - refused
    - denied
    - verify
```

第一步：关键词扫描。

```bash
rg -i "error|exception|timeout|fail|failed|panic|fatal|refused|denied|verify" extracted/
```

第二步：提取上下文。

```bash
rg -i -C 50 "error|exception|timeout|fail|failed|panic|fatal|refused|denied|verify" extracted/
```

第三步：正则归一化和计数。

第一版不做复杂聚类，先对日志行做简单归一化：

- 数字替换为 `<num>`
- UUID 替换为 `<uuid>`
- IP 替换为 `<ip>`
- 时间戳替换为 `<ts>`
- 路径中过长随机片段替换为 `<token>`

输出 `error_summary.json`：

```json
{
  "topPatterns": [
    {
      "pattern": "TimeoutException while connecting to <ip>:<num>",
      "count": 50,
      "examples": [
        "app.log:1234 TimeoutException while connecting to 10.0.0.1:6379"
      ]
    }
  ]
}
```

输出 `contexts.jsonl`：

```json
{"file":"app.log","line":1234,"keyword":"timeout","context":"..."}
```

这部分是 MVP 质量核心，工时按 7~10 天预估。

### 7.3 外部工具辅助分析

在 `rg` 完成基础证据提取后，Tool Runner 根据配置判断是否调用外部工具。

触发方式分三类：

- 自动触发：文件名、后缀、关键词命中工具配置。
- 手动触发：任务详情页允许用户选择要运行的工具。
- Agent 触发：Analysis Agent 可以返回 `run_tool` action，Server 校验白名单、预算和幂等后执行。

执行流程：

1. 扫描 `manifest.json` 和 `contexts.jsonl`。
2. 根据工具配置匹配候选文件。
3. 为每个工具生成执行计划。
4. 同步调用工具，限制 timeout 和输出大小。
5. 保存 `tool_results/*.json`。
6. 将工具摘要回填 Analysis Agent。

执行计划示例：

```json
{
  "taskId": "task_456",
  "tool": "flux_query_analyzer",
  "inputFiles": [
    "extracted/query.log"
  ],
  "commandPreview": "/opt/logagent/tools/flux_query_analyzer --input extracted/query.log --format json"
}
```

需要注意：`commandPreview` 只用于展示和审计，不应该直接作为 shell 字符串执行。实现时应使用参数数组调用进程，避免命令注入。

### 7.4 代码证据生成

当任务包含 `product` 和 `version` 时，服务端尝试生成代码证据。

执行流程：

1. 根据 `product` 找到配置的代码仓。
2. 根据 `version` 找到 tag 或 branch。
3. 准备对应版本 worktree。
4. 从日志错误模式、工具 findings、用户问题中提取关键词。
5. 在配置的 `search_roots` 中检索。
6. 抽取相关文件、行号、函数名和上下文。
7. 生成 `code_evidence.json`。

关键词来源示例：

- 日志中的错误码、函数名、模块名
- `flux_query_analyzer` 输出的规则名、算子名
- `influxql_analyzer` 输出的 SQL 语句类型
- 用户问题中的功能域，例如 compaction、write、query、planner

代码证据不应该直接替代日志证据。它的作用是解释“为什么这个日志现象可能由某段逻辑导致”，并帮助 LLM 把现象、工具结果和实现细节串起来。

### 7.5 测试环境采集

当任务来源是 `environment` 时，流程从环境采集开始，而不是文件上传。

执行流程：

1. 根据用户选择的环境加载 SSH/SCP 配置。
2. 连接目标节点。
3. 拉取白名单日志和配置。
4. 执行白名单诊断命令。
5. 生成 `environment_evidence.json` 和 `manifest.json`。
6. 采集结果回填 Analysis Agent；该动作默认要求用户批准。

统一后的任务来源：

```text
upload/environment source
  -> initial extract/collect/search
  -> Analysis Agent
  -> action -> Server validation/execution -> evidence
  -> ask user / request approval / next round
  -> final result
```

## 8. Analysis Agent 与 LLM Gateway 设计

第一版只做单 Analysis Agent、任务级上下文，不做 Multi-Agent 和用户级长期记忆。

### 8.1 Analysis Agent

每轮执行：

```text
读取用户问题、当前证据和历史事件
  -> 更新事实、假设和信息缺口
  -> 通过 LLM Gateway 生成一个结构化决策
  -> Server 校验并执行 action，或进入等待状态
  -> 新证据和事件持久化
  -> 下一轮或 final_answer
```

支持 action：

- `search_logs`
- `run_tool`
- `collect_code_evidence`
- `collect_environment`
- `ask_user`
- `final_answer`

安全只读日志搜索、白名单工具和已配置代码仓检索可自动执行。SSH/SCP 环境采集默认进入 `WAITING_FOR_APPROVAL`。`ask_user` 进入 `WAITING_FOR_USER`，用户回答后继续原任务。

持久化：

```text
analysis_state.json
analysis_events.jsonl
result.json
result.md
```

状态保存事实、候选假设、信息缺口、证据引用、待处理请求、action fingerprint 和预算。事件流记录用户消息、决策摘要、动作结果、审批和状态变化。禁止保存模型隐藏思维链。

预算至少限制最大轮数、模型调用数、动作数、重复动作次数、token 和总运行时间。等待用户/审批不计入运行时间。预算耗尽、动作重复、用户拒绝或证据不足时，输出带不确定性和缺失信息的结果。

### 8.2 LLM Gateway

LLM Provider 必须配置化。Gateway 只负责模型适配、Prompt、裁剪、调用、结构化解析和有限重试。

```yaml
llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model: "gpt-4.1"
  max_input_tokens: 64000
  max_output_tokens: 4096
  request_timeout_seconds: 120
  max_retries: 2

analysis_agent:
  max_rounds: 12
  max_llm_calls: 12
  max_actions: 20
  max_repeated_action: 2
  max_questions_per_round: 3
  max_running_seconds: 900
  approval_required_actions:
    - collect_environment
```

Gateway 输入：

- 用户问题
- manifest 摘要
- Top 20 错误模式
- Top 20 关键上下文
- 外部工具分析结果摘要
- 对应版本代码证据
- 测试环境采集摘要
- Top 5 相似历史 Case
- 当前事实、假设、信息缺口、最近事件和剩余预算

Token 预算建议：

| 证据类型 | 上限 |
|----------|------|
| 用户问题和任务元信息 | 2k tokens |
| manifest 摘要 | 2k tokens |
| Top 错误模式 | 8k tokens |
| 日志上下文 | 20k tokens |
| 工具结果 | 8k tokens |
| 代码证据 | 10k tokens |
| 环境证据 | 6k tokens |
| 相似 Case | 6k tokens |

裁剪优先级：

1. 用户问题和任务元信息必须保留。
2. 有文件和行号的日志证据优先。
3. 工具 finding 优先于 raw output。
4. 与错误关键词命中的代码证据优先。
5. Case 只保留 Top 5 的标题、现象、根因和解决方案摘要。

Gateway 每次只能输出一个 action 或最终答案候选。最终输出结构：

```markdown
# 结论

# 问题现象

# 关键证据

# 根因分析

# 修复建议

# 置信度
```

Prompt 核心约束：

- 必须引用日志文件名和行号作为证据
- 引用工具结论时必须标明工具名，例如 `flux_query_analyzer`
- 引用代码证据时必须标明版本、文件和行号
- 引用环境证据时必须标明节点名和采集命令或文件路径
- 无证据时明确说明不确定
- 区分“已确认事实”和“推测”
- 修复建议要可执行
- 输出置信度：高 / 中 / 低
- 不输出隐藏思维链，只输出简短决策依据

工具结果进入 Prompt 的形式：

```markdown
## 工具分析结果

### flux_query_analyzer

- 状态：OK
- 摘要：发现 2 个可能导致慢查询的 range/filter 顺序问题
- 证据：
  - query.log:120 filter 下推失败，可能导致扫描数据量过大

### influxql_analyzer

- 状态：OK
- 摘要：检测到 SHOW SERIES 查询可能扫描高基数 measurement
```

最终输出中应区分：

- 日志原文证据
- 工具分析证据
- 代码实现证据
- 环境采集证据
- 历史 Case 参考

## 9. Case 沉淀与召回

任务分析完成后，WebUI 提供：

- 确认为 Case
- 修改后确认
- 放弃

确认后生成：

- 标题
- 现象
- 根因
- 解决方案
- embedding 文本

embedding 文本建议：

```text
title + symptom + root_cause + solution
```

MVP 存储策略：

- 第一版：embedding 写入本地 JSONL 或 SQLite，服务端内存加载后做余弦相似度
- 后续：迁移到 PostgreSQL + pgvector

新任务分析前先召回 Top 5 相似 Case，并加入 Agent 输入。

## 10. WebUI

只做 3 个页面。

### 10.1 任务列表

展示：

- 任务名
- 状态
- 来源：上传 / 测试环境
- 产品和版本
- 上传文件
- 创建时间
- 简要结论

### 10.2 任务详情

展示：

- 日志基本信息
- 来源信息：上传文件或测试环境节点
- 产品、版本、代码 ref
- 用户问题输入框
- 任务状态和处理阶段
- Top 错误模式
- 关键上下文证据
- 外部工具结果
- 对应版本代码证据
- 环境采集摘要
- 调查时间线、事实、假设和信息缺口
- 待补充问题和待审批动作
- Analysis Agent 最终结果
- Case 确认入口

### 10.3 Case 库

展示：

- Case 搜索
- Case 详情
- 编辑标题、现象、根因、解决方案
- 删除或禁用 Case

## 11. 安全与可靠性

MVP 至少要处理以下问题：

- 服务端 API 使用简单 API Key
- Native Agent 只接受 `127.0.0.1` 请求
- Native Agent 校验文件路径，避免任意文件上传
- 不在日志或数据库中保存 Cookie、Authorization、session token
- 上传文件大小限制
- 任务失败要保留错误信息，方便定位
- LLM 输出必须保留原始证据引用，避免只有结论没有依据
- 外部工具只能从白名单配置中调用，禁止 LLM 直接生成任意命令
- 调用外部工具时使用参数数组，不拼接 shell 字符串
- 限制外部工具执行时间、输出大小和可访问目录
- 工具执行结果要保留 exit code、stderr 和原始输出路径，方便审计
- 代码仓只允许使用配置中的本地 repo 和 ref，不允许用户传任意 repo URL
- 代码检索只读执行，禁止任务流程中自动修改代码、提交代码或运行构建脚本
- SSH/SCP 只允许访问配置中的测试环境节点
- SSH 诊断命令必须使用白名单 argv 数组，不允许拼接用户输入
- 采集文件路径必须在配置白名单内，避免任意文件读取
- SSH key、API Key、repo path 等敏感配置不进入 LLM Prompt
- API Key 从 `logagent.yaml` 引用的环境变量读取，启动时校验存在性
- LLM Provider、model、base_url 和 token 预算必须配置化
- LLM 输入必须经过证据裁剪，不能直接塞入全部日志和工具输出
- Analysis Agent 只能产生结构化意图，Server 校验 action schema、预算、白名单、幂等和审批
- SSH/SCP 等远程采集默认需要用户批准
- 用户消息、日志和 Case 都视为不可信输入，不能覆盖系统安全策略
- 不保存模型隐藏思维链，只保存可审计决策摘要
- action 重复或预算耗尽时必须终止，不能自动扩大权限

## 12. 工期预估

| 组件 | 预估 |
|------|------|
| Chrome 插件 | 3~4 天 |
| Native Agent | 2~3 天 |
| Rust Server | 5~7 天 |
| rg 分析器 | 7~10 天 |
| Tool Runner | 2~3 天 |
| Code Evidence | 4~6 天 |
| Environment Collector | 4~6 天 |
| Analysis Agent | 6~9 天 |
| LLM Gateway | 3~4 天 |
| Case 库 | 3~4 天 |
| WebUI | 5~7 天 |
| 配置 / 接口 / 部署 / 测试补齐 | 3~5 天 |
| 合计 | 5~8 周 |

## 13. 迭代顺序

### 第 1 阶段：持久化任务基础

目标：不依赖浏览器插件，先跑通核心分析。

内容：

- Server 上传接口
- 任务创建
- 解压和 manifest
- rg 检索
- 统一 `logagent.yaml`
- 持久化任务列表、稳定状态和执行阶段
- Metadata 接入 task context
- analysis state/event schema
- 核心 fixture 测试
- 任务详情页

### 第 2 阶段：证据能力

目标：让分析结论能结合用户输入的软件版本和实际代码。

内容：

- 产品/版本输入
- 版本到 tag/branch 映射配置
- 本地代码仓 worktree 管理
- `rg` / `git grep` 代码检索
- `code_evidence.json`
- LLM 输出中引用代码文件和行号
- worktree 清理策略
- 关键词提取规则
- Tool Runner 白名单工具
- Environment Collector 白名单 SSH/SCP

### 第 3 阶段：Analysis Agent 闭环

目标：支持多轮调查、信息缺口、用户追问、动作审批和预算终止。

内容：

- `analysis_state.json` 和 `analysis_events.jsonl`
- 结构化 action 和 Server executor
- `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL`
- message / decision API
- 轮次、动作、重复、token 和运行时间预算
- 幂等和重启恢复

### 第 4 阶段：LLM Gateway 和 WebUI 调查交互

目标：接入结构化推理后端并让用户参与调查闭环。

内容：

- Provider、Prompt、token 裁剪和结构化响应
- LLM stub 和有限重试
- 调查时间线、待补充问题和审批卡片
- 最终结果和证据跳转

### 第 5 阶段：Case 库

目标：让人工确认结果沉淀为可复用经验。

内容：

- Case 编辑体验
- Case 禁用 / 删除
- Case 检索优化
- Agent 输入中的 Case 摘要质量优化
- 只保存人工确认的最终结果，不保存中间假设

### 第 6 阶段：质量提升

目标：提高分析稳定性。

内容：

- 更好的日志模式归一化
- 更多关键词和规则配置
- 失败任务诊断
- 大文件上传优化或分片上传
- 工具结果 JSON schema 标准化
- 版本间 diff / commit 对比
- 更多测试环境采集模板
- pgvector 迁移

## 14. 结论

该方案可行，但 MVP 重点应该放在“可持久化任务 + 受控证据能力 + Analysis Agent 调查闭环”，而不是把单次 LLM 调用当作 Agent，或建设完整日志平台。

优先顺序调整为：持久化任务基础 -> 工具/代码/环境证据 -> Analysis Agent 多轮闭环 -> LLM Gateway 与用户交互 -> Case 管理。这样可以先建立可恢复、安全和可审计的调查能力，再逐步提高模型和证据质量。
