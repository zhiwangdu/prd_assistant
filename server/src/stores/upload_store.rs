use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use chrono::Utc;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt},
    sync::RwLock,
};
use tracing::warn;

use crate::domain::models::{UploadRecord, UploadStatus};

#[derive(Debug, Clone)]
pub struct UploadStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, UploadRecord>>>,
}

impl UploadStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut uploads = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let mut upload: UploadRecord = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid upload record {}: {err}", path.display())
            })?;
            validate_record_path(&dir, &path, &upload)?;
            let metadata = fs::metadata(&upload.path).with_context(|| {
                format!(
                    "upload payload for {} is missing at {}",
                    upload.upload_id,
                    upload.path.display()
                )
            })?;
            let actual_size = metadata.len();
            if let Some(expected_size) = upload.expected_size {
                if actual_size > expected_size {
                    anyhow::bail!(
                        "upload {} size {} exceeds expected size {}",
                        upload.upload_id,
                        actual_size,
                        expected_size
                    );
                }
            }
            if upload.status == UploadStatus::Complete {
                if upload.size != actual_size {
                    anyhow::bail!(
                        "completed upload {} recorded size {} does not match payload size {}",
                        upload.upload_id,
                        upload.size,
                        actual_size
                    );
                }
                if let Some(expected_size) = upload.expected_size {
                    if actual_size != expected_size {
                        anyhow::bail!(
                            "completed upload {} size {} does not match expected size {}",
                            upload.upload_id,
                            actual_size,
                            expected_size
                        );
                    }
                }
            } else if upload.size != actual_size {
                upload.size = actual_size;
                upload.updated_at = Utc::now();
                persist_record(&dir, &upload)?;
            }
            if uploads.insert(upload.upload_id.clone(), upload).is_some() {
                anyhow::bail!("duplicate upload record in {}", path.display());
            }
        }

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let upload_id = entry.file_name().to_string_lossy().into_owned();
            if upload_id.starts_with("upl_") && !uploads.contains_key(&upload_id) {
                warn!(upload_id, path = %entry.path().display(), "orphan upload directory");
            }
        }

        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(uploads)),
        })
    }

    pub async fn create(&self, record: UploadRecord) -> anyhow::Result<()> {
        validate_upload_id(&record.upload_id)?;
        let record_path = self.dir.join(format!("{}.json", record.upload_id));
        validate_record_path(&self.dir, &record_path, &record)?;
        validate_payload(&record)?;
        let mut uploads = self.inner.write().await;
        if uploads.contains_key(&record.upload_id) {
            anyhow::bail!("upload {} already exists", record.upload_id);
        }
        persist_record(&self.dir, &record)?;
        uploads.insert(record.upload_id.clone(), record);
        Ok(())
    }

    pub async fn get(&self, upload_id: &str) -> Option<UploadRecord> {
        self.inner.read().await.get(upload_id).cloned()
    }

    pub async fn append_chunk(
        &self,
        upload_id: &str,
        offset: u64,
        body: &[u8],
        max_upload_bytes: u64,
    ) -> anyhow::Result<UploadRecord> {
        let mut uploads = self.inner.write().await;
        let mut candidate = uploads
            .get(upload_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown uploadId"))?;
        if candidate.status != UploadStatus::Uploading {
            anyhow::bail!("upload {upload_id} is already complete");
        }
        let actual_size = tokio::fs::metadata(&candidate.path).await?.len();
        if candidate.size != actual_size {
            candidate.size = actual_size;
            candidate.updated_at = Utc::now();
            persist_record(&self.dir, &candidate)?;
            uploads.insert(upload_id.to_string(), candidate.clone());
        }
        if offset != candidate.size {
            anyhow::bail!(
                "chunk offset {offset} does not match received bytes {}",
                candidate.size
            );
        }
        let received_bytes = offset
            .checked_add(body.len() as u64)
            .context("chunk offset overflow")?;
        if received_bytes > max_upload_bytes {
            anyhow::bail!(
                "upload size {received_bytes} exceeds max_upload_bytes {max_upload_bytes}"
            );
        }
        if let Some(expected_size) = candidate.expected_size {
            if received_bytes > expected_size {
                anyhow::bail!("upload size {received_bytes} exceeds expected size {expected_size}");
            }
        }

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&candidate.path)
            .await
            .with_context(|| format!("failed to open upload file {}", candidate.path.display()))?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        file.write_all(body).await?;
        file.flush().await?;

        candidate.size = received_bytes;
        candidate.updated_at = Utc::now();
        persist_record(&self.dir, &candidate)?;
        uploads.insert(upload_id.to_string(), candidate.clone());
        Ok(candidate)
    }

    pub async fn complete(&self, upload_id: &str) -> anyhow::Result<UploadRecord> {
        let mut uploads = self.inner.write().await;
        let mut candidate = uploads
            .get(upload_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown uploadId"))?;
        let actual_size = tokio::fs::metadata(&candidate.path).await?.len();
        if let Some(expected_size) = candidate.expected_size {
            if actual_size != expected_size {
                anyhow::bail!(
                    "upload size {actual_size} does not match expected size {expected_size}"
                );
            }
        }
        candidate.size = actual_size;
        candidate.status = UploadStatus::Complete;
        candidate.updated_at = Utc::now();
        persist_record(&self.dir, &candidate)?;
        uploads.insert(upload_id.to_string(), candidate.clone());
        Ok(candidate)
    }
}

fn validate_record_path(
    dir: &std::path::Path,
    record_path: &std::path::Path,
    upload: &UploadRecord,
) -> anyhow::Result<()> {
    validate_upload_id(&upload.upload_id)?;
    if upload.schema_version != 1 {
        anyhow::bail!(
            "unsupported upload schema version {} in {}",
            upload.schema_version,
            record_path.display()
        );
    }
    let filename = std::path::Path::new(&upload.filename);
    if filename.file_name().and_then(|value| value.to_str()) != Some(upload.filename.as_str())
        || upload.filename == "."
        || upload.filename == ".."
    {
        anyhow::bail!(
            "upload {} contains invalid filename {}",
            upload.upload_id,
            upload.filename
        );
    }
    let expected_record_path = dir.join(format!("{}.json", upload.upload_id));
    if record_path != expected_record_path {
        anyhow::bail!(
            "upload record {} contains mismatched id {}",
            record_path.display(),
            upload.upload_id
        );
    }
    let expected_payload_path = dir.join(&upload.upload_id).join(&upload.filename);
    if upload.path != expected_payload_path {
        anyhow::bail!(
            "upload {} contains unsafe payload path {}",
            upload.upload_id,
            upload.path.display()
        );
    }
    Ok(())
}

fn validate_payload(upload: &UploadRecord) -> anyhow::Result<()> {
    let actual_size = fs::metadata(&upload.path)
        .with_context(|| format!("upload payload is missing at {}", upload.path.display()))?
        .len();
    if actual_size != upload.size {
        anyhow::bail!(
            "upload {} recorded size {} does not match payload size {}",
            upload.upload_id,
            upload.size,
            actual_size
        );
    }
    if let Some(expected_size) = upload.expected_size {
        if actual_size > expected_size {
            anyhow::bail!(
                "upload {} size {} exceeds expected size {}",
                upload.upload_id,
                actual_size,
                expected_size
            );
        }
        if upload.status == UploadStatus::Complete && actual_size != expected_size {
            anyhow::bail!(
                "completed upload {} size {} does not match expected size {}",
                upload.upload_id,
                actual_size,
                expected_size
            );
        }
    }
    Ok(())
}

fn validate_upload_id(upload_id: &str) -> anyhow::Result<()> {
    let valid = upload_id.starts_with("upl_")
        && upload_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid upload id {upload_id}")
    }
}

fn persist_record(dir: &std::path::Path, upload: &UploadRecord) -> anyhow::Result<()> {
    let path = dir.join(format!("{}.json", upload.upload_id));
    let temp = dir.join(format!(".{}.json.tmp", upload.upload_id));
    fs::write(&temp, serde_json::to_vec_pretty(upload)?)?;
    fs::rename(&temp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn persists_reloads_and_completes_chunked_uploads() {
        let dir = temp_dir("upload-store");
        let upload_id = "upl_test";
        let payload_dir = dir.join(upload_id);
        fs::create_dir_all(&payload_dir).unwrap();
        let path = payload_dir.join("sample.log");
        fs::write(&path, b"").unwrap();
        let now = Utc::now();
        let store = UploadStore::load(dir.clone()).unwrap();
        store
            .create(UploadRecord {
                schema_version: 1,
                upload_id: upload_id.to_string(),
                filename: "sample.log".to_string(),
                size: 0,
                expected_size: Some(5),
                status: UploadStatus::Uploading,
                path: path.clone(),
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        store
            .append_chunk(upload_id, 0, b"hello", 1024)
            .await
            .unwrap();

        let reloaded = UploadStore::load(dir.clone()).unwrap();
        let upload = reloaded.get(upload_id).await.unwrap();
        assert_eq!(upload.size, 5);
        assert_eq!(upload.status, UploadStatus::Uploading);
        let upload = reloaded.complete(upload_id).await.unwrap();
        assert_eq!(upload.status, UploadStatus::Complete);

        let completed = UploadStore::load(dir.clone()).unwrap();
        assert_eq!(
            completed.get(upload_id).await.unwrap().status,
            UploadStatus::Complete
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn rejects_non_sequential_chunks_and_incomplete_completion() {
        let dir = temp_dir("upload-store-offset");
        let upload_id = "upl_test";
        let payload_dir = dir.join(upload_id);
        fs::create_dir_all(&payload_dir).unwrap();
        let path = payload_dir.join("sample.log");
        fs::write(&path, b"").unwrap();
        let now = Utc::now();
        let store = UploadStore::load(dir.clone()).unwrap();
        store
            .create(UploadRecord {
                schema_version: 1,
                upload_id: upload_id.to_string(),
                filename: "sample.log".to_string(),
                size: 0,
                expected_size: Some(5),
                status: UploadStatus::Uploading,
                path,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        assert!(store.append_chunk(upload_id, 1, b"x", 1024).await.is_err());
        store.append_chunk(upload_id, 0, b"hi", 1024).await.unwrap();
        assert!(store.complete(upload_id).await.is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn reload_reconciles_interrupted_upload_size_from_payload() {
        let dir = temp_dir("upload-store-reconcile");
        let upload_id = "upl_test";
        let payload_dir = dir.join(upload_id);
        fs::create_dir_all(&payload_dir).unwrap();
        let path = payload_dir.join("sample.log");
        fs::write(&path, b"").unwrap();
        let now = Utc::now();
        let store = UploadStore::load(dir.clone()).unwrap();
        store
            .create(UploadRecord {
                schema_version: 1,
                upload_id: upload_id.to_string(),
                filename: "sample.log".to_string(),
                size: 0,
                expected_size: Some(5),
                status: UploadStatus::Uploading,
                path: path.clone(),
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        fs::write(&path, b"hel").unwrap();
        let reloaded = UploadStore::load(dir.clone()).unwrap();

        assert_eq!(reloaded.get(upload_id).await.unwrap().size, 3);
        reloaded
            .append_chunk(upload_id, 3, b"lo", 1024)
            .await
            .unwrap();
        assert_eq!(reloaded.complete(upload_id).await.unwrap().size, 5);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_upload_json_fails_loading() {
        let dir = temp_dir("upload-store-corrupt");
        fs::write(dir.join("upl_bad.json"), b"{bad").unwrap();
        assert!(UploadStore::load(dir.clone()).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unsafe_upload_path_fails_loading() {
        let dir = temp_dir("upload-store-unsafe");
        let outside = dir.parent().unwrap().join("outside.log");
        fs::write(&outside, b"x").unwrap();
        let now = Utc::now();
        let record = UploadRecord {
            schema_version: 1,
            upload_id: "upl_bad".to_string(),
            filename: "outside.log".to_string(),
            size: 1,
            expected_size: Some(1),
            status: UploadStatus::Complete,
            path: outside.clone(),
            created_at: now,
            updated_at: now,
        };
        fs::write(
            dir.join("upl_bad.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();

        assert!(UploadStore::load(dir.clone()).is_err());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn inconsistent_completed_upload_fails_loading() {
        let dir = temp_dir("upload-store-inconsistent");
        let upload_id = "upl_bad";
        let payload_dir = dir.join(upload_id);
        fs::create_dir_all(&payload_dir).unwrap();
        let path = payload_dir.join("sample.log");
        fs::write(&path, b"two").unwrap();
        let now = Utc::now();
        let record = UploadRecord {
            schema_version: 1,
            upload_id: upload_id.to_string(),
            filename: "sample.log".to_string(),
            size: 1,
            expected_size: Some(1),
            status: UploadStatus::Complete,
            path,
            created_at: now,
            updated_at: now,
        };
        fs::write(
            dir.join(format!("{upload_id}.json")),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();

        assert!(UploadStore::load(dir.clone()).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "logagent-{name}-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
