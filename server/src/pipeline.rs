use std::{fs, sync::Arc};

use tokio::task;

use crate::{
    config::AppConfig,
    error::AppError,
    log_analyzer::LogAnalyzer,
    models::{Manifest, PipelineOutput, TaskContext, UploadRecord},
};

pub async fn run_upload_pipeline(
    config: Arc<AppConfig>,
    upload: UploadRecord,
    ctx: TaskContext,
) -> Result<PipelineOutput, AppError> {
    let raw_dir = ctx.workspace.join("raw");
    let extracted_dir = ctx.workspace.join("extracted");
    tokio::fs::create_dir_all(&raw_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create raw dir: {err}")))?;
    tokio::fs::create_dir_all(&extracted_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create extracted dir: {err}")))?;

    let raw_path = raw_dir.join(&upload.filename);
    tokio::fs::copy(&upload.path, &raw_path)
        .await
        .map_err(|err| AppError::internal(format!("failed to copy upload to workspace: {err}")))?;

    let manifest_path = ctx.workspace.join("manifest.json");
    let grep_results_path = ctx.workspace.join("grep_results.json");
    let manifest_path_out = manifest_path.clone();
    let grep_results_path_out = grep_results_path.clone();

    task::spawn_blocking(move || {
        let analyzer = LogAnalyzer::new(config.log_analyzer.clone());
        analyzer.extract_upload(&raw_path, &extracted_dir)?;
        let files = analyzer.collect_manifest_files(&extracted_dir)?;
        let manifest = Manifest {
            upload_id: upload.upload_id,
            task_id: ctx.task_id,
            source: ctx.source,
            filename: upload.filename,
            source_url: ctx.source_url,
            files,
        };
        write_json(&manifest_path, &manifest)?;
        let grep = analyzer.run_simple_grep(&extracted_dir)?;
        write_json(&grep_results_path, &grep)?;
        anyhow::Ok(())
    })
    .await
    .map_err(|err| AppError::internal(format!("task worker panicked: {err}")))?
    .map_err(|err| AppError::internal(format!("task processing failed: {err}")))?;

    Ok(PipelineOutput {
        manifest_path: manifest_path_out,
        grep_results_path: grep_results_path_out,
    })
}

fn write_json<T: serde::Serialize>(path: &std::path::Path, value: &T) -> anyhow::Result<()> {
    let file = fs::File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}
