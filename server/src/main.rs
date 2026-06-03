use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader},
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context};
use axum::{
    extract::{Multipart, State},
    http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::RwLock};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{error, info};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Parser, Debug)]
#[command(author, version, about = "LogAgent MVP server")]
struct Args {
    #[arg(long, default_value = "logagent.yaml")]
    config: PathBuf,
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
}

#[derive(Debug, Clone, Deserialize)]
struct LogAnalyzerConfig {
    #[serde(default = "default_keywords")]
    keywords: Vec<String>,
}

#[derive(Debug, Clone)]
struct AppConfig {
    bind: String,
    public_base_url: String,
    api_keys: Vec<String>,
    data_dir: PathBuf,
    max_upload_bytes: u64,
    keywords: Vec<String>,
}

#[derive(Debug, Clone)]
struct AppState {
    config: AppConfig,
    uploads: Arc<RwLock<HashMap<String, UploadRecord>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadRecord {
    upload_id: String,
    filename: String,
    size: u64,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResponse {
    upload_id: String,
    filename: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskRequest {
    upload_id: String,
    source_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskResponse {
    task_id: String,
    url: String,
    status: String,
    manifest_path: String,
    grep_results_path: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    upload_id: String,
    task_id: String,
    filename: String,
    source_url: Option<String>,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    path: String,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrepResults {
    keywords: Vec<String>,
    total_matches: usize,
    matches: Vec<GrepMatch>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrepMatch {
    file: String,
    line: usize,
    keyword: String,
    text: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = load_config(&args.config).context("failed to load server config")?;
    let bind: SocketAddr = config
        .bind
        .parse()
        .with_context(|| format!("invalid bind address '{}'", config.bind))?;
    prepare_dirs(&config)?;

    let state = Arc::new(AppState {
        config,
        uploads: Arc::new(RwLock::new(HashMap::new())),
    });

    let protected = Router::new()
        .route("/api/uploads", post(upload))
        .route("/api/tasks", post(create_task))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    let app = Router::new()
        .route("/health", get(health))
        .merge(protected)
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(bind).await?;
    info!("server listening on http://{}", bind);
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

async fn require_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let expected = &state.config.api_keys;
    if expected.is_empty() {
        return Err(AppError::internal("server has no configured API keys"));
    }

    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("missing bearer token"))?;

    if !expected.iter().any(|key| key == token) {
        return Err(AppError::unauthorized("invalid bearer token"));
    }

    Ok(next.run(request).await)
}

async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let upload_id = next_id("upl");
    let upload_dir = state.config.data_dir.join("uploads").join(&upload_id);
    tokio::fs::create_dir_all(&upload_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create upload dir: {err}")))?;

    let mut filename: Option<String> = None;
    let mut file_path: Option<PathBuf> = None;
    let mut size: u64 = 0;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad_request(format!("invalid multipart request: {err}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "filename" {
            let value = field
                .text()
                .await
                .map_err(|err| AppError::bad_request(format!("invalid filename field: {err}")))?;
            filename = Some(sanitize_filename(&value)?);
            continue;
        }

        if field_name != "file" {
            continue;
        }

        let fallback_name = field.file_name().unwrap_or("upload.bin").to_string();
        let safe_name = sanitize_filename(filename.as_deref().unwrap_or(&fallback_name))?;
        let path = upload_dir.join(&safe_name);
        let mut out = tokio::fs::File::create(&path)
            .await
            .map_err(|err| AppError::internal(format!("failed to create upload file: {err}")))?;
        let data = field
            .bytes()
            .await
            .map_err(|err| AppError::bad_request(format!("failed to read upload field: {err}")))?;
        size = data.len() as u64;
        if size > state.config.max_upload_bytes {
            return Err(AppError::bad_request(format!(
                "upload size {size} exceeds max_upload_bytes {}",
                state.config.max_upload_bytes
            )));
        }
        out.write_all(&data)
            .await
            .map_err(|err| AppError::internal(format!("failed to write upload file: {err}")))?;
        filename = Some(safe_name);
        file_path = Some(path);
    }

    let filename = filename.ok_or_else(|| AppError::bad_request("missing filename"))?;
    let path = file_path.ok_or_else(|| AppError::bad_request("missing file field"))?;
    let record = UploadRecord {
        upload_id: upload_id.clone(),
        filename: filename.clone(),
        size,
        path,
    };
    state
        .uploads
        .write()
        .await
        .insert(upload_id.clone(), record);

    Ok(Json(UploadResponse {
        upload_id,
        filename,
        size,
    }))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    let upload = state
        .uploads
        .read()
        .await
        .get(&req.upload_id)
        .cloned()
        .ok_or_else(|| AppError::bad_request("unknown uploadId"))?;

    let task_id = next_id("task");
    let workspace = state.config.data_dir.join("workspaces").join(&task_id);
    let raw_dir = workspace.join("raw");
    let extracted_dir = workspace.join("extracted");
    tokio::fs::create_dir_all(&raw_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create raw dir: {err}")))?;
    tokio::fs::create_dir_all(&extracted_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create extracted dir: {err}")))?;

    let raw_path = raw_dir.join(&upload.filename);
    tokio::fs::copy(&upload.path, &raw_path)
        .await
        .map_err(|err| AppError::internal(format!("failed to copy upload to workspace: {err}")))?;

    let config = state.config.clone();
    let source_url = req.source_url.clone();
    let task_id_for_blocking = task_id.clone();
    let manifest_path = workspace.join("manifest.json");
    let grep_results_path = workspace.join("grep_results.json");

    let manifest_path_out = manifest_path.clone();
    let grep_results_path_out = grep_results_path.clone();
    tokio::task::spawn_blocking(move || {
        extract_upload(&raw_path, &extracted_dir)?;
        let files = collect_manifest_files(&extracted_dir)?;
        let manifest = Manifest {
            upload_id: upload.upload_id,
            task_id: task_id_for_blocking,
            filename: upload.filename,
            source_url,
            files,
        };
        write_json(&manifest_path, &manifest)?;
        let grep = run_simple_grep(&extracted_dir, &config.keywords)?;
        write_json(&grep_results_path, &grep)?;
        anyhow::Ok(())
    })
    .await
    .map_err(|err| AppError::internal(format!("task worker panicked: {err}")))?
    .map_err(|err| AppError::internal(format!("task processing failed: {err}")))?;

    let url = format!(
        "{}/tasks/{}",
        state.config.public_base_url.trim_end_matches('/'),
        task_id
    );

    Ok(Json(TaskResponse {
        task_id,
        url,
        status: "DONE".to_string(),
        manifest_path: manifest_path_out.display().to_string(),
        grep_results_path: grep_results_path_out.display().to_string(),
    }))
}

fn extract_upload(raw_path: &Path, extracted_dir: &Path) -> anyhow::Result<()> {
    let name = raw_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = fs::File::open(raw_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry.path()?.to_path_buf();
            let safe_path = safe_join(extracted_dir, &entry_path)?;
            if let Some(parent) = safe_path.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(safe_path)?;
        }
        return Ok(());
    }

    if name.ends_with(".zip") {
        let file = fs::File::open(raw_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let Some(enclosed) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
                continue;
            };
            let safe_path = safe_join(extracted_dir, &enclosed)?;
            if entry.is_dir() {
                fs::create_dir_all(&safe_path)?;
            } else {
                if let Some(parent) = safe_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out = fs::File::create(&safe_path)?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
        return Ok(());
    }

    let filename = raw_path
        .file_name()
        .ok_or_else(|| anyhow!("raw upload missing filename"))?;
    fs::copy(raw_path, extracted_dir.join(filename))?;
    Ok(())
}

fn collect_manifest_files(root: &Path) -> anyhow::Result<Vec<ManifestFile>> {
    let mut files = Vec::new();
    collect_manifest_files_inner(root, root, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn collect_manifest_files_inner(
    root: &Path,
    dir: &Path,
    files: &mut Vec<ManifestFile>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_manifest_files_inner(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(ManifestFile {
                path: relative_string(root, &path)?,
                size: metadata.len(),
            });
        }
    }
    Ok(())
}

fn run_simple_grep(root: &Path, keywords: &[String]) -> anyhow::Result<GrepResults> {
    let lower_keywords: Vec<String> = keywords
        .iter()
        .map(|keyword| keyword.to_ascii_lowercase())
        .collect();
    let mut matches = Vec::new();
    grep_dir(root, root, &lower_keywords, &mut matches)?;
    Ok(GrepResults {
        keywords: keywords.to_vec(),
        total_matches: matches.len(),
        matches,
    })
}

fn grep_dir(
    root: &Path,
    dir: &Path,
    keywords: &[String],
    matches: &mut Vec<GrepMatch>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            grep_dir(root, &path, keywords, matches)?;
        } else if metadata.is_file() {
            grep_file(root, &path, keywords, matches)?;
        }
    }
    Ok(())
}

fn grep_file(
    root: &Path,
    path: &Path,
    keywords: &[String],
    matches: &mut Vec<GrepMatch>,
) -> anyhow::Result<()> {
    const MAX_MATCHES: usize = 200;
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    for (line_index, line) in reader.lines().enumerate() {
        if matches.len() >= MAX_MATCHES {
            return Ok(());
        }
        let Ok(line) = line else {
            continue;
        };
        let lower = line.to_ascii_lowercase();
        if let Some(keyword) = keywords
            .iter()
            .find(|keyword| lower.contains(keyword.as_str()))
        {
            matches.push(GrepMatch {
                file: relative_string(root, path)?,
                line: line_index + 1,
                keyword: keyword.clone(),
                text: line.chars().take(500).collect(),
            });
        }
    }
    Ok(())
}

fn safe_join(root: &Path, child: &Path) -> anyhow::Result<PathBuf> {
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

fn relative_string(root: &Path, path: &Path) -> anyhow::Result<String> {
    Ok(path
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let file = fs::File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

fn sanitize_filename(value: &str) -> Result<String, AppError> {
    let filename = Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::bad_request("invalid filename"))?;
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err(AppError::bad_request("invalid filename"));
    }
    Ok(filename.to_string())
}

fn next_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{millis}_{counter}")
}

fn prepare_dirs(config: &AppConfig) -> anyhow::Result<()> {
    fs::create_dir_all(config.data_dir.join("uploads"))?;
    fs::create_dir_all(config.data_dir.join("workspaces"))?;
    Ok(())
}

fn load_config(path: &Path) -> anyhow::Result<AppConfig> {
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

    Ok(AppConfig {
        bind: server.bind,
        public_base_url: server.public_base_url,
        api_keys,
        data_dir: storage.data_dir,
        max_upload_bytes: storage.max_upload_bytes,
        keywords: analyzer
            .keywords
            .into_iter()
            .map(|keyword| keyword.to_ascii_lowercase())
            .collect(),
    })
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
    }
}

fn default_log_analyzer_config() -> LogAnalyzerConfig {
    LogAnalyzerConfig {
        keywords: default_keywords(),
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

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!(status = %self.status, message = %self.message);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, headers, body).into_response()
    }
}
