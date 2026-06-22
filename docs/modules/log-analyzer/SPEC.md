# Log Analyzer Spec

## Supported Inputs

```text
.log
.txt
.zip
.tar.gz
.tgz
.tar
```

## Requirements

- 解压不能逃逸 workspace。
- gzip 轮转日志透明读取。
- manifest 稳定可审计。
- search result 生成 artifact refs。

## Acceptance

- fixture archive 测试覆盖成功和恶意路径。
