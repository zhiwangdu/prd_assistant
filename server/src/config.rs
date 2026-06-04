use std::{env, fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub storage: StorageSettings,
    pub log_analyzer: LogAnalyzerSettings,
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub bind: String,
    pub public_base_url: String,
}

#[derive(Debug, Clone)]
pub struct AuthSettings {
    pub api_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StorageSettings {
    pub data_dir: PathBuf,
    pub max_upload_bytes: u64,
    pub max_chunk_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct LogAnalyzerSettings {
    pub keywords: Vec<String>,
    pub max_matches: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigFile {
    server: Option<ServerConfig>,
    auth: Option<AuthConfig>,
    storage: Option<StorageConfig>,
    log_analyzer: Option<LogAnalyzerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default = "default_public_base_url")]
    public_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthConfig {
    #[serde(default)]
    api_keys: Vec<ApiKeyConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiKeyConfig {
    #[allow(dead_code)]
    name: String,
    value_env: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StorageConfig {
    #[serde(default = "default_data_dir")]
    data_dir: PathBuf,
    #[serde(default = "default_max_upload_bytes")]
    max_upload_bytes: u64,
    #[serde(default = "default_max_chunk_bytes")]
    max_chunk_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct LogAnalyzerConfig {
    #[serde(default = "default_keywords")]
    keywords: Vec<String>,
    #[serde(default = "default_max_matches")]
    max_matches: usize,
}

impl AppConfig {
    pub fn prepare_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(self.storage.uploads_dir())?;
        fs::create_dir_all(self.storage.workspaces_dir())?;
        fs::create_dir_all(self.storage.metadata_dir())?;
        fs::create_dir_all(self.storage.metadata_imports_dir())?;
        Ok(())
    }
}

impl StorageSettings {
    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }

    pub fn upload_dir(&self, upload_id: &str) -> PathBuf {
        self.uploads_dir().join(upload_id)
    }

    pub fn workspaces_dir(&self) -> PathBuf {
        self.data_dir.join("workspaces")
    }

    pub fn workspace_dir(&self, task_id: &str) -> PathBuf {
        self.workspaces_dir().join(task_id)
    }

    pub fn metadata_dir(&self) -> PathBuf {
        self.data_dir.join("metadata")
    }

    pub fn metadata_imports_dir(&self) -> PathBuf {
        self.metadata_dir().join("imports")
    }
}

pub fn load_config(path: &std::path::Path) -> anyhow::Result<Arc<AppConfig>> {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let parsed: ConfigFile = if raw.trim().is_empty() {
        ConfigFile {
            server: None,
            auth: None,
            storage: None,
            log_analyzer: None,
        }
    } else {
        serde_yaml::from_str(&raw).context("invalid YAML")?
    };

    let server = parsed.server.unwrap_or_else(default_server_config);
    let auth = parsed.auth.unwrap_or_else(default_auth_config);
    let storage = parsed.storage.unwrap_or_else(default_storage_config);
    let analyzer = parsed
        .log_analyzer
        .unwrap_or_else(default_log_analyzer_config);

    let mut api_keys = Vec::new();
    for api_key in auth.api_keys {
        api_keys.push(
            env::var(&api_key.value_env)
                .with_context(|| format!("missing API key env var {}", api_key.value_env))?,
        );
    }
    if api_keys.is_empty() {
        api_keys.push(
            env::var("LOGAGENT_NATIVE_API_KEY")
                .context("missing API key config and fallback env var LOGAGENT_NATIVE_API_KEY")?,
        );
    }

    Ok(Arc::new(AppConfig {
        server: ServerSettings {
            bind: server.bind,
            public_base_url: server.public_base_url,
        },
        auth: AuthSettings { api_keys },
        storage: StorageSettings {
            data_dir: storage.data_dir,
            max_upload_bytes: storage.max_upload_bytes,
            max_chunk_bytes: storage.max_chunk_bytes,
        },
        log_analyzer: LogAnalyzerSettings {
            keywords: analyzer
                .keywords
                .into_iter()
                .map(|keyword| keyword.to_ascii_lowercase())
                .collect(),
            max_matches: analyzer.max_matches,
        },
    }))
}

fn default_server_config() -> ServerConfig {
    ServerConfig {
        bind: default_bind(),
        public_base_url: default_public_base_url(),
    }
}

fn default_auth_config() -> AuthConfig {
    AuthConfig { api_keys: vec![] }
}

fn default_storage_config() -> StorageConfig {
    StorageConfig {
        data_dir: default_data_dir(),
        max_upload_bytes: default_max_upload_bytes(),
        max_chunk_bytes: default_max_chunk_bytes(),
    }
}

fn default_log_analyzer_config() -> LogAnalyzerConfig {
    LogAnalyzerConfig {
        keywords: default_keywords(),
        max_matches: default_max_matches(),
    }
}

fn default_bind() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_public_base_url() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./data/logagent")
}

fn default_max_upload_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}

fn default_max_chunk_bytes() -> u64 {
    512 * 1024
}

fn default_max_matches() -> usize {
    200
}

fn default_keywords() -> Vec<String> {
    [
        "error",
        "exception",
        "timeout",
        "fail",
        "failed",
        "panic",
        "fatal",
        "refused",
        "denied",
        "verify",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}
