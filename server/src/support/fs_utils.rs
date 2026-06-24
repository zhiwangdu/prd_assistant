use std::path::{Component, Path, PathBuf};

use anyhow::anyhow;

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
