use std::{collections::BTreeMap, env, fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub config_path: PathBuf,
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub storage: StorageSettings,
    pub skills: SkillSettings,
    pub log_analyzer: LogAnalyzerSettings,
    pub tools: ToolsSettings,
    pub fetch: FetchSettings,
    pub huawei_cloud: HuaweiCloudSettings,
    pub remote_execution: RemoteExecutionSettings,
    pub llm: LlmSettings,
    pub claude_code: ClaudeCodeSettings,
    pub mcp: McpSettings,
    pub analysis: AnalysisSettings,
    #[allow(dead_code)]
    pub embedding: EmbeddingSettings,
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
pub struct SkillSettings {
    pub enabled: bool,
    pub roots: Vec<PathBuf>,
    pub max_skill_chars: usize,
    pub max_reference_chars: usize,
}

#[derive(Debug, Clone)]
pub struct LogAnalyzerSettings {
    pub keywords: Vec<String>,
    pub max_matches: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ToolsSettings {
    pub tools: BTreeMap<String, ToolSettings>,
}

#[derive(Debug, Clone)]
pub struct ToolSettings {
    pub name: String,
    pub enabled: bool,
    pub path: PathBuf,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
    pub max_input_files: usize,
    pub args: Vec<String>,
    pub match_settings: ToolMatchSettings,
}

#[derive(Debug, Clone, Default)]
pub struct ToolMatchSettings {
    pub file_patterns: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FetchSettings {
    pub enabled: bool,
    #[allow(dead_code)]
    pub secret_key_env: Option<String>,
    pub secret_key: Option<[u8; 32]>,
    pub allowed_hosts: Vec<FetchAllowedHost>,
    pub request_timeout_seconds: u64,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_redirects: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchAllowedHost {
    pub scheme: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

impl Default for FetchSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            secret_key_env: Some("LOGAGENT_FETCH_SECRET_KEY".to_string()),
            secret_key: None,
            allowed_hosts: Vec::new(),
            request_timeout_seconds: default_fetch_request_timeout(),
            max_request_bytes: default_fetch_max_request_bytes(),
            max_response_bytes: default_fetch_max_response_bytes(),
            max_redirects: default_fetch_max_redirects(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HuaweiCloudSettings {
    pub package_sync: HuaweiPackageSyncSettings,
}

#[derive(Debug, Clone)]
pub struct HuaweiPackageSyncSettings {
    pub enabled: bool,
    pub timeout_seconds: u64,
    pub obs: HuaweiObsSettings,
    pub gaussdb: HuaweiGaussDbSettings,
}

impl Default for HuaweiPackageSyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_seconds: default_huawei_package_sync_timeout(),
            obs: HuaweiObsSettings::default(),
            gaussdb: HuaweiGaussDbSettings::default(),
        }
    }
}

#[derive(Clone, Default)]
pub struct HuaweiObsSettings {
    pub endpoint: String,
    pub bucket: String,
    pub object_prefix: String,
    pub access_key_env: Option<String>,
    pub access_key: Option<String>,
    pub secret_key_env: Option<String>,
    pub secret_key: Option<String>,
    pub security_token_env: Option<String>,
    pub security_token: Option<String>,
}

impl std::fmt::Debug for HuaweiObsSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HuaweiObsSettings")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("object_prefix", &self.object_prefix)
            .field("access_key_env", &self.access_key_env)
            .field(
                "access_key",
                &self.access_key.as_ref().map(|_| "<redacted>"),
            )
            .field("secret_key_env", &self.secret_key_env)
            .field(
                "secret_key",
                &self.secret_key.as_ref().map(|_| "<redacted>"),
            )
            .field("security_token_env", &self.security_token_env)
            .field(
                "security_token",
                &self.security_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[derive(Clone)]
pub struct HuaweiGaussDbSettings {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password_env: Option<String>,
    pub password: Option<String>,
    pub sslmode: String,
}

impl Default for HuaweiGaussDbSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_huawei_gaussdb_port(),
            database: String::new(),
            user: String::new(),
            password_env: None,
            password: None,
            sslmode: default_huawei_gaussdb_sslmode(),
        }
    }
}

impl std::fmt::Debug for HuaweiGaussDbSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HuaweiGaussDbSettings")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password_env", &self.password_env)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("sslmode", &self.sslmode)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RemoteExecutionSettings {
    pub enabled: bool,
    pub ssh_binary: PathBuf,
    pub host_key_policy: String,
    pub connect_timeout_seconds: u64,
    pub command_timeout_seconds: u64,
    pub max_output_bytes: usize,
    pub commands: BTreeMap<String, RemoteCommandTemplateSettings>,
}

#[derive(Debug, Clone)]
pub struct RemoteCommandTemplateSettings {
    pub command_id: String,
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    pub argv: Vec<String>,
    pub timeout_seconds: Option<u64>,
}

impl Default for RemoteExecutionSettings {
    fn default() -> Self {
        Self {
            enabled: default_remote_execution_enabled(),
            ssh_binary: default_ssh_binary(),
            host_key_policy: default_remote_host_key_policy(),
            connect_timeout_seconds: default_remote_connect_timeout(),
            command_timeout_seconds: default_remote_command_timeout(),
            max_output_bytes: default_remote_max_output_bytes(),
            commands: default_remote_command_templates(),
        }
    }
}

#[derive(Clone)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub binary_path: Option<PathBuf>,
    pub binary_max_output_bytes: usize,
    pub model: String,
    pub request_timeout_seconds: u64,
    pub max_input_chars: usize,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeSettings {
    pub command_path: PathBuf,
    pub default_mode: AnalysisMode,
    pub max_session_seconds: u64,
    pub max_output_bytes: usize,
    pub permission_profiles: BTreeMap<AnalysisMode, PermissionProfileSettings>,
}

#[derive(Debug, Clone)]
pub struct PermissionProfileSettings {
    pub name: String,
    pub permission_mode: String,
    pub tools: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub native_bash: bool,
    pub native_edit: bool,
    pub worktree_required: bool,
}

pub const LOGAGENT_MCP_ALLOWED_TOOL_GLOB: &str = "mcp__logagent__*";

#[derive(Debug, Clone)]
pub struct McpSettings {
    pub enabled: bool,
    pub transport: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    Diagnose,
    CodeInvestigation,
    Fix,
}

impl AnalysisMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diagnose => "diagnose",
            Self::CodeInvestigation => "code_investigation",
            Self::Fix => "fix",
        }
    }
}

impl std::str::FromStr for AnalysisMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_analysis_mode(value)
    }
}

impl Default for ClaudeCodeSettings {
    fn default() -> Self {
        Self {
            command_path: PathBuf::from("/usr/bin/claude"),
            default_mode: AnalysisMode::Diagnose,
            max_session_seconds: default_claude_code_max_session_seconds(),
            max_output_bytes: default_claude_code_max_output_bytes(),
            permission_profiles: default_permission_profiles(),
        }
    }
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            transport: "stdio".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisSettings {
    pub max_rounds: u32,
    pub max_llm_calls: u32,
    #[allow(dead_code)]
    pub max_actions: u32,
    #[allow(dead_code)]
    pub max_repeated_action_fingerprints: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EmbeddingSettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub store: String,
}

impl std::fmt::Debug for LlmSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LlmSettings")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("binary_path", &self.binary_path)
            .field("binary_max_output_bytes", &self.binary_max_output_bytes)
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
    Binary,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigFile {
    server: Option<ServerConfig>,
    auth: Option<AuthConfig>,
    storage: Option<StorageConfig>,
    skills: Option<SkillConfig>,
    log_analyzer: Option<LogAnalyzerConfig>,
    #[serde(default)]
    tools: BTreeMap<String, ToolConfig>,
    fetch: Option<FetchConfig>,
    huawei_cloud: Option<HuaweiCloudConfig>,
    remote_execution: Option<RemoteExecutionConfig>,
    llm: Option<LlmConfig>,
    claude_code: Option<ClaudeCodeConfig>,
    mcp: Option<McpConfig>,
    analysis: Option<AnalysisConfig>,
    embedding: Option<EmbeddingConfig>,
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
struct SkillConfig {
    #[serde(default = "default_skills_enabled")]
    enabled: bool,
    #[serde(default = "default_skill_roots")]
    roots: Vec<PathBuf>,
    #[serde(default = "default_max_skill_chars")]
    max_skill_chars: usize,
    #[serde(default = "default_max_reference_chars")]
    max_reference_chars: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct LogAnalyzerConfig {
    #[serde(default = "default_keywords")]
    keywords: Vec<String>,
    #[serde(default = "default_max_matches")]
    max_matches: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ToolConfig {
    #[serde(default = "default_tool_enabled")]
    enabled: bool,
    path: Option<PathBuf>,
    path_env: Option<String>,
    #[serde(default = "default_tool_timeout")]
    timeout_seconds: u64,
    #[serde(default = "default_tool_max_output_bytes")]
    max_output_bytes: usize,
    #[serde(default = "default_tool_max_input_files")]
    max_input_files: usize,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    #[serde(rename = "match")]
    match_settings: ToolMatchConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ToolMatchConfig {
    #[serde(default)]
    file_patterns: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FetchConfig {
    #[serde(default)]
    enabled: bool,
    secret_key_env: Option<String>,
    #[serde(default)]
    allowed_hosts: Vec<String>,
    #[serde(default = "default_fetch_request_timeout")]
    request_timeout_seconds: u64,
    #[serde(default = "default_fetch_max_request_bytes")]
    max_request_bytes: usize,
    #[serde(default = "default_fetch_max_response_bytes")]
    max_response_bytes: usize,
    #[serde(default = "default_fetch_max_redirects")]
    max_redirects: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct HuaweiCloudConfig {
    package_sync: Option<HuaweiPackageSyncConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct HuaweiPackageSyncConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_huawei_package_sync_timeout")]
    timeout_seconds: u64,
    #[serde(default)]
    obs: HuaweiObsConfig,
    #[serde(default)]
    gaussdb: HuaweiGaussDbConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct HuaweiObsConfig {
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    object_prefix: String,
    access_key_env: Option<String>,
    secret_key_env: Option<String>,
    security_token_env: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HuaweiGaussDbConfig {
    #[serde(default)]
    host: String,
    #[serde(default = "default_huawei_gaussdb_port")]
    port: u16,
    #[serde(default)]
    database: String,
    #[serde(default)]
    user: String,
    password_env: Option<String>,
    #[serde(default = "default_huawei_gaussdb_sslmode")]
    sslmode: String,
}

impl Default for HuaweiGaussDbConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_huawei_gaussdb_port(),
            database: String::new(),
            user: String::new(),
            password_env: None,
            sslmode: default_huawei_gaussdb_sslmode(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteExecutionConfig {
    #[serde(default = "default_remote_execution_enabled")]
    enabled: bool,
    #[serde(default = "default_ssh_binary")]
    ssh_binary: PathBuf,
    #[serde(default = "default_remote_host_key_policy")]
    host_key_policy: String,
    #[serde(default = "default_remote_connect_timeout")]
    connect_timeout_seconds: u64,
    #[serde(default = "default_remote_command_timeout")]
    command_timeout_seconds: u64,
    #[serde(default = "default_remote_max_output_bytes")]
    max_output_bytes: usize,
    #[serde(default)]
    commands: BTreeMap<String, RemoteCommandTemplateConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct RemoteCommandTemplateConfig {
    display_name: Option<String>,
    description: Option<String>,
    #[serde(default = "default_remote_command_enabled")]
    enabled: bool,
    argv: Vec<String>,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct LlmConfig {
    #[serde(default = "default_llm_provider")]
    provider: String,
    base_url_env: Option<String>,
    api_key_env: Option<String>,
    binary_path: Option<PathBuf>,
    binary_path_env: Option<String>,
    #[serde(default = "default_llm_binary_max_output_bytes")]
    binary_max_output_bytes: usize,
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

#[derive(Debug, Clone, Deserialize)]
struct ClaudeCodeConfig {
    command_path: Option<PathBuf>,
    command_path_env: Option<String>,
    #[serde(default = "default_claude_code_mode")]
    default_mode: AnalysisMode,
    #[serde(default = "default_claude_code_max_session_seconds")]
    max_session_seconds: u64,
    #[serde(default = "default_claude_code_max_output_bytes")]
    max_output_bytes: usize,
    #[serde(default)]
    permission_profiles: BTreeMap<String, PermissionProfileConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct PermissionProfileConfig {
    name: Option<String>,
    #[serde(default = "default_permission_mode")]
    permission_mode: String,
    #[serde(default = "default_permission_tools")]
    tools: String,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    disallowed_tools: Vec<String>,
    #[serde(default)]
    native_bash: bool,
    #[serde(default)]
    native_edit: bool,
    #[serde(default)]
    worktree_required: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct McpConfig {
    #[serde(default = "default_mcp_enabled")]
    enabled: bool,
    #[serde(default = "default_mcp_transport")]
    transport: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AnalysisConfig {
    #[serde(default = "default_analysis_max_rounds")]
    max_rounds: u32,
    #[serde(default = "default_analysis_max_llm_calls")]
    max_llm_calls: u32,
    #[serde(default = "default_analysis_max_actions")]
    max_actions: u32,
    #[serde(default = "default_analysis_max_repeated_action_fingerprints")]
    max_repeated_action_fingerprints: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_embedding_provider")]
    provider: String,
    #[serde(default = "default_embedding_model")]
    model: String,
    api_key_env: Option<String>,
    #[serde(default = "default_embedding_store")]
    store: String,
}

impl AppConfig {
    pub fn prepare_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(self.storage.uploads_dir())?;
        fs::create_dir_all(self.storage.workspaces_dir())?;
        fs::create_dir_all(self.storage.tasks_dir())?;
        fs::create_dir_all(self.storage.sessions_dir())?;
        fs::create_dir_all(self.storage.session_workspaces_dir())?;
        fs::create_dir_all(self.storage.cases_dir())?;
        fs::create_dir_all(self.storage.memory_dir())?;
        fs::create_dir_all(self.storage.case_imports_dir())?;
        fs::create_dir_all(self.storage.executors_dir())?;
        fs::create_dir_all(self.storage.metadata_dir())?;
        fs::create_dir_all(self.storage.metadata_imports_dir())?;
        fs::create_dir_all(self.storage.system_context_dir())?;
        fs::create_dir_all(self.storage.fetch_dir())?;
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

    pub fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    pub fn session_workspaces_dir(&self) -> PathBuf {
        self.data_dir.join("session_workspaces")
    }

    pub fn cases_dir(&self) -> PathBuf {
        self.data_dir.join("cases")
    }

    pub fn memory_dir(&self) -> PathBuf {
        self.data_dir.join("memory")
    }

    pub fn memory_db_path(&self) -> PathBuf {
        self.memory_dir().join("memory.sqlite")
    }

    pub fn case_imports_dir(&self) -> PathBuf {
        self.data_dir.join("case_imports")
    }

    pub fn executors_dir(&self) -> PathBuf {
        self.data_dir.join("executors")
    }

    pub fn metadata_dir(&self) -> PathBuf {
        self.data_dir.join("metadata")
    }

    pub fn metadata_imports_dir(&self) -> PathBuf {
        self.metadata_dir().join("imports")
    }

    pub fn system_context_dir(&self) -> PathBuf {
        self.data_dir.join("system_context").join("resources")
    }

    pub fn fetch_dir(&self) -> PathBuf {
        self.data_dir.join("fetch")
    }
}

pub fn load_config(path: &std::path::Path) -> anyhow::Result<Arc<AppConfig>> {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let parsed: ConfigFile = if raw.trim().is_empty() {
        ConfigFile {
            server: None,
            auth: None,
            storage: None,
            skills: None,
            log_analyzer: None,
            tools: BTreeMap::new(),
            fetch: None,
            huawei_cloud: None,
            remote_execution: None,
            llm: None,
            claude_code: None,
            mcp: None,
            analysis: None,
            embedding: None,
        }
    } else {
        serde_yaml::from_str(&raw).context("invalid YAML")?
    };

    let server = parsed.server.unwrap_or_else(default_server_config);
    let auth = parsed.auth.unwrap_or_else(default_auth_config);
    let storage = parsed.storage.unwrap_or_else(default_storage_config);
    let config_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let config_dir = config_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let skills = resolve_skills(
        parsed.skills.unwrap_or_else(default_skill_config),
        &config_dir,
    )?;
    let analyzer = parsed
        .log_analyzer
        .unwrap_or_else(default_log_analyzer_config);
    let tools = resolve_tools(parsed.tools)?;
    let fetch = resolve_fetch(parsed.fetch.unwrap_or_else(default_fetch_config))?;
    let huawei_cloud = resolve_huawei_cloud(
        parsed
            .huawei_cloud
            .unwrap_or_else(default_huawei_cloud_config),
    )?;
    let remote_execution = resolve_remote_execution(
        parsed
            .remote_execution
            .unwrap_or_else(default_remote_execution_config),
    )?;
    let llm = parsed.llm.unwrap_or_else(default_llm_config);
    let claude_code = resolve_claude_code(parsed.claude_code)?;
    let mcp = resolve_mcp(parsed.mcp.unwrap_or_else(default_mcp_config))?;
    let analysis = parsed.analysis.unwrap_or_else(default_analysis_config);
    let embedding = parsed.embedding.unwrap_or_else(default_embedding_config);

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
        "binary" => LlmProvider::Binary,
        value => anyhow::bail!("unsupported llm.provider {value}"),
    };
    let model = resolve_llm_model(&llm)?;
    let (base_url, api_key, binary_path) = match provider {
        LlmProvider::Stub => (None, None, None),
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
                None,
            )
        }
        LlmProvider::Binary => (None, None, Some(resolve_llm_binary_path(&llm)?)),
    };

    Ok(Arc::new(AppConfig {
        config_path,
        server: ServerSettings {
            bind: server.bind,
            public_base_url: server.public_base_url,
            max_concurrent_tasks: server.max_concurrent_tasks.max(1),
        },
        auth: AuthSettings { api_keys },
        storage: StorageSettings {
            data_dir: expand_path_env_vars(storage.data_dir)?,
            max_upload_bytes: storage.max_upload_bytes,
            max_chunk_bytes: storage.max_chunk_bytes,
        },
        skills,
        log_analyzer: LogAnalyzerSettings {
            keywords: analyzer
                .keywords
                .into_iter()
                .map(|keyword| keyword.to_ascii_lowercase())
                .collect(),
            max_matches: analyzer.max_matches,
        },
        tools,
        fetch,
        huawei_cloud,
        remote_execution,
        llm: LlmSettings {
            provider,
            base_url,
            api_key,
            binary_path,
            binary_max_output_bytes: llm.binary_max_output_bytes.max(1024),
            model,
            request_timeout_seconds: llm.request_timeout_seconds.max(1),
            max_input_chars: llm.max_input_chars.max(1024),
            max_output_tokens: llm.max_output_tokens.max(1),
        },
        claude_code,
        mcp,
        analysis: AnalysisSettings {
            max_rounds: analysis.max_rounds.max(1),
            max_llm_calls: analysis.max_llm_calls.max(1),
            max_actions: analysis.max_actions.max(1),
            max_repeated_action_fingerprints: analysis.max_repeated_action_fingerprints.max(1),
        },
        embedding: EmbeddingSettings {
            enabled: embedding.enabled,
            provider: embedding.provider,
            model: embedding.model,
            api_key_env: embedding.api_key_env,
            store: embedding.store,
        },
    }))
}

fn resolve_claude_code(raw: Option<ClaudeCodeConfig>) -> anyhow::Result<ClaudeCodeSettings> {
    let raw = raw.unwrap_or_else(default_claude_code_config);
    let path = if let Some(path) = &raw.command_path {
        path.clone()
    } else {
        let path_env = raw
            .command_path_env
            .as_deref()
            .context("claude_code.command_path or command_path_env is required")?;
        let value = env::var(path_env)
            .with_context(|| format!("missing Claude Code command path env var {path_env}"))?;
        let value = value.trim();
        if value.is_empty() {
            anyhow::bail!("Claude Code command path env var {path_env} must not be empty");
        }
        PathBuf::from(value)
    };
    if !path.is_absolute() {
        anyhow::bail!("claude_code.command_path must be absolute");
    }
    let mut permission_profiles = default_permission_profiles();
    for (mode, profile) in raw.permission_profiles {
        let mode = parse_analysis_mode(&mode)
            .with_context(|| format!("invalid claude_code.permission_profiles key {mode}"))?;
        permission_profiles.insert(mode, resolve_permission_profile(mode, profile)?);
    }
    if !permission_profiles.contains_key(&raw.default_mode) {
        anyhow::bail!(
            "claude_code.default_mode {} has no permission profile",
            raw.default_mode.as_str()
        );
    }
    Ok(ClaudeCodeSettings {
        command_path: path,
        default_mode: raw.default_mode,
        max_session_seconds: raw.max_session_seconds.max(1),
        max_output_bytes: raw.max_output_bytes.max(1024),
        permission_profiles,
    })
}

fn resolve_permission_profile(
    mode: AnalysisMode,
    raw: PermissionProfileConfig,
) -> anyhow::Result<PermissionProfileSettings> {
    let permission_mode = raw.permission_mode.trim();
    if permission_mode.is_empty() {
        anyhow::bail!(
            "permission profile {} has empty permission_mode",
            mode.as_str()
        );
    }
    Ok(PermissionProfileSettings {
        name: raw.name.unwrap_or_else(|| mode.as_str().to_string()),
        permission_mode: permission_mode.to_string(),
        tools: raw.tools,
        allowed_tools: with_logagent_mcp_allowed_tools(
            raw.allowed_tools
                .into_iter()
                .map(|tool| tool.trim().to_string())
                .filter(|tool| !tool.is_empty())
                .collect(),
        ),
        disallowed_tools: raw
            .disallowed_tools
            .into_iter()
            .map(|tool| tool.trim().to_string())
            .filter(|tool| !tool.is_empty())
            .collect(),
        native_bash: raw.native_bash,
        native_edit: raw.native_edit,
        worktree_required: raw.worktree_required,
    })
}

fn resolve_mcp(raw: McpConfig) -> anyhow::Result<McpSettings> {
    let transport = raw.transport.trim();
    if transport != "stdio" {
        anyhow::bail!("mcp.transport currently only supports stdio");
    }
    Ok(McpSettings {
        enabled: raw.enabled,
        transport: transport.to_string(),
    })
}

fn resolve_fetch(raw: FetchConfig) -> anyhow::Result<FetchSettings> {
    let allowed_hosts = raw
        .allowed_hosts
        .iter()
        .map(|value| parse_fetch_allowed_host(value))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let secret_key_env = raw
        .secret_key_env
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let secret_key = if raw.enabled {
        if allowed_hosts.is_empty() {
            anyhow::bail!("fetch.allowed_hosts must not be empty when fetch is enabled");
        }
        let secret_key_env = secret_key_env
            .as_deref()
            .context("fetch.secret_key_env is required when fetch is enabled")?;
        let encoded = env::var(secret_key_env)
            .with_context(|| format!("missing fetch secret key env var {secret_key_env}"))?;
        let decoded = BASE64
            .decode(encoded.trim())
            .with_context(|| format!("fetch secret key env var {secret_key_env} is not base64"))?;
        let key: [u8; 32] = decoded.try_into().map_err(|value: Vec<u8>| {
            anyhow::anyhow!(
                "fetch secret key env var {secret_key_env} must decode to 32 bytes, got {}",
                value.len()
            )
        })?;
        Some(key)
    } else {
        None
    };
    Ok(FetchSettings {
        enabled: raw.enabled,
        secret_key_env,
        secret_key,
        allowed_hosts,
        request_timeout_seconds: raw.request_timeout_seconds.max(1),
        max_request_bytes: raw.max_request_bytes.max(1),
        max_response_bytes: raw.max_response_bytes.max(1),
        max_redirects: raw.max_redirects,
    })
}

fn resolve_huawei_cloud(raw: HuaweiCloudConfig) -> anyhow::Result<HuaweiCloudSettings> {
    let package_sync = resolve_huawei_package_sync(
        raw.package_sync
            .unwrap_or_else(default_huawei_package_sync_config),
    )?;
    Ok(HuaweiCloudSettings { package_sync })
}

fn resolve_huawei_package_sync(
    raw: HuaweiPackageSyncConfig,
) -> anyhow::Result<HuaweiPackageSyncSettings> {
    let enabled = raw.enabled;
    let endpoint = raw.obs.endpoint.trim().trim_end_matches('/').to_string();
    let bucket = raw.obs.bucket.trim().to_string();
    let object_prefix = normalize_huawei_object_prefix(&raw.obs.object_prefix)?;
    let access_key_env = non_empty_optional(raw.obs.access_key_env);
    let secret_key_env = non_empty_optional(raw.obs.secret_key_env);
    let security_token_env = non_empty_optional(raw.obs.security_token_env);
    let host = raw.gaussdb.host.trim().to_string();
    let database = raw.gaussdb.database.trim().to_string();
    let user = raw.gaussdb.user.trim().to_string();
    let password_env = non_empty_optional(raw.gaussdb.password_env);
    let sslmode = raw.gaussdb.sslmode.trim().to_ascii_lowercase();

    let (access_key, secret_key, security_token, password) = if enabled {
        if endpoint.is_empty() {
            anyhow::bail!("huawei_cloud.package_sync.obs.endpoint is required when enabled");
        }
        let parsed_endpoint = reqwest::Url::parse(&endpoint)
            .with_context(|| format!("invalid Huawei OBS endpoint {endpoint}"))?;
        if !matches!(parsed_endpoint.scheme(), "http" | "https") {
            anyhow::bail!("huawei_cloud.package_sync.obs.endpoint must use http or https");
        }
        if parsed_endpoint.host_str().is_none() {
            anyhow::bail!("huawei_cloud.package_sync.obs.endpoint must include host");
        }
        if parsed_endpoint.path() != "/" {
            anyhow::bail!("huawei_cloud.package_sync.obs.endpoint must not include a path");
        }
        if !parsed_endpoint.username().is_empty()
            || parsed_endpoint.password().is_some()
            || parsed_endpoint.query().is_some()
            || parsed_endpoint.fragment().is_some()
        {
            anyhow::bail!(
                "huawei_cloud.package_sync.obs.endpoint must not include credentials, query, or fragment"
            );
        }
        if bucket.is_empty() {
            anyhow::bail!("huawei_cloud.package_sync.obs.bucket is required when enabled");
        }
        if !is_valid_huawei_bucket_name(&bucket) {
            anyhow::bail!("huawei_cloud.package_sync.obs.bucket contains unsupported characters");
        }
        let access_key_env = access_key_env
            .as_deref()
            .context("huawei_cloud.package_sync.obs.access_key_env is required when enabled")?;
        let secret_key_env = secret_key_env
            .as_deref()
            .context("huawei_cloud.package_sync.obs.secret_key_env is required when enabled")?;
        if host.is_empty() {
            anyhow::bail!("huawei_cloud.package_sync.gaussdb.host is required when enabled");
        }
        if database.is_empty() {
            anyhow::bail!("huawei_cloud.package_sync.gaussdb.database is required when enabled");
        }
        if user.is_empty() {
            anyhow::bail!("huawei_cloud.package_sync.gaussdb.user is required when enabled");
        }
        let password_env = password_env
            .as_deref()
            .context("huawei_cloud.package_sync.gaussdb.password_env is required when enabled")?;
        if sslmode != "disable" {
            anyhow::bail!(
                "huawei_cloud.package_sync.gaussdb.sslmode currently only supports disable"
            );
        }
        (
            Some(resolve_required_env(
                access_key_env,
                "Huawei OBS access key",
            )?),
            Some(resolve_required_env(
                secret_key_env,
                "Huawei OBS secret key",
            )?),
            match security_token_env.as_deref() {
                Some(env_name) => {
                    Some(resolve_required_env(env_name, "Huawei OBS security token")?)
                }
                None => None,
            },
            Some(resolve_required_env(
                password_env,
                "Huawei GaussDB password",
            )?),
        )
    } else {
        (None, None, None, None)
    };

    Ok(HuaweiPackageSyncSettings {
        enabled,
        timeout_seconds: raw.timeout_seconds.max(1),
        obs: HuaweiObsSettings {
            endpoint,
            bucket,
            object_prefix,
            access_key_env,
            access_key,
            secret_key_env,
            secret_key,
            security_token_env,
            security_token,
        },
        gaussdb: HuaweiGaussDbSettings {
            host,
            port: raw.gaussdb.port,
            database,
            user,
            password_env,
            password,
            sslmode,
        },
    })
}

fn non_empty_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_required_env(env_name: &str, label: &str) -> anyhow::Result<String> {
    let value =
        env::var(env_name).with_context(|| format!("missing {label} env var {env_name}"))?;
    let value = value.trim().to_string();
    if value.is_empty() {
        anyhow::bail!("{label} env var {env_name} must not be empty");
    }
    Ok(value)
}

fn normalize_huawei_object_prefix(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    validate_huawei_object_key(trimmed)
        .with_context(|| "invalid huawei_cloud.package_sync.obs.object_prefix")?;
    Ok(trimmed.to_string())
}

pub fn validate_huawei_object_key(value: &str) -> anyhow::Result<()> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("object key must not be empty");
    }
    if value.len() > 1024 {
        anyhow::bail!("object key must be at most 1024 bytes");
    }
    if value.starts_with('/') || value.contains('\\') || value.contains('?') || value.contains('#')
    {
        anyhow::bail!("object key must be relative and must not contain \\, ?, or #");
    }
    if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        anyhow::bail!("object key must not contain empty, . or .. path segments");
    }
    if value.chars().any(char::is_control) {
        anyhow::bail!("object key must not contain control characters");
    }
    Ok(())
}

fn is_valid_huawei_bucket_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'.' || ch == b'-')
}

fn parse_fetch_allowed_host(raw: &str) -> anyhow::Result<FetchAllowedHost> {
    let raw = raw.trim();
    if raw.is_empty() {
        anyhow::bail!("fetch.allowed_hosts entries must not be empty");
    }
    if raw.contains("://") {
        let url = reqwest::Url::parse(raw)
            .with_context(|| format!("invalid fetch allowed host {raw}"))?;
        let scheme = url.scheme();
        if !matches!(scheme, "http" | "https") {
            anyhow::bail!("fetch allowed host scheme must be http or https");
        }
        let host = url
            .host_str()
            .context("fetch allowed host URL is missing host")?
            .to_ascii_lowercase();
        return Ok(FetchAllowedHost {
            scheme: Some(scheme.to_string()),
            host,
            port: url.port_or_known_default(),
        });
    }
    let (host, port) = match raw.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = port
                .parse::<u16>()
                .with_context(|| format!("invalid fetch allowed host port in {raw}"))?;
            (host, Some(port))
        }
        _ => (raw, None),
    };
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() || host == "*" {
        anyhow::bail!("fetch allowed host must be an explicit host");
    }
    Ok(FetchAllowedHost {
        scheme: None,
        host,
        port,
    })
}

pub fn parse_analysis_mode(value: &str) -> anyhow::Result<AnalysisMode> {
    match value {
        "diagnose" => Ok(AnalysisMode::Diagnose),
        "code_investigation" => Ok(AnalysisMode::CodeInvestigation),
        "fix" => Ok(AnalysisMode::Fix),
        value => anyhow::bail!("unsupported analysis mode {value}"),
    }
}

fn resolve_tools(raw: BTreeMap<String, ToolConfig>) -> anyhow::Result<ToolsSettings> {
    let mut tools = BTreeMap::new();
    for (name, tool) in raw {
        validate_tool_name(&name)?;
        let path = resolve_tool_path(&name, &tool)?;
        if tool.enabled {
            if !path.is_absolute() {
                anyhow::bail!("tools.{name}.path must be absolute");
            }
        }
        tools.insert(
            name.clone(),
            ToolSettings {
                name,
                enabled: tool.enabled,
                path,
                timeout_seconds: tool.timeout_seconds.max(1),
                max_output_bytes: tool.max_output_bytes.max(1024),
                max_input_files: tool.max_input_files.max(1),
                args: tool.args,
                match_settings: ToolMatchSettings {
                    file_patterns: tool
                        .match_settings
                        .file_patterns
                        .into_iter()
                        .map(|pattern| pattern.to_ascii_lowercase())
                        .collect(),
                    keywords: tool
                        .match_settings
                        .keywords
                        .into_iter()
                        .map(|keyword| keyword.to_ascii_lowercase())
                        .collect(),
                },
            },
        );
    }
    Ok(ToolsSettings { tools })
}

fn resolve_remote_execution(raw: RemoteExecutionConfig) -> anyhow::Result<RemoteExecutionSettings> {
    if raw.enabled && !raw.ssh_binary.is_absolute() {
        anyhow::bail!("remote_execution.ssh_binary must be absolute when enabled");
    }
    let host_key_policy = raw.host_key_policy.trim();
    if !matches!(host_key_policy, "accept-new" | "strict" | "no") {
        anyhow::bail!("remote_execution.host_key_policy must be one of accept-new, strict, or no");
    }
    let commands = if raw.commands.is_empty() {
        default_remote_command_templates()
    } else {
        raw.commands
            .into_iter()
            .map(|(command_id, command)| {
                validate_remote_command_id(&command_id)?;
                let argv = command
                    .argv
                    .into_iter()
                    .map(|arg| arg.trim().to_string())
                    .filter(|arg| !arg.is_empty())
                    .collect::<Vec<_>>();
                if argv.is_empty() {
                    anyhow::bail!("remote_execution.commands.{command_id}.argv must not be empty");
                }
                Ok((
                    command_id.clone(),
                    RemoteCommandTemplateSettings {
                        display_name: command
                            .display_name
                            .unwrap_or_else(|| command_id.replace('_', " ")),
                        description: command.description.unwrap_or_default(),
                        enabled: command.enabled,
                        argv,
                        timeout_seconds: command.timeout_seconds.map(|value| value.max(1)),
                        command_id,
                    },
                ))
            })
            .collect::<anyhow::Result<BTreeMap<_, _>>>()?
    };
    Ok(RemoteExecutionSettings {
        enabled: raw.enabled,
        ssh_binary: raw.ssh_binary,
        host_key_policy: host_key_policy.to_string(),
        connect_timeout_seconds: raw.connect_timeout_seconds.max(1),
        command_timeout_seconds: raw.command_timeout_seconds.max(1),
        max_output_bytes: raw.max_output_bytes.max(1024),
        commands,
    })
}

fn resolve_skills(raw: SkillConfig, config_dir: &std::path::Path) -> anyhow::Result<SkillSettings> {
    let mut roots = Vec::new();
    for root in raw.roots {
        let root = expand_path_env_vars(root)?;
        let root = if root.is_absolute() {
            root
        } else {
            let config_relative = config_dir.join(&root);
            if config_relative.exists() {
                config_relative
            } else {
                std::env::current_dir()?.join(root)
            }
        };
        if !roots.iter().any(|existing| existing == &root) {
            roots.push(root);
        }
    }
    Ok(SkillSettings {
        enabled: raw.enabled,
        roots,
        max_skill_chars: raw.max_skill_chars.clamp(200, 40_000),
        max_reference_chars: raw.max_reference_chars.clamp(200, 80_000),
    })
}

fn resolve_tool_path(name: &str, tool: &ToolConfig) -> anyhow::Result<PathBuf> {
    if let Some(path) = &tool.path {
        return expand_path_env_vars(path.clone());
    }
    if !tool.enabled {
        return Ok(PathBuf::new());
    }
    if let Some(path_env) = tool.path_env.as_deref() {
        let value = env::var(path_env)
            .with_context(|| format!("missing tool path env var {path_env} for tools.{name}"))?;
        let value = value.trim();
        if value.is_empty() {
            anyhow::bail!("tool path env var {path_env} for tools.{name} must not be empty");
        }
        return Ok(PathBuf::from(value));
    }
    anyhow::bail!("tools.{name}.path or tools.{name}.path_env is required when enabled")
}

fn expand_path_env_vars(path: PathBuf) -> anyhow::Result<PathBuf> {
    let raw = path
        .to_str()
        .context("storage.data_dir must be valid UTF-8 when using config env expansion")?;
    Ok(PathBuf::from(expand_env_vars_with(raw, |name| {
        env::var(name)
    })?))
}

fn expand_env_vars_with(
    value: &str,
    read_env: impl Fn(&str) -> Result<String, env::VarError>,
) -> anyhow::Result<String> {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find('}') else {
            anyhow::bail!("unclosed environment variable placeholder in config value {value}");
        };
        let name = &after_start[..end];
        if name.trim().is_empty() {
            anyhow::bail!("empty environment variable placeholder in config value {value}");
        }
        let replacement = read_env(name)
            .with_context(|| format!("missing config environment variable {name}"))?;
        if replacement.trim().is_empty() {
            anyhow::bail!("config environment variable {name} must not be empty");
        }
        output.push_str(&replacement);
        remaining = &after_start[end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn resolve_llm_binary_path(llm: &LlmConfig) -> anyhow::Result<PathBuf> {
    let path = if let Some(path) = &llm.binary_path {
        path.clone()
    } else {
        let binary_path_env = llm
            .binary_path_env
            .as_deref()
            .context("llm.binary_path or llm.binary_path_env is required for binary provider")?;
        let value = env::var(binary_path_env)
            .with_context(|| format!("missing LLM binary path env var {binary_path_env}"))?;
        let value = value.trim();
        if value.is_empty() {
            anyhow::bail!("LLM binary path env var {binary_path_env} must not be empty");
        }
        PathBuf::from(value)
    };
    if !path.is_absolute() {
        anyhow::bail!("llm.binary_path must be absolute for binary provider");
    }
    Ok(path)
}

fn validate_tool_name(name: &str) -> anyhow::Result<()> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid tool name {name}")
    }
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

fn default_skill_config() -> SkillConfig {
    SkillConfig {
        enabled: default_skills_enabled(),
        roots: default_skill_roots(),
        max_skill_chars: default_max_skill_chars(),
        max_reference_chars: default_max_reference_chars(),
    }
}

fn default_log_analyzer_config() -> LogAnalyzerConfig {
    LogAnalyzerConfig {
        keywords: default_keywords(),
        max_matches: default_max_matches(),
    }
}

fn default_fetch_config() -> FetchConfig {
    FetchConfig {
        enabled: false,
        secret_key_env: Some("LOGAGENT_FETCH_SECRET_KEY".to_string()),
        allowed_hosts: Vec::new(),
        request_timeout_seconds: default_fetch_request_timeout(),
        max_request_bytes: default_fetch_max_request_bytes(),
        max_response_bytes: default_fetch_max_response_bytes(),
        max_redirects: default_fetch_max_redirects(),
    }
}

fn default_huawei_cloud_config() -> HuaweiCloudConfig {
    HuaweiCloudConfig {
        package_sync: Some(default_huawei_package_sync_config()),
    }
}

fn default_huawei_package_sync_config() -> HuaweiPackageSyncConfig {
    HuaweiPackageSyncConfig {
        enabled: false,
        timeout_seconds: default_huawei_package_sync_timeout(),
        obs: HuaweiObsConfig::default(),
        gaussdb: HuaweiGaussDbConfig::default(),
    }
}

fn default_llm_config() -> LlmConfig {
    LlmConfig {
        provider: default_llm_provider(),
        base_url_env: None,
        api_key_env: None,
        binary_path: None,
        binary_path_env: None,
        binary_max_output_bytes: default_llm_binary_max_output_bytes(),
        model_env: None,
        model: default_llm_model(),
        request_timeout_seconds: default_llm_timeout(),
        max_input_chars: default_llm_max_input_chars(),
        max_output_tokens: default_llm_max_output_tokens(),
    }
}

fn default_claude_code_config() -> ClaudeCodeConfig {
    ClaudeCodeConfig {
        command_path: None,
        command_path_env: Some("LOGAGENT_CLAUDE_CODE_PATH".to_string()),
        default_mode: default_claude_code_mode(),
        max_session_seconds: default_claude_code_max_session_seconds(),
        max_output_bytes: default_claude_code_max_output_bytes(),
        permission_profiles: BTreeMap::new(),
    }
}

fn default_mcp_config() -> McpConfig {
    McpConfig {
        enabled: default_mcp_enabled(),
        transport: default_mcp_transport(),
    }
}

fn default_analysis_config() -> AnalysisConfig {
    AnalysisConfig {
        max_rounds: default_analysis_max_rounds(),
        max_llm_calls: default_analysis_max_llm_calls(),
        max_actions: default_analysis_max_actions(),
        max_repeated_action_fingerprints: default_analysis_max_repeated_action_fingerprints(),
    }
}

fn default_remote_execution_config() -> RemoteExecutionConfig {
    RemoteExecutionConfig {
        enabled: default_remote_execution_enabled(),
        ssh_binary: default_ssh_binary(),
        host_key_policy: default_remote_host_key_policy(),
        connect_timeout_seconds: default_remote_connect_timeout(),
        command_timeout_seconds: default_remote_command_timeout(),
        max_output_bytes: default_remote_max_output_bytes(),
        commands: BTreeMap::new(),
    }
}

fn default_embedding_config() -> EmbeddingConfig {
    EmbeddingConfig {
        enabled: false,
        provider: default_embedding_provider(),
        model: default_embedding_model(),
        api_key_env: None,
        store: default_embedding_store(),
    }
}

fn default_tool_enabled() -> bool {
    true
}

fn default_tool_timeout() -> u64 {
    30
}

fn default_tool_max_output_bytes() -> usize {
    1024 * 1024
}

fn default_tool_max_input_files() -> usize {
    1
}

fn default_fetch_request_timeout() -> u64 {
    30
}

fn default_fetch_max_request_bytes() -> usize {
    1024 * 1024
}

fn default_fetch_max_response_bytes() -> usize {
    2 * 1024 * 1024
}

fn default_fetch_max_redirects() -> usize {
    3
}

fn default_huawei_package_sync_timeout() -> u64 {
    60
}

fn default_huawei_gaussdb_port() -> u16 {
    8000
}

fn default_huawei_gaussdb_sslmode() -> String {
    "disable".to_string()
}

fn default_remote_execution_enabled() -> bool {
    true
}

fn default_ssh_binary() -> PathBuf {
    PathBuf::from("/usr/bin/ssh")
}

fn default_remote_host_key_policy() -> String {
    "accept-new".to_string()
}

fn default_remote_connect_timeout() -> u64 {
    10
}

fn default_remote_command_timeout() -> u64 {
    30
}

fn default_remote_max_output_bytes() -> usize {
    1024 * 1024
}

fn default_remote_command_enabled() -> bool {
    true
}

fn default_remote_command_templates() -> BTreeMap<String, RemoteCommandTemplateSettings> {
    BTreeMap::from([(
        "smoke_ls_root".to_string(),
        RemoteCommandTemplateSettings {
            command_id: "smoke_ls_root".to_string(),
            display_name: "Smoke: list /root".to_string(),
            description: "Run a low-risk ls command to verify SSH execution.".to_string(),
            enabled: true,
            argv: vec!["ls".to_string(), "-la".to_string(), "/root".to_string()],
            timeout_seconds: Some(10),
        },
    )])
}

fn validate_remote_command_id(command_id: &str) -> anyhow::Result<()> {
    let valid = !command_id.is_empty()
        && command_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid remote command id {command_id}")
    }
}

fn default_claude_code_max_session_seconds() -> u64 {
    600
}

fn default_claude_code_max_output_bytes() -> usize {
    1024 * 1024
}

fn default_claude_code_mode() -> AnalysisMode {
    AnalysisMode::Diagnose
}

fn default_mcp_enabled() -> bool {
    true
}

fn default_mcp_transport() -> String {
    "stdio".to_string()
}

fn default_permission_mode() -> String {
    "dontAsk".to_string()
}

fn default_permission_tools() -> String {
    String::new()
}

fn default_permission_profiles() -> BTreeMap<AnalysisMode, PermissionProfileSettings> {
    BTreeMap::from([
        (
            AnalysisMode::Diagnose,
            PermissionProfileSettings {
                name: "diagnose".to_string(),
                permission_mode: "dontAsk".to_string(),
                tools: String::new(),
                allowed_tools: logagent_mcp_allowed_tools(),
                disallowed_tools: vec![
                    "Bash".to_string(),
                    "Edit".to_string(),
                    "Write".to_string(),
                    "Read".to_string(),
                    "Grep".to_string(),
                ],
                native_bash: false,
                native_edit: false,
                worktree_required: false,
            },
        ),
        (
            AnalysisMode::CodeInvestigation,
            PermissionProfileSettings {
                name: "code_investigation".to_string(),
                permission_mode: "dontAsk".to_string(),
                tools: "Read,Grep,Bash".to_string(),
                allowed_tools: with_logagent_mcp_allowed_tools(vec![
                    "Read".to_string(),
                    "Grep".to_string(),
                    "Bash".to_string(),
                ]),
                disallowed_tools: vec!["Edit".to_string(), "Write".to_string()],
                native_bash: true,
                native_edit: false,
                worktree_required: false,
            },
        ),
        (
            AnalysisMode::Fix,
            PermissionProfileSettings {
                name: "fix".to_string(),
                permission_mode: "acceptEdits".to_string(),
                tools: "Read,Grep,Bash,Edit,Write".to_string(),
                allowed_tools: with_logagent_mcp_allowed_tools(vec![
                    "Read".to_string(),
                    "Grep".to_string(),
                    "Bash".to_string(),
                    "Edit".to_string(),
                    "Write".to_string(),
                ]),
                disallowed_tools: Vec::new(),
                native_bash: true,
                native_edit: true,
                worktree_required: true,
            },
        ),
    ])
}

fn logagent_mcp_allowed_tools() -> Vec<String> {
    vec![LOGAGENT_MCP_ALLOWED_TOOL_GLOB.to_string()]
}

fn with_logagent_mcp_allowed_tools(mut allowed_tools: Vec<String>) -> Vec<String> {
    if !allowed_tools
        .iter()
        .any(|tool| tool == LOGAGENT_MCP_ALLOWED_TOOL_GLOB)
    {
        allowed_tools.push(LOGAGENT_MCP_ALLOWED_TOOL_GLOB.to_string());
    }
    allowed_tools
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

fn default_skills_enabled() -> bool {
    true
}

fn default_skill_roots() -> Vec<PathBuf> {
    vec![PathBuf::from("skills")]
}

fn default_max_skill_chars() -> usize {
    4000
}

fn default_max_reference_chars() -> usize {
    20_000
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

fn default_llm_binary_max_output_bytes() -> usize {
    1024 * 1024
}

fn default_analysis_max_rounds() -> u32 {
    4
}

fn default_analysis_max_llm_calls() -> u32 {
    4
}

fn default_analysis_max_actions() -> u32 {
    6
}

fn default_analysis_max_repeated_action_fingerprints() -> u32 {
    1
}

fn default_embedding_provider() -> String {
    "openai_compatible".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_embedding_store() -> String {
    "sqlite".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llm_config(model: &str, model_env: Option<&str>) -> LlmConfig {
        LlmConfig {
            provider: "openai_compatible".to_string(),
            base_url_env: Some("BASE_URL".to_string()),
            api_key_env: Some("API_KEY".to_string()),
            binary_path: None,
            binary_path_env: None,
            binary_max_output_bytes: default_llm_binary_max_output_bytes(),
            model_env: model_env.map(ToString::to_string),
            model: model.to_string(),
            request_timeout_seconds: 120,
            max_input_chars: 60_000,
            max_output_tokens: 4096,
        }
    }

    #[test]
    fn resolves_default_claude_code_config() {
        temp_env_set("LOGAGENT_CLAUDE_CODE_PATH", "/opt/bin/claude", || {
            let settings = resolve_claude_code(None).unwrap();

            assert_eq!(settings.command_path, PathBuf::from("/opt/bin/claude"));
            assert_eq!(settings.default_mode, AnalysisMode::Diagnose);
            assert_eq!(settings.max_session_seconds, 600);
            let diagnose = settings
                .permission_profiles
                .get(&AnalysisMode::Diagnose)
                .unwrap();
            assert_eq!(diagnose.permission_mode, "dontAsk");
            assert_eq!(
                diagnose.allowed_tools,
                vec![LOGAGENT_MCP_ALLOWED_TOOL_GLOB.to_string()]
            );
            assert!(!diagnose.native_bash);
            assert!(!diagnose.native_edit);
        });
    }

    #[test]
    fn rejects_missing_default_claude_code_config() {
        let parsed = serde_yaml::from_str::<ConfigFile>(
            r#"
claude_code:
  command_path_env: LOGAGENT_TEST_MISSING_CLAUDE_CODE_PATH
"#,
        )
        .unwrap();
        assert!(resolve_claude_code(parsed.claude_code)
            .unwrap_err()
            .to_string()
            .contains("LOGAGENT_TEST_MISSING_CLAUDE_CODE_PATH"));
    }

    #[test]
    fn resolves_claude_code_permission_profile_override() {
        let parsed = serde_yaml::from_str::<ConfigFile>(
            r#"
claude_code:
  command_path: /opt/bin/claude
  default_mode: code_investigation
  max_session_seconds: 30
  max_output_bytes: 4096
  permission_profiles:
    code_investigation:
      permission_mode: plan
      tools: "Read,Grep,Bash"
      allowed_tools: ["Read", "Grep", "Bash"]
      disallowed_tools: ["Edit", "Write"]
      native_bash: true
"#,
        )
        .unwrap();
        let settings = resolve_claude_code(parsed.claude_code).unwrap();
        let profile = settings
            .permission_profiles
            .get(&AnalysisMode::CodeInvestigation)
            .unwrap();
        assert_eq!(settings.default_mode, AnalysisMode::CodeInvestigation);
        assert_eq!(settings.max_session_seconds, 30);
        assert_eq!(settings.max_output_bytes, 4096);
        assert_eq!(profile.permission_mode, "plan");
        assert_eq!(
            profile.allowed_tools,
            vec!["Read", "Grep", "Bash", LOGAGENT_MCP_ALLOWED_TOOL_GLOB]
        );
        assert_eq!(profile.disallowed_tools, vec!["Edit", "Write"]);
        assert!(profile.native_bash);
        assert!(!profile.native_edit);
    }

    #[test]
    fn rejects_claude_code_relative_command_and_unknown_mcp_transport() {
        let relative = serde_yaml::from_str::<ConfigFile>(
            r#"
claude_code:
  command_path: bin/claude
"#,
        )
        .unwrap();
        assert!(resolve_claude_code(relative.claude_code)
            .unwrap_err()
            .to_string()
            .contains("must be absolute"));

        assert!(resolve_mcp(McpConfig {
            enabled: true,
            transport: "http".to_string(),
        })
        .unwrap_err()
        .to_string()
        .contains("stdio"));
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

    #[test]
    fn analysis_config_has_bounded_defaults() {
        let config = default_analysis_config();

        assert_eq!(config.max_rounds, 4);
        assert_eq!(config.max_llm_calls, 4);
        assert_eq!(config.max_actions, 6);
        assert_eq!(config.max_repeated_action_fingerprints, 1);
    }

    #[test]
    fn expands_config_env_placeholders() {
        let expanded = expand_env_vars_with("${LOGAGENT_APP_DIR}/data", |name| {
            assert_eq!(name, "LOGAGENT_APP_DIR");
            Ok("/opt/logagent".to_string())
        })
        .unwrap();

        assert_eq!(expanded, "/opt/logagent/data");

        let missing = expand_env_vars_with("${MISSING}/data", |_| Err(env::VarError::NotPresent))
            .unwrap_err()
            .to_string();
        assert!(missing.contains("missing config environment variable MISSING"));

        let unclosed = expand_env_vars_with("${BROKEN/data", |_| unreachable!())
            .unwrap_err()
            .to_string();
        assert!(unclosed.contains("unclosed environment variable placeholder"));
    }

    #[test]
    fn resolves_binary_llm_provider_path() {
        let config = LlmConfig {
            provider: "binary".to_string(),
            base_url_env: None,
            api_key_env: None,
            binary_path: Some(PathBuf::from("/opt/logagent/bin/xxx")),
            binary_path_env: None,
            binary_max_output_bytes: 0,
            model_env: None,
            model: "binary-model".to_string(),
            request_timeout_seconds: 120,
            max_input_chars: 60_000,
            max_output_tokens: 4096,
        };

        assert_eq!(
            resolve_llm_binary_path(&config).unwrap(),
            PathBuf::from("/opt/logagent/bin/xxx")
        );

        let relative = LlmConfig {
            binary_path: Some(PathBuf::from("xxx")),
            ..config.clone()
        };
        assert!(resolve_llm_binary_path(&relative)
            .unwrap_err()
            .to_string()
            .contains("must be absolute"));
    }

    #[test]
    fn resolves_tool_config_and_rejects_unsafe_values() {
        let valid = serde_yaml::from_str::<ConfigFile>(
            r#"
tools:
  flux_query_analyzer:
    path: /opt/logagent/tools/flux_query_analyzer
    timeout_seconds: 5
    max_output_bytes: 2048
    max_input_files: 3
    args: ["--input", "{input_file}"]
    match:
      file_patterns: ["*.log"]
      keywords: ["Flux"]
"#,
        )
        .unwrap();
        let tools = resolve_tools(valid.tools).unwrap();
        let tool = tools.tools.get("flux_query_analyzer").unwrap();
        assert!(tool.enabled);
        assert_eq!(tool.timeout_seconds, 5);
        assert_eq!(tool.max_output_bytes, 2048);
        assert_eq!(tool.max_input_files, 3);
        assert_eq!(tool.match_settings.keywords, vec!["flux"]);

        let relative = serde_yaml::from_str::<ConfigFile>(
            r#"
tools:
  bad:
    path: relative/tool
"#,
        )
        .unwrap();
        assert!(resolve_tools(relative.tools)
            .unwrap_err()
            .to_string()
            .contains("must be absolute"));

        let invalid_name = serde_yaml::from_str::<ConfigFile>(
            r#"
tools:
  "bad/tool":
    path: /tmp/tool
"#,
        )
        .unwrap();
        assert!(resolve_tools(invalid_name.tools)
            .unwrap_err()
            .to_string()
            .contains("invalid tool name"));
    }

    #[test]
    fn resolves_tool_path_env_and_ignores_disabled_tool_env() {
        let env_name = "LOGAGENT_TEST_TOOL_PATH_ENV";
        temp_env_set(env_name, "/opt/logagent/tools/influxql_analyzer", || {
            let valid = serde_yaml::from_str::<ConfigFile>(
                r#"
tools:
  influxql_analyzer:
    path_env: LOGAGENT_TEST_TOOL_PATH_ENV
    args: ["--input", "{input_file}"]
"#,
            )
            .unwrap();
            let tools = resolve_tools(valid.tools).unwrap();
            let tool = tools.tools.get("influxql_analyzer").unwrap();
            assert_eq!(
                tool.path,
                PathBuf::from("/opt/logagent/tools/influxql_analyzer")
            );
        });

        let disabled = serde_yaml::from_str::<ConfigFile>(
            r#"
tools:
  flux_query_analyzer:
    enabled: false
    path_env: LOGAGENT_TEST_MISSING_TOOL_PATH_ENV
"#,
        )
        .unwrap();
        let tools = resolve_tools(disabled.tools).unwrap();
        let tool = tools.tools.get("flux_query_analyzer").unwrap();
        assert!(!tool.enabled);
        assert_eq!(tool.path, PathBuf::new());
    }

    #[test]
    fn expands_fixed_tool_path_placeholders() {
        temp_env_set("LOGAGENT_TEST_TOOL_ROOT", "/opt/logagent", || {
            let valid = serde_yaml::from_str::<ConfigFile>(
                r#"
tools:
  influxql_analyzer:
    path: ${LOGAGENT_TEST_TOOL_ROOT}/bin/tools/influxql-analyzer
    args: ["-input", "{input_file}", "-output", "json"]
"#,
            )
            .unwrap();
            let tools = resolve_tools(valid.tools).unwrap();
            let tool = tools.tools.get("influxql_analyzer").unwrap();
            assert_eq!(
                tool.path,
                PathBuf::from("/opt/logagent/bin/tools/influxql-analyzer")
            );
        });
    }

    #[test]
    fn rejects_missing_or_empty_tool_path_env_when_enabled() {
        let missing = serde_yaml::from_str::<ConfigFile>(
            r#"
tools:
  influxql_analyzer:
    path_env: LOGAGENT_TEST_MISSING_TOOL_PATH_ENV
"#,
        )
        .unwrap();
        assert!(resolve_tools(missing.tools)
            .unwrap_err()
            .to_string()
            .contains("missing tool path env var LOGAGENT_TEST_MISSING_TOOL_PATH_ENV"));

        temp_env_set("LOGAGENT_TEST_EMPTY_TOOL_PATH_ENV", "  ", || {
            let empty = serde_yaml::from_str::<ConfigFile>(
                r#"
tools:
  influxql_analyzer:
    path_env: LOGAGENT_TEST_EMPTY_TOOL_PATH_ENV
"#,
            )
            .unwrap();
            assert!(resolve_tools(empty.tools)
                .unwrap_err()
                .to_string()
                .contains("must not be empty"));
        });
    }

    #[test]
    fn resolves_fetch_config_with_base64_key_and_allowlist() {
        temp_env_set("LOGAGENT_TEST_FETCH_KEY", &BASE64.encode([9u8; 32]), || {
            let parsed = serde_yaml::from_str::<ConfigFile>(
                r#"
fetch:
  enabled: true
  secret_key_env: LOGAGENT_TEST_FETCH_KEY
  allowed_hosts:
    - "http://127.0.0.1:50992"
    - "api.example.com"
  request_timeout_seconds: 5
"#,
            )
            .unwrap();
            let fetch = resolve_fetch(parsed.fetch.unwrap()).unwrap();
            assert!(fetch.enabled);
            assert_eq!(fetch.secret_key, Some([9u8; 32]));
            assert_eq!(fetch.allowed_hosts.len(), 2);
            assert_eq!(fetch.allowed_hosts[0].scheme.as_deref(), Some("http"));
            assert_eq!(fetch.allowed_hosts[0].port, Some(50992));
            assert_eq!(fetch.allowed_hosts[1].host, "api.example.com");
            assert_eq!(fetch.request_timeout_seconds, 5);
        });
    }

    #[test]
    fn rejects_enabled_fetch_without_valid_key_or_allowlist() {
        let missing_allowlist = serde_yaml::from_str::<ConfigFile>(
            r#"
fetch:
  enabled: true
  secret_key_env: LOGAGENT_TEST_FETCH_KEY
"#,
        )
        .unwrap();
        assert!(resolve_fetch(missing_allowlist.fetch.unwrap())
            .unwrap_err()
            .to_string()
            .contains("allowed_hosts"));

        temp_env_set(
            "LOGAGENT_TEST_FETCH_BAD_KEY",
            &BASE64.encode([1u8; 16]),
            || {
                let bad_key = serde_yaml::from_str::<ConfigFile>(
                    r#"
fetch:
  enabled: true
  secret_key_env: LOGAGENT_TEST_FETCH_BAD_KEY
  allowed_hosts: ["127.0.0.1"]
"#,
                )
                .unwrap();
                assert!(resolve_fetch(bad_key.fetch.unwrap())
                    .unwrap_err()
                    .to_string()
                    .contains("32 bytes"));
            },
        );
    }

    #[test]
    fn resolves_huawei_package_sync_config_only_when_enabled() {
        let disabled = serde_yaml::from_str::<ConfigFile>(
            r#"
huawei_cloud:
  package_sync:
    enabled: false
    obs:
      access_key_env: LOGAGENT_TEST_MISSING_OBS_AK
      secret_key_env: LOGAGENT_TEST_MISSING_OBS_SK
    gaussdb:
      password_env: LOGAGENT_TEST_MISSING_GAUSSDB_PASSWORD
"#,
        )
        .unwrap();
        let settings = resolve_huawei_cloud(disabled.huawei_cloud.unwrap()).unwrap();
        assert!(!settings.package_sync.enabled);
        assert!(settings.package_sync.obs.access_key.is_none());
        assert!(settings.package_sync.gaussdb.password.is_none());

        temp_env_set("LOGAGENT_TEST_OBS_AK", "ak", || {
            temp_env_set("LOGAGENT_TEST_OBS_SK", "sk", || {
                temp_env_set("LOGAGENT_TEST_OBS_TOKEN", "token", || {
                    temp_env_set("LOGAGENT_TEST_GAUSSDB_PASSWORD", "pwd", || {
                        let enabled = serde_yaml::from_str::<ConfigFile>(
                            r#"
huawei_cloud:
  package_sync:
    enabled: true
    timeout_seconds: 9
    obs:
      endpoint: "https://obs.cn-north-4.myhuaweicloud.com"
      bucket: "pkg-bucket"
      object_prefix: "/packages/releases/"
      access_key_env: LOGAGENT_TEST_OBS_AK
      secret_key_env: LOGAGENT_TEST_OBS_SK
      security_token_env: LOGAGENT_TEST_OBS_TOKEN
    gaussdb:
      host: "gaussdb.internal"
      port: 8000
      database: "pkgdb"
      user: "pkguser"
      password_env: LOGAGENT_TEST_GAUSSDB_PASSWORD
      sslmode: "disable"
"#,
                        )
                        .unwrap();
                        let settings = resolve_huawei_cloud(enabled.huawei_cloud.unwrap()).unwrap();
                        let package_sync = settings.package_sync;
                        assert!(package_sync.enabled);
                        assert_eq!(package_sync.timeout_seconds, 9);
                        assert_eq!(package_sync.obs.object_prefix, "packages/releases");
                        assert_eq!(package_sync.obs.access_key.as_deref(), Some("ak"));
                        assert_eq!(package_sync.obs.secret_key.as_deref(), Some("sk"));
                        assert_eq!(package_sync.obs.security_token.as_deref(), Some("token"));
                        assert_eq!(package_sync.gaussdb.password.as_deref(), Some("pwd"));
                    });
                });
            });
        });
    }

    #[test]
    fn rejects_invalid_huawei_package_sync_config() {
        let missing_env = serde_yaml::from_str::<ConfigFile>(
            r#"
huawei_cloud:
  package_sync:
    enabled: true
    obs:
      endpoint: "https://obs.cn-north-4.myhuaweicloud.com"
      bucket: "pkg-bucket"
      access_key_env: LOGAGENT_TEST_MISSING_OBS_AK
      secret_key_env: LOGAGENT_TEST_MISSING_OBS_SK
    gaussdb:
      host: "gaussdb.internal"
      database: "pkgdb"
      user: "pkguser"
      password_env: LOGAGENT_TEST_MISSING_GAUSSDB_PASSWORD
"#,
        )
        .unwrap();
        assert!(resolve_huawei_cloud(missing_env.huawei_cloud.unwrap())
            .unwrap_err()
            .to_string()
            .contains("LOGAGENT_TEST_MISSING_OBS_AK"));

        temp_env_set("LOGAGENT_TEST_OBS_AK2", "ak", || {
            temp_env_set("LOGAGENT_TEST_OBS_SK2", "sk", || {
                temp_env_set("LOGAGENT_TEST_GAUSSDB_PASSWORD2", "pwd", || {
                    let endpoint_with_query = serde_yaml::from_str::<ConfigFile>(
                        r#"
huawei_cloud:
  package_sync:
    enabled: true
    obs:
      endpoint: "https://obs.cn-north-4.myhuaweicloud.com?region=cn"
      bucket: "pkg-bucket"
      access_key_env: LOGAGENT_TEST_OBS_AK2
      secret_key_env: LOGAGENT_TEST_OBS_SK2
    gaussdb:
      host: "gaussdb.internal"
      database: "pkgdb"
      user: "pkguser"
      password_env: LOGAGENT_TEST_GAUSSDB_PASSWORD2
"#,
                    )
                    .unwrap();
                    assert!(
                        resolve_huawei_cloud(endpoint_with_query.huawei_cloud.unwrap())
                            .unwrap_err()
                            .to_string()
                            .contains("credentials, query, or fragment")
                    );

                    let unsupported_sslmode = serde_yaml::from_str::<ConfigFile>(
                        r#"
huawei_cloud:
  package_sync:
    enabled: true
    obs:
      endpoint: "https://obs.cn-north-4.myhuaweicloud.com"
      bucket: "pkg-bucket"
      access_key_env: LOGAGENT_TEST_OBS_AK2
      secret_key_env: LOGAGENT_TEST_OBS_SK2
    gaussdb:
      host: "gaussdb.internal"
      database: "pkgdb"
      user: "pkguser"
      password_env: LOGAGENT_TEST_GAUSSDB_PASSWORD2
      sslmode: "require"
"#,
                    )
                    .unwrap();
                    assert!(
                        resolve_huawei_cloud(unsupported_sslmode.huawei_cloud.unwrap())
                            .unwrap_err()
                            .to_string()
                            .contains("sslmode")
                    );
                });
            });
        });

        assert!(validate_huawei_object_key("prefix/pkg.tar.gz").is_ok());
        assert!(validate_huawei_object_key("../pkg.tar.gz").is_err());
        assert!(validate_huawei_object_key("prefix//pkg.tar.gz").is_err());
    }

    fn temp_env_set(name: &str, value: &str, test: impl FnOnce()) {
        let previous = env::var(name).ok();
        // SAFETY: these unit tests use unique environment variable names and restore them
        // immediately after the closure. They do not share these names with runtime code.
        unsafe {
            env::set_var(name, value);
        }
        test();
        unsafe {
            if let Some(previous) = previous {
                env::set_var(name, previous);
            } else {
                env::remove_var(name);
            }
        }
    }
}
