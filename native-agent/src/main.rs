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
use tokio::{io::AsyncReadExt, net::TcpListener};
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
    #[serde(default = "default_upload_chunk_bytes")]
    upload_chunk_bytes: u64,
    #[serde(default = "default_state_path")]
    state_path: PathBuf,
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
    upload_chunk_bytes: u64,
    state_path: PathBuf,
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
    session_id: String,
    task_id: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionRequest {
    title: String,
    source_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    session_id: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachUploadsRequest {
    upload_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NativeAgentState {
    current_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceCurrentRequest {
    session_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceCurrentResponse {
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InitUploadRequest {
    filename: String,
    size: u64,
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
        .route(
            "/workspace/current",
            get(get_current_workspace)
                .put(put_current_workspace)
                .delete(delete_current_workspace),
        )
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
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
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
    let session_id = match read_current_session(&state.config).await? {
        Some(session_id) => session_id,
        None => {
            let filename = req
                .filename
                .clone()
                .or_else(|| {
                    file_path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "log import".to_string());
            let session_id = create_session(
                &state,
                format!("Native import {filename}"),
                req.source_url.clone(),
            )
            .await?;
            write_current_session(&state.config, Some(session_id.clone())).await?;
            session_id
        }
    };
    attach_upload_to_session(&state, &session_id, &upload_id).await?;

    Ok(Json(ImportResponse {
        upload_id,
        session_id: session_id.clone(),
        task_id: None,
        url: Some(format!(
            "{}/sessions/{}",
            state.config.server_base_url.trim_end_matches('/'),
            session_id
        )),
    }))
}

async fn get_current_workspace(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WorkspaceCurrentResponse>, AppError> {
    Ok(Json(WorkspaceCurrentResponse {
        session_id: read_current_session(&state.config).await?,
    }))
}

async fn put_current_workspace(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WorkspaceCurrentRequest>,
) -> Result<Json<WorkspaceCurrentResponse>, AppError> {
    let session_id = req.session_id.trim().to_string();
    validate_session_id(&session_id)?;
    write_current_session(&state.config, Some(session_id.clone())).await?;
    Ok(Json(WorkspaceCurrentResponse {
        session_id: Some(session_id),
    }))
}

async fn delete_current_workspace(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WorkspaceCurrentResponse>, AppError> {
    write_current_session(&state.config, None).await?;
    Ok(Json(WorkspaceCurrentResponse { session_id: None }))
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
    let metadata = tokio::fs::metadata(file_path)
        .await
        .map_err(|err| AppError::bad_request(format!("cannot read file metadata: {err}")))?;
    if metadata.len() > state.config.upload_chunk_bytes {
        return upload_file_chunked(state, file_path, req, metadata.len()).await;
    }

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

async fn upload_file_chunked(
    state: &AppState,
    file_path: &Path,
    req: &ImportRequest,
    size: u64,
) -> Result<String, AppError> {
    let filename = req
        .filename
        .clone()
        .or_else(|| {
            file_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "upload.bin".to_string());

    let upload_id = init_chunked_upload(state, filename, size).await?;
    let mut file = tokio::fs::File::open(file_path).await.map_err(|err| {
        AppError::bad_request(format!("cannot open file for chunked upload: {err}"))
    })?;
    let mut offset = 0_u64;
    let mut buffer = vec![0_u8; state.config.upload_chunk_bytes as usize];

    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|err| AppError::bad_request(format!("cannot read file chunk: {err}")))?;
        if read == 0 {
            break;
        }
        upload_chunk(state, &upload_id, offset, &buffer[..read]).await?;
        offset += read as u64;
    }

    complete_chunked_upload(state, &upload_id).await?;
    Ok(upload_id)
}

async fn init_chunked_upload(
    state: &AppState,
    filename: String,
    size: u64,
) -> Result<String, AppError> {
    let url = format!(
        "{}/api/uploads/init",
        state.config.server_base_url.trim_end_matches('/')
    );
    let response = state
        .client
        .post(url)
        .bearer_auth(&state.config.api_key)
        .json(&InitUploadRequest { filename, size })
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("init upload request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "init upload failed with status {status}: {body}"
        )));
    }

    let body: serde_json::Value = response.json().await.map_err(|err| {
        AppError::bad_gateway(format!("invalid init upload response JSON: {err}"))
    })?;
    upload_id_from_value(&body)
        .ok_or_else(|| AppError::bad_gateway("server init upload response missing uploadId/id"))
}

async fn upload_chunk(
    state: &AppState,
    upload_id: &str,
    offset: u64,
    bytes: &[u8],
) -> Result<(), AppError> {
    let url = format!(
        "{}/api/uploads/{}/chunks?offset={}",
        state.config.server_base_url.trim_end_matches('/'),
        upload_id,
        offset
    );
    let response = state
        .client
        .post(url)
        .bearer_auth(&state.config.api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(bytes.to_vec())
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("upload chunk request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "upload chunk failed with status {status}: {body}"
        )));
    }
    let _ = response.bytes().await;
    Ok(())
}

async fn complete_chunked_upload(state: &AppState, upload_id: &str) -> Result<(), AppError> {
    let url = format!(
        "{}/api/uploads/{}/complete",
        state.config.server_base_url.trim_end_matches('/'),
        upload_id
    );
    let response = state
        .client
        .post(url)
        .bearer_auth(&state.config.api_key)
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("complete upload request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "complete upload failed with status {status}: {body}"
        )));
    }
    let _ = response.bytes().await;
    Ok(())
}

async fn create_session(
    state: &AppState,
    title: String,
    source_url: Option<String>,
) -> Result<String, AppError> {
    let url = format!(
        "{}/api/sessions",
        state.config.server_base_url.trim_end_matches('/')
    );
    let payload = CreateSessionRequest { title, source_url };
    let response = state
        .client
        .post(&url)
        .bearer_auth(&state.config.api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("create session request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "create session failed with status {status}: {body}"
        )));
    }

    let response: SessionResponse = response
        .json()
        .await
        .map_err(|err| AppError::bad_gateway(format!("invalid session response JSON: {err}")))?;
    response
        .session_id
        .or(response.id)
        .ok_or_else(|| AppError::bad_gateway("server session response missing sessionId/id"))
}

async fn attach_upload_to_session(
    state: &AppState,
    session_id: &str,
    upload_id: &str,
) -> Result<(), AppError> {
    let url = format!(
        "{}/api/sessions/{}/uploads",
        state.config.server_base_url.trim_end_matches('/'),
        session_id
    );
    let response = state
        .client
        .post(url)
        .bearer_auth(&state.config.api_key)
        .json(&AttachUploadsRequest {
            upload_ids: vec![upload_id.to_string()],
        })
        .send()
        .await
        .map_err(|err| AppError::bad_gateway(format!("attach upload request failed: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::bad_gateway(format!(
            "attach upload failed with status {status}: {body}"
        )));
    }
    let _ = response.bytes().await;
    Ok(())
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
        upload_chunk_bytes: native.upload_chunk_bytes,
        state_path: expand_home(&native.state_path),
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
        upload_chunk_bytes: default_upload_chunk_bytes(),
        state_path: default_state_path(),
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

fn default_upload_chunk_bytes() -> u64 {
    512 * 1024
}

fn default_state_path() -> PathBuf {
    PathBuf::from("~/.logagent/native-agent-state.json")
}

fn default_max_upload_bytes() -> u64 {
    2 * 1024 * 1024 * 1024
}

fn default_file_suffixes() -> Vec<String> {
    [".log", ".txt", ".zip", ".tar.gz", ".tgz", ".tar"]
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

async fn read_current_session(config: &AppConfig) -> Result<Option<String>, AppError> {
    let raw = match tokio::fs::read_to_string(&config.state_path).await {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::bad_request(format!(
                "cannot read native agent state: {err}"
            )))
        }
    };
    let state: NativeAgentState = serde_json::from_str(&raw)
        .map_err(|err| AppError::bad_request(format!("invalid native agent state: {err}")))?;
    Ok(state.current_session_id)
}

async fn write_current_session(
    config: &AppConfig,
    session_id: Option<String>,
) -> Result<(), AppError> {
    if let Some(session_id) = session_id.as_deref() {
        validate_session_id(session_id)?;
    }
    if let Some(parent) = config.state_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| AppError::bad_request(format!("cannot create state dir: {err}")))?;
    }
    let state = NativeAgentState {
        current_session_id: session_id,
    };
    let encoded = serde_json::to_vec_pretty(&state)
        .map_err(|err| AppError::bad_request(format!("cannot encode state: {err}")))?;
    tokio::fs::write(&config.state_path, encoded)
        .await
        .map_err(|err| AppError::bad_request(format!("cannot write native agent state: {err}")))?;
    Ok(())
}

fn validate_session_id(session_id: &str) -> Result<(), AppError> {
    let valid = session_id.starts_with("sess_")
        && session_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid sessionId"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(state_path: PathBuf) -> AppConfig {
        AppConfig {
            bind: "127.0.0.1:0".to_string(),
            server_base_url: "http://127.0.0.1:0".to_string(),
            api_key: "test-key".to_string(),
            allowed_dirs: Vec::new(),
            allowed_suffixes: default_file_suffixes(),
            max_upload_bytes: 1024 * 1024,
            request_timeout_seconds: 1,
            upload_chunk_bytes: 512 * 1024,
            state_path,
        }
    }

    #[tokio::test]
    async fn active_session_state_round_trips() {
        let root =
            std::env::temp_dir().join(format!("logagent-native-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let config = test_config(root.join("native-agent-state.json"));

        assert_eq!(read_current_session(&config).await.unwrap(), None);
        write_current_session(&config, Some("sess_test".to_string()))
            .await
            .unwrap();
        assert_eq!(
            read_current_session(&config).await.unwrap(),
            Some("sess_test".to_string())
        );
        write_current_session(&config, None).await.unwrap();
        assert_eq!(read_current_session(&config).await.unwrap(), None);

        let _ = std::fs::remove_dir_all(root);
    }
}
