# Log Analyzer

Log Analyzer 负责把上传日志包预处理为工具可消费输入和基础搜索结果。

## 职责

- 安全解压常见归档。
- 防路径逃逸。
- 生成 manifest。
- 支持 grep/search。
- 为 source-built analyzers 生成 input index。
