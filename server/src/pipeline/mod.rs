pub mod executor;

use std::{fs, path::Path, sync::Arc};

use tokio::task;

use crate::services::llm_gateway::LlmGateway;
use crate::{
    domain::models::{
        AnalysisResult, Confidence, GrepResults, LogGroupSummary, Manifest, ManifestFile,
        ManifestUpload, ResultOutput, SystemContextBundle, TaskInput, TaskRecord, ToolInputEntry,
        ToolInputIndex, UploadRecord,
    },
    services::{
        log_analyzer::{parse_log_package_filename, LogAnalyzer},
        metadata::TaskMetadataContext,
        tool_runner::ToolRunRecord,
    },
    stores::{analysis_state, case_store::CaseSearchHit},
    support::{
        config::{AppConfig, LogAnalyzerSettings},
        error::AppError,
        fs_utils::relative_string,
    },
};

pub async fn prepare_raw_snapshot(
    workspace: &Path,
    uploads: &[UploadRecord],
) -> Result<Vec<TaskInput>, AppError> {
    let raw_dir = workspace.join("raw");
    tokio::fs::create_dir_all(&raw_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create raw dir: {err}")))?;

    let mut inputs = Vec::with_capacity(uploads.len());
    for upload in uploads {
        let raw_upload_dir = raw_dir.join(&upload.upload_id);
        tokio::fs::create_dir_all(&raw_upload_dir)
            .await
            .map_err(|err| AppError::internal(format!("failed to create raw upload dir: {err}")))?;
        let raw_path = raw_upload_dir.join(&upload.filename);
        tokio::fs::copy(&upload.path, &raw_path)
            .await
            .map_err(|err| {
                AppError::internal(format!("failed to copy upload to workspace: {err}"))
            })?;
        inputs.push(TaskInput {
            upload_id: upload.upload_id.clone(),
            filename: upload.filename.clone(),
            size: upload.size,
            raw_path: relative_string(workspace, &raw_path)
                .map_err(|err| AppError::internal(err.to_string()))?,
        });
    }
    Ok(inputs)
}

pub async fn write_metadata_context(
    workspace: &Path,
    context: &TaskMetadataContext,
) -> Result<std::path::PathBuf, AppError> {
    tokio::fs::create_dir_all(workspace)
        .await
        .map_err(|err| AppError::internal(format!("failed to create workspace: {err}")))?;
    let path = workspace.join("metadata_context.json");
    let temp = workspace.join(".metadata_context.json.tmp");
    let encoded = serde_json::to_vec_pretty(context)
        .map_err(|err| AppError::internal(format!("failed to encode metadata context: {err}")))?;
    tokio::fs::write(&temp, encoded)
        .await
        .map_err(|err| AppError::internal(format!("failed to write metadata context: {err}")))?;
    tokio::fs::rename(&temp, &path)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist metadata context: {err}")))?;
    Ok(path)
}

pub async fn write_case_context(
    workspace: &Path,
    query: &str,
    cases: &[CaseSearchHit],
) -> Result<std::path::PathBuf, AppError> {
    tokio::fs::create_dir_all(workspace)
        .await
        .map_err(|err| AppError::internal(format!("failed to create workspace: {err}")))?;
    let path = workspace.join("case_context.json");
    let temp = workspace.join(".case_context.json.tmp");
    let context = serde_json::json!({
        "schemaVersion": 1,
        "query": query,
        "cases": cases,
    });
    let encoded = serde_json::to_vec_pretty(&context)
        .map_err(|err| AppError::internal(format!("failed to encode case context: {err}")))?;
    tokio::fs::write(&temp, encoded)
        .await
        .map_err(|err| AppError::internal(format!("failed to write case context: {err}")))?;
    tokio::fs::rename(&temp, &path)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist case context: {err}")))?;
    Ok(path)
}

pub async fn write_system_context(
    workspace: &Path,
    context: &SystemContextBundle,
) -> Result<std::path::PathBuf, AppError> {
    tokio::fs::create_dir_all(workspace)
        .await
        .map_err(|err| AppError::internal(format!("failed to create workspace: {err}")))?;
    let path = workspace.join("system_context.json");
    let temp = workspace.join(".system_context.json.tmp");
    let encoded = serde_json::to_vec_pretty(context)
        .map_err(|err| AppError::internal(format!("failed to encode system context: {err}")))?;
    tokio::fs::write(&temp, encoded)
        .await
        .map_err(|err| AppError::internal(format!("failed to write system context: {err}")))?;
    tokio::fs::rename(&temp, &path)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist system context: {err}")))?;
    Ok(path)
}

pub async fn write_session_text_input(
    workspace: &Path,
    question: &str,
) -> Result<std::path::PathBuf, AppError> {
    tokio::fs::create_dir_all(workspace)
        .await
        .map_err(|err| AppError::internal(format!("failed to create workspace: {err}")))?;
    let path = workspace.join("session_text_input.json");
    let temp = workspace.join(".session_text_input.json.tmp");
    let context = serde_json::json!({
        "schemaVersion": 1,
        "question": question,
    });
    let encoded = serde_json::to_vec_pretty(&context)
        .map_err(|err| AppError::internal(format!("failed to encode session text input: {err}")))?;
    tokio::fs::write(&temp, encoded)
        .await
        .map_err(|err| AppError::internal(format!("failed to write session text input: {err}")))?;
    tokio::fs::rename(&temp, &path).await.map_err(|err| {
        AppError::internal(format!("failed to persist session text input: {err}"))
    })?;
    Ok(path)
}

pub async fn prepare_pipeline_run(workspace: &Path) -> Result<(), AppError> {
    let extracted = workspace.join("extracted");
    let tool_inputs = workspace.join("tool_inputs");
    match tokio::fs::remove_dir_all(&extracted).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to clean extracted dir: {err}"
            )))
        }
    }
    match tokio::fs::remove_dir_all(&tool_inputs).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to clean tool_inputs dir: {err}"
            )))
        }
    }
    for name in [
        "manifest.json",
        "grep_results.json",
        "result.json",
        "result.md",
    ] {
        match tokio::fs::remove_file(workspace.join(name)).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(AppError::internal(format!(
                    "failed to remove old artifact {name}: {err}"
                )))
            }
        }
    }
    tokio::fs::create_dir_all(&extracted)
        .await
        .map_err(|err| AppError::internal(format!("failed to create extracted dir: {err}")))
}

pub async fn extract_task(config: Arc<AppConfig>, task_record: TaskRecord) -> Result<(), AppError> {
    let workspace = config.storage.workspace_dir(&task_record.task_id);
    let extracted_dir = workspace.join("extracted");
    let tool_inputs_dir = workspace.join("tool_inputs");
    task::spawn_blocking(move || {
        let analyzer = LogAnalyzer::new(config.log_analyzer.clone());
        let mut used_extracted_dirs = Vec::new();
        let mut manifest_uploads = Vec::with_capacity(task_record.inputs.len());
        let mut preprocessed_files = std::collections::BTreeMap::<String, ManifestFileMeta>::new();
        let mut tool_inputs = Vec::<ToolInputEntry>::new();
        for input in &task_record.inputs {
            let raw_relative = safe_raw_path(&input.raw_path)?;
            let raw_path = workspace.join(raw_relative);
            let package = parse_log_package_filename(&input.filename);
            let upload_extracted_dir = match &package {
                Some(package) => extracted_dir
                    .join(&package.node_id)
                    .join(&package.package_timestamp),
                None => {
                    let extracted_name = unique_dir_name(&input.filename, &mut used_extracted_dirs);
                    extracted_dir.join(extracted_name)
                }
            };
            fs::create_dir_all(&upload_extracted_dir)?;
            let extraction = analyzer.extract_upload(
                &raw_path,
                &upload_extracted_dir,
                Some(&tool_inputs_dir),
            )?;
            let extracted_dir_relative = relative_string(&workspace, &upload_extracted_dir)?;
            let mut package_id = None;
            let mut instance_id = None;
            let mut node_id = None;
            let mut package_timestamp = None;
            let mut node_dir = None;
            let mut log_groups = Vec::<LogGroupSummary>::new();
            let mut ignored_file_count = 0;
            let mut ignored_path_samples = Vec::new();
            let mut warnings = Vec::new();
            if let Some(preprocessed) = extraction.preprocessed {
                package_id = Some(preprocessed.package_id.clone());
                instance_id = Some(preprocessed.instance_id.clone());
                node_id = Some(preprocessed.node_id.clone());
                package_timestamp = Some(preprocessed.package_timestamp.clone());
                node_dir = Some(preprocessed.node_dir.clone());
                log_groups = preprocessed.log_groups.clone();
                ignored_file_count = preprocessed.ignored_file_count;
                ignored_path_samples = preprocessed.ignored_path_samples.clone();
                warnings = preprocessed.warnings.clone();
                tool_inputs.extend(preprocessed.tool_inputs.clone());
                let upload_prefix = relative_string(&extracted_dir, &upload_extracted_dir)?;
                for file in preprocessed.files {
                    let manifest_path = if upload_prefix.is_empty() {
                        file.output_relative_path.clone()
                    } else {
                        format!("{upload_prefix}/{}", file.output_relative_path)
                    };
                    preprocessed_files.insert(
                        manifest_path,
                        ManifestFileMeta {
                            upload_id: input.upload_id.clone(),
                            instance_id: preprocessed.instance_id.clone(),
                            node_id: preprocessed.node_id.clone(),
                            package_timestamp: preprocessed.package_timestamp.clone(),
                            log_group: file.log_group,
                            original_path: file.original_path,
                            compressed: file.compressed,
                            compression: file.compression,
                        },
                    );
                }
            }
            manifest_uploads.push(ManifestUpload {
                upload_id: input.upload_id.clone(),
                filename: input.filename.clone(),
                size: input.size,
                raw_path: input.raw_path.clone(),
                extracted_dir: extracted_dir_relative,
                package_id,
                instance_id,
                node_id,
                package_timestamp,
                node_dir,
                log_groups,
                ignored_file_count,
                ignored_path_samples,
                warnings,
            });
        }
        let mut files = analyzer.collect_manifest_files(&extracted_dir)?;
        enrich_manifest_files(&mut files, &preprocessed_files);
        let tool_inputs_path = if tool_inputs.is_empty() {
            None
        } else {
            fs::create_dir_all(&tool_inputs_dir)?;
            let index = ToolInputIndex {
                schema_version: 1,
                generated_by: "log_package_preprocessor".to_string(),
                inputs: tool_inputs,
            };
            let index_path = tool_inputs_dir.join("index.json");
            write_json(&index_path, &index)?;
            Some(relative_string(&workspace, &index_path)?)
        };
        let (upload_id, filename) = task_record
            .inputs
            .first()
            .map(|input| (input.upload_id.clone(), input.filename.clone()))
            .unwrap_or_else(|| ("".to_string(), "session_text_input".to_string()));
        let manifest = Manifest {
            upload_id,
            upload_ids: task_record.upload_ids.clone(),
            uploads: manifest_uploads,
            task_id: task_record.task_id,
            source: task_record.source,
            filename,
            source_url: task_record.source_url,
            tool_inputs_path,
            files,
        };
        write_json(&workspace.join("manifest.json"), &manifest)
    })
    .await
    .map_err(|err| AppError::internal(format!("task worker panicked: {err}")))?
    .map_err(|err| AppError::internal(format!("task extraction failed: {err}")))
}

#[derive(Debug, Clone)]
struct ManifestFileMeta {
    upload_id: String,
    instance_id: String,
    node_id: String,
    package_timestamp: String,
    log_group: String,
    original_path: String,
    compressed: bool,
    compression: Option<String>,
}

fn enrich_manifest_files(
    files: &mut [ManifestFile],
    preprocessed_files: &std::collections::BTreeMap<String, ManifestFileMeta>,
) {
    for file in files {
        if let Some(meta) = preprocessed_files.get(&file.path) {
            file.upload_id = Some(meta.upload_id.clone());
            file.instance_id = Some(meta.instance_id.clone());
            file.node_id = Some(meta.node_id.clone());
            file.package_timestamp = Some(meta.package_timestamp.clone());
            file.log_group = Some(meta.log_group.clone());
            file.original_path = Some(meta.original_path.clone());
            file.compressed = Some(meta.compressed);
            file.compression = meta.compression.clone();
        }
    }
}

pub async fn search_task(config: Arc<AppConfig>, task_id: &str) -> Result<(), AppError> {
    search_task_with_settings(config.clone(), task_id, config.log_analyzer.clone()).await
}

pub async fn search_task_with_settings(
    config: Arc<AppConfig>,
    task_id: &str,
    settings: LogAnalyzerSettings,
) -> Result<(), AppError> {
    let workspace = config.storage.workspace_dir(task_id);
    let extracted_dir = workspace.join("extracted");
    let grep_results_path = workspace.join("grep_results.json");
    let grep_path = grep_results_path.clone();
    task::spawn_blocking(move || {
        let analyzer = LogAnalyzer::new(settings);
        let grep: GrepResults = analyzer.run_simple_grep(&extracted_dir)?;
        write_json(&grep_path, &grep)
    })
    .await
    .map_err(|err| AppError::internal(format!("task worker panicked: {err}")))?
    .map_err(|err| AppError::internal(format!("task search failed: {err}")))?;
    Ok(())
}

pub async fn generate_task_result(
    config: Arc<AppConfig>,
    gateway: LlmGateway,
    task: TaskRecord,
) -> Result<ResultOutput, AppError> {
    let workspace = config.storage.workspace_dir(&task.task_id);
    let manifest: Manifest = read_json(&workspace.join("manifest.json")).await?;
    let grep: GrepResults = read_json(&workspace.join("grep_results.json")).await?;
    let tool_results = read_tool_results(&workspace).await?;
    let case_context = read_optional_json(&workspace.join("case_context.json")).await?;
    let system_context = match task.system_context_path.as_deref() {
        Some(path) => {
            let expected = workspace.join("system_context.json");
            if Path::new(path) != expected {
                return Err(AppError::internal(
                    "task contains invalid systemContextPath",
                ));
            }
            Some(read_json(&expected).await?)
        }
        None => read_optional_json(&workspace.join("system_context.json")).await?,
    };
    let metadata_context = match task.metadata_context_path.as_deref() {
        Some(path) => {
            let expected = workspace.join("metadata_context.json");
            if Path::new(path) != expected {
                return Err(AppError::internal(
                    "task contains invalid metadataContextPath",
                ));
            }
            Some(read_json(&expected).await?)
        }
        None => None,
    };
    let result = gateway
        .generate_result(
            &task.question,
            &manifest,
            &grep,
            metadata_context.as_ref(),
            system_context.as_ref(),
            case_context.as_ref(),
            &tool_results,
        )
        .await
        .map_err(|err| AppError::internal(format!("LLM result generation failed: {err}")))?;
    persist_task_result(&workspace, result).await
}

async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("failed to read {}: {err}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse {}: {err}", path.display())))
}

pub async fn read_optional_json<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, AppError> {
    match tokio::fs::read_to_string(path).await {
        Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|err| {
            AppError::internal(format!("failed to parse {}: {err}", path.display()))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AppError::internal(format!(
            "failed to read {}: {err}",
            path.display()
        ))),
    }
}

pub async fn persist_task_result(
    workspace: &Path,
    result: AnalysisResult,
) -> Result<ResultOutput, AppError> {
    persist_task_result_inner(workspace, result, true).await
}

pub async fn persist_final_answer_decision_result(
    workspace: &Path,
    result: AnalysisResult,
) -> Result<ResultOutput, AppError> {
    persist_task_result_inner(workspace, result, false).await
}

async fn persist_task_result_inner(
    workspace: &Path,
    result: AnalysisResult,
    increment_llm_calls: bool,
) -> Result<ResultOutput, AppError> {
    let result_json_path = workspace.join("result.json");
    let result_markdown_path = workspace.join("result.md");
    let json_path = result_json_path.clone();
    let markdown_path = result_markdown_path.clone();
    let result_for_state = result.clone();
    task::spawn_blocking(move || {
        write_json(&json_path, &result)?;
        fs::write(&markdown_path, render_result_markdown(&result))?;
        anyhow::Ok(())
    })
    .await
    .map_err(|err| AppError::internal(format!("result writer panicked: {err}")))?
    .map_err(|err| AppError::internal(format!("failed to persist LLM result: {err}")))?;
    if increment_llm_calls {
        analysis_state::record_final_result(workspace, &result_json_path, &result_for_state)
            .map_err(|err| {
                AppError::internal(format!("failed to persist analysis state: {err}"))
            })?;
    } else {
        analysis_state::record_final_answer_decision_result(
            workspace,
            &result_json_path,
            &result_for_state,
        )
        .map_err(|err| AppError::internal(format!("failed to persist analysis state: {err}")))?;
    }
    Ok(ResultOutput {
        result_json_path,
        result_markdown_path,
    })
}

pub async fn read_tool_results(workspace: &Path) -> Result<Vec<ToolRunRecord>, AppError> {
    let root = workspace.join("tool_results");
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to read tool results: {err}"
            )))
        }
    };
    let mut paths = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| AppError::internal(format!("failed to list tool results: {err}")))?
    {
        let path = entry.path().join("result.json");
        if path.exists() {
            paths.push(path);
        }
    }
    paths.sort();
    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        results.push(read_json(&path).await?);
    }
    Ok(results)
}

fn render_result_markdown(result: &AnalysisResult) -> String {
    let confidence = match result.confidence {
        Confidence::Low => "low",
        Confidence::Medium => "medium",
        Confidence::High => "high",
    };
    let mut output = format!(
        "# Log Analysis Result\n\n## Summary\n\n{}\n\n**Confidence:** `{confidence}`\n",
        result.summary
    );
    append_list(&mut output, "Symptoms", &result.symptoms);
    output.push_str("\n## Likely Root Causes\n");
    if result.likely_root_causes.is_empty() {
        output.push_str("\n- None identified from current evidence.\n");
    } else {
        for cause in &result.likely_root_causes {
            output.push_str(&format!("\n- {}\n", cause.cause));
            for evidence_ref in &cause.evidence_refs {
                output.push_str(&format!("  - Evidence: `{evidence_ref}`\n"));
            }
        }
    }
    append_list(&mut output, "Next Checks", &result.next_checks);
    append_list(&mut output, "Fix Suggestions", &result.fix_suggestions);
    append_list(
        &mut output,
        "Missing Information",
        &result.missing_information,
    );
    output
}

fn append_list(output: &mut String, title: &str, items: &[String]) {
    output.push_str(&format!("\n## {title}\n"));
    if items.is_empty() {
        output.push_str("\n- None.\n");
    } else {
        for item in items {
            output.push_str(&format!("\n- {item}"));
        }
        output.push('\n');
    }
}

fn safe_raw_path(raw_path: &str) -> anyhow::Result<&Path> {
    let path = Path::new(raw_path);
    let safe = !path.is_absolute()
        && path.starts_with("raw")
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if !safe {
        anyhow::bail!("invalid task rawPath {raw_path}");
    }
    Ok(path)
}

fn unique_dir_name(filename: &str, used: &mut Vec<String>) -> String {
    let base = upload_dir_name(filename);
    let mut candidate = base.clone();
    let mut index = 2;
    while used.iter().any(|value| value == &candidate) {
        candidate = format!("{base}_{index}");
        index += 1;
    }
    used.push(candidate.clone());
    candidate
}

fn upload_dir_name(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    let suffixes = [".tar.gz", ".tgz", ".zip", ".tar", ".log", ".txt"];
    let without_suffix = suffixes
        .iter()
        .find_map(|suffix| {
            lower
                .ends_with(suffix)
                .then(|| &filename[..filename.len() - suffix.len()])
        })
        .unwrap_or(filename);
    let safe = without_suffix
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string();
    if safe.is_empty() {
        "upload".to_string()
    } else {
        safe
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let file = fs::File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use flate2::{write::GzEncoder, Compression};

    use std::{
        io::Write,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        domain::models::{TaskSource, TaskStatus, UploadStatus},
        support::config::{
            AnalysisSettings, AuthSettings, EmbeddingSettings, LlmProvider, LlmSettings,
            LogAnalyzerSettings, ServerSettings, StorageSettings, ToolsSettings,
        },
    };

    #[tokio::test]
    async fn rerun_replaces_derived_artifacts_from_raw_snapshot() {
        let fixture = Fixture::new("pipeline-rerun");
        fixture.write_upload("one.log", "INFO one\nERROR first\n");
        fixture.write_upload("two.log", "INFO two\nTIMEOUT second\n");
        let config = fixture.config();
        let workspace = config.storage.workspace_dir("task_batch");
        let uploads = vec![
            fixture.upload_record("upl_one", "one.log"),
            fixture.upload_record("upl_two", "two.log"),
        ];
        let inputs = prepare_raw_snapshot(&workspace, &uploads).await.unwrap();
        let record = task_record(inputs);

        prepare_pipeline_run(&workspace).await.unwrap();
        extract_task(config.clone(), record.clone()).await.unwrap();
        search_task(config.clone(), "task_batch").await.unwrap();
        fs::write(workspace.join("metadata_context.json"), "{}").unwrap();
        fs::write(workspace.join("extracted/stale.log"), "stale").unwrap();
        fs::write(workspace.join("result.json"), "{}").unwrap();
        fs::write(workspace.join("result.md"), "stale").unwrap();

        prepare_pipeline_run(&workspace).await.unwrap();
        extract_task(config.clone(), record).await.unwrap();
        search_task(config, "task_batch").await.unwrap();

        assert!(!workspace.join("extracted/stale.log").exists());
        assert!(workspace.join("metadata_context.json").exists());
        assert!(!workspace.join("result.json").exists());
        assert!(!workspace.join("result.md").exists());
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["files"][0]["path"], "one/one.log");
        assert_eq!(manifest["files"][1]["path"], "two/two.log");
        let grep: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("grep_results.json")).unwrap())
                .unwrap();
        assert_eq!(grep["totalMatches"], 2);
    }

    #[tokio::test]
    async fn extract_task_preprocesses_node_log_package_and_tool_inputs() {
        let fixture = Fixture::new("pipeline-log-package");
        let filename = "pkg123_instance123_node123_2026_06_16_09_58_02_561564_logs.tar.gz";
        fixture.write_node_log_package(filename);
        let config = fixture.config();
        let workspace = config.storage.workspace_dir("task_batch");
        let uploads = vec![fixture.upload_record("upl_pkg", filename)];
        let inputs = prepare_raw_snapshot(&workspace, &uploads).await.unwrap();
        let record = task_record(inputs);

        prepare_pipeline_run(&workspace).await.unwrap();
        extract_task(config.clone(), record).await.unwrap();
        search_task(config, "task_batch").await.unwrap();

        assert!(workspace
            .join("extracted/node123/2026_06_16_09_58_02_561564/tsdb/influxdb.log")
            .exists());
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["toolInputsPath"], "tool_inputs/index.json");
        assert_eq!(manifest["uploads"][0]["instanceId"], "instance123");
        assert_eq!(manifest["uploads"][0]["nodeId"], "node123");
        assert_eq!(
            manifest["files"][0]["path"],
            "node123/2026_06_16_09_58_02_561564/agent/agent.log"
        );
        assert!(manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["compressed"] == true
                && file["path"] == "node123/2026_06_16_09_58_02_561564/tsdb/influxdb-rotated"));

        let index: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join("tool_inputs/index.json")).unwrap(),
        )
        .unwrap();
        assert!(index["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input["path"]
                == "tool_inputs/influxql_analyzer/node123/2026_06_16_09_58_02_561564.jsonl"
                && input["recordCount"] == 1));

        let grep: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("grep_results.json")).unwrap())
                .unwrap();
        assert!(grep["matches"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["text"].as_str().unwrap().contains("ERROR rotated")));
    }

    #[test]
    fn rejects_unsafe_persisted_raw_paths() {
        assert!(safe_raw_path("../outside.log").is_err());
        assert!(safe_raw_path("/tmp/outside.log").is_err());
        assert!(safe_raw_path("raw/upl_one/one.log").is_ok());
    }

    fn task_record(inputs: Vec<TaskInput>) -> TaskRecord {
        let now = Utc::now();
        TaskRecord {
            schema_version: 1,
            task_id: "task_batch".to_string(),
            alias: None,
            session_id: Some("sess_test".to_string()),
            task_kind: crate::domain::models::TaskKind::LogAnalysis,
            analysis_mode: crate::support::config::AnalysisMode::Diagnose,
            analysis_language: crate::domain::models::AnalysisLanguage::ZhCn,
            source: TaskSource::Upload,
            upload_ids: inputs.iter().map(|input| input.upload_id.clone()).collect(),
            inputs,
            source_url: Some("batch-test".to_string()),
            tool_id: None,
            tool_params: serde_json::Value::Null,
            tool_result_path: None,
            remote_executor_id: None,
            remote_command_id: None,
            remote_command_params: serde_json::Value::Null,
            remote_result_path: None,
            instance_id: None,
            cluster_id: None,
            node_id: None,
            question: "analyze".to_string(),
            status: TaskStatus::Running,
            phase: None,
            attempts: 1,
            error: None,
            manifest_path: None,
            grep_results_path: None,
            metadata_context_path: None,
            system_context_path: None,
            result_json_path: None,
            result_markdown_path: None,
            created_at: now,
            updated_at: now,
        }
    }

    struct Fixture {
        root: PathBuf,
        uploads: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("logagent-{name}-{now}"));
            let uploads = root.join("uploads");
            fs::create_dir_all(&uploads).unwrap();
            Self { root, uploads }
        }

        fn config(&self) -> Arc<AppConfig> {
            Arc::new(AppConfig {
                config_path: self.root.join("logagent-test.yaml"),
                server: ServerSettings {
                    bind: "127.0.0.1:0".to_string(),
                    public_base_url: "http://127.0.0.1:0".to_string(),
                    max_concurrent_tasks: 2,
                },
                auth: AuthSettings { api_keys: vec![] },
                storage: StorageSettings {
                    data_dir: self.root.join("data"),
                    max_upload_bytes: 1024 * 1024,
                    max_chunk_bytes: 512 * 1024,
                },
                skills: crate::support::config::SkillSettings {
                    enabled: false,
                    roots: Vec::new(),
                    max_skill_chars: 4000,
                    max_reference_chars: 20_000,
                },
                log_analyzer: LogAnalyzerSettings {
                    keywords: vec!["error".to_string(), "timeout".to_string()],
                    max_matches: 20,
                },
                tools: ToolsSettings::default(),
                fetch: crate::support::config::FetchSettings::default(),
                huawei_cloud: crate::support::config::HuaweiCloudSettings::default(),
                remote_execution: crate::support::config::RemoteExecutionSettings::default(),
                llm: LlmSettings {
                    provider: LlmProvider::Stub,
                    base_url: None,
                    api_key: None,
                    binary_path: None,
                    binary_max_output_bytes: 1024 * 1024,
                    model: "stub".to_string(),
                    request_timeout_seconds: 1,
                    max_input_chars: 60_000,
                    max_output_tokens: 100,
                },
                claude_code: crate::support::config::ClaudeCodeSettings::default(),
                mcp: crate::support::config::McpSettings::default(),
                analysis: test_analysis_settings(),
                embedding: test_embedding_settings(),
            })
        }

        fn write_upload(&self, filename: &str, content: &str) {
            fs::write(self.uploads.join(filename), content).unwrap();
        }

        fn write_node_log_package(&self, filename: &str) {
            let source = self.root.join("source");
            fs::create_dir_all(source.join("var/chroot/gemini/log/tsdb")).unwrap();
            fs::create_dir_all(source.join("home/Ruby/log")).unwrap();
            fs::write(
                source.join("var/chroot/gemini/log/tsdb/influxdb.log"),
                r#"{"query":"select * from cpu"}"#,
            )
            .unwrap();
            let rotated =
                fs::File::create(source.join("var/chroot/gemini/log/tsdb/influxdb-rotated"))
                    .unwrap();
            let mut encoder = GzEncoder::new(rotated, Compression::default());
            encoder.write_all(b"INFO old\nERROR rotated\n").unwrap();
            encoder.finish().unwrap();
            fs::write(source.join("home/Ruby/log/agent.log"), "agent ok\n").unwrap();

            let file = fs::File::create(self.uploads.join(filename)).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);
            builder.append_dir_all("var", source.join("var")).unwrap();
            builder.append_dir_all("home", source.join("home")).unwrap();
            builder.finish().unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }

        fn upload_record(&self, upload_id: &str, filename: &str) -> UploadRecord {
            let size = fs::metadata(self.uploads.join(filename)).unwrap().len();
            let now = Utc::now();
            UploadRecord {
                schema_version: 1,
                upload_id: upload_id.to_string(),
                filename: filename.to_string(),
                size,
                expected_size: Some(size),
                status: UploadStatus::Complete,
                path: self.uploads.join(filename),
                created_at: now,
                updated_at: now,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn test_analysis_settings() -> AnalysisSettings {
        AnalysisSettings {
            max_rounds: 4,
            max_llm_calls: 4,
            max_actions: 6,
            max_repeated_action_fingerprints: 1,
        }
    }

    fn test_embedding_settings() -> EmbeddingSettings {
        EmbeddingSettings {
            enabled: false,
            provider: "openai_compatible".to_string(),
            model: "text-embedding-3-small".to_string(),
            api_key_env: None,
            store: "sqlite".to_string(),
        }
    }
}
