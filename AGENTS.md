# AGENTS.md

## 工作方式

- 这是 LogAgent Local Tool/MCP Workbench 重构分支。开始任何开发前先读根目录 `README.md`、`SPEC.md`，再读相关组件目录下的 `README.md` 和 `SPEC.md`。
- 后续每次修改或新增功能，必须同步更新对应组件的 `README.md` 和 `SPEC.md`。
- 后续每次修改完文件，必须同步更新根目录 `PROGRESS.md`，记录项目进展、行为变化、验证结果或下一步变化。
- 用户已明确要求：每次实现或修改完成后自动 `commit` 并 `push`。
- 除非用户明确要求，避免提交临时 review 输入文件、IDE 配置、密钥、运行数据或生成的大目录。
- 修改代码后优先跑能覆盖本次改动的检查；涉及 Rust 时至少跑 `cargo fmt --check`、`cargo check`，必要时跑 `cargo test`。
- 修改 WebUI 后至少在 `webui/` 下跑 `npm run lint`、`npm run typecheck`、`npm run build`。

## 产品定位

LogAgent 当前目标不是自研通用 Agent，而是一个开箱即用、个人本地部署的 Rust 单二进制工具工作台和 MCP 工具合集。

主入口：

- Web 管理页面：配置、运行和审计工具。
- MCP Server：给 Claude Code、Codex、Cursor、OpenCode 等外部客户端提供受控 tools/resources。
- 可选 Native Agent / Chrome Extension：把浏览器下载文件导入本地工作台。

核心能力：

- Tool Catalog / Tool Runner
- Artifact / Run History
- Metadata 管理
- Fetch Endpoint 管理
- SSH/SCP Executor
- Code Evidence
- Log Analyzer
- Skills / System Context
- Case Notes / Memory
- MCP resources/tools

非核心能力：

- 自研多轮 Analysis Agent
- Claude Code 作为 Server 后端
- LangChain/LangGraph 工作流编排
- 企业级日志平台、CMDB、复杂权限体系

这些能力如需保留，只能作为可选自动化 workflow 或外部 MCP client 示例，不能成为默认运行依赖。

## 技术原则

- Server 使用 Rust/Axum，目标交付为单个 `logagent-local` 二进制。
- WebUI 使用 React + Vite + Tailwind CSS，`npm run build` 输出 `webui/out`，由 Rust Server 托管。
- 外部诊断工具优先复用已有二进制或源码构建产物，通过 Tool Runner 统一执行。
- 数据默认存本机目录，优先使用 JSON/SQLite/本地 artifact 文件，不引入 PostgreSQL、Redis、Elasticsearch。
- 所有执行能力必须通过 allowlist、参数 schema、预算、超时、artifact 审计和敏感信息脱敏。

## 常用命令

```bash
cargo fmt --check
cargo check
cargo test
cd webui && npm run lint
cd webui && npm run typecheck
cd webui && npm run build
```

本地运行目标命令后续以文档和实现为准，优先收敛到：

```bash
cargo run -p logagent-server -- --config examples/local-toolhub.yaml
```

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

- 对应组件 `README.md` 和 `SPEC.md` 已更新。
- 根目录 `PROGRESS.md` 已更新。
- 没有误提交 `.idea/`、临时说明、密钥、运行数据、构建缓存或第三方生成产物。
