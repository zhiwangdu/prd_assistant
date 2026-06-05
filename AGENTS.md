# AGENTS.md

## 工作方式

- 这是 LogAgent MVP 仓库。开始任何开发前先读根目录 `README.md`、`SPEC.md`，再读相关组件目录下的 `README.md` 和 `SPEC.md`。
- 后续每次修改或新增功能，必须同步更新对应组件的 `README.md` 和 `SPEC.md`。
- 后续每次修改完文件，必须同步更新根目录 `PROGRESS.md`，记录项目进展、行为变化、验证结果或下一步变化。
- 用户已明确要求：每次实现或修改完成后自动 `commit` 并 `push`。
- 除非用户明确要求，避免提交临时 review 输入文件，例如 `review_context.md`。
- 修改代码后优先跑能覆盖本次改动的检查；涉及 Rust 时至少跑 `cargo fmt --check`、`cargo check`，必要时跑 `cargo test`。
- 修改 WEBUI JS 后至少跑 `node --check webui/app.js`。

## 技术原则

- 新实现优先使用 Rust。
- 语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

- 已有编译工具可以直接复用，不强制重写；例如后续 Tool Runner 需要支持 `flux_query_analyzer`、`influxql_analyzer`。
- 前端第一版不引入构建链，当前 WEBUI 是静态 HTML/CSS/JS，由 Rust Server 托管。

## 当前实现状态

已实现组件：

- `chrome-extension/`
  - Manifest V3。
  - 监听 Chrome 下载完成。
  - URL 前缀和文件后缀匹配。
  - 点击 notification 后调用 Native Agent `/imports`。

- `native-agent/`
  - Rust/Axum 本地服务。
  - `GET /health`
  - `POST /imports`
  - 校验 `allowed_dirs`、`allowed_suffixes`、`max_upload_bytes`。
  - 小文件 multipart 上传，较大文件分片上传。
  - 上传后调用 Server 创建 task。

- `server/`
  - Rust/Axum 服务。
  - API Key middleware。
  - multipart 上传、multipart 批量上传、分片上传、task 创建、artifact 查询。
  - Metadata 查询、导入预览和确认。
  - 支持 openGemini `/getdata` 真实元数据 URL 拉取和归一化。
  - 静态托管 `webui/`。
  - 当前 task pipeline：copy raw -> per-upload extract -> manifest -> simple grep。

- `log-analyzer/`
  - 作为 Server 内部模块实现。
  - 支持 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz`、`.tar`。
  - `.tar.gz` / `.tgz` 失败后会 fallback 按 `.tar` 解包。
  - 生成 `manifest.json` 和 `grep_results.json`。

- `webui/`
  - 静态页面。
  - 支持健康检查、API Key、手动批量上传、小文件/分片上传、创建 task、查看 artifacts。
  - 支持 Metadata 查询、YAML/JSON/openGemini 导入预览和确认。
  - 任务列表当前只保存在浏览器 localStorage。

- `metadata/`
  - 已实现基础 store/API/WEBUI。
  - 管理实例 ID、集群节点和 YAML/JSON/openGemini 模板导入。
  - 数据持久化到 `storage.data_dir/metadata` 下的 JSON 文件。

规划中组件：

- `tool-runner/`：白名单调用外部工具，优先接入 `flux_query_analyzer`、`influxql_analyzer`。
- `code-evidence/`：根据用户输入的软件版本定位代码分支/tag/ref，收集文件行号证据。
- `environment-collector/`：测试环境通过 SSH/SCP 采集信息，不需要浏览器下载或本地上传。
- `llm-agent/`：证据裁剪、Prompt 组装、结构化分析结果。
- `case-store/`：人工确认后的 Case 沉淀和相似召回。

## 常用运行命令

Server 测试配置：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/server-test.yaml
```

测试配置端口：

```text
http://127.0.0.1:50992/
```

默认配置：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/logagent.yaml
```

Native Agent 对接远端测试 Server：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-native-agent -- --config examples/native-agent-remote-50992.yaml
```

常用检查：

```bash
cargo fmt --check
cargo check
cargo test
node --check webui/app.js
```

## API 快速参考

公开接口：

```http
GET /health
GET /
```

受保护接口：

```http
POST /api/uploads
POST /api/uploads/batch
POST /api/uploads/init
POST /api/uploads/:upload_id/chunks?offset=<bytes>
POST /api/uploads/:upload_id/complete
POST /api/tasks
GET /api/tasks/:task_id/artifacts
```

受保护接口必须携带：

```text
Authorization: Bearer <api-key>
```

Native Agent 本地接口：

```http
GET /health
POST /imports
```

## 数据目录和产物

Server workspace 结构：

```text
data_dir/
  uploads/
    upl_xxx/
  workspaces/
    task_xxx/
      raw/
        upl_xxx/
      extracted/
        package_name/
      manifest.json
      grep_results.json
```

测试配置 `examples/server-test.yaml` 使用：

```text
/tmp/logagent-server-test
```

## 安全边界

- API Key 只通过环境变量读取。
- 不把 API Key 写入日志、manifest、grep 结果或提交历史。
- Native Agent 只能读取 `allowed_dirs` 下的文件。
- Server 解压压缩包必须防止路径逃逸。
- Tool Runner 后续只能调用配置白名单中的工具。
- Environment Collector 后续只能访问配置中的测试环境节点和白名单路径/命令。
- LLM Agent 不能直接执行命令，只能基于已有证据分析。

## 近期开发优先级

1. Server 持久化任务列表和状态机，让 WEBUI 不只依赖 localStorage。
2. Metadata 接入 task context 并写入 `metadata_context.json`。
3. Tool Runner 接入 `flux_query_analyzer` 和 `influxql_analyzer`。
4. Code Evidence 支持版本到代码 ref 映射，并在独立 worktree/cache 中只读检索。
5. Environment Collector 支持 SSH/SCP 测试环境采集。
6. LLM Agent 结构化输出。
7. Case Store 保存和召回。

## 提交流程

完成实现后：

```bash
git status --short
git diff --stat
git add <相关文件>
git commit -m "<type>: <summary>"
git push
```

提交前确认：

- 没有误提交临时文件、密钥、生成的大数据目录。
- 对应组件 `README.md` 和 `SPEC.md` 已更新。
- 根目录 `PROGRESS.md` 已更新。
- 工作区最终应保持干净。
