use std::{fs, sync::Arc};

use tokio::task;

use crate::{
    config::AppConfig,
    error::AppError,
    fs_utils::relative_string,
    log_analyzer::LogAnalyzer,
    models::{Manifest, ManifestUpload, PipelineOutput, TaskContext, UploadRecord},
};

pub async fn run_upload_pipeline(
    config: Arc<AppConfig>,
    uploads: Vec<UploadRecord>,
    ctx: TaskContext,
) -> Result<PipelineOutput, AppError> {
    if uploads.is_empty() {
        return Err(AppError::bad_request("missing uploads"));
    }

    let raw_dir = ctx.workspace.join("raw");
    let extracted_dir = ctx.workspace.join("extracted");
    tokio::fs::create_dir_all(&raw_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create raw dir: {err}")))?;
    tokio::fs::create_dir_all(&extracted_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create extracted dir: {err}")))?;

    let mut prepared_uploads = Vec::with_capacity(uploads.len());
    let mut used_extracted_dirs = Vec::new();
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

        let extracted_name = unique_dir_name(&upload.filename, &mut used_extracted_dirs);
        let upload_extracted_dir = extracted_dir.join(&extracted_name);
        tokio::fs::create_dir_all(&upload_extracted_dir)
            .await
            .map_err(|err| {
                AppError::internal(format!("failed to create upload extracted dir: {err}"))
            })?;

        prepared_uploads.push(PreparedUpload {
            upload,
            raw_path,
            extracted_dir: upload_extracted_dir,
        });
    }

    let manifest_path = ctx.workspace.join("manifest.json");
    let grep_results_path = ctx.workspace.join("grep_results.json");
    let manifest_path_out = manifest_path.clone();
    let grep_results_path_out = grep_results_path.clone();

    task::spawn_blocking(move || {
        let analyzer = LogAnalyzer::new(config.log_analyzer.clone());
        let mut manifest_uploads = Vec::with_capacity(prepared_uploads.len());
        for prepared in &prepared_uploads {
            analyzer.extract_upload(&prepared.raw_path, &prepared.extracted_dir)?;
            manifest_uploads.push(ManifestUpload {
                upload_id: prepared.upload.upload_id.clone(),
                filename: prepared.upload.filename.clone(),
                size: prepared.upload.size,
                raw_path: relative_string(&ctx.workspace, &prepared.raw_path)?,
                extracted_dir: relative_string(&ctx.workspace, &prepared.extracted_dir)?,
            });
        }
        let files = analyzer.collect_manifest_files(&extracted_dir)?;
        let first = prepared_uploads
            .first()
            .ok_or_else(|| anyhow::anyhow!("missing prepared uploads"))?;
        let manifest = Manifest {
            upload_id: first.upload.upload_id.clone(),
            upload_ids: prepared_uploads
                .iter()
                .map(|prepared| prepared.upload.upload_id.clone())
                .collect(),
            uploads: manifest_uploads,
            task_id: ctx.task_id,
            source: ctx.source,
            filename: first.upload.filename.clone(),
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

struct PreparedUpload {
    upload: UploadRecord,
    raw_path: std::path::PathBuf,
    extracted_dir: std::path::PathBuf,
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

fn write_json<T: serde::Serialize>(path: &std::path::Path, value: &T) -> anyhow::Result<()> {
    let file = fs::File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        config::{AuthSettings, LogAnalyzerSettings, ServerSettings, StorageSettings},
        models::{TaskSource, UploadRecord},
    };

    #[tokio::test]
    async fn batch_uploads_are_extracted_under_package_dirs() {
        let fixture = Fixture::new("batch-pipeline");
        fixture.write_upload("one.log", "INFO one\nERROR first\n");
        fixture.write_upload("two.log", "INFO two\nTIMEOUT second\n");

        let config = Arc::new(AppConfig {
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
            },
            auth: AuthSettings { api_keys: vec![] },
            storage: StorageSettings {
                data_dir: fixture.root.join("data"),
                max_upload_bytes: 1024 * 1024,
                max_chunk_bytes: 512 * 1024,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string(), "timeout".to_string()],
                max_matches: 20,
            },
        });
        let uploads = vec![
            fixture.upload_record("upl_one", "one.log"),
            fixture.upload_record("upl_two", "two.log"),
        ];
        let workspace = fixture.root.join("workspace");
        let ctx = TaskContext {
            task_id: "task_batch".to_string(),
            source: TaskSource::Upload,
            source_url: Some("batch-test".to_string()),
            workspace: workspace.clone(),
        };

        run_upload_pipeline(config, uploads, ctx).await.unwrap();

        assert!(workspace.join("extracted/one/one.log").exists());
        assert!(workspace.join("extracted/two/two.log").exists());

        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(
            manifest["uploadIds"],
            serde_json::json!(["upl_one", "upl_two"])
        );
        assert_eq!(manifest["uploads"].as_array().unwrap().len(), 2);
        assert_eq!(manifest["files"].as_array().unwrap().len(), 2);
        assert_eq!(manifest["files"][0]["path"], "one/one.log");
        assert_eq!(manifest["files"][1]["path"], "two/two.log");

        let grep: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(workspace.join("grep_results.json")).unwrap())
                .unwrap();
        assert_eq!(grep["totalMatches"], 2);
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

        fn write_upload(&self, filename: &str, content: &str) {
            fs::write(self.uploads.join(filename), content).unwrap();
        }

        fn upload_record(&self, upload_id: &str, filename: &str) -> UploadRecord {
            UploadRecord {
                upload_id: upload_id.to_string(),
                filename: filename.to_string(),
                size: fs::metadata(self.uploads.join(filename)).unwrap().len(),
                path: self.uploads.join(filename),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
