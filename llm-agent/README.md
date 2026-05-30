# LLM Agent 方案

## 实现建议

优先使用 Rust 实现编排、证据裁剪和 Prompt 组装。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

如果某些模型 SDK 只有 Python/Java 支持，可以把它们作为窄接口适配层；核心任务编排仍优先放在 Rust 服务端。

## LLM 配置

LLM Provider 必须配置化。MVP 推荐使用 OpenAI-compatible 接口，方便切换 OpenAI、企业网关或本地兼容服务。

```yaml
llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model: "gpt-4.1"
  max_input_tokens: 64000
  max_output_tokens: 4096
```

## Token 预算

多证据输入必须裁剪，不能简单把所有内容塞进 Prompt。

默认预算建议：

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

1. 用户问题、任务元信息必须保留。
2. 有文件和行号的日志证据优先。
3. 工具 finding 优先于 raw output。
4. 与错误关键词命中的代码证据优先。
5. Case 只保留 Top 5 的标题、现象、根因和解决方案摘要。

## 职责

LLM Agent 负责把日志证据、工具结果、代码证据、环境证据和历史 Case 组织成结构化输入，并输出故障分析结论。

第一版只做单 Agent，不做 Multi-Agent。

## 输入

- 用户问题
- manifest 摘要
- Top 20 错误模式
- Top 20 关键上下文
- 外部工具分析结果摘要
- 对应版本代码证据
- 测试环境采集摘要
- Top 5 相似历史 Case

## 输出结构

```markdown
# 结论

# 问题现象

# 关键证据

# 根因分析

# 修复建议

# 置信度
```

## Prompt 约束

- 必须引用日志文件名和行号作为证据。
- 引用工具结论时必须标明工具名，例如 `flux_query_analyzer`。
- 引用代码证据时必须标明版本、文件和行号。
- 引用环境证据时必须标明节点名和采集命令或文件路径。
- 无证据时明确说明不确定。
- 区分“已确认事实”和“推测”。
- 修复建议要可执行。
- 输出置信度：高 / 中 / 低。

## 证据分类

LLM 输出中应区分：

- 日志原文证据
- 工具分析证据
- 代码实现证据
- 环境采集证据
- 历史 Case 参考

## 工具结果 Prompt 片段

```markdown
## 工具分析结果

### flux_query_analyzer

- 状态：OK
- 摘要：发现 2 个可能导致慢查询的 range/filter 顺序问题
- 证据：
  - query.log:120 filter 下推失败，可能导致扫描数据量过大
```
