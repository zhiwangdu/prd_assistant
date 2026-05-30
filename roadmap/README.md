# Roadmap

## 工期预估

| 组件 | 预估 |
|------|------|
| Chrome 插件 | 3~4 天 |
| Native Agent | 2~3 天 |
| Rust Server | 5~7 天 |
| rg 分析器 | 7~10 天 |
| Tool Runner | 2~3 天 |
| Code Evidence | 4~6 天 |
| Environment Collector | 4~6 天 |
| LLM Agent | 3~4 天 |
| Case 库 | 3~4 天 |
| WebUI | 5~7 天 |
| 配置 / 接口 / 部署 / 测试补齐 | 3~5 天 |
| 合计 | 5~8 周 |

## 第 1 阶段：手动上传闭环

目标：不依赖浏览器插件，先跑通核心分析。

内容：

- Server 上传接口
- 任务创建
- 解压和 manifest
- rg 检索
- 外部工具白名单配置和手动调用
- 统一 `logagent.yaml`
- 基础 Case 确认、embedding 和 Top 5 召回
- LLM Provider 配置和 token 预算裁剪
- 核心 fixture 测试
- LLM 输出结果
- 任务详情页

## 第 2 阶段：版本感知代码证据

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

## 第 3 阶段：测试环境采集

目标：支持测试环境直接 SSH/SCP 采集信息。

内容：

- 测试环境配置
- 节点白名单
- 文件采集白名单
- 诊断命令白名单
- `environment_evidence.json`
- 采集结果接入统一分析流程
- SSH 并发、超时和重试策略

## 第 4 阶段：浏览器和 Native Agent

目标：把“下载日志 -> 上传分析”自动化。

内容：

- Native Agent 本地 HTTP Server
- Chrome 插件通过 `chrome.downloads.onChanged` 监听下载完成
- 下载完成后确认上传
- 自动打开任务详情页

## 第 5 阶段：Case 库

目标：让人工确认结果沉淀为可复用经验。

内容：

- Case 编辑体验
- Case 禁用 / 删除
- Case 检索优化
- Agent 输入中的 Case 摘要质量优化

## 第 6 阶段：质量提升

目标：提高分析稳定性。

内容：

- 更好的日志模式归一化
- 更多关键词和规则配置
- 失败任务诊断
- 大文件上传优化或分片上传
- Agent 建议触发外部工具，再由服务端审批执行
- 工具结果 JSON schema 标准化
- 版本间 diff / commit 对比
- 更多测试环境采集模板
- pgvector 迁移
