# Testing 方案

## 目标

MVP 至少覆盖核心分析链路，避免只靠人工跑 demo。

## 测试层次

### 单元测试

- 日志行归一化
- 错误模式计数
- Tool Runner 参数模板替换
- Code Evidence 关键词提取
- Token 预算裁剪
- Agent action schema 和 fingerprint
- 状态 revision、幂等 message/decision
- 分析预算和终止条件
- Case 相似度计算

### Fixture 测试

准备固定样例：

```text
fixtures/
  redis_timeout/
    logs/
    expected_error_summary.json
  influxql_slow_query/
    logs/
    tool_outputs/
    expected_code_keywords.json
  environment_disk_full/
    collected/
    expected_environment_evidence.json
```

### 集成测试

覆盖 upload 来源：

```text
upload -> extract -> rg -> Agent -> action -> evidence -> LLM stub -> result
```

覆盖 environment 来源：

```text
environment approval -> collect stub -> Agent continuation -> result
```

当前任务系统测试覆盖：

- Upload Store 创建、原子持久化、重新加载、损坏 JSON 启动失败和中断进度校正。
- 分片 offset、预期大小、完成状态以及未完成上传创建任务的拒绝路径。
- `/api/uploads` 和 `/api/uploads/batch` multipart 路径覆盖 payload flush 后再持久化记录。
- Upload API 并发测试使用进程内原子序号隔离临时数据目录，避免目录碰撞导致 payload 被其他测试清理。
- Metadata task context 的 ID 推导、冲突拒绝、workspace 快照、artifact API 和 Prompt 摘要。
- Task Store 创建、更新、重新加载、倒序列表、损坏 JSON 失败和终态保护。
- `RUNNING -> QUEUED` 启动恢复、phase/attempt 保留和阶段级幂等继续执行。
- expected phase 推进校验和损坏状态启动失败。
- raw 快照重复执行、派生产物清理和结果重建。
- API `202` 创建、列表、详情、404 和 artifacts 409。
- stub LLM 单次任务闭环和 result API。
- Task API 并发测试使用进程内原子序号隔离临时数据目录，避免目录碰撞导致后台任务误删数据。
- Prompt 裁剪、Chat Completions 内容解析、Provider 状态分类和 evidence ref 校验。
- LLM evidence ref 覆盖 canonical refs、裸日志行号/范围、索引范围和无法映射引用的拒绝路径。
- LLM root cause 解析覆盖真实模型返回的字符串数组形态，并抽取内嵌 `evidenceRefs`。
- LLM 列表字段解析覆盖真实模型返回的单字符串 `missingInformation` 并规范化为数组。
- Chat Completions 解析覆盖纯 JSON、完整 JSON 代码围栏、自然语言包裹的唯一 JSON object，以及多个 JSON object 的拒绝路径。
- LLM Gateway 测试覆盖 schema 修正重试提示，以及解析错误中包含具体字段/类型原因。
- Tool Runner 覆盖配置校验、规则 action、fake tool 执行、timeout、幂等复用、dispatcher 接入和 artifacts API。

### LLM 测试策略

开发和 CI 中默认使用 LLM stub，不直接调用真实模型。

Stub 必须支持脚本化多轮响应：

- 首轮请求日志搜索，次轮输出结论。
- 请求用户信息，回答后恢复。
- 请求环境采集，批准和拒绝分支。
- 重复 action、预算耗尽和无效 schema。
- Server 重启后从 state/event 恢复。

真实模型调用只做手动验收：

- 小日志包
- 固定问题
- 固定期望证据
- 检查输出是否引用日志、工具、代码和环境证据
- 当前使用 `examples/server-llm-openai-compatible.yaml` 验证单次日志结果；不要在自动测试中使用真实模型。
- 手工真实模型验收需要设置 `LOGAGENT_LLM_BASE_URL`、`LOGAGENT_LLM_API_KEY` 和 `LOGAGENT_LLM_MODEL`。

## 验收标准

- 任务失败时有明确错误原因。
- LLM 输入不会超过配置的 token 预算。
- 输出结论必须能追溯到证据文件。
- 外部工具失败不会导致整个任务无结果，除非工具标记为必需。
- 不保存或快照测试隐藏思维链。
