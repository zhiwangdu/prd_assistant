# Metadata Spec

## Requirements

- `instanceId` 是用户主键。
- 重复导入同一实例覆盖旧快照。
- Raw JSON 只按需展示。
- Field type 映射必须兼容 openGemini 类型码。
- MCP 查询必须 bounded。

## Acceptance

- WebUI 可导入、刷新、删除和查看 snapshot。
- MCP 可列实例并查询 field/tag fields。
