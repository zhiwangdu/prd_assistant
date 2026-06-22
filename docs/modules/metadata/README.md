# Metadata

Metadata 管理本地导入的实例、集群、节点、DB/RP/PT/Shard/Schema 快照。它为 WebUI、Tools 和 MCP 提供上下文查询。

## 职责

- 从 URL、文件或文本导入。
- 支持 openGemini `/getdata`。
- 保存 raw snapshot 和 normalized snapshot。
- 提供 field/tag 查询。
- WebUI 可浏览拓扑和诊断。
