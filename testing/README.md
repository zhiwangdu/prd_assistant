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

## 验收标准

- 任务失败时有明确错误原因。
- LLM 输入不会超过配置的 token 预算。
- 输出结论必须能追溯到证据文件。
- 外部工具失败不会导致整个任务无结果，除非工具标记为必需。
- 不保存或快照测试隐藏思维链。
