use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, ETAG},
    Url,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::Duration;
use tokio_gaussdb::SimpleQueryMessage;
use tokio_util::io::ReaderStream;
use tracing::{info, warn};

use crate::{
    app::AppState,
    domain::models::{TaskInput, TaskRecord},
    support::{
        config::{
            validate_huawei_object_key, HuaweiGaussDbSettings, HuaweiObsSettings,
            HuaweiPackageSyncSettings,
        },
        error::AppError,
        fs_utils::{relative_string, write_json_atomic},
    },
};

pub const HUAWEI_PACKAGE_SYNC_TOOL_ID: &str = "logagent.huawei_cloud_package_sync";
const MAX_QUERY_ROWS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HuaweiPackageSyncParams {
    #[serde(default)]
    pub object_key: String,
    pub update_sql: String,
    pub query_sql: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObsPutOutput {
    status_code: u16,
    etag: Option<String>,
    content_length: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObsHeadOutput {
    status_code: u16,
    etag: Option<String>,
    content_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GaussQueryOutput {
    rows: Vec<BTreeMap<String, Option<String>>>,
    row_count: usize,
    truncated: bool,
}

#[allow(async_fn_in_trait)]
trait ObsPackageClient {
    async fn put_object(
        &self,
        object_key: &str,
        package_path: &Path,
    ) -> anyhow::Result<ObsPutOutput>;
    async fn head_object(&self, object_key: &str) -> anyhow::Result<ObsHeadOutput>;
    fn object_url(&self, object_key: &str) -> anyhow::Result<String>;
}

#[allow(async_fn_in_trait)]
trait GaussDbPackageClient {
    async fn execute_update(&self, sql: &str) -> anyhow::Result<u64>;
    async fn query_rows(&self, sql: &str) -> anyhow::Result<GaussQueryOutput>;
}

pub fn validate_params(value: &serde_json::Value) -> Result<HuaweiPackageSyncParams, AppError> {
    let params: HuaweiPackageSyncParams = serde_json::from_value(value.clone()).map_err(|err| {
        AppError::bad_request(format!("invalid Huawei package sync params: {err}"))
    })?;
    let object_key = params.object_key.trim().to_string();
    if !object_key.is_empty() {
        validate_huawei_object_key(&object_key)
            .map_err(|err| AppError::bad_request(format!("invalid objectKey: {err}")))?;
    }
    let update_sql = params.update_sql.trim().to_string();
    if update_sql.is_empty() {
        return Err(AppError::bad_request("updateSql is required"));
    }
    let query_sql = params.query_sql.trim().to_string();
    if query_sql.is_empty() {
        return Err(AppError::bad_request("querySql is required"));
    }
    Ok(HuaweiPackageSyncParams {
        object_key,
        update_sql,
        query_sql,
    })
}

pub async fn run_huawei_package_sync_task(
    state: Arc<AppState>,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    let settings = &state.config.huawei_cloud.package_sync;
    if !settings.enabled {
        return Err(AppError::bad_request(
            "Huawei package sync is disabled by server config",
        ));
    }
    let params = validate_params(&task.tool_params)?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|err| AppError::internal(format!("failed to create workspace: {err}")))?;
    let input = task
        .inputs
        .first()
        .ok_or_else(|| AppError::bad_request("Huawei package sync requires one upload"))?;
    if task.inputs.len() != 1 {
        return Err(AppError::bad_request(
            "Huawei package sync requires exactly one upload",
        ));
    }
    let raw_path = validate_workspace_relative_path(&input.raw_path)?;
    let package_path = workspace.join(raw_path);
    if !package_path.is_file() {
        return Err(AppError::bad_request(format!(
            "uploaded package file does not exist: {}",
            input.raw_path
        )));
    }
    let object_key = resolve_object_key(settings, &params, input)?;
    let obs = HuaweiObsClient::new(&settings.obs, settings.timeout_seconds)?;
    let gaussdb = HuaweiGaussDbClient::new(&settings.gaussdb, settings.timeout_seconds);
    let action_id = format!("act_tool_huawei_package_sync_{}", task.task_id);
    execute_package_sync_to_artifacts(
        &workspace,
        &action_id,
        settings,
        input,
        &package_path,
        &object_key,
        &params,
        &obs,
        &gaussdb,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_package_sync_to_artifacts<O, G>(
    workspace: &Path,
    action_id: &str,
    settings: &HuaweiPackageSyncSettings,
    input: &TaskInput,
    package_path: &Path,
    object_key: &str,
    params: &HuaweiPackageSyncParams,
    obs: &O,
    gaussdb: &G,
) -> Result<PathBuf, AppError>
where
    O: ObsPackageClient,
    G: GaussDbPackageClient,
{
    let result_dir = workspace.join("tool_results").join(action_id);
    tokio::fs::create_dir_all(&result_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let result_path = result_dir.join("result.json");
    let result_artifact_path = relative_string(workspace, &result_path)
        .map_err(|err| AppError::internal(err.to_string()))?;
    let total_started = Instant::now();
    let timeout_seconds = settings.timeout_seconds;
    let mut warnings = Vec::new();
    let mut failed_step = None::<String>;
    let mut error = None::<String>;
    let mut obs_put = None::<ObsPutOutput>;
    let mut obs_head = None::<ObsHeadOutput>;
    let mut update_affected_rows = None::<u64>;
    let mut query = None::<GaussQueryOutput>;
    let mut timing_obs_put_ms = None::<u128>;
    let mut timing_gauss_update_ms = None::<u128>;
    let mut timing_obs_head_ms = None::<u128>;
    let mut timing_gauss_query_ms = None::<u128>;

    info!(
        action_id,
        object_key,
        upload_id = %input.upload_id,
        filename = %input.filename,
        "starting Huawei package sync tool"
    );

    let step_started = Instant::now();
    match run_with_timeout(
        "obs_put",
        timeout_seconds,
        obs.put_object(object_key, package_path),
    )
    .await
    {
        Ok(output) => {
            timing_obs_put_ms = Some(step_started.elapsed().as_millis());
            obs_put = Some(output);
        }
        Err(err) => {
            failed_step = Some("obs_put".to_string());
            error = Some(err.to_string());
        }
    }

    if error.is_none() {
        let step_started = Instant::now();
        match run_with_timeout(
            "gaussdb_update",
            timeout_seconds,
            gaussdb.execute_update(&params.update_sql),
        )
        .await
        {
            Ok(affected) => {
                timing_gauss_update_ms = Some(step_started.elapsed().as_millis());
                update_affected_rows = Some(affected);
            }
            Err(err) => {
                failed_step = Some("gaussdb_update".to_string());
                error = Some(err.to_string());
            }
        }
    }

    if error.is_none() {
        let step_started = Instant::now();
        match run_with_timeout("obs_head", timeout_seconds, obs.head_object(object_key)).await {
            Ok(output) => {
                timing_obs_head_ms = Some(step_started.elapsed().as_millis());
                obs_head = Some(output);
            }
            Err(err) => {
                failed_step = Some("obs_head".to_string());
                error = Some(err.to_string());
            }
        }
    }

    if error.is_none() {
        let step_started = Instant::now();
        match run_with_timeout(
            "gaussdb_query",
            timeout_seconds,
            gaussdb.query_rows(&params.query_sql),
        )
        .await
        {
            Ok(output) => {
                timing_gauss_query_ms = Some(step_started.elapsed().as_millis());
                if output.truncated {
                    warnings.push(format!(
                        "GaussDB query rows truncated to first {MAX_QUERY_ROWS} row(s)"
                    ));
                }
                query = Some(output);
            }
            Err(err) => {
                failed_step = Some("gaussdb_query".to_string());
                error = Some(err.to_string());
            }
        }
    }

    let object_url = obs.object_url(object_key).map_err(|err| {
        AppError::internal(format!(
            "failed to construct Huawei OBS object URL: {err:#}"
        ))
    })?;
    let status = if error.is_some() { "FAILED" } else { "OK" };
    let summary = if let Some(step) = failed_step.as_deref() {
        format!("Huawei package sync failed at {step}")
    } else {
        format!(
            "Uploaded {} to OBS and queried GaussDB records",
            input.filename
        )
    };
    let result = json!({
        "schemaVersion": 1,
        "toolId": HUAWEI_PACKAGE_SYNC_TOOL_ID,
        "tool": HUAWEI_PACKAGE_SYNC_TOOL_ID,
        "actionId": action_id,
        "status": status,
        "summary": summary,
        "error": error,
        "failedStep": failed_step,
        "warnings": warnings,
        "input": {
            "uploadId": input.upload_id,
            "filename": input.filename,
            "size": input.size,
            "rawPath": input.raw_path,
        },
        "obs": {
            "endpoint": settings.obs.endpoint,
            "bucket": settings.obs.bucket,
            "objectKey": object_key,
            "url": object_url,
            "put": obs_put,
            "head": obs_head,
        },
        "gaussdb": {
            "host": settings.gaussdb.host,
            "port": settings.gaussdb.port,
            "database": settings.gaussdb.database,
            "user": settings.gaussdb.user,
            "sslmode": settings.gaussdb.sslmode,
            "updateAffectedRows": update_affected_rows,
            "queryRowCount": query.as_ref().map(|value| value.row_count),
            "queryRows": query.as_ref().map(|value| &value.rows),
            "queryRowsTruncated": query.as_ref().map(|value| value.truncated).unwrap_or(false),
        },
        "sql": {
            "updateSqlProvided": true,
            "updateSqlLength": params.update_sql.len(),
            "querySqlProvided": true,
            "querySqlLength": params.query_sql.len(),
        },
        "timings": {
            "obsPutMs": timing_obs_put_ms,
            "gaussdbUpdateMs": timing_gauss_update_ms,
            "obsHeadMs": timing_obs_head_ms,
            "gaussdbQueryMs": timing_gauss_query_ms,
            "totalMs": total_started.elapsed().as_millis(),
        },
        "credentialMetadata": {
            "obsAccessKeyEnv": settings.obs.access_key_env,
            "obsSecretKeyEnv": settings.obs.secret_key_env,
            "obsSecurityTokenEnv": settings.obs.security_token_env,
            "gaussdbPasswordEnv": settings.gaussdb.password_env,
        },
        "evidenceRefs": [result_artifact_path],
        "createdAt": Utc::now(),
    });
    write_json_atomic(result_path.clone(), &result).await?;
    if status == "OK" {
        info!(action_id, result_path = %result_path.display(), "Huawei package sync tool completed");
    } else {
        warn!(
            action_id,
            result_path = %result_path.display(),
            "Huawei package sync tool completed with failure result"
        );
    }
    Ok(result_path)
}

async fn run_with_timeout<T>(
    label: &'static str,
    timeout_seconds: u64,
    future: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    tokio::time::timeout(Duration::from_secs(timeout_seconds), future)
        .await
        .map_err(|_| anyhow::anyhow!("{label} timed out after {timeout_seconds}s"))?
}

fn resolve_object_key(
    settings: &HuaweiPackageSyncSettings,
    params: &HuaweiPackageSyncParams,
    input: &TaskInput,
) -> Result<String, AppError> {
    if !params.object_key.trim().is_empty() {
        validate_huawei_object_key(&params.object_key)
            .map_err(|err| AppError::bad_request(format!("invalid objectKey: {err}")))?;
        return Ok(params.object_key.trim().to_string());
    }
    let filename = Path::new(&input.filename)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("uploaded filename is invalid"))?;
    validate_huawei_object_key(filename).map_err(|err| {
        AppError::bad_request(format!(
            "uploaded filename cannot be used as object key: {err}"
        ))
    })?;
    let key = if settings.obs.object_prefix.is_empty() {
        filename.to_string()
    } else {
        format!(
            "{}/{}",
            settings.obs.object_prefix.trim_matches('/'),
            filename
        )
    };
    validate_huawei_object_key(&key)
        .map_err(|err| AppError::bad_request(format!("invalid generated object key: {err}")))?;
    Ok(key)
}

struct HuaweiObsClient {
    client: reqwest::Client,
    endpoint: Url,
    bucket: String,
    signer: reqsign::Signer<reqsign::huaweicloud::Credential>,
}

impl HuaweiObsClient {
    fn new(settings: &HuaweiObsSettings, timeout_seconds: u64) -> Result<Self, AppError> {
        let access_key = settings
            .access_key
            .as_deref()
            .ok_or_else(|| AppError::internal("Huawei OBS access key is missing"))?;
        let secret_key = settings
            .secret_key
            .as_deref()
            .ok_or_else(|| AppError::internal("Huawei OBS secret key is missing"))?;
        let provider = match settings.security_token.as_deref() {
            Some(token) => reqsign::huaweicloud::StaticCredentialProvider::with_security_token(
                access_key, secret_key, token,
            ),
            None => reqsign::huaweicloud::StaticCredentialProvider::new(access_key, secret_key),
        };
        let signer = reqsign::Signer::new(
            reqsign::Context::new(),
            provider,
            reqsign::huaweicloud::RequestSigner::new(&settings.bucket),
        );
        let endpoint = Url::parse(&settings.endpoint)
            .map_err(|err| AppError::internal(format!("invalid Huawei OBS endpoint: {err}")))?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|err| AppError::internal(format!("failed to build OBS HTTP client: {err}")))?;
        Ok(Self {
            client,
            endpoint,
            bucket: settings.bucket.clone(),
            signer,
        })
    }

    fn build_object_url(&self, object_key: &str) -> anyhow::Result<Url> {
        validate_huawei_object_key(object_key)?;
        let mut url = self.endpoint.clone();
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("OBS endpoint is missing host"))?;
        let bucket_host_prefix = format!("{}.", self.bucket);
        if host != self.bucket && !host.starts_with(&bucket_host_prefix) {
            let new_host = format!("{}.{}", self.bucket, host);
            url.set_host(Some(&new_host))
                .map_err(|_| anyhow::anyhow!("failed to set OBS bucket host"))?;
        }
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("OBS endpoint cannot be used as base URL"))?;
            segments.clear();
            for segment in object_key.split('/') {
                segments.push(segment);
            }
        }
        Ok(url)
    }

    async fn signed_headers(
        &self,
        method: http::Method,
        url: &Url,
        headers: HeaderMap,
    ) -> anyhow::Result<HeaderMap> {
        let mut request = http::Request::builder()
            .method(method)
            .uri(url.as_str())
            .body(())?;
        *request.headers_mut() = headers;
        let (mut parts, _) = request.into_parts();
        self.signer.sign(&mut parts, None).await?;
        Ok(parts.headers)
    }
}

impl ObsPackageClient for HuaweiObsClient {
    async fn put_object(
        &self,
        object_key: &str,
        package_path: &Path,
    ) -> anyhow::Result<ObsPutOutput> {
        let url = self.build_object_url(object_key)?;
        let metadata = tokio::fs::metadata(package_path)
            .await
            .with_context(|| format!("failed to stat package {}", package_path.display()))?;
        let content_length = metadata.len();
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        headers.insert(
            CONTENT_LENGTH,
            HeaderValue::from_str(&content_length.to_string())?,
        );
        let headers = self
            .signed_headers(http::Method::PUT, &url, headers)
            .await
            .context("failed to sign OBS PUT request")?;
        let file = tokio::fs::File::open(package_path)
            .await
            .with_context(|| format!("failed to open package {}", package_path.display()))?;
        let response = self
            .client
            .put(url)
            .headers(headers)
            .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
            .send()
            .await
            .context("failed to send OBS PUT request")?;
        let status = response.status();
        let etag = header_to_string(response.headers().get(ETAG));
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "OBS PUT failed with HTTP {status}: {}",
                truncate_text(&body, 512)
            );
        }
        Ok(ObsPutOutput {
            status_code: status.as_u16(),
            etag,
            content_length,
        })
    }

    async fn head_object(&self, object_key: &str) -> anyhow::Result<ObsHeadOutput> {
        let url = self.build_object_url(object_key)?;
        let headers = self
            .signed_headers(http::Method::HEAD, &url, HeaderMap::new())
            .await
            .context("failed to sign OBS HEAD request")?;
        let response = self
            .client
            .head(url)
            .headers(headers)
            .send()
            .await
            .context("failed to send OBS HEAD request")?;
        let status = response.status();
        let etag = header_to_string(response.headers().get(ETAG));
        let content_length = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if !status.is_success() {
            anyhow::bail!("OBS HEAD failed with HTTP {status}");
        }
        Ok(ObsHeadOutput {
            status_code: status.as_u16(),
            etag,
            content_length,
        })
    }

    fn object_url(&self, object_key: &str) -> anyhow::Result<String> {
        Ok(self.build_object_url(object_key)?.to_string())
    }
}

struct HuaweiGaussDbClient {
    settings: HuaweiGaussDbSettings,
    timeout_seconds: u64,
}

impl HuaweiGaussDbClient {
    fn new(settings: &HuaweiGaussDbSettings, timeout_seconds: u64) -> Self {
        Self {
            settings: settings.clone(),
            timeout_seconds,
        }
    }

    async fn connect(&self) -> anyhow::Result<tokio_gaussdb::Client> {
        let conn = format!(
            "host={} port={} user={} password={} dbname={} sslmode={} connect_timeout={}",
            gaussdb_conn_value(&self.settings.host),
            self.settings.port,
            gaussdb_conn_value(&self.settings.user),
            gaussdb_conn_value(
                self.settings
                    .password
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("GaussDB password is missing"))?,
            ),
            gaussdb_conn_value(&self.settings.database),
            self.settings.sslmode,
            self.timeout_seconds,
        );
        let config: tokio_gaussdb::Config = conn.parse().context("invalid GaussDB config")?;
        let (client, connection) = config
            .connect(tokio_gaussdb::NoTls)
            .await
            .context("failed to connect to GaussDB")?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                warn!("GaussDB connection task ended with error: {err}");
            }
        });
        Ok(client)
    }
}

impl GaussDbPackageClient for HuaweiGaussDbClient {
    async fn execute_update(&self, sql: &str) -> anyhow::Result<u64> {
        let client = self.connect().await?;
        let messages = client
            .simple_query(sql)
            .await
            .context("failed to execute GaussDB update SQL")?;
        Ok(sum_command_complete(messages))
    }

    async fn query_rows(&self, sql: &str) -> anyhow::Result<GaussQueryOutput> {
        let client = self.connect().await?;
        let messages = client
            .simple_query(sql)
            .await
            .context("failed to execute GaussDB query SQL")?;
        Ok(collect_query_rows(messages)?)
    }
}

fn collect_query_rows(messages: Vec<SimpleQueryMessage>) -> anyhow::Result<GaussQueryOutput> {
    let mut rows = Vec::new();
    let mut row_count = 0usize;
    for message in messages {
        if let SimpleQueryMessage::Row(row) = message {
            row_count += 1;
            if rows.len() >= MAX_QUERY_ROWS {
                continue;
            }
            let mut output = BTreeMap::new();
            let mut names = BTreeMap::<String, usize>::new();
            for (index, column) in row.columns().iter().enumerate() {
                let base = column.name().to_string();
                let count = names.entry(base.clone()).or_insert(0);
                *count += 1;
                let key = if *count == 1 {
                    base
                } else {
                    format!("{base}_{count}")
                };
                output.insert(
                    key,
                    row.try_get(index)
                        .with_context(|| format!("failed to read query column {index}"))?
                        .map(ToString::to_string),
                );
            }
            rows.push(output);
        }
    }
    Ok(GaussQueryOutput {
        rows,
        row_count,
        truncated: row_count > MAX_QUERY_ROWS,
    })
}

fn sum_command_complete(messages: Vec<SimpleQueryMessage>) -> u64 {
    messages
        .into_iter()
        .filter_map(|message| match message {
            SimpleQueryMessage::CommandComplete(rows) => Some(rows),
            _ => None,
        })
        .sum()
}

fn gaussdb_conn_value(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

fn validate_workspace_relative_path(value: &str) -> Result<&Path, AppError> {
    let path = Path::new(value);
    let valid = !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if valid {
        Ok(path)
    } else {
        Err(AppError::internal("tool task contains unsafe raw path"))
    }
}

fn header_to_string(value: Option<&HeaderValue>) -> Option<String> {
    value
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc as StdArc,
    };

    #[derive(Clone)]
    struct FakeObs {
        calls: StdArc<AtomicUsize>,
    }

    impl FakeObs {
        fn new() -> Self {
            Self {
                calls: StdArc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl ObsPackageClient for FakeObs {
        async fn put_object(
            &self,
            _object_key: &str,
            _package_path: &Path,
        ) -> anyhow::Result<ObsPutOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ObsPutOutput {
                status_code: StatusCode::OK.as_u16(),
                etag: Some("etag-put".to_string()),
                content_length: 12,
            })
        }

        async fn head_object(&self, _object_key: &str) -> anyhow::Result<ObsHeadOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ObsHeadOutput {
                status_code: StatusCode::OK.as_u16(),
                etag: Some("etag-head".to_string()),
                content_length: Some(12),
            })
        }

        fn object_url(&self, object_key: &str) -> anyhow::Result<String> {
            Ok(format!("https://bucket.obs.example.com/{object_key}"))
        }
    }

    struct FakeGaussDb;

    impl GaussDbPackageClient for FakeGaussDb {
        async fn execute_update(&self, _sql: &str) -> anyhow::Result<u64> {
            Ok(1)
        }

        async fn query_rows(&self, _sql: &str) -> anyhow::Result<GaussQueryOutput> {
            Ok(GaussQueryOutput {
                rows: vec![BTreeMap::from([(
                    "package".to_string(),
                    Some("demo.tar.gz".to_string()),
                )])],
                row_count: 1,
                truncated: false,
            })
        }
    }

    #[test]
    fn validates_required_params_and_object_key() {
        assert!(validate_params(&json!({
            "objectKey": "../bad",
            "updateSql": "update t set a=1",
            "querySql": "select 1"
        }))
        .is_err());
        let params = validate_params(&json!({
            "objectKey": "pkg/demo.tar.gz",
            "updateSql": " update t set a=1 ",
            "querySql": " select 1 "
        }))
        .unwrap();
        assert_eq!(params.object_key, "pkg/demo.tar.gz");
        assert_eq!(params.update_sql, "update t set a=1");
        assert_eq!(params.query_sql, "select 1");
    }

    #[tokio::test]
    async fn writes_success_result_with_fake_clients() {
        let root =
            std::env::temp_dir().join(format!("huawei-package-sync-test-{}", std::process::id()));
        let workspace = root.join("workspace");
        tokio::fs::create_dir_all(workspace.join("raw/upl_1"))
            .await
            .unwrap();
        let package_path = workspace.join("raw/upl_1/demo.tar.gz");
        tokio::fs::write(&package_path, b"demo").await.unwrap();
        let settings = HuaweiPackageSyncSettings {
            enabled: true,
            timeout_seconds: 5,
            obs: HuaweiObsSettings {
                endpoint: "https://obs.example.com".to_string(),
                bucket: "bucket".to_string(),
                object_prefix: "prefix".to_string(),
                ..HuaweiObsSettings::default()
            },
            gaussdb: HuaweiGaussDbSettings {
                host: "gaussdb.local".to_string(),
                port: 8000,
                database: "db".to_string(),
                user: "user".to_string(),
                password_env: Some("PWD".to_string()),
                password: Some("secret".to_string()),
                sslmode: "disable".to_string(),
            },
        };
        let input = TaskInput {
            upload_id: "upl_1".to_string(),
            filename: "demo.tar.gz".to_string(),
            size: 4,
            raw_path: "raw/upl_1/demo.tar.gz".to_string(),
        };
        let params = HuaweiPackageSyncParams {
            object_key: "prefix/demo.tar.gz".to_string(),
            update_sql: "update t set path='x'".to_string(),
            query_sql: "select path from t".to_string(),
        };
        let result_path = execute_package_sync_to_artifacts(
            &workspace,
            "act_test",
            &settings,
            &input,
            &package_path,
            "prefix/demo.tar.gz",
            &params,
            &FakeObs::new(),
            &FakeGaussDb,
        )
        .await
        .unwrap();
        let result: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&result_path).await.unwrap()).unwrap();
        assert_eq!(result["status"], "OK");
        assert_eq!(result["obs"]["objectKey"], "prefix/demo.tar.gz");
        assert_eq!(result["gaussdb"]["updateAffectedRows"], 1);
        assert_eq!(result["gaussdb"]["queryRows"][0]["package"], "demo.tar.gz");
        assert!(serde_json::to_string(&result)
            .unwrap()
            .contains("updateSqlLength"));
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("update t set"));
    }
}
