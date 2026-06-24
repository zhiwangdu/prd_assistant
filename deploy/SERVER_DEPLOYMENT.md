# LocalToolHub Server 部署手册

本文档说明如何把 LocalToolHub Server 部署到一台 Linux 机器上，提供 WebUI、Tool Runner、Run History、Artifact Store 和 MCP Server。当前仓库仍保留 `logagent-server` 二进制名、`LOGAGENT_*` 环境变量、`logagent.*` tool id 和 `logagent://` MCP namespace。

## 1. 部署目标

推荐部署形态：

```text
Developer workstation / external MCP client
  -> http://<server>:50992/ or SSH tunnel to 127.0.0.1:50992
  -> LocalToolHub Server
    -> WebUI static files
    -> tool catalog and tool runner
    -> uploads / runs / artifacts
    -> MCP /api/mcp
```

推荐运行目录：

```text
/opt/localtoolhub/
  bin/
    logagent-server
    tools/
  data/
    uploads/
    runs/
    artifacts/
    workspaces/
    dev_selftest/
  deploy/
    logagent.yaml
    .env
    logagentctl.sh
    rebuild-install.sh
  webui/out/
```

源码目录与运行目录建议分离：

```text
/srv/localtoolhub-src/prd_assistant      # git checkout
/opt/localtoolhub                        # runtime
```

## 2. 前置条件

基础依赖：

- Linux 主机，建议 Ubuntu 22.04+、Debian 12+、Rocky/CentOS 9+ 或同类发行版。
- `git`、`curl`、`ca-certificates`。
- C/C++ 构建工具和 `pkg-config`。
- Rust toolchain (`cargo`)。
- Node.js + npm，用于构建 `webui/out`。
- Go toolchain，用于构建 source-built analyzer。

可选依赖：

- Docker + compose plugin：仅在使用 `dev_selftest` Docker 路径时需要。
- `systemd`：用于生产式守护进程托管。
- `rg`：排障时更方便，不是运行必需。

仓库提供依赖安装脚本：

```bash
cd /srv/localtoolhub-src/prd_assistant
deploy/install-deps.sh
```

先查看将执行的命令：

```bash
deploy/install-deps.sh --dry-run
```

内网环境需要提前准备系统包源、Rust/Node/Go 源、Go proxy、git submodule 镜像；构建工具脚本支持 `LOGAGENT_SUBMODULE_*` 和 `GOPROXY` 等环境变量。

## 3. 获取源码

```bash
sudo mkdir -p /srv/localtoolhub-src
sudo chown -R "$USER":"$USER" /srv/localtoolhub-src

cd /srv/localtoolhub-src
git clone <repo-url> prd_assistant
cd prd_assistant
git checkout rewrite/local-toolhub-rust
```

如需固定版本，部署前记录 commit：

```bash
git rev-parse HEAD
```

`deploy/rebuild-install.sh` 会调用 `scripts/build-tools.sh`，该脚本会按需初始化相关 `third_party/` submodule。

## 4. 准备运行目录

```bash
export LOGAGENT_APP_DIR=/opt/localtoolhub
export LOGAGENT_SRC_DIR=/srv/localtoolhub-src/prd_assistant

sudo mkdir -p "$LOGAGENT_APP_DIR"/{bin/tools,data,deploy,webui}
sudo chown -R "$USER":"$USER" "$LOGAGENT_APP_DIR"

cp "$LOGAGENT_SRC_DIR/deploy/logagentctl.sh" "$LOGAGENT_APP_DIR/deploy/logagentctl.sh"
cp "$LOGAGENT_SRC_DIR/deploy/rebuild-install.sh" "$LOGAGENT_APP_DIR/deploy/rebuild-install.sh"
cp "$LOGAGENT_SRC_DIR/deploy/logagent.example.yaml" "$LOGAGENT_APP_DIR/deploy/logagent.yaml"
chmod +x "$LOGAGENT_APP_DIR/deploy/logagentctl.sh" "$LOGAGENT_APP_DIR/deploy/rebuild-install.sh"
```

创建运行环境文件：

```bash
cat > "$LOGAGENT_APP_DIR/deploy/.env" <<'EOF'
LOGAGENT_APP_DIR=/opt/localtoolhub
LOGAGENT_SRC_DIR=/srv/localtoolhub-src/prd_assistant
LOGAGENT_CONFIG=/opt/localtoolhub/deploy/logagent.yaml
LOGAGENT_SERVER_BIN=/opt/localtoolhub/bin/logagent-server
LOGAGENT_HEALTH_URL=http://127.0.0.1:50992/health

# Required by auth.api_keys[].value_env in logagent.yaml.
LOGAGENT_NATIVE_API_KEY=replace-with-a-long-random-token

# Optional: source-built analyzer / third_party mirror overrides.
# LOGAGENT_SUBMODULE_BASE_URL=https://git.example.com/mirrors
# GOPROXY=https://goproxy.cn,direct
# GOSUMDB=off
EOF
chmod 600 "$LOGAGENT_APP_DIR/deploy/.env"
```

生成 API key 示例：

```bash
openssl rand -hex 32
```

把生成值写入 `.env` 的 `LOGAGENT_NATIVE_API_KEY`。

## 5. 配置 Server

运行配置文件是：

```text
/opt/localtoolhub/deploy/logagent.yaml
```

最小关键项：

```yaml
server:
  bind: "127.0.0.1:50992"
  public_base_url: "http://127.0.0.1:50992"
  max_concurrent_tasks: 2

storage:
  data_dir: "${LOGAGENT_APP_DIR}/data"

auth:
  api_keys:
    - name: "webui"
      value_env: "LOGAGENT_NATIVE_API_KEY"

mcp:
  enabled: true
  transport: "stdio"
```

绑定地址建议：

- 本机使用或经 SSH tunnel 使用：`server.bind: "127.0.0.1:50992"`。
- 局域网直接访问：`server.bind: "0.0.0.0:50992"`，同时必须配防火墙、强 API key；如暴露给浏览器跨域 MCP client，还需要反向代理 TLS，并配置 `mcp.allowed_origins`。

MCP 远程访问推荐用 SSH tunnel：

```bash
ssh -N -L 50992:127.0.0.1:50992 <user>@<linux-server>
```

然后客户端使用：

```text
http://127.0.0.1:50992/api/mcp
Authorization: Bearer <LOGAGENT_NATIVE_API_KEY>
```

## 6. 构建与安装

首次构建安装：

```bash
cd /opt/localtoolhub
deploy/rebuild-install.sh --no-restart
```

脚本行为：

- 从 `LOGAGENT_SRC_DIR` 构建 Rust Server。
- 当前脚本使用 `cargo build` 的 debug profile；如果后续需要 release profile，应先调整部署脚本或改用 release 构建安装流程。
- 调用 `scripts/build-tools.sh` 构建 source-built analyzers 到 `bin/tools/`。
- 构建 WebUI 到 `webui/out`。
- 创建 data 子目录。
- 安装 `bin/logagent-server`。
- 不删除已有运行数据。

如果只替换 Server 二进制：

```bash
deploy/rebuild-install.sh --server-only
```

如果服务已在运行，默认会自动重启；不希望重启时使用：

```bash
deploy/rebuild-install.sh --no-restart
```

## 7. 启停与健康检查

启动：

```bash
/opt/localtoolhub/deploy/logagentctl.sh start
```

查看状态：

```bash
/opt/localtoolhub/deploy/logagentctl.sh status
```

查看日志：

```bash
/opt/localtoolhub/deploy/logagentctl.sh logs
```

重启：

```bash
/opt/localtoolhub/deploy/logagentctl.sh restart
```

停止：

```bash
/opt/localtoolhub/deploy/logagentctl.sh stop
```

健康检查：

```bash
curl -sS http://127.0.0.1:50992/health
```

带鉴权检查 tools：

```bash
source /opt/localtoolhub/deploy/.env
curl -sS \
  -H "Authorization: Bearer $LOGAGENT_NATIVE_API_KEY" \
  http://127.0.0.1:50992/api/tools
```

检查 MCP：

```bash
source /opt/localtoolhub/deploy/.env
curl -sS \
  -H "Authorization: Bearer $LOGAGENT_NATIVE_API_KEY" \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  http://127.0.0.1:50992/api/mcp
```

WebUI：

```text
http://127.0.0.1:50992/
```

页面顶部填写 `.env` 中的 `LOGAGENT_NATIVE_API_KEY`。

## 8. systemd 托管（可选）

如果需要开机自启，可以创建 systemd unit。下面示例假设运行用户为 `localtoolhub`，且 `/opt/localtoolhub` 已归该用户所有。

```bash
sudo useradd --system --create-home --home-dir /opt/localtoolhub --shell /usr/sbin/nologin localtoolhub || true
sudo chown -R localtoolhub:localtoolhub /opt/localtoolhub
```

创建 `/etc/systemd/system/localtoolhub.service`：

```ini
[Unit]
Description=LocalToolHub Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=localtoolhub
Group=localtoolhub
WorkingDirectory=/opt/localtoolhub
EnvironmentFile=/opt/localtoolhub/deploy/.env
ExecStart=/opt/localtoolhub/bin/logagent-server --config /opt/localtoolhub/deploy/logagent.yaml
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

启用：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now localtoolhub
sudo systemctl status localtoolhub
```

查看日志：

```bash
journalctl -u localtoolhub -f
```

如果使用 Docker dev_selftest，systemd 运行用户需要有 Docker 权限；生产环境更推荐通过 rootless Docker 或明确的 docker group 权限控制。

## 9. WebUI 与 MCP client 接入

WebUI 直接访问 Server 根路径：

```text
http://<server>:50992/
```

HTTP MCP endpoint：

```text
POST http://<server>:50992/api/mcp
Authorization: Bearer <api-key>
MCP-Protocol-Version: 2025-06-18
```

推荐远程工作站通过 SSH tunnel 接入：

```bash
ssh -N -L 50992:127.0.0.1:50992 <user>@<linux-server>
```

客户端配置示例：

```json
{
  "mcpServers": {
    "localtoolhub": {
      "type": "http",
      "url": "http://127.0.0.1:50992/api/mcp",
      "headers": {
        "Authorization": "Bearer <LOGAGENT_NATIVE_API_KEY>",
        "MCP-Protocol-Version": "2025-06-18"
      }
    }
  }
}
```

同机 stdio MCP：

```json
{
  "mcpServers": {
    "localtoolhub": {
      "command": "/opt/localtoolhub/bin/logagent-server",
      "args": ["mcp-serve"]
    }
  }
}
```

stdio 模式同样读取配置；如果客户端不会自动注入环境变量，需要在客户端配置中补齐 `LOGAGENT_CONFIG`、`LOGAGENT_APP_DIR` 和 API key 相关环境变量，或用 wrapper script 加载 `/opt/localtoolhub/deploy/.env` 后再执行 `mcp-serve`。

## 10. 可选能力配置

### 10.1 Source-built analyzers

`deploy/rebuild-install.sh` 会调用：

```bash
scripts/build-tools.sh
```

产物写入：

```text
/opt/localtoolhub/bin/tools/
```

如果某个 analyzer 构建失败但当前不需要，可在 `logagent.yaml` 中把对应 `tools.<id>.enabled` 改为 `false`，然后重新启动 Server。

内网 submodule 镜像可用环境变量覆盖：

```bash
LOGAGENT_SUBMODULE_BASE_URL=https://git.example.com/mirrors
LOGAGENT_SUBMODULE_INFLUXQL_URL=https://git.example.com/mirrors/influxql.git
LOGAGENT_SUBMODULE_FLUX_URL=https://git.example.com/mirrors/flux.git
LOGAGENT_SUBMODULE_OPENGEMINI_URL=https://git.example.com/mirrors/openGemini.git
LOGAGENT_SUBMODULE_INFLUXDB_URL=https://git.example.com/mirrors/influxdb.git
```

### 10.2 Dev Self-Test Docker

启用 `dev_selftest` Docker 路径前确认：

```bash
docker version
docker compose version
```

运行用户需要 Docker 权限：

```bash
sudo usermod -aG docker "$USER"
newgrp docker
```

示例配置见：

```text
examples/server-dev-selftest.yaml
deploy/devselftest/opengemini/README.md
```

内网常用覆盖：

```bash
OG_BASE_IMAGE=<registry>/ubuntu:24.04
GOPROXY=<internal-go-proxy>,direct
GOSUMDB=off
DEVSELFTEST_TEST_IMAGE=<registry>/alpine:3.20
```

## 11. 升级流程

升级前备份：

```bash
ts="$(date +%Y%m%d-%H%M%S)"
cp /opt/localtoolhub/bin/logagent-server "/opt/localtoolhub/bin/logagent-server.$ts"
tar -czf "/opt/localtoolhub-backup-$ts.tgz" \
  -C /opt \
  localtoolhub/deploy/logagent.yaml \
  localtoolhub/deploy/.env \
  localtoolhub/data
```

拉取代码：

```bash
cd /srv/localtoolhub-src/prd_assistant
git fetch --all --prune
git checkout rewrite/local-toolhub-rust
git pull --ff-only
```

构建安装并重启：

```bash
/opt/localtoolhub/deploy/rebuild-install.sh
/opt/localtoolhub/deploy/logagentctl.sh status
```

如果只改 Rust Server 且 WebUI 未变：

```bash
/opt/localtoolhub/deploy/rebuild-install.sh --server-only
```

## 12. 回滚流程

停止服务：

```bash
/opt/localtoolhub/deploy/logagentctl.sh stop
```

恢复二进制：

```bash
cp /opt/localtoolhub/bin/logagent-server.<timestamp> /opt/localtoolhub/bin/logagent-server
chmod +x /opt/localtoolhub/bin/logagent-server
```

如配置或数据也需要回滚：

```bash
tar -xzf /opt/localtoolhub-backup-<timestamp>.tgz -C /opt
```

启动并验证：

```bash
/opt/localtoolhub/deploy/logagentctl.sh start
/opt/localtoolhub/deploy/logagentctl.sh status
```

## 13. 备份与数据保留

必须备份：

- `deploy/logagent.yaml`
- `deploy/.env`
- `data/`
- 如手工放置工具二进制：`bin/tools/`

不建议备份：

- `logagent-server.log`
- `logagent-server.pid`
- 临时构建缓存
- `target/`
- `webui/out/`（可重新构建）

示例：

```bash
ts="$(date +%Y%m%d-%H%M%S)"
tar -czf "/opt/localtoolhub-runtime-$ts.tgz" \
  --exclude='localtoolhub/logagent-server.log' \
  --exclude='localtoolhub/logagent-server.pid' \
  -C /opt \
  localtoolhub/deploy \
  localtoolhub/data \
  localtoolhub/bin/tools
```

## 14. 安全检查清单

- `.env` 权限为 `0600`，不提交仓库。
- `LOGAGENT_NATIVE_API_KEY` 使用长随机值。
- `server.bind` 默认用 `127.0.0.1`；跨机器优先 SSH tunnel。
- 若直接开放 HTTP，必须经 TLS 反向代理，并限制防火墙来源。
- `mcp.allowed_origins` 仅在确实需要浏览器跨域访问时配置为明确 origin。
- `dev_selftest` 子系统按需启用；未使用保持 disabled。
- 所有 secret 通过环境变量引用，不写入 YAML、日志、artifact 或导出包。
- Artifact 对外只暴露逻辑 ID，不把任意本机路径交给用户。

## 15. 常见排障

### 15.1 启动失败：`Config not found`

确认：

```bash
ls -l /opt/localtoolhub/deploy/logagent.yaml
source /opt/localtoolhub/deploy/.env
echo "$LOGAGENT_CONFIG"
```

### 15.2 启动失败：API key env missing

`logagent.yaml` 中 `auth.api_keys[].value_env` 指向的环境变量必须存在：

```bash
source /opt/localtoolhub/deploy/.env
test -n "$LOGAGENT_NATIVE_API_KEY" && echo ok
```

### 15.3 WebUI 空白或 404

确认 WebUI 已构建安装：

```bash
ls -l /opt/localtoolhub/webui/out/index.html
/opt/localtoolhub/deploy/rebuild-install.sh --no-restart
/opt/localtoolhub/deploy/logagentctl.sh restart
```

### 15.4 `/api/tools` 401

所有受保护 API 都需要：

```text
Authorization: Bearer <api-key>
```

WebUI 顶部也要填写同一个 API key。

### 15.5 MCP HTTP 连接失败

检查：

```bash
curl -sS http://127.0.0.1:50992/health
curl -sS \
  -H "Authorization: Bearer $LOGAGENT_NATIVE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  http://127.0.0.1:50992/api/mcp
```

跨机器时优先确认 SSH tunnel：

```bash
ssh -N -L 50992:127.0.0.1:50992 <user>@<linux-server>
```

### 15.6 Analyzer 构建失败

先确认 Go/Rust/Node：

```bash
go version
cargo --version
node --version
npm --version
```

内网常见问题是 submodule 或 Go module 源不可达。配置 `LOGAGENT_SUBMODULE_*`、`GOPROXY`、`GOSUMDB=off` 后重跑：

```bash
/opt/localtoolhub/deploy/rebuild-install.sh --no-restart
```

### 15.7 Docker dev_selftest 权限失败

确认运行用户能访问 Docker：

```bash
docker ps
docker compose version
```

如果使用 systemd，确认 service 的 `User=` 具备 Docker 权限。

## 16. 最小验收

部署完成后至少执行：

```bash
source /opt/localtoolhub/deploy/.env

curl -sS http://127.0.0.1:50992/health

curl -sS \
  -H "Authorization: Bearer $LOGAGENT_NATIVE_API_KEY" \
  http://127.0.0.1:50992/api/tools

curl -sS \
  -H "Authorization: Bearer $LOGAGENT_NATIVE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  http://127.0.0.1:50992/api/mcp
```

同时在浏览器打开：

```text
http://127.0.0.1:50992/
```

确认：

- WebUI 可加载。
- API key 填写后 Tools 页面能显示 catalog。
- MCP 页面能显示 server、protocol、tools/resources。
- `logagentctl.sh status` 返回 running。
- 日志中无持续重启、panic 或鉴权配置错误。
