use axum::{extract::State, Json};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::{
    error::AppError,
    fs_utils::sanitize_filename,
    id::next_id,
    models::{UploadRecord, UploadResponse},
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

    while let Some(field) = multipart
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
        let data = field
            .bytes()
            .await
            .map_err(|err| AppError::bad_request(format!("failed to read upload field: {err}")))?;
        size = data.len() as u64;
        if size > state.config.storage.max_upload_bytes {
            return Err(AppError::bad_request(format!(
                "upload size {size} exceeds max_upload_bytes {}",
                state.config.storage.max_upload_bytes
            )));
        }
        out.write_all(&data)
            .await
            .map_err(|err| AppError::internal(format!("failed to write upload file: {err}")))?;
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
