use std::{collections::BTreeMap, env, fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub storage: StorageSettings,
    pub log_analyzer: LogAnalyzerSettings,
    pub tools: ToolsSettings,
    pub remote_execution: RemoteExecutionSettings,
    pub mcp: McpSettings,
    pub dev_selftest: DevSelftestSettings,
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub bind: String,
    pub public_base_url: String,
    pub max_concurrent_tasks: usize,
    #[allow(dead_code)]
    pub max_input_chars: usize,
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
pub struct RemoteExecutionSettings {
    pub commands: BTreeMap<String, RemoteCommandTemplateSettings>,
}

#[derive(Debug, Clone)]
pub struct RemoteCommandTemplateSettings {
    #[allow(dead_code)]
    pub command_id: String,
    #[allow(dead_code)]
    pub display_name: String,
    #[allow(dead_code)]
    pub description: String,
    pub enabled: bool,
    pub argv: Vec<String>,
    pub timeout_seconds: Option<u64>,
}

impl Default for RemoteExecutionSettings {
    fn default() -> Self {
        Self {
            commands: default_remote_command_templates(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpSettings {
    pub enabled: bool,
    /// When non-empty, cross-origin browser requests to `POST /api/mcp` are rejected
    /// unless their `Origin` header is in this list (tightens CORS for direct remote
    /// exposure). Empty ⇒ check disabled (localhost / SSH-tunnel usage).
    pub allowed_origins: Vec<String>,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_origins: Vec::new(),
        }
    }
}

/// Dev self-test pipeline settings (P1: docker self-test closed loop). All
/// commands/binaries/paths are allowlisted here; tool params only select profile
/// ids and carry a `runId`. Disabled by default.
#[derive(Debug, Clone)]
pub struct DevSelftestSettings {
    pub enabled: bool,
    pub build_timeout_seconds: u64,
    pub max_output_bytes: usize,
    pub git: DevSelftestGitSettings,
    pub builds: BTreeMap<String, DevSelftestBuildProfile>,
    pub docker: DevSelftestDockerSettings,
    pub test_suites: BTreeMap<String, DevSelftestTestSuite>,
}

#[derive(Debug, Clone)]
pub struct DevSelftestGitSettings {
    pub enabled: bool,
    pub binary: PathBuf,
    /// Allowlist of clone-able repos + refs.
    pub repos: Vec<DevSelftestGitRepo>,
}

#[derive(Debug, Clone)]
pub struct DevSelftestGitRepo {
    pub url: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DevSelftestBuildProfile {
    #[allow(dead_code)]
    pub display_name: String,
    /// First element is the binary, the rest are args. Run with `working_dir` as cwd.
    pub command: Vec<String>,
    /// Working dir relative to the run's `source/` (empty = `source/`).
    pub working_dir: String,
    pub artifact_globs: Vec<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DevSelftestDockerSettings {
    pub binary: PathBuf,
    pub clusters: BTreeMap<String, DevSelftestDockerCluster>,
}

#[derive(Debug, Clone)]
pub struct DevSelftestDockerCluster {
    pub compose_file: PathBuf,
    pub exposed_port: Option<u16>,
    pub health_check: Option<DevSelftestHealthCheck>,
}

#[derive(Debug, Clone)]
pub struct DevSelftestHealthCheck {
    pub cmd: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct DevSelftestTestSuite {
    #[allow(dead_code)]
    pub display_name: String,
    /// Local command (binary + args) run on the server host when `docker` is absent (P1
    /// stub). When `docker`/`executor` is set, this is the in-container command instead.
    /// Mutually exclusive with `command`.
    pub argv: Vec<String>,
    /// Optional id of a `remote_execution.commands` template supplying argv + timeout for
    /// the docker run (docker/executor mode only). Mutually exclusive with a non-empty `argv`.
    pub command: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub env: BTreeMap<String, String>,
    /// When set, `run_tests` dispatches the suite through the executor docker runner
    /// (`docker run --rm --network ... <image> <argv>`) instead of the local stub.
    pub docker: Option<DevSelftestTestDocker>,
}

/// Inline docker target for a dev_selftest test suite — shared `DockerTargetSpec`
/// (`support::docker_target`), validated with `allow_devselftest_placeholders = true` so
/// volume host sides may use `${DEVSELFTEST_*}` placeholders interpolated at run time.
pub use crate::support::docker_target::DockerTargetSpec as DevSelftestTestDocker;

impl Default for DevSelftestSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            build_timeout_seconds: default_dev_selftest_build_timeout(),
            max_output_bytes: default_dev_selftest_max_output_bytes(),
            git: DevSelftestGitSettings::default(),
            builds: BTreeMap::new(),
            docker: DevSelftestDockerSettings::default(),
            test_suites: BTreeMap::new(),
        }
    }
}

impl Default for DevSelftestGitSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            binary: default_git_binary(),
            repos: Vec::new(),
        }
    }
}

impl Default for DevSelftestDockerSettings {
    fn default() -> Self {
        Self {
            binary: default_docker_binary(),
            clusters: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigFile {
    server: Option<ServerConfig>,
    auth: Option<AuthConfig>,
    storage: Option<StorageConfig>,
    log_analyzer: Option<LogAnalyzerConfig>,
    #[serde(default)]
    tools: BTreeMap<String, ToolConfig>,
    remote_execution: Option<RemoteExecutionConfig>,
    mcp: Option<McpConfig>,
    dev_selftest: Option<DevSelftestConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default = "default_public_base_url")]
    public_base_url: String,
    #[serde(default = "default_max_concurrent_tasks")]
    max_concurrent_tasks: usize,
    #[serde(default = "default_max_input_chars")]
    max_input_chars: usize,
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
struct RemoteExecutionConfig {
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
struct McpConfig {
    #[serde(default = "default_mcp_enabled")]
    enabled: bool,
    #[serde(default = "default_mcp_transport")]
    transport: String,
    #[serde(default)]
    allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DevSelftestConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_dev_selftest_build_timeout")]
    build_timeout_seconds: u64,
    #[serde(default = "default_dev_selftest_max_output_bytes")]
    max_output_bytes: usize,
    #[serde(default)]
    git: DevSelftestGitConfig,
    #[serde(default)]
    builds: BTreeMap<String, DevSelftestBuildConfig>,
    #[serde(default)]
    docker: DevSelftestDockerConfig,
    #[serde(default)]
    test_suites: BTreeMap<String, DevSelftestTestSuiteConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DevSelftestGitConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_git_binary")]
    binary: PathBuf,
    #[serde(default)]
    repos: Vec<DevSelftestGitRepoConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct DevSelftestGitRepoConfig {
    #[serde(default)]
    url: String,
    #[serde(default)]
    refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DevSelftestBuildConfig {
    #[serde(default)]
    display_name: Option<String>,
    command: Vec<String>,
    #[serde(default)]
    working_dir: String,
    #[serde(default)]
    artifact_globs: Vec<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct DevSelftestDockerConfig {
    #[serde(default = "default_docker_binary")]
    binary: PathBuf,
    #[serde(default)]
    clusters: BTreeMap<String, DevSelftestDockerClusterConfig>,
}

impl Default for DevSelftestDockerConfig {
    fn default() -> Self {
        Self {
            binary: default_docker_binary(),
            clusters: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DevSelftestDockerClusterConfig {
    compose_file: PathBuf,
    #[serde(default)]
    exposed_port: Option<u16>,
    #[serde(default)]
    health_check: Option<DevSelftestHealthCheckConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct DevSelftestHealthCheckConfig {
    cmd: Vec<String>,
    #[serde(default = "default_dev_selftest_health_timeout")]
    timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DevSelftestTestSuiteConfig {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    argv: Vec<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    docker: Option<crate::support::docker_target::DockerTargetSpec>,
}

impl AppConfig {
    pub fn prepare_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(self.storage.uploads_dir())?;
        fs::create_dir_all(self.storage.workspaces_dir())?;
        fs::create_dir_all(self.storage.tasks_dir())?;
        fs::create_dir_all(self.storage.dev_selftest_dir())?;
        fs::create_dir_all(self.storage.dev_selftest_runs_dir())?;
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

    pub fn dev_selftest_dir(&self) -> PathBuf {
        self.data_dir.join("dev_selftest")
    }

    pub fn dev_selftest_runs_dir(&self) -> PathBuf {
        self.dev_selftest_dir().join("runs")
    }

    pub fn dev_selftest_run_dir(&self, run_id: &str) -> PathBuf {
        self.dev_selftest_runs_dir().join(run_id)
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
            tools: BTreeMap::new(),
            remote_execution: None,
            mcp: None,
            dev_selftest: None,
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
    let tools = resolve_tools(parsed.tools)?;
    let remote_execution = resolve_remote_execution(
        parsed
            .remote_execution
            .unwrap_or_else(default_remote_execution_config),
    )?;
    let mcp = resolve_mcp(parsed.mcp.unwrap_or_else(default_mcp_config))?;
    let dev_selftest = resolve_dev_selftest(
        parsed
            .dev_selftest
            .unwrap_or_else(default_dev_selftest_config),
    )?;

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
            max_concurrent_tasks: server.max_concurrent_tasks.max(1),
            max_input_chars: server.max_input_chars.max(1024),
        },
        auth: AuthSettings { api_keys },
        storage: StorageSettings {
            data_dir: expand_path_env_vars(storage.data_dir)?,
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
        tools,
        remote_execution,
        mcp,
        dev_selftest,
    }))
}

fn resolve_mcp(raw: McpConfig) -> anyhow::Result<McpSettings> {
    let transport = raw.transport.trim();
    if transport != "stdio" {
        anyhow::bail!("mcp.transport currently only supports stdio");
    }
    Ok(McpSettings {
        enabled: raw.enabled,
        allowed_origins: raw.allowed_origins,
    })
}

fn resolve_dev_selftest(raw: DevSelftestConfig) -> anyhow::Result<DevSelftestSettings> {
    let enabled = raw.enabled;
    let git = resolve_dev_selftest_git(raw.git, enabled)?;
    let docker = resolve_dev_selftest_docker(raw.docker, enabled)?;
    let builds = raw
        .builds
        .into_iter()
        .map(|(id, build)| {
            validate_dev_selftest_profile_id(&id)?;
            let command = build
                .command
                .into_iter()
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<_>>();
            if enabled && command.is_empty() {
                anyhow::bail!("dev_selftest.builds.{id}.command must not be empty");
            }
            let display_name = build.display_name.unwrap_or_else(|| id.clone());
            Ok((
                id,
                DevSelftestBuildProfile {
                    display_name,
                    command,
                    working_dir: build.working_dir.trim().to_string(),
                    artifact_globs: build.artifact_globs,
                    timeout_seconds: build.timeout_seconds.map(|value| value.max(1)),
                },
            ))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    let test_suites = raw
        .test_suites
        .into_iter()
        .map(|(id, suite)| {
            validate_dev_selftest_profile_id(&id)?;
            let argv = suite
                .argv
                .into_iter()
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<_>>();
            let command = suite
                .command
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let docker = suite
                .docker
                .map(|raw| resolve_dev_selftest_test_docker(&id, raw, enabled))
                .transpose()?;
            let has_argv = !argv.is_empty();
            let has_command = command.is_some();
            let has_docker = docker.is_some();
            // command and a non-empty argv are mutually exclusive; exactly one is required.
            if has_command && has_argv {
                anyhow::bail!(
                    "dev_selftest.test_suites.{id}: command and argv are mutually exclusive"
                );
            }
            if !has_command && !has_argv {
                anyhow::bail!(
                    "dev_selftest.test_suites.{id}: either command or argv is required"
                );
            }
            // command references a remote_execution command template run inside the
            // container, so it is only meaningful with a docker target.
            if has_command && !has_docker {
                anyhow::bail!("dev_selftest.test_suites.{id}: command requires a docker block");
            }
            if let Some(cmd) = command.as_deref() {
                let valid = cmd
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
                if !valid {
                    anyhow::bail!(
                        "dev_selftest.test_suites.{id}: command must be a template id (alphanumeric, '_', '-')"
                    );
                }
            }
            let display_name = suite.display_name.unwrap_or_else(|| id.clone());
            Ok((
                id,
                DevSelftestTestSuite {
                    display_name,
                    argv,
                    command,
                    timeout_seconds: suite.timeout_seconds.map(|value| value.max(1)),
                    env: suite.env,
                    docker,
                },
            ))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    Ok(DevSelftestSettings {
        enabled,
        build_timeout_seconds: raw.build_timeout_seconds.max(1),
        max_output_bytes: raw.max_output_bytes.max(1024),
        git,
        builds,
        docker,
        test_suites,
    })
}

fn resolve_dev_selftest_git(
    raw: DevSelftestGitConfig,
    dev_selftest_enabled: bool,
) -> anyhow::Result<DevSelftestGitSettings> {
    if dev_selftest_enabled && raw.enabled && !raw.binary.is_absolute() {
        anyhow::bail!("dev_selftest.git.binary must be absolute when enabled");
    }
    let repos = raw
        .repos
        .into_iter()
        .map(|repo| {
            let url = repo.url.trim().to_string();
            if url.is_empty() {
                anyhow::bail!("dev_selftest.git.repos[].url must not be empty");
            }
            if !matches!(
                reqwest::Url::parse(&url).map(|u| u.scheme().to_ascii_lowercase()),
                Ok(scheme) if matches!(scheme.as_str(), "http" | "https" | "ssh" | "git")
            ) {
                anyhow::bail!("dev_selftest.git.repos[].url must use http, https, ssh or git");
            }
            let refs = repo
                .refs
                .into_iter()
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>();
            if refs.is_empty() {
                anyhow::bail!("dev_selftest.git.repos[].refs must not be empty");
            }
            Ok(DevSelftestGitRepo { url, refs })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(DevSelftestGitSettings {
        enabled: raw.enabled,
        binary: raw.binary,
        repos,
    })
}

fn resolve_dev_selftest_docker(
    raw: DevSelftestDockerConfig,
    dev_selftest_enabled: bool,
) -> anyhow::Result<DevSelftestDockerSettings> {
    if dev_selftest_enabled && !raw.binary.is_absolute() {
        anyhow::bail!("dev_selftest.docker.binary must be absolute");
    }
    let clusters = raw
        .clusters
        .into_iter()
        .map(|(id, cluster)| {
            validate_dev_selftest_profile_id(&id)?;
            if dev_selftest_enabled && !cluster.compose_file.is_absolute() {
                anyhow::bail!("dev_selftest.docker.clusters.{id}.compose_file must be absolute");
            }
            let health_check = cluster.health_check.map(|hc| {
                let cmd = hc
                    .cmd
                    .into_iter()
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>();
                DevSelftestHealthCheck {
                    cmd,
                    timeout_seconds: hc.timeout_seconds.max(1),
                }
            });
            if health_check.as_ref().is_some_and(|hc| hc.cmd.is_empty()) {
                anyhow::bail!(
                    "dev_selftest.docker.clusters.{id}.health_check.cmd must not be empty"
                );
            }
            Ok((
                id,
                DevSelftestDockerCluster {
                    compose_file: cluster.compose_file,
                    exposed_port: cluster.exposed_port,
                    health_check,
                },
            ))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    Ok(DevSelftestDockerSettings {
        binary: raw.binary,
        clusters,
    })
}

fn resolve_dev_selftest_test_docker(
    suite_id: &str,
    raw: crate::support::docker_target::DockerTargetSpec,
    enabled: bool,
) -> anyhow::Result<DevSelftestTestDocker> {
    let docker = crate::support::docker_target::DockerTargetSpec {
        image: raw.image.trim().to_string(),
        network: raw
            .network
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        workdir: raw
            .workdir
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        volumes: raw
            .volumes
            .into_iter()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect(),
        env: raw.env,
    };
    if enabled {
        let context = format!("dev_selftest.test_suites.{suite_id}.docker");
        crate::support::docker_target::validate_docker_target(&docker, &context, true)?;
    }
    Ok(docker)
}

fn validate_dev_selftest_profile_id(id: &str) -> anyhow::Result<()> {
    let valid = !id.is_empty()
        && id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid dev_selftest profile id {id}")
    }
}

fn default_dev_selftest_config() -> DevSelftestConfig {
    DevSelftestConfig {
        enabled: false,
        build_timeout_seconds: default_dev_selftest_build_timeout(),
        max_output_bytes: default_dev_selftest_max_output_bytes(),
        git: DevSelftestGitConfig::default(),
        builds: BTreeMap::new(),
        docker: DevSelftestDockerConfig::default(),
        test_suites: BTreeMap::new(),
    }
}

fn default_dev_selftest_build_timeout() -> u64 {
    600
}

fn default_dev_selftest_max_output_bytes() -> usize {
    8 * 1024 * 1024
}

fn default_dev_selftest_health_timeout() -> u64 {
    60
}

fn default_git_binary() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("git.exe")
    } else {
        PathBuf::from("/usr/bin/git")
    }
}

fn default_docker_binary() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("docker.exe")
    } else {
        PathBuf::from("/usr/bin/docker")
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
    Ok(RemoteExecutionSettings { commands })
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
        max_input_chars: default_max_input_chars(),
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

fn default_mcp_config() -> McpConfig {
    McpConfig {
        enabled: default_mcp_enabled(),
        transport: default_mcp_transport(),
        allowed_origins: Vec::new(),
    }
}

fn default_remote_execution_config() -> RemoteExecutionConfig {
    RemoteExecutionConfig {
        commands: BTreeMap::new(),
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

fn default_mcp_enabled() -> bool {
    true
}

fn default_mcp_transport() -> String {
    "stdio".to_string()
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

fn default_max_input_chars() -> usize {
    60_000
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_mcp_transport() {
        assert!(resolve_mcp(McpConfig {
            enabled: true,
            transport: "http".to_string(),
            allowed_origins: Vec::new(),
        })
        .unwrap_err()
        .to_string()
        .contains("stdio"));
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
    fn dev_selftest_disabled_allows_placeholder_docker_binary() {
        let disabled = serde_yaml::from_str::<ConfigFile>(
            r#"
dev_selftest:
  enabled: false
  docker:
    binary: docker
"#,
        )
        .unwrap();
        let settings = resolve_dev_selftest(disabled.dev_selftest.unwrap()).unwrap();
        assert!(!settings.enabled);
        assert_eq!(settings.docker.binary, PathBuf::from("docker"));

        let enabled = serde_yaml::from_str::<ConfigFile>(
            r#"
dev_selftest:
  enabled: true
  docker:
    binary: docker
"#,
        )
        .unwrap();
        assert!(resolve_dev_selftest(enabled.dev_selftest.unwrap())
            .unwrap_err()
            .to_string()
            .contains("dev_selftest.docker.binary must be absolute"));
    }

    #[test]
    fn dev_selftest_test_suite_command_argv_rules() {
        fn resolve(yaml: &str) -> anyhow::Result<DevSelftestSettings> {
            let raw: DevSelftestConfig = serde_yaml::from_str(yaml).unwrap();
            resolve_dev_selftest(raw)
        }
        // docker + command -> ok
        assert!(resolve(
            r#"
enabled: true
test_suites:
  smoke:
    command: opengemini_smoke
    docker: { image: "alpine:3.20", volumes: ["/repo/tests:/tests:ro"] }
"#
        )
        .is_ok());
        // docker + argv (no command) -> ok
        assert!(resolve(
            r#"
enabled: true
test_suites:
  smoke:
    argv: ["sh", "/tests/smoke.sh"]
    docker: { image: "alpine:3.20" }
"#
        )
        .is_ok());
        // no docker + argv -> ok (P1 stub)
        assert!(resolve(
            r#"
enabled: true
test_suites:
  smoke:
    argv: ["curl", "http://127.0.0.1:8086"]
"#
        )
        .is_ok());
        // command + argv both -> rejected
        let err = resolve(
            r#"
enabled: true
test_suites:
  smoke:
    argv: ["sh", "/t.sh"]
    command: opengemini_smoke
    docker: { image: "alpine:3.20" }
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("mutually exclusive"), "{err}");
        // neither command nor argv -> rejected
        let err = resolve(
            r#"
enabled: true
test_suites:
  smoke:
    docker: { image: "alpine:3.20" }
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("either command or argv is required"), "{err}");
        // command without docker block -> rejected
        let err = resolve(
            r#"
enabled: true
test_suites:
  smoke:
    command: opengemini_smoke
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("command requires a docker block"), "{err}");
    }

    #[test]
    fn dev_selftest_test_docker_security_validation() {
        fn resolve(yaml: &str) -> anyhow::Result<DevSelftestSettings> {
            let raw: DevSelftestConfig = serde_yaml::from_str(yaml).unwrap();
            resolve_dev_selftest(raw)
        }
        let suite = |docker_block: &str| -> String {
            format!(
                "enabled: true\ntest_suites:\n  smoke:\n    argv: [\"sh\", \"/tests/smoke.sh\"]\n    docker:\n{docker_block}"
            )
        };
        let err = |docker_block: &str| resolve(&suite(docker_block)).unwrap_err().to_string();
        // valid baseline (absolute + ${DEVSELFTEST_*} host, host/bridge network)
        assert!(resolve(&suite(
            "      image: \"alpine:3.20\"\n      network: host\n      volumes:\n        - \"/repo/tests:/tests:ro\"\n        - \"${DEVSELFTEST_SOURCE_DIR}/build:/build:ro\"\n      env: { OK_VAR: v }\n"
        ))
        .is_ok());
        assert!(resolve(&suite(
            "      image: \"alpine:3.20\"\n      network: bridge\n"
        ))
        .is_ok());
        // image empty / leading dash
        assert!(err("      image: \"\"\n").contains("image must not be empty"));
        assert!(err("      image: \"-flag\"\n").contains("must not start with '-'"));
        // network bad
        assert!(
            err("      image: \"alpine:3.20\"\n      network: \"bad net\"\n").contains("network")
        );
        assert!(err("      image: \"alpine:3.20\"\n      network: \"-x\"\n").contains("network"));
        // workdir relative / with ..
        assert!(
            err("      image: \"alpine:3.20\"\n      workdir: \"relative\"\n").contains("workdir")
        );
        assert!(
            err("      image: \"alpine:3.20\"\n      workdir: \"/foo/../bar\"\n")
                .contains("workdir")
        );
        // volume host relative / container .. / bad mode / missing colon
        assert!(
            err("      image: \"alpine:3.20\"\n      volumes: [\"relative:/c:ro\"]\n")
                .contains("host must be an absolute path")
        );
        assert!(
            err("      image: \"alpine:3.20\"\n      volumes: [\"/h:/c/../d\"]\n")
                .contains("container path")
        );
        assert!(
            err("      image: \"alpine:3.20\"\n      volumes: [\"/h:/c:rx\"]\n")
                .contains("mode must be ro or rw")
        );
        assert!(
            err("      image: \"alpine:3.20\"\n      volumes: [\"nope\"]\n")
                .contains("must be host:container")
        );
        // env bad key (lowercase / leading digit)
        assert!(
            err("      image: \"alpine:3.20\"\n      env: { lower: v }\n").contains("invalid key")
        );
        assert!(
            err("      image: \"alpine:3.20\"\n      env: { 1BAD: v }\n").contains("invalid key")
        );
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
