# Interfaces Spec

## 目标

定义模块之间的边界、数据结构和状态机，避免各组件直接耦合实现细节。

## 当前状态

已有 Server DTO 和 Pipeline 内部模型，跨模块 trait 尚未拆分。

## 核心接口

上传：

```text
UploadRecord -> TaskContext -> PipelineOutput
```

证据产物：

```text
manifest.json
grep_results.json
tool_results/*.json
code_evidence.json
environment_evidence.json
metadata_context.json
result.json
```

任务状态规划：

```text
CREATED
UPLOADED
COLLECTING
EXTRACTING
SEARCHING
RUNNING_TOOLS
COLLECTING_CODE
ANALYZING
DONE
FAILED
```

## Rust 优先接口

后续模块优先定义 Rust trait：

- `LogAnalyzer`
- `ToolRunner`
- `CodeEvidenceCollector`
- `EnvironmentCollector`
- `MetadataStore`
- `LlmAnalyzer`
- `CaseStore`

## 验收标准

- 新模块先明确输入输出，再接入 pipeline。
- 公共 JSON 产物需要稳定字段名和版本兼容策略。
- README 和 SPEC 在接口或状态机变更时同步更新。
