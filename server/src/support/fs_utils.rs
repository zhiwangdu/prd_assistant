use std::path::{Component, Path, PathBuf};

use anyhow::anyhow;
use serde::Serialize;

use crate::support::error::AppError;

pub fn sanitize_filename(value: &str) -> Result<String, AppError> {
    let filename = Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::bad_request("invalid filename"))?;
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err(AppError::bad_request("invalid filename"));
    }
    Ok(filename.to_string())
}

pub fn safe_join(root: &Path, child: &Path) -> anyhow::Result<PathBuf> {
    let mut safe = PathBuf::from(root);
    for component in child.components() {
        match component {
            Component::Normal(value) => safe.push(value),
            Component::CurDir => {}
            _ => return Err(anyhow!("archive contains unsafe path {}", child.display())),
        }
    }
    Ok(safe)
}

pub fn relative_string(root: &Path, path: &Path) -> anyhow::Result<String> {
    Ok(path
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/"))
}

/// Atomically write `value` as pretty JSON to `path` via a temp file + rename.
pub async fn write_json_atomic<T: Serialize>(path: PathBuf, value: &T) -> Result<(), AppError> {
    let tmp = path.with_extension("json.tmp");
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|err| AppError::internal(format!("failed to encode json: {err}")))?;
    tokio::fs::write(&tmp, encoded)
        .await
        .map_err(|err| AppError::internal(format!("failed to write {}: {err}", path.display())))?;
    tokio::fs::rename(&tmp, &path).await.map_err(|err| {
        AppError::internal(format!("failed to persist {}: {err}", path.display()))
    })?;
    Ok(())
}
