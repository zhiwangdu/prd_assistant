# Testing Spec

## 目标

用自动测试和少量 smoke 覆盖本地工具平台核心闭环：catalog -> run -> artifact -> MCP/WebUI 展示。

## 必测能力

- Rust 编译和单测。
- WebUI lint/typecheck/build。
- Tool Runner 成功、失败、timeout、非 JSON 输出和 JSON result 解析。
- Artifact 逻辑路径、防路径逃逸和鉴权下载。
- Upload 解压和输入文件校验。
- Log analyzer preprocess / batch analysis / configured analyzer smoke。
- dev_selftest 配置校验、inline Docker target 校验、`run_tests.testParams` 安全校验和 fake docker 闭环。
- MCP resources/tools 基础 JSON-RPC。

## 不进入自动测试的内容

- 真实 LLM 请求。
- 真实 Claude Code/Codex 调用。
- 真实生产 SSH 节点。
- 已移除的 Fetch/Metadata/Case/Skills/Executor API。
- 需要外网的 Git clone 或工具下载。

## 验收

- `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- 新增工具必须至少有 fake tool 单测或 smoke 说明。
- 新增 WebUI 交互必须有构建检查。
- README/SPEC 在测试策略或 fixture 变更时同步更新。
