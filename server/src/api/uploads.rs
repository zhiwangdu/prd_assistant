use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::{
    error::AppError,
    fs_utils::sanitize_filename,
    id::next_id,
    models::{
        BatchUploadResponse, ChunkQuery, ChunkUploadResponse, InitUploadRequest, UploadRecord,
        UploadResponse, UploadStatus,
    },
    state::AppState,
};

pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let upload_id = next_id("upl");
    let upload_dir = state.config.storage.upload_dir(&upload_id);
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload dir: {err}")))?;

    let mut filename: Option<String> = None;
    let mut file_path = None;
    let mut size = 0_u64;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad_request(format!("invalid multipart request: {err}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "filename" {
            let value = field
                .text()
                .await
                .map_err(|err| AppError::bad_request(format!("invalid filename field: {err}")))?;
            filename = Some(sanitize_filename(&value)?);
            continue;
        }

        if field_name != "file" {
            continue;
        }

        let fallback_name = field.file_name().unwrap_or("upload.bin").to_string();
        let safe_name = sanitize_filename(filename.as_deref().unwrap_or(&fallback_name))?;
        let path = upload_dir.join(&safe_name);
        let mut out = tokio::fs::File::create(&path)
            .await
            .map_err(|err| AppError::internal(format!("failed to create upload file: {err}")))?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|err| AppError::bad_request(format!("failed to read upload field: {err}")))?
        {
            size += chunk.len() as u64;
            if size > state.config.storage.max_upload_bytes {
                return Err(AppError::bad_request(format!(
                    "upload size {size} exceeds max_upload_bytes {}",
                    state.config.storage.max_upload_bytes
                )));
            }
            out.write_all(&chunk)
                .await
                .map_err(|err| AppError::internal(format!("failed to write upload file: {err}")))?;
        }
        out.flush()
            .await
            .map_err(|err| AppError::internal(format!("failed to flush upload file: {err}")))?;
        drop(out);
        filename = Some(safe_name);
        file_path = Some(path);
    }

    let path = file_path.ok_or_else(|| AppError::bad_request("missing file field"))?;
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::internal("upload path is missing filename"))?
        .to_string();
    let now = Utc::now();
    let record = UploadRecord {
        schema_version: 1,
        upload_id: upload_id.clone(),
        filename: filename.clone(),
        size,
        expected_size: Some(size),
        status: UploadStatus::Complete,
        path,
        created_at: now,
        updated_at: now,
    };
    state
        .uploads
        .create(record)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist upload: {err}")))?;

    Ok(Json(UploadResponse {
        upload_id,
        filename,
        size,
    }))
}

pub async fn batch_upload(
    State(state): State<Arc<AppState>>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<BatchUploadResponse>, AppError> {
    let mut uploads = Vec::new();
    let mut total_size = 0_u64;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad_request(format!("invalid multipart request: {err}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name != "file" && field_name != "files" {
            continue;
        }

        let upload = receive_upload_field(state.clone(), field).await?;
        total_size += upload.size;
        uploads.push(upload);
    }

    if uploads.is_empty() {
        return Err(AppError::bad_request("missing file fields"));
    }

    Ok(Json(BatchUploadResponse {
        uploads,
        total_size,
    }))
}

pub async fn init_upload(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InitUploadRequest>,
) -> Result<Json<UploadResponse>, AppError> {
    if req.size > state.config.storage.max_upload_bytes {
        return Err(AppError::bad_request(format!(
            "upload size {} exceeds max_upload_bytes {}",
            req.size, state.config.storage.max_upload_bytes
        )));
    }

    let upload_id = next_id("upl");
    let filename = sanitize_filename(&req.filename)?;
    let upload_dir = state.config.storage.upload_dir(&upload_id);
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload dir: {err}")))?;
    let path = upload_dir.join(&filename);
    tokio::fs::File::create(&path)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload file: {err}")))?;

    let now = Utc::now();
    let record = UploadRecord {
        schema_version: 1,
        upload_id: upload_id.clone(),
        filename: filename.clone(),
        size: 0,
        expected_size: Some(req.size),
        status: UploadStatus::Uploading,
        path,
        created_at: now,
        updated_at: now,
    };
    state
        .uploads
        .create(record)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist upload: {err}")))?;

    Ok(Json(UploadResponse {
        upload_id,
        filename,
        size: 0,
    }))
}

async fn receive_upload_field(
    state: Arc<AppState>,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<UploadResponse, AppError> {
    let upload_id = next_id("upl");
    let upload_dir = state.config.storage.upload_dir(&upload_id);
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload dir: {err}")))?;

    let fallback_name = field.file_name().unwrap_or("upload.bin").to_string();
    let filename = sanitize_filename(&fallback_name)?;
    let path = upload_dir.join(&filename);
    let mut out = tokio::fs::File::create(&path)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload file: {err}")))?;
    let mut size = 0_u64;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|err| AppError::bad_request(format!("failed to read upload field: {err}")))?
    {
        size += chunk.len() as u64;
        if size > state.config.storage.max_upload_bytes {
            return Err(AppError::bad_request(format!(
                "upload size {size} exceeds max_upload_bytes {}",
                state.config.storage.max_upload_bytes
            )));
        }
        out.write_all(&chunk)
            .await
            .map_err(|err| AppError::internal(format!("failed to write upload file: {err}")))?;
    }
    out.flush()
        .await
        .map_err(|err| AppError::internal(format!("failed to flush upload file: {err}")))?;
    drop(out);

    let now = Utc::now();
    let record = UploadRecord {
        schema_version: 1,
        upload_id: upload_id.clone(),
        filename: filename.clone(),
        size,
        expected_size: Some(size),
        status: UploadStatus::Complete,
        path,
        created_at: now,
        updated_at: now,
    };
    state
        .uploads
        .create(record)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist upload: {err}")))?;

    Ok(UploadResponse {
        upload_id,
        filename,
        size,
    })
}

pub async fn upload_chunk(
    State(state): State<Arc<AppState>>,
    Path(upload_id): Path<String>,
    Query(query): Query<ChunkQuery>,
    body: Bytes,
) -> Result<Json<ChunkUploadResponse>, AppError> {
    if body.len() as u64 > state.config.storage.max_chunk_bytes {
        return Err(AppError::bad_request(format!(
            "chunk size {} exceeds max_chunk_bytes {}",
            body.len(),
            state.config.storage.max_chunk_bytes
        )));
    }

    let upload = state
        .uploads
        .append_chunk(
            &upload_id,
            query.offset,
            &body,
            state.config.storage.max_upload_bytes,
        )
        .await
        .map_err(|err| AppError::bad_request(err.to_string()))?;

    Ok(Json(ChunkUploadResponse {
        upload_id,
        received_bytes: upload.size,
    }))
}

pub async fn complete_upload(
    State(state): State<Arc<AppState>>,
    Path(upload_id): Path<String>,
) -> Result<Json<UploadResponse>, AppError> {
    let upload = state
        .uploads
        .complete(&upload_id)
        .await
        .map_err(|err| AppError::bad_request(err.to_string()))?;

    Ok(Json(UploadResponse {
        upload_id: upload.upload_id,
        filename: upload.filename,
        size: upload.size,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use tower::ServiceExt;

    use crate::{
        api,
        config::{
            AppConfig, AuthSettings, LlmProvider, LlmSettings, LogAnalyzerSettings, ServerSettings,
            StorageSettings,
        },
        state::AppState,
    };

    #[tokio::test]
    async fn multipart_upload_flushes_payload_before_persisting_record() {
        let (state, root) = test_state();
        let app = api::router(state.clone()).with_state(state.clone());
        let response = app
            .oneshot(multipart_request(
                "/api/uploads",
                "upload-boundary",
                vec![
                    text_part("filename", "sample.log"),
                    file_part("file", "browser-name.log", "ERROR sample\n"),
                ],
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "unexpected response: {}",
            String::from_utf8_lossy(&body)
        );
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let upload_id = body["uploadId"].as_str().unwrap();
        let record = state.uploads.get(upload_id).await.unwrap();
        assert_eq!(record.filename, "sample.log");
        assert_eq!(record.size, 13);
        assert_eq!(std::fs::metadata(record.path).unwrap().len(), 13);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn batch_upload_flushes_each_payload_before_persisting_records() {
        let (state, root) = test_state();
        let app = api::router(state.clone()).with_state(state.clone());
        let response = app
            .oneshot(multipart_request(
                "/api/uploads/batch",
                "batch-boundary",
                vec![
                    file_part("files", "one.log", "ERROR one\n"),
                    file_part("files", "two.log", "TIMEOUT two\n"),
                ],
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "unexpected response: {}",
            String::from_utf8_lossy(&body)
        );
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let uploads = body["uploads"].as_array().unwrap();
        assert_eq!(uploads.len(), 2);
        for upload in uploads {
            let upload_id = upload["uploadId"].as_str().unwrap();
            let record = state.uploads.get(upload_id).await.unwrap();
            assert_eq!(std::fs::metadata(record.path).unwrap().len(), record.size);
            assert!(record.size > 0);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    fn multipart_request(path: &str, boundary: &str, parts: Vec<String>) -> Request<Body> {
        let body = format!(
            "{}--{boundary}--\r\n",
            parts
                .into_iter()
                .map(|part| format!("--{boundary}\r\n{part}"))
                .collect::<String>()
        );
        Request::post(path)
            .header("authorization", "Bearer test-key")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("content-length", body.len().to_string())
            .body(Body::from(body))
            .unwrap()
    }

    fn text_part(name: &str, value: &str) -> String {
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
    }

    fn file_part(name: &str, filename: &str, value: &str) -> String {
        format!(
            "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: text/plain\r\n\r\n{value}\r\n"
        )
    }

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-upload-api-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let config = Arc::new(AppConfig {
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 2,
            },
            auth: AuthSettings {
                api_keys: vec!["test-key".to_string()],
            },
            storage: StorageSettings {
                data_dir: root.join("data"),
                max_upload_bytes: 1024 * 1024,
                max_chunk_bytes: 512 * 1024,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            llm: LlmSettings {
                provider: LlmProvider::Stub,
                base_url: None,
                api_key: None,
                model: "stub".to_string(),
                request_timeout_seconds: 1,
                max_input_chars: 60_000,
                max_output_tokens: 100,
            },
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }
}
