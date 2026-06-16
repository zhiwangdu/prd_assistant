use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    sync::Arc,
};

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::{
    services::fetch::{
        endpoint_view, FetchEndpointDraft, FetchEndpointRecord, FetchEndpointView,
        FetchResolvedEndpoint,
    },
    support::config::FetchSettings,
};

#[derive(Debug, Clone)]
pub struct FetchStore {
    dir: PathBuf,
    key: Option<[u8; 32]>,
    inner: Arc<RwLock<BTreeMap<String, FetchEndpointRecord>>>,
}

impl FetchStore {
    pub fn load(dir: PathBuf, settings: &FetchSettings) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut endpoints = BTreeMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let endpoint: FetchEndpointRecord = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid fetch endpoint {}: {err}", path.display())
            })?;
            if endpoints
                .insert(endpoint.fetch_id.clone(), endpoint)
                .is_some()
            {
                anyhow::bail!("duplicate fetch endpoint in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            key: settings.secret_key,
            inner: Arc::new(RwLock::new(endpoints)),
        })
    }

    pub async fn list(&self) -> Vec<FetchEndpointView> {
        self.inner
            .read()
            .await
            .values()
            .map(endpoint_view)
            .collect()
    }

    pub async fn get_view(&self, fetch_id: &str) -> Option<FetchEndpointView> {
        self.inner.read().await.get(fetch_id).map(endpoint_view)
    }

    pub async fn get_resolved(
        &self,
        fetch_id: &str,
    ) -> anyhow::Result<Option<FetchResolvedEndpoint>> {
        let endpoint = match self.inner.read().await.get(fetch_id).cloned() {
            Some(endpoint) => endpoint,
            None => return Ok(None),
        };
        let credentials = self.decrypt_credentials(&endpoint)?;
        Ok(Some(FetchResolvedEndpoint {
            endpoint,
            credentials,
        }))
    }

    pub async fn create(&self, draft: FetchEndpointDraft) -> anyhow::Result<FetchEndpointView> {
        let mut endpoint = draft.record;
        endpoint.credential_set.credentials = draft
            .plaintext_credentials
            .iter()
            .map(|(key, value)| self.encrypt_credential(key, value))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut endpoints = self.inner.write().await;
        if endpoints.contains_key(&endpoint.fetch_id) {
            anyhow::bail!("fetch endpoint {} already exists", endpoint.fetch_id);
        }
        self.persist(&endpoint)?;
        endpoints.insert(endpoint.fetch_id.clone(), endpoint.clone());
        Ok(endpoint_view(&endpoint))
    }

    pub async fn update_metadata(
        &self,
        fetch_id: &str,
        name: Option<String>,
        description: Option<Option<String>>,
        tags: Option<Vec<String>>,
        enabled: Option<bool>,
    ) -> anyhow::Result<FetchEndpointView> {
        let mut endpoints = self.inner.write().await;
        let mut endpoint = endpoints
            .get(fetch_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown fetchId {fetch_id}"))?;
        if let Some(name) = name {
            endpoint.name = name;
        }
        if let Some(description) = description {
            endpoint.description = description;
        }
        if let Some(tags) = tags {
            endpoint.tags = tags;
        }
        if let Some(enabled) = enabled {
            endpoint.enabled = enabled;
        }
        endpoint.updated_at = Utc::now();
        self.persist(&endpoint)?;
        endpoints.insert(fetch_id.to_string(), endpoint.clone());
        Ok(endpoint_view(&endpoint))
    }

    pub async fn set_last_run(
        &self,
        fetch_id: &str,
        task_id: String,
    ) -> anyhow::Result<Option<FetchEndpointView>> {
        let mut endpoints = self.inner.write().await;
        let Some(mut endpoint) = endpoints.get(fetch_id).cloned() else {
            return Ok(None);
        };
        endpoint.last_run_task_id = Some(task_id);
        endpoint.updated_at = Utc::now();
        self.persist(&endpoint)?;
        endpoints.insert(fetch_id.to_string(), endpoint.clone());
        Ok(Some(endpoint_view(&endpoint)))
    }

    pub async fn delete(&self, fetch_id: &str) -> anyhow::Result<bool> {
        let mut endpoints = self.inner.write().await;
        let existed = endpoints.remove(fetch_id).is_some();
        if existed {
            let path = self.endpoint_path(fetch_id);
            if let Err(err) = fs::remove_file(&path) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    return Err(err).with_context(|| {
                        format!("failed to delete fetch endpoint {}", path.display())
                    });
                }
            }
        }
        Ok(existed)
    }

    fn encrypt_credential(
        &self,
        key: &str,
        value: &str,
    ) -> anyhow::Result<crate::services::fetch::FetchEncryptedCredential> {
        let cipher = self.cipher()?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|err| anyhow::anyhow!("failed to encrypt fetch credential {key}: {err}"))?;
        Ok(crate::services::fetch::FetchEncryptedCredential {
            key: key.to_string(),
            nonce: BASE64.encode(nonce_bytes),
            ciphertext: BASE64.encode(ciphertext),
        })
    }

    fn decrypt_credentials(
        &self,
        endpoint: &FetchEndpointRecord,
    ) -> anyhow::Result<HashMap<String, String>> {
        let cipher = self.cipher()?;
        let mut output = HashMap::new();
        for credential in &endpoint.credential_set.credentials {
            let nonce_bytes = BASE64.decode(&credential.nonce).with_context(|| {
                format!("invalid nonce for fetch credential {}", credential.key)
            })?;
            if nonce_bytes.len() != 12 {
                anyhow::bail!(
                    "invalid nonce length for fetch credential {}",
                    credential.key
                );
            }
            let ciphertext = BASE64.decode(&credential.ciphertext).with_context(|| {
                format!("invalid ciphertext for fetch credential {}", credential.key)
            })?;
            let plaintext = cipher
                .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
                .map_err(|err| {
                    anyhow::anyhow!(
                        "failed to decrypt fetch credential {}: {err}",
                        credential.key
                    )
                })?;
            output.insert(
                credential.key.clone(),
                String::from_utf8(plaintext)
                    .with_context(|| format!("fetch credential {} is not UTF-8", credential.key))?,
            );
        }
        Ok(output)
    }

    fn cipher(&self) -> anyhow::Result<Aes256Gcm> {
        let key = self
            .key
            .as_ref()
            .context("fetch secret key is not configured")?;
        Ok(Aes256Gcm::new_from_slice(key).expect("AES-256-GCM accepts 32-byte keys"))
    }

    fn persist(&self, endpoint: &FetchEndpointRecord) -> anyhow::Result<()> {
        let path = self.endpoint_path(&endpoint.fetch_id);
        let tmp = path.with_extension("json.tmp");
        let raw = serde_json::to_vec_pretty(endpoint)?;
        fs::write(&tmp, raw)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    fn endpoint_path(&self, fetch_id: &str) -> PathBuf {
        self.dir.join(format!("{fetch_id}.json"))
    }
}
