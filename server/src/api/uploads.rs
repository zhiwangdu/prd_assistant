use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::{
    error::AppError,
    fs_utils::sanitize_filename,
    id::next_id,
    models::{ChunkQuery, ChunkUploadResponse, InitUploadRequest, UploadRecord, UploadResponse},
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
        filename = Some(safe_name);
        file_path = Some(path);
    }

    let filename = filename.ok_or_else(|| AppError::bad_request("missing filename"))?;
    let path = file_path.ok_or_else(|| AppError::bad_request("missing file field"))?;
    let record = UploadRecord {
        upload_id: upload_id.clone(),
        filename: filename.clone(),
        size,
        path,
    };
    state.uploads.insert(record.clone()).await;

    Ok(Json(UploadResponse {
        upload_id,
        filename,
        size,
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

    let record = UploadRecord {
        upload_id: upload_id.clone(),
        filename: filename.clone(),
        size: 0,
        path,
    };
    state.uploads.insert(record).await;

    Ok(Json(UploadResponse {
        upload_id,
        filename,
        size: 0,
    }))
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
        .get(&upload_id)
        .await
        .ok_or_else(|| AppError::bad_request("unknown uploadId"))?;
    let received_bytes = query.offset + body.len() as u64;
    if received_bytes > state.config.storage.max_upload_bytes {
        return Err(AppError::bad_request(format!(
            "upload size {received_bytes} exceeds max_upload_bytes {}",
            state.config.storage.max_upload_bytes
        )));
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&upload.path)
        .await
        .map_err(|err| AppError::internal(format!("failed to open upload file: {err}")))?;
    file.seek(std::io::SeekFrom::Start(query.offset))
        .await
        .map_err(|err| AppError::internal(format!("failed to seek upload file: {err}")))?;
    file.write_all(&body)
        .await
        .map_err(|err| AppError::internal(format!("failed to write upload chunk: {err}")))?;
    file.flush()
        .await
        .map_err(|err| AppError::internal(format!("failed to flush upload chunk: {err}")))?;

    state.uploads.update_size(&upload_id, received_bytes).await;

    Ok(Json(ChunkUploadResponse {
        upload_id,
        received_bytes,
    }))
}

pub async fn complete_upload(
    State(state): State<Arc<AppState>>,
    Path(upload_id): Path<String>,
) -> Result<Json<UploadResponse>, AppError> {
    let upload = state
        .uploads
        .get(&upload_id)
        .await
        .ok_or_else(|| AppError::bad_request("unknown uploadId"))?;
    let metadata = tokio::fs::metadata(&upload.path)
        .await
        .map_err(|err| AppError::internal(format!("failed to stat upload file: {err}")))?;
    let upload = state
        .uploads
        .update_size(&upload_id, metadata.len())
        .await
        .ok_or_else(|| AppError::bad_request("unknown uploadId"))?;

    Ok(Json(UploadResponse {
        upload_id: upload.upload_id,
        filename: upload.filename,
        size: upload.size,
    }))
}
