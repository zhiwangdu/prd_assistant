# Local Runtime Deploy

`deploy/` 保存 LogAgent 两模块 LocalToolHub 的运行时部署模板。目标是把 Rust server binary、
WebUI 静态文件、日志 analyzer 二进制和本地 data 目录组织成可复制目录。

完整 Server 部署步骤见 [SERVER_DEPLOYMENT.md](./SERVER_DEPLOYMENT.md)，包括运行目录初始化、配置、构建安装、启停、MCP 接入、systemd 托管、升级、备份、回滚和排障。

## 目标目录

```text
$LOGAGENT_APP_DIR/
  bin/
    logagent-server
    tools/
      flux_query_analyzer
      influxql_analyzer
      opengemini-storage-analyzer
      influxdb_storage_analyzer
  data/
    uploads/
    workspaces/
    tasks/
    dev_selftest/
  webui/out/
  deploy/
    logagent.yaml
    .env
    logagentctl.sh
```

## 环境变量

必需：

- `LOGAGENT_APP_DIR`
- `LOGAGENT_SRC_DIR`
- `LOGAGENT_NATIVE_API_KEY` 或后续统一的 `LOGAGENT_API_KEY`

可选：

- `LOGAGENT_CONFIG`
- `LOGAGENT_SERVER_BIN`
- `LOGAGENT_HEALTH_URL`
- `LOGAGENT_SUBMODULE_BASE_URL`
- `LOGAGENT_SUBMODULE_INFLUXQL_URL`
- `LOGAGENT_SUBMODULE_FLUX_URL`
- `LOGAGENT_SUBMODULE_OPENGEMINI_URL`
- `LOGAGENT_SUBMODULE_INFLUXDB_URL`
- `LOGAGENT_INFLUXDB_REPO_URL`
- `LOGAGENT_INFLUXDB_REF`
- `INFLUXDB_BUILDER_IMAGE`
- `INFLUXDB_BASE_IMAGE`
- `INFLUXDB_PORT`

LLM/Agent/Fetch/Executor/Metadata/Case/Skills 相关变量不再是默认部署项。

## 构建安装

```bash
./rebuild-install.sh
```

目标行为：

- 构建 Rust Server binary。
- 构建 WebUI 到 `webui/out`。
- 按需构建 `third_party/` source-built analyzers 到 `bin/tools/`（Linux/macOS 用 `scripts/build-tools.sh`，Windows 用 `scripts/build-tools.ps1`，产物分别为 `bin/tools/<name>` 与 `bin/tools/<name>.exe`）。
- 创建 data 子目录。
- 不删除已有运行数据。

## openGemini dev_selftest 配置生成

```bash
deploy/probe-opengemini-config.sh \
  --git-ref devselftest/go126-sonic-latest-20260625-233438 \
  --print
```

该脚本会探测 `LOGAGENT_APP_DIR`、`LOGAGENT_SRC_DIR`、`git`、`docker`、`curl`、Docker
daemon/compose、openGemini demo 文件、`8086` 端口和 allowlisted git repo/ref，然后生成：

```text
$LOGAGENT_APP_DIR/deploy/server-opengemini.yaml
```

脚本优先读当前环境变量；若非交互 shell 没有加载变量，会直接解析 `~/.bashrc` 中的
`export LOGAGENT_APP_DIR=...` / `export LOGAGENT_SRC_DIR=...`。默认 repo/ref 指向已验证的
`ssh://git@github.com/zhiwangdu/openGemini.git` +
`devselftest/go126-sonic-latest-20260625-233438`，可用 `--repo-url` / `--git-ref` 覆盖。

## openGemini cloud runner image

`deploy/devselftest/opengemini-cloud-runner/` 提供一个最小 Python 测试框架镜像，
用于 `run_tests.testParams` 场景：外部/internal skill 创建云 openGemini/influxdb 实例，
ToolHub 只在 Docker 中运行测试用例。

```bash
docker build -t localtoolhub/opengemini-selftest:dev \
  deploy/devselftest/opengemini-cloud-runner
```

本地验证可在 `examples/server-dev-selftest.yaml` 中使用该 image。内网部署时只需要把
`dev_selftest.test_suites.cloud_opengemini_case.docker.image` 替换成内部 registry 的镜像；
云实例创建、凭据获取和内部 SDK 逻辑留在内部 skill 或内部镜像中。

## InfluxDB dev_selftest 配置生成

InfluxDB OSS 只支持单机版；本仓库提供单节点 `influxd` Docker demo：

```bash
deploy/probe-influxdb-config.sh --print
```

该脚本会探测 `LOGAGENT_APP_DIR`、`LOGAGENT_SRC_DIR`、`git`、`docker`、`curl`、
Docker daemon/compose、InfluxDB demo 文件、`8086` 端口和 allowlisted git repo/ref，然后生成：

```text
$LOGAGENT_APP_DIR/deploy/server-influxdb.yaml
```

默认 repo/ref 为 `ssh://git@github.com/zhiwangdu/influxdb.git` + `master-1.x`。生成配置使用
Docker-backed build profile，在 `golang:1.26-bookworm` 中构建 Linux `build/influxd`；构建脚本会在
缺少 `pkg-config/curl` 时对 Debian/Ubuntu builder 执行一次 `apt-get install`，并通过 rustup 安装
Rust 1.83（Flux `libflux` 需要）。随后由 `ubuntu:24.04` 单容器 compose 启动并通过 `alpine:3.20`
smoke 容器验证 v1 HTTP API。内网可用
`--builder-image` / `--base-image` / `--test-image` / `--db-port` 或对应环境变量覆盖。

## 启停

```bash
./logagentctl.sh start
./logagentctl.sh status
./logagentctl.sh logs
./logagentctl.sh restart
./logagentctl.sh stop
```

启动后访问：

```text
http://127.0.0.1:50992/
```

## 安全

- `.env` 不提交。
- secret 只放环境变量。
- 导出包不包含 API Key、Cookie、Authorization header 或数据目录内容。
- 跨机器 MCP 接入优先走 SSH tunnel；LogAgent 不保存 SSH 私钥，也不提供 SSH/SCP executor。
