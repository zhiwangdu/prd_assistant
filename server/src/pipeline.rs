use std::{fs, path::Path, sync::Arc};

use tokio::task;

use crate::llm_gateway::LlmGateway;
use crate::{
    analysis_state,
    config::AppConfig,
    error::AppError,
    fs_utils::relative_string,
    log_analyzer::LogAnalyzer,
    metadata::TaskMetadataContext,
    models::{
        AnalysisResult, Confidence, GrepResults, Manifest, ManifestUpload, ResultOutput, TaskInput,
        TaskRecord, UploadRecord,
    },
    tool_runner::ToolRunRecord,
};

pub async fn prepare_raw_snapshot(
    workspace: &Path,
    uploads: &[UploadRecord],
) -> Result<Vec<TaskInput>, AppError> {
    if uploads.is_empty() {
        return Err(AppError::bad_request("missing uploads"));
    }
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

pub async fn prepare_pipeline_run(workspace: &Path) -> Result<(), AppError> {
    let extracted = workspace.join("extracted");
    match tokio::fs::remove_dir_all(&extracted).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to clean extracted dir: {err}"
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
    task::spawn_blocking(move || {
        let analyzer = LogAnalyzer::new(config.log_analyzer.clone());
        let mut used_extracted_dirs = Vec::new();
        let mut manifest_uploads = Vec::with_capacity(task_record.inputs.len());
        for input in &task_record.inputs {
            let raw_relative = safe_raw_path(&input.raw_path)?;
            let raw_path = workspace.join(raw_relative);
            let extracted_name = unique_dir_name(&input.filename, &mut used_extracted_dirs);
            let upload_extracted_dir = extracted_dir.join(&extracted_name);
            fs::create_dir_all(&upload_extracted_dir)?;
            analyzer.extract_upload(&raw_path, &upload_extracted_dir)?;
            manifest_uploads.push(ManifestUpload {
                upload_id: input.upload_id.clone(),
                filename: input.filename.clone(),
                size: input.size,
                raw_path: input.raw_path.clone(),
                extracted_dir: relative_string(&workspace, &upload_extracted_dir)?,
            });
        }
        let files = analyzer.collect_manifest_files(&extracted_dir)?;
        let first = task_record
            .inputs
            .first()
            .ok_or_else(|| anyhow::anyhow!("task has no inputs"))?;
        let manifest = Manifest {
            upload_id: first.upload_id.clone(),
            upload_ids: task_record.upload_ids.clone(),
            uploads: manifest_uploads,
            task_id: task_record.task_id,
            source: task_record.source,
            filename: first.filename.clone(),
            source_url: task_record.source_url,
            files,
        };
        write_json(&workspace.join("manifest.json"), &manifest)
    })
    .await
    .map_err(|err| AppError::internal(format!("task worker panicked: {err}")))?
    .map_err(|err| AppError::internal(format!("task extraction failed: {err}")))
}

pub async fn search_task(config: Arc<AppConfig>, task_id: &str) -> Result<(), AppError> {
    let workspace = config.storage.workspace_dir(task_id);
    let extracted_dir = workspace.join("extracted");
    let grep_results_path = workspace.join("grep_results.json");
    let grep_path = grep_results_path.clone();
    task::spawn_blocking(move || {
        let analyzer = LogAnalyzer::new(config.log_analyzer.clone());
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
            &tool_results,
        )
        .await
        .map_err(|err| AppError::internal(format!("LLM result generation failed: {err}")))?;
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
    analysis_state::record_final_result(&workspace, &result_json_path, &result_for_state)
        .map_err(|err| AppError::internal(format!("failed to persist analysis state: {err}")))?;
    Ok(ResultOutput {
        result_json_path,
        result_markdown_path,
    })
}

async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("failed to read {}: {err}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse {}: {err}", path.display())))
}

async fn read_tool_results(workspace: &Path) -> Result<Vec<ToolRunRecord>, AppError> {
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
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        config::{
            AuthSettings, LlmProvider, LlmSettings, LogAnalyzerSettings, ServerSettings,
            StorageSettings, ToolsSettings,
        },
        models::{TaskSource, TaskStatus, UploadStatus},
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
            source: TaskSource::Upload,
            upload_ids: inputs.iter().map(|input| input.upload_id.clone()).collect(),
            inputs,
            source_url: Some("batch-test".to_string()),
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
                log_analyzer: LogAnalyzerSettings {
                    keywords: vec!["error".to_string(), "timeout".to_string()],
                    max_matches: 20,
                },
                tools: ToolsSettings::default(),
                llm: LlmSettings {
                    provider: LlmProvider::Stub,
                    base_url: None,
                    api_key: None,
                    model: "stub".to_string(),
                    request_timeout_seconds: 1,
                    max_input_chars: 60_000,
                    max_output_tokens: 100,
                },
            })
        }

        fn write_upload(&self, filename: &str, content: &str) {
            fs::write(self.uploads.join(filename), content).unwrap();
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
}
