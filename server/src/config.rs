use std::{env, fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub storage: StorageSettings,
    pub log_analyzer: LogAnalyzerSettings,
    pub llm: LlmSettings,
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub bind: String,
    pub public_base_url: String,
    pub max_concurrent_tasks: usize,
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

#[derive(Clone)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: String,
    pub request_timeout_seconds: u64,
    pub max_input_chars: usize,
    pub max_output_tokens: u32,
}

impl std::fmt::Debug for LlmSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LlmSettings")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("model", &self.model)
            .field("request_timeout_seconds", &self.request_timeout_seconds)
            .field("max_input_chars", &self.max_input_chars)
            .field("max_output_tokens", &self.max_output_tokens)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    Stub,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigFile {
    server: Option<ServerConfig>,
    auth: Option<AuthConfig>,
    storage: Option<StorageConfig>,
    log_analyzer: Option<LogAnalyzerConfig>,
    llm: Option<LlmConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default = "default_public_base_url")]
    public_base_url: String,
    #[serde(default = "default_max_concurrent_tasks")]
    max_concurrent_tasks: usize,
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

#[derive(Debug, Clone, Deserialize)]
struct LlmConfig {
    #[serde(default = "default_llm_provider")]
    provider: String,
    base_url_env: Option<String>,
    api_key_env: Option<String>,
    model_env: Option<String>,
    #[serde(default = "default_llm_model")]
    model: String,
    #[serde(default = "default_llm_timeout")]
    request_timeout_seconds: u64,
    #[serde(default = "default_llm_max_input_chars")]
    max_input_chars: usize,
    #[serde(default = "default_llm_max_output_tokens")]
    max_output_tokens: u32,
}

impl AppConfig {
    pub fn prepare_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(self.storage.uploads_dir())?;
        fs::create_dir_all(self.storage.workspaces_dir())?;
        fs::create_dir_all(self.storage.tasks_dir())?;
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

    pub fn tasks_dir(&self) -> PathBuf {
        self.data_dir.join("tasks")
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
            llm: None,
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
    let llm = parsed.llm.unwrap_or_else(default_llm_config);

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

    let provider = match llm.provider.as_str() {
        "stub" => LlmProvider::Stub,
        "openai_compatible" => LlmProvider::OpenAiCompatible,
        value => anyhow::bail!("unsupported llm.provider {value}"),
    };
    let model = resolve_llm_model(&llm)?;
    let (base_url, api_key) = match provider {
        LlmProvider::Stub => (None, None),
        LlmProvider::OpenAiCompatible => {
            let base_url_env = llm
                .base_url_env
                .as_deref()
                .context("llm.base_url_env is required for openai_compatible")?;
            let api_key_env = llm
                .api_key_env
                .as_deref()
                .context("llm.api_key_env is required for openai_compatible")?;
            (
                Some(
                    env::var(base_url_env)
                        .with_context(|| format!("missing LLM base URL env var {base_url_env}"))?,
                ),
                Some(
                    env::var(api_key_env)
                        .with_context(|| format!("missing LLM API key env var {api_key_env}"))?,
                ),
            )
        }
    };

    Ok(Arc::new(AppConfig {
        server: ServerSettings {
            bind: server.bind,
            public_base_url: server.public_base_url,
            max_concurrent_tasks: server.max_concurrent_tasks.max(1),
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
        llm: LlmSettings {
            provider,
            base_url,
            api_key,
            model,
            request_timeout_seconds: llm.request_timeout_seconds.max(1),
            max_input_chars: llm.max_input_chars.max(1024),
            max_output_tokens: llm.max_output_tokens.max(1),
        },
    }))
}

fn default_server_config() -> ServerConfig {
    ServerConfig {
        bind: default_bind(),
        public_base_url: default_public_base_url(),
        max_concurrent_tasks: default_max_concurrent_tasks(),
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

fn default_llm_config() -> LlmConfig {
    LlmConfig {
        provider: default_llm_provider(),
        base_url_env: None,
        api_key_env: None,
        model_env: None,
        model: default_llm_model(),
        request_timeout_seconds: default_llm_timeout(),
        max_input_chars: default_llm_max_input_chars(),
        max_output_tokens: default_llm_max_output_tokens(),
    }
}

fn default_bind() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_public_base_url() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_max_concurrent_tasks() -> usize {
    2
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

fn default_llm_provider() -> String {
    "stub".to_string()
}

fn default_llm_model() -> String {
    "configured-model".to_string()
}

fn resolve_llm_model(llm: &LlmConfig) -> anyhow::Result<String> {
    resolve_llm_model_with(llm, |name| env::var(name))
}

fn resolve_llm_model_with(
    llm: &LlmConfig,
    read_env: impl Fn(&str) -> Result<String, env::VarError>,
) -> anyhow::Result<String> {
    let model = match llm.model_env.as_deref() {
        Some(model_env) => {
            let model_env = model_env.trim();
            if model_env.is_empty() {
                anyhow::bail!("llm.model_env must not be empty");
            }
            read_env(model_env).with_context(|| format!("missing LLM model env var {model_env}"))?
        }
        None => llm.model.clone(),
    };
    let model = model.trim();
    if model.is_empty() {
        anyhow::bail!("LLM model must not be empty");
    }
    Ok(model.to_string())
}

fn default_llm_timeout() -> u64 {
    120
}

fn default_llm_max_input_chars() -> usize {
    60_000
}

fn default_llm_max_output_tokens() -> u32 {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llm_config(model: &str, model_env: Option<&str>) -> LlmConfig {
        LlmConfig {
            provider: "openai_compatible".to_string(),
            base_url_env: Some("BASE_URL".to_string()),
            api_key_env: Some("API_KEY".to_string()),
            model_env: model_env.map(ToString::to_string),
            model: model.to_string(),
            request_timeout_seconds: 120,
            max_input_chars: 60_000,
            max_output_tokens: 4096,
        }
    }

    #[test]
    fn resolves_static_llm_model_when_model_env_is_not_configured() {
        let config = llm_config(" static-model ", None);

        let model = resolve_llm_model_with(&config, |_| unreachable!()).unwrap();

        assert_eq!(model, "static-model");
    }

    #[test]
    fn model_env_overrides_static_llm_model() {
        let config = llm_config("static-model", Some("LOGAGENT_LLM_MODEL"));

        let model = resolve_llm_model_with(&config, |name| {
            assert_eq!(name, "LOGAGENT_LLM_MODEL");
            Ok(" env-model ".to_string())
        })
        .unwrap();

        assert_eq!(model, "env-model");
    }

    #[test]
    fn missing_or_empty_model_env_value_is_rejected() {
        let missing = llm_config("static-model", Some("LOGAGENT_LLM_MODEL"));
        let error = resolve_llm_model_with(&missing, |_| Err(env::VarError::NotPresent))
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing LLM model env var LOGAGENT_LLM_MODEL"));

        let empty = llm_config("static-model", Some("LOGAGENT_LLM_MODEL"));
        let error = resolve_llm_model_with(&empty, |_| Ok("  ".to_string()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("LLM model must not be empty"));
    }
}
