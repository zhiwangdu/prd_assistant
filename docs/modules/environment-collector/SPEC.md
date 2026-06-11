# Environment Collector Spec

## 目标

Environment Collector 面向测试环境节点，通过 SSH/SCP 收集日志和环境信息，替代浏览器下载和本地上传链路。

## 当前状态

未实现代码，已有设计方向。

## 输入

- 测试环境节点配置
- SSH 用户和密钥环境变量
- 采集路径白名单
- 可选命令白名单
- 用户问题和软件版本
- 经批准的 `action_id`、环境和采集项选择

## 处理流程

```text
connect ssh
  -> collect files by scp
  -> run whitelisted commands
  -> store collected/
  -> generate environment_evidence.json
  -> hand off to Server pipeline
```

## 输出

```text
collected/
environment_evidence.json
```

建议记录：

- hostname
- os/kernel
- process status
- disk/memory
- service logs
- collected file list
- command outputs
- action id、批准事件引用和采集时间

## 安全约束

- 只连接配置中的测试环境节点。
- SCP 路径必须在白名单内。
- SSH 命令必须在白名单内。
- 不支持任意远程 shell。
- 默认必须有有效批准记录，配置可对特定只读采集项另行放宽。
- Claude Code 不能直接执行 SSH/SCP，只能通过 LogAgent MCP 请求审批和结构化采集意图。

## 验收标准

- 配置节点可连通时能收集文件到 workspace。
- 非白名单路径和命令被拒绝。
- 采集失败保留错误原因。
- 拒绝动作不会执行连接，并能作为分析事件继续任务。
- README 和 SPEC 在采集协议或配置变更时同步更新。
