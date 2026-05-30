use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(author, version, about = "LogAgent local native agent")]
struct Args {
    #[arg(long, default_value = "logagent.yaml")]
    config: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigFile {
    native_agent: Option<NativeAgentConfig>,
    storage: Option<StorageConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct NativeAgentConfig {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default = "default_server_base_url")]
    server_base_url: String,
    #[serde(default = "default_api_key_env")]
    api_key_env: String,
    #[serde(default)]
    allowed_dirs: Vec<PathBuf>,
    #[serde(default = "default_file_suffixes")]
    allowed_suffixes: Vec<String>,
    #[serde(default = "default_request_timeout_seconds")]
    request_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct StorageConfig {
    #[serde(default = "default_max_upload_bytes")]
    max_upload_bytes: u64,
}

#[derive(Debug, Clone)]
struct AppConfig {
    bind: String,
    server_base_url: String,
    api_key: String,
    allowed_dirs: Vec<PathBuf>,
    allowed_suffixes: Vec<String>,
    max_upload_bytes: u64,
    request_timeout_seconds: u64,
}

#[derive(Debug, Clone)]
struct AppState {
    config: AppConfig,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportRequest {
    file_path: PathBuf,
    filename: Option<String>,
    source_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResponse {
    upload_id: String,
    task_id: String,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskRequest {
    upload_id: String,
    source_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskResponse {
    task_id: Option<String>,
    id: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = load_config(&args.config).context("failed to load native agent config")?;
    let bind: SocketAddr = config
        .bind
        .parse()
        .with_context(|| format!("invalid bind address '{}'", config.bind))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_seconds))
        .build()
        .context("failed to build HTTP client")?;

    let state = Arc::new(AppState { config, client });
    let app = Router::new()
        .route("/health", get(health))
        .route("/imports", post(import_file))
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(bind).await?;
    info!("native agent listening on http://{}", bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn import_file(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<ImportResponse>, AppError> {
    let file_path = validate_import(&state.config, &req).await?;
    let upload_id = upload_file(&state, &file_path, &req).await?;
    let task = create_task(&state, &upload_id, req.source_url.clone()).await?;
    let task_id = task
        .task_id
        .or(task.id)
        .ok_or_else(|| AppError::bad_gateway("server task response missing taskId/id"))?;

    Ok(Json(ImportResponse {
        upload_id,
        task_id,
        url: task.url,
    }))
}

async fn validate_import(config: &AppConfig, req: &ImportRequest) -> Result<PathBuf, AppError> {
    let canonical = tokio::fs::canonicalize(&req.file_path)
        .await
        .map_err(|err| AppError::bad_request(format!("file does not exist: {err}")))?;

    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|err| AppError::bad_request(format!("cannot read file metadata: {err}")))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("path is not a regular file"));
    }
    if metadata.len() > config.max_upload_bytes {
        return Err(AppError::bad_request(format!(
            "file size {} exceeds max_upload_bytes {}",
            metadata.len(),
            config.max_upload_bytes
        )));
    }

    let filename = req
        .filename
        .as_deref()
        .or_else(|| canonical.file_name().and_then(|name| name.to_str()))
        .ok_or_else(|| AppError::bad_request("missing filename"))?;
    if !has_allowed_suffix(filename, &config.allowed_suffixes) {
        return Err(AppError::bad_request(format!(
            "file suffix is not allowed: {filename}"
        )));
    }

    if !config.allowed_dirs.is_empty() {
        let mut allowed = false;
        for dir in &config.allowed_dirs {
            let expanded = expand_home(dir);
            let canonical_dir = tokio::fs::canonicalize(&expanded).await.map_err(|err| {
                AppError::bad_request(format!(
                    "allowed dir '{}' is invalid: {err}",
                    expanded.display()
                ))
            })?;
            if canonical.starts_with(&canonical_dir) {
                allowed = true;
                break;
            }
        }
        if !allowed {
            return Err(AppError::bad_request(
                "file is outside configured allowed_dirs",
            ));
        }
    }

    Ok(canonical)
}

async fn upload_file(
    state: &AppState,
    file_path: &Path,
    req: &ImportRequest,
) -> Result<String, AppError> {
    let part = multipart::Part::file(file_path)
        .await
        .map_err(|err| AppError::bad_request(format!("cannot open file for upload: {err}")))?;
    let filename = req
        .filename
        .clone()
        .or_else(|| {
            file_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "upload.bin".to_string());

    let form = multipart::Form::new()
        .part("file", part.file_name(filename.clone()))
        .text("filename", filename)
        .text("source", "native-agent".to_string());

    let url = format!(
        "{}/api/uploads",
        state.config.server_base_url.trim_end_matches('/')
    );
    let response = state
        .client
        .post(url)
        .bearer_auth(&state.config.api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("upload request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "upload failed with status {status}: {body}"
        )));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| AppError::bad_gateway(format!("invalid upload response JSON: {err}")))?;
    upload_id_from_value(&body)
        .ok_or_else(|| AppError::bad_gateway("server upload response missing uploadId/id"))
}

async fn create_task(
    state: &AppState,
    upload_id: &str,
    source_url: Option<String>,
) -> Result<TaskResponse, AppError> {
    let url = format!(
        "{}/api/tasks",
        state.config.server_base_url.trim_end_matches('/')
    );
    let response = state
        .client
        .post(url)
        .bearer_auth(&state.config.api_key)
        .json(&CreateTaskRequest {
            upload_id: upload_id.to_string(),
            source_url,
        })
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("create task request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "create task failed with status {status}: {body}"
        )));
    }

    response
        .json()
        .await
        .map_err(|err| AppError::bad_gateway(format!("invalid task response JSON: {err}")))
}

fn upload_id_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .get("uploadId")
        .or_else(|| value.get("upload_id"))
        .or_else(|| value.get("id"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

fn load_config(path: &Path) -> anyhow::Result<AppConfig> {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let parsed: ConfigFile = if raw.trim().is_empty() {
        ConfigFile {
            native_agent: None,
            storage: None,
        }
    } else {
        serde_yaml::from_str(&raw).context("invalid YAML")?
    };

    let native = parsed
        .native_agent
        .unwrap_or_else(default_native_agent_config);
    let storage = parsed.storage.unwrap_or_else(default_storage_config);
    let api_key = env::var(&native.api_key_env)
        .with_context(|| format!("missing API key env var {}", native.api_key_env))?;

    Ok(AppConfig {
        bind: native.bind,
        server_base_url: native.server_base_url,
        api_key,
        allowed_dirs: native.allowed_dirs,
        allowed_suffixes: native
            .allowed_suffixes
            .into_iter()
            .map(|suffix| suffix.to_ascii_lowercase())
            .collect(),
        max_upload_bytes: storage.max_upload_bytes,
        request_timeout_seconds: native.request_timeout_seconds,
    })
}

fn default_native_agent_config() -> NativeAgentConfig {
    NativeAgentConfig {
        bind: default_bind(),
        server_base_url: default_server_base_url(),
        api_key_env: default_api_key_env(),
        allowed_dirs: vec![],
        allowed_suffixes: default_file_suffixes(),
        request_timeout_seconds: default_request_timeout_seconds(),
    }
}

fn default_storage_config() -> StorageConfig {
    StorageConfig {
        max_upload_bytes: default_max_upload_bytes(),
    }
}

fn default_bind() -> String {
    "127.0.0.1:17321".to_string()
}

fn default_server_base_url() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_api_key_env() -> String {
    "LOGAGENT_NATIVE_API_KEY".to_string()
}

fn default_request_timeout_seconds() -> u64 {
    300
}

fn default_max_upload_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}

fn default_file_suffixes() -> Vec<String> {
    [".log", ".txt", ".zip", ".tar.gz", ".tgz"]
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

fn has_allowed_suffix(filename: &str, suffixes: &[String]) -> bool {
    let lower = filename.to_ascii_lowercase();
    suffixes.iter().any(|suffix| lower.ends_with(suffix))
}

fn expand_home(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(status = %self.status, message = %self.message);
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, headers, body).into_response()
    }
}
