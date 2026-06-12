use std::{
    io::{self, BufRead, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use chrono::Utc;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use crate::{
    domain::{
        contracts::{ActionKind, ActionRisk, AgentAction, EvidenceProvider, TaskContext},
        models::{GrepResults, SystemContextBundle, TaskRecord},
    },
    pipeline::{read_tool_results, search_task_with_settings},
    services::{
        agent_contracts::write_json_atomic, skill_registry::SkillRegistry, tool_runner::ToolRunner,
    },
    stores::{analysis_state, case_store::CaseStore, task_store::TaskStore},
    support::{
        config::{AnalysisMode, AppConfig, LogAnalyzerSettings},
        id::next_id,
    },
};

pub async fn run_stdio(
    config: Arc<AppConfig>,
    task_id: String,
    mode: AnalysisMode,
) -> anyhow::Result<()> {
    if !config.mcp.enabled {
        anyhow::bail!("MCP is disabled by configuration");
    }
    let tasks = TaskStore::load(config.storage.tasks_dir())?;
    let task = tasks
        .get(&task_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown taskId {task_id}"))?;
    let skills = SkillRegistry::load(config.skills.clone())?;
    let workspace = config.storage.workspace_dir(&task_id);
    tokio::fs::create_dir_all(&workspace).await?;
    info!(
        task_id = %task_id,
        mode = %mode.as_str(),
        workspace = %workspace.display(),
        "MCP stdio server started"
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(err) => {
                write_response(&mut stdout, None, json_rpc_error(-32700, err.to_string()))?;
                continue;
            }
        };
        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if id.is_none() {
            continue;
        }
        let response = match method {
            "initialize" => Ok(initialize_result()),
            "ping" => Ok(json!({})),
            "resources/list" => Ok(resources_list_result(&workspace, &task_id).await?),
            "resources/read" => {
                let uri = request
                    .pointer("/params/uri")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| anyhow::anyhow!("resources/read requires params.uri"))?;
                read_resource_result(&workspace, &task, uri).await
            }
            "tools/list" => Ok(tools_list_result()),
            "tools/call" => {
                let name = request
                    .pointer("/params/name")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| anyhow::anyhow!("tools/call requires params.name"))?;
                let arguments = request
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                call_tool(&config, &skills, &workspace, &task, mode, name, arguments).await
            }
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err(anyhow::anyhow!("unsupported MCP method {method}")),
        };
        match response {
            Ok(result) => {
                info!(task_id = %task_id, method = %method, "MCP request succeeded");
                write_response(&mut stdout, id, json!({ "result": result }))?
            }
            Err(err) => {
                warn!(
                    task_id = %task_id,
                    method = %method,
                    error = %err,
                    "MCP request failed"
                );
                write_response(&mut stdout, id, json_rpc_error(-32000, format!("{err:#}")))?
            }
        }
    }
    Ok(())
}

fn write_response(
    stdout: &mut io::Stdout,
    id: Option<serde_json::Value>,
    body: serde_json::Value,
) -> anyhow::Result<()> {
    let response = match body.get("error") {
        Some(_) => json!({ "jsonrpc": "2.0", "id": id, "error": body["error"] }),
        None => json!({ "jsonrpc": "2.0", "id": id, "result": body["result"] }),
    };
    stdout.write_all(serde_json::to_string(&response)?.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn json_rpc_error(code: i64, message: String) -> serde_json::Value {
    json!({ "error": { "code": code, "message": message } })
}

fn initialize_result() -> serde_json::Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "resources": {},
            "tools": {}
        },
        "serverInfo": {
            "name": "logagent",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

async fn resources_list_result(
    workspace: &Path,
    task_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let mut resources = vec![
        resource(task_id, "summary", "Task summary", "application/json"),
        resource(
            task_id,
            "artifact_index",
            "Artifact index",
            "application/json",
        ),
    ];
    for (name, description) in [
        ("manifest", "Manifest"),
        ("grep_results", "Grep results"),
        ("metadata_context", "Metadata context"),
        ("system_context", "System context"),
        ("case_context", "Case context"),
        ("tool_results", "Tool results"),
    ] {
        if resource_path(workspace, name).exists() || name == "tool_results" {
            resources.push(resource(task_id, name, description, "application/json"));
        }
    }
    Ok(json!({ "resources": resources }))
}

fn resource(task_id: &str, name: &str, description: &str, mime_type: &str) -> serde_json::Value {
    json!({
        "uri": format!("logagent://task/{task_id}/{name}"),
        "name": name,
        "description": description,
        "mimeType": mime_type
    })
}

async fn read_resource_result(
    workspace: &Path,
    task: &TaskRecord,
    uri: &str,
) -> anyhow::Result<serde_json::Value> {
    let prefix = format!("logagent://task/{}/", task.task_id);
    let name = uri
        .strip_prefix(&prefix)
        .ok_or_else(|| anyhow::anyhow!("resource URI does not belong to task {}", task.task_id))?;
    let value = match name {
        "summary" => json!({
            "schemaVersion": 1,
            "taskId": task.task_id,
            "sessionId": task.session_id,
            "analysisMode": task.analysis_mode,
            "question": task.question,
            "sourceUrl": task.source_url,
            "instanceId": task.instance_id,
            "clusterId": task.cluster_id,
            "nodeId": task.node_id,
            "uploadIds": task.upload_ids,
        }),
        "artifact_index" => artifact_index(workspace).await?,
        "tool_results" => json!({ "toolResults": read_tool_results(workspace).await? }),
        other => read_json_resource(workspace, other).await?,
    };
    log_mcp_call(
        workspace,
        "resources/read",
        json!({ "uri": uri }),
        "succeeded",
        json!({ "resource": name }),
        Vec::new(),
    )
    .await?;
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(&value)?
        }]
    }))
}

async fn read_json_resource(workspace: &Path, name: &str) -> anyhow::Result<serde_json::Value> {
    let path = resource_path(workspace, name);
    let raw = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("failed to read resource {name}"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn resource_path(workspace: &Path, name: &str) -> PathBuf {
    match name {
        "manifest" => workspace.join("manifest.json"),
        "grep_results" => workspace.join("grep_results.json"),
        "metadata_context" => workspace.join("metadata_context.json"),
        "system_context" => workspace.join("system_context.json"),
        "case_context" => workspace.join("case_context.json"),
        value => workspace.join(value),
    }
}

async fn artifact_index(workspace: &Path) -> anyhow::Result<serde_json::Value> {
    let mut artifacts = Vec::new();
    for name in [
        "session_text_input.json",
        "manifest.json",
        "grep_results.json",
        "metadata_context.json",
        "system_context.json",
        "case_context.json",
        "analysis_package.json",
        "claude_mcp_config.json",
        "claude_session.json",
        "agent_response.json",
        "mcp_calls.jsonl",
    ] {
        let path = workspace.join(name);
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            artifacts.push(json!({
                "path": name,
                "bytes": metadata.len(),
            }));
        }
    }
    Ok(json!({
        "schemaVersion": 1,
        "artifacts": artifacts,
    }))
}

fn tools_list_result() -> serde_json::Value {
    json!({
        "tools": [
            {
                "name": "logagent.search_logs",
                "description": "Search task logs with LogAgent grep and persist grep_results.json.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "keywords": { "type": "array", "items": { "type": "string" } },
                        "maxMatches": { "type": "integer", "minimum": 1, "maximum": 200 }
                    },
                    "required": ["keywords"]
                }
            },
            {
                "name": "logagent.get_log_slice",
                "description": "Persist and return a bounded slice from a raw or extracted log file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "startLine": { "type": "integer", "minimum": 1 },
                        "endLine": { "type": "integer", "minimum": 1 }
                    },
                    "required": ["path", "startLine", "endLine"]
                }
            },
            {
                "name": "logagent.run_domain_tool",
                "description": "Run one configured domain tool through the Tool Runner whitelist.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tool": { "type": "string" },
                        "inputFile": { "type": "string" }
                    },
                    "required": ["tool", "inputFile"]
                }
            },
            {
                "name": "logagent.recall_cases",
                "description": "Recall active enabled cases from LogAgent memory.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "logagent.get_metadata_topology",
                "description": "Read the task metadata topology snapshot.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "logagent.get_skill_reference",
                "description": "Read one reference declared by a diagnostic skill selected for this task. Returned refs are background only.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "skillId": { "type": "string" },
                        "referenceId": { "type": "string" },
                        "path": { "type": "string" }
                    },
                    "required": ["skillId"]
                }
            },
            {
                "name": "logagent.request_user_input",
                "description": "Persist a request for user input for this task.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" },
                        "reason": { "type": "string" },
                        "required": { "type": "boolean" },
                        "answerFormat": { "type": "string" }
                    },
                    "required": ["question"]
                }
            },
            {
                "name": "logagent.request_approval",
                "description": "Persist an approval request for an approval-gated action.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "reason": { "type": "string" },
                        "actionType": { "type": "string" },
                        "input": { "type": "object" },
                        "evidenceRefs": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["reason"]
                }
            }
        ]
    })
}

async fn call_tool(
    config: &Arc<AppConfig>,
    skills: &SkillRegistry,
    workspace: &Path,
    task: &TaskRecord,
    _mode: AnalysisMode,
    name: &str,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let result = match name {
        "logagent.search_logs" => {
            search_logs_tool(config.clone(), workspace, task, arguments.clone()).await?
        }
        "logagent.get_log_slice" => get_log_slice_tool(workspace, arguments.clone()).await?,
        "logagent.run_domain_tool" => {
            run_domain_tool(config.clone(), workspace, task, arguments.clone()).await?
        }
        "logagent.recall_cases" => {
            recall_cases_tool(config.clone(), workspace, arguments.clone()).await?
        }
        "logagent.get_metadata_topology" => {
            read_json_resource(workspace, "metadata_context").await?
        }
        "logagent.get_skill_reference" => {
            get_skill_reference_tool(skills, workspace, arguments.clone()).await?
        }
        "logagent.request_user_input" => {
            waiting_marker_tool(workspace, "waiting_for_user", arguments.clone()).await?
        }
        "logagent.request_approval" => {
            waiting_marker_tool(workspace, "waiting_for_approval", arguments.clone()).await?
        }
        other => anyhow::bail!("unknown MCP tool {other}"),
    };
    let evidence_refs = result
        .get("evidenceRefs")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let evidence_ref_count = evidence_refs.len();
    log_mcp_call(
        workspace,
        name,
        arguments,
        "succeeded",
        result.clone(),
        evidence_refs,
    )
    .await?;
    info!(
        task_id = %task.task_id,
        tool = %name,
        evidence_ref_count,
        "MCP tool call completed"
    );
    Ok(json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result)? }],
        "isError": false
    }))
}

async fn search_logs_tool(
    config: Arc<AppConfig>,
    workspace: &Path,
    task: &TaskRecord,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let keywords = string_array_arg(&arguments, "keywords")?;
    let max_matches = arguments
        .get("maxMatches")
        .and_then(|value| value.as_u64())
        .unwrap_or(50)
        .clamp(1, 200) as usize;
    search_task_with_settings(
        config,
        &task.task_id,
        LogAnalyzerSettings {
            keywords,
            max_matches,
        },
    )
    .await?;
    let raw = tokio::fs::read_to_string(workspace.join("grep_results.json")).await?;
    let grep: GrepResults = serde_json::from_str(&raw)?;
    analysis_state::record_log_search(workspace, &grep)?;
    let evidence_refs = (0..grep.matches.len())
        .map(|index| format!("grep_results.json#matches/{index}"))
        .collect::<Vec<_>>();
    Ok(json!({
        "artifactPath": "grep_results.json",
        "totalMatches": grep.total_matches,
        "evidenceRefs": evidence_refs,
    }))
}

async fn get_log_slice_tool(
    workspace: &Path,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let path = required_string(&arguments, "path")?;
    validate_safe_log_path(&path)?;
    let start = arguments
        .get("startLine")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| anyhow::anyhow!("startLine is required"))?
        .max(1) as usize;
    let end = arguments
        .get("endLine")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| anyhow::anyhow!("endLine is required"))?
        .max(1) as usize;
    if end < start || end.saturating_sub(start) > 500 {
        anyhow::bail!("line range must be ordered and contain at most 500 lines");
    }
    let raw = tokio::fs::read_to_string(workspace.join(&path)).await?;
    let lines = raw
        .lines()
        .enumerate()
        .filter_map(|(index, text)| {
            let line = index + 1;
            (line >= start && line <= end).then(|| json!({ "line": line, "text": text }))
        })
        .collect::<Vec<_>>();
    let slice_id = format!("slice_{}", stable_json_hash(&arguments));
    let artifact_path = format!("log_slices/{slice_id}.json");
    tokio::fs::create_dir_all(workspace.join("log_slices")).await?;
    let artifact = json!({
        "schemaVersion": 1,
        "sourcePath": path,
        "startLine": start,
        "endLine": end,
        "lines": lines,
    });
    write_json_atomic(workspace.join(&artifact_path), &artifact).await?;
    Ok(json!({
        "artifactPath": artifact_path,
        "evidenceRefs": [format!("{artifact_path}#lines")],
        "lines": artifact["lines"],
    }))
}

async fn run_domain_tool(
    config: Arc<AppConfig>,
    workspace: &Path,
    task: &TaskRecord,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let tool = required_string(&arguments, "tool")?;
    let input_file = required_string(&arguments, "inputFile")?;
    validate_safe_log_path(&input_file)?;
    let action = AgentAction {
        schema_version: 1,
        action_id: format!("act_mcp_tool_{}", stable_json_hash(&arguments)),
        kind: ActionKind::RunTool,
        reason: "Claude Code MCP requested domain tool".to_string(),
        evidence_refs: Vec::new(),
        input: json!({ "tool": tool, "inputFile": input_file }),
        risk: ActionRisk::SafeReadOnly,
        fingerprint: format!("mcp_tool:{}", stable_json_hash(&arguments)),
    };
    let context = TaskContext::from_record(task, workspace.to_path_buf(), None);
    let runner = ToolRunner::new(config.tools.clone());
    let artifact = runner.execute(&context, &action).await?;
    analysis_state::record_tool_artifact(workspace, &action, &artifact)?;
    Ok(json!({
        "artifactPath": artifact.artifact_path,
        "summary": artifact.summary,
        "evidenceRefs": [artifact.artifact_path],
    }))
}

async fn recall_cases_tool(
    config: Arc<AppConfig>,
    workspace: &Path,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let query = required_string(&arguments, "query")?;
    let limit = arguments
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(5)
        .clamp(1, 20) as usize;
    let cases =
        CaseStore::load_with_memory(config.storage.cases_dir(), config.storage.memory_db_path())?;
    let hits = cases.search(Some(&query), limit, false).await;
    let artifact_path = format!("case_recall/recall_{}.json", stable_json_hash(&arguments));
    tokio::fs::create_dir_all(workspace.join("case_recall")).await?;
    let artifact = json!({
        "schemaVersion": 1,
        "query": query,
        "cases": hits,
    });
    write_json_atomic(workspace.join(&artifact_path), &artifact).await?;
    let evidence_refs = artifact["cases"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .enumerate()
        .map(|(index, _)| format!("{artifact_path}#cases/{index}"))
        .collect::<Vec<_>>();
    Ok(json!({
        "artifactPath": artifact_path,
        "caseCount": artifact["cases"].as_array().map(Vec::len).unwrap_or(0),
        "evidenceRefs": evidence_refs,
    }))
}

async fn get_skill_reference_tool(
    skills: &SkillRegistry,
    workspace: &Path,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let skill_id = required_string(&arguments, "skillId")?;
    let reference_id = optional_string(&arguments, "referenceId");
    let reference_path = optional_string(&arguments, "path");
    let bundle = read_json_resource(workspace, "system_context").await?;
    let bundle: SystemContextBundle = serde_json::from_value(bundle)?;
    let reference = skills
        .read_reference_from_snapshot(
            &bundle,
            &skill_id,
            reference_id.as_deref(),
            reference_path.as_deref(),
        )
        .await?;
    let selected_skill_id = reference.skill_id;
    let selected_revision = reference.skill_revision;
    let reference_summary = reference.reference;
    let content = reference.content;
    let truncated = reference.truncated;
    let stable = stable_json_hash(&json!({
        "skillId": selected_skill_id.clone(),
        "revision": selected_revision.clone(),
        "path": reference_summary.path.clone(),
    }));
    let artifact_path = format!("skill_references/skill_ref_{stable:016x}.json");
    tokio::fs::create_dir_all(workspace.join("skill_references")).await?;
    let background_ref = format!("{artifact_path}#content");
    let artifact = json!({
        "schemaVersion": 1,
        "skillId": selected_skill_id,
        "skillRevision": selected_revision,
        "referenceId": reference_summary.reference_id,
        "path": reference_summary.path,
        "title": reference_summary.title,
        "summary": reference_summary.summary,
        "content": content,
        "truncated": truncated,
        "canonicalRef": background_ref,
        "finalEvidenceAllowed": false,
        "createdAt": Utc::now(),
    });
    write_json_atomic(workspace.join(&artifact_path), &artifact).await?;
    Ok(json!({
        "artifactPath": artifact_path,
        "backgroundRef": background_ref,
        "evidenceRefs": [background_ref],
        "finalEvidenceAllowed": false,
        "title": artifact["title"],
        "summary": artifact["summary"],
        "truncated": artifact["truncated"],
    }))
}

async fn waiting_marker_tool(
    workspace: &Path,
    status: &str,
    arguments: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let artifact = json!({
        "schemaVersion": 1,
        "runtimeStatus": status,
        "request": arguments,
        "createdAt": Utc::now(),
    });
    write_json_atomic(workspace.join("mcp_waiting_request.json"), &artifact).await?;
    Ok(json!({
        "artifactPath": "mcp_waiting_request.json",
        "runtimeStatus": status,
        "evidenceRefs": ["mcp_waiting_request.json#request"],
    }))
}

async fn log_mcp_call(
    workspace: &Path,
    name: &str,
    arguments: serde_json::Value,
    status: &str,
    result: serde_json::Value,
    evidence_refs: Vec<String>,
) -> anyhow::Result<()> {
    let path = workspace.join("mcp_calls.jsonl");
    let record = json!({
        "schemaVersion": 1,
        "callId": next_id("mcpcall"),
        "createdAt": Utc::now(),
        "name": name,
        "arguments": arguments,
        "status": status,
        "result": result,
        "evidenceRefs": evidence_refs,
    });
    let mut line = serde_json::to_vec(&record)?;
    line.push(b'\n');
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(&line).await?;
    file.flush().await?;
    Ok(())
}

fn required_string(arguments: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

fn optional_string(arguments: &serde_json::Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn string_array_arg(arguments: &serde_json::Value, key: &str) -> anyhow::Result<Vec<String>> {
    let values = arguments
        .get(key)
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("{key} must be an array"))?;
    let mut out = Vec::new();
    for value in values {
        let item = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("{key} entries must be non-empty strings"))?;
        out.push(item.to_string());
    }
    if out.is_empty() || out.len() > 10 {
        anyhow::bail!("{key} must contain 1..=10 entries");
    }
    Ok(out)
}

fn validate_safe_log_path(path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    let valid = !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && (path.starts_with("raw") || path.starts_with("extracted"));
    if valid {
        Ok(())
    } else {
        anyhow::bail!("path must be a safe raw/ or extracted/ workspace-relative path")
    }
}

fn stable_json_hash(value: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};

    let encoded = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    encoded.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::models::TaskKind, services::skill_registry::ResolveSkillsInput,
        stores::system_context_store::system_context_bundle, support::config::SkillSettings,
    };

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-mcp-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[tokio::test]
    async fn skill_reference_tool_writes_background_artifact_and_rejects_bad_refs() {
        let root = temp_dir("skills");
        let workspace = temp_dir("workspace");
        let skill_dir = root.join("opengemini-diagnosis");
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: openGemini Diagnosis\ndescription: Diagnose openGemini.\n---\nUse task evidence first.\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("references/topology.md"),
            "Topology reference content.",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("logagent.json"),
            r#"{"schemaVersion":1,"skillId":"opengemini-diagnosis","products":["opengemini"],"taskKinds":["log_analysis"],"includeByDefault":true,"references":[{"path":"references/topology.md","title":"Topology","summary":"Topology rules"}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(&workspace).unwrap();

        let registry = SkillRegistry::load(SkillSettings {
            enabled: true,
            roots: vec![root.clone()],
            max_skill_chars: 4000,
            max_reference_chars: 20_000,
        })
        .unwrap();
        let resources = registry
            .resolve_items(ResolveSkillsInput {
                explicit_skill_ids: &["opengemini-diagnosis".to_string()],
                task_kind: TaskKind::LogAnalysis,
                product: None,
                version: None,
                environment: None,
            })
            .unwrap();
        write_json_atomic(
            workspace.join("system_context.json"),
            &system_context_bundle(resources),
        )
        .await
        .unwrap();

        let result = get_skill_reference_tool(
            &registry,
            &workspace,
            json!({
                "skillId": "opengemini-diagnosis",
                "path": "references/topology.md"
            }),
        )
        .await
        .unwrap();
        assert_eq!(result["finalEvidenceAllowed"], false);
        let artifact_path = result["artifactPath"].as_str().unwrap();
        assert!(artifact_path.starts_with("skill_references/skill_ref_"));
        assert_eq!(result["backgroundRef"], format!("{artifact_path}#content"));
        let artifact: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(workspace.join(artifact_path)).unwrap())
                .unwrap();
        assert_eq!(artifact["content"], "Topology reference content.");
        assert_eq!(artifact["finalEvidenceAllowed"], false);

        let bad_path = get_skill_reference_tool(
            &registry,
            &workspace,
            json!({
                "skillId": "opengemini-diagnosis",
                "path": "../secret.md"
            }),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(bad_path.contains("workspace-relative without traversal"));

        let undeclared = get_skill_reference_tool(
            &registry,
            &workspace,
            json!({
                "skillId": "opengemini-diagnosis",
                "path": "references/missing.md"
            }),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(undeclared.contains("not declared"));

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(workspace);
    }
}
