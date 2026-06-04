# Roadmap Spec

## 目标

定义 MVP 开发顺序，优先打通可验证闭环，再逐步补证据质量。

## 当前进度

已完成：

- Chrome Extension MVP
- Native Agent MVP
- Server 上传和任务框架
- 大文件分片上传
- zip/tar/tar.gz 解压
- WEBUI 第一版
- artifact 查询接口

## 下一阶段优先级

1. Server 持久化任务列表和状态机。
2. Metadata 框架：实例 ID、集群节点、模板导入、WEBUI 展示。
3. Tool Runner 接入 `flux_query_analyzer` 和 `influxql_analyzer`。
4. Code Evidence 版本到分支/ref 映射。
5. Environment Collector SSH/SCP 测试环境采集。
6. LLM Agent 结构化分析。
7. Case Store 保存和召回。

## 验收标准

- 每个阶段结束都能从 WEBUI 或 API 验证。
- 每个功能都更新对应 README 和 SPEC。
- 重要链路保持可回归测试。
