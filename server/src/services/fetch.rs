use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, LOCATION},
    Method, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    app::AppState,
    domain::models::TaskRecord,
    services::agent_contracts::write_json_atomic,
    stores::fetch_store::FetchStore,
    support::{
        config::{AppConfig, FetchAllowedHost},
        error::AppError,
        fs_utils::relative_string,
    },
};

pub const FETCH_TOOL_ID: &str = "logagent.fetch";
const REDACTED: &str = "<redacted>";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchEndpointRecord {
    pub schema_version: u32,
    pub fetch_id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub enabled: bool,
    pub method: String,
    pub url_template: String,
    #[serde(default)]
    pub query: Vec<FetchValueSlot>,
    #[serde(default)]
    pub headers: Vec<FetchValueSlot>,
    pub body: Option<FetchBodyRecord>,
    pub follow_redirects: bool,
    pub credential_set: FetchCredentialSet,
    pub refresh_policy: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_run_task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchValueSlot {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchBodyRecord {
    pub kind: FetchBodyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub fields: Vec<FetchValueSlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchBodyKind {
    Raw,
    Form,
    JsonObject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchCredentialSet {
    pub version: u64,
    #[serde(default)]
    pub credentials: Vec<FetchEncryptedCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchEncryptedCredential {
    pub key: String,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone)]
pub struct FetchEndpointDraft {
    pub record: FetchEndpointRecord,
    pub plaintext_credentials: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct FetchResolvedEndpoint {
    pub endpoint: FetchEndpointRecord,
    pub credentials: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchEndpointView {
    pub fetch_id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub method: String,
    pub url_template: String,
    pub query: Vec<FetchValueView>,
    pub headers: Vec<FetchValueView>,
    pub body: Option<FetchBodyView>,
    pub follow_redirects: bool,
    pub credential_version: u64,
    pub refresh_policy: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchValueView {
    pub name: String,
    pub value: Value,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchBodyView {
    pub kind: FetchBodyKind,
    pub text: Option<String>,
    pub fields: Vec<FetchValueView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchImportPreview {
    pub endpoint: FetchEndpointView,
    pub detected_sensitive_fields: Vec<FetchSensitiveField>,
    pub unsupported_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchSensitiveField {
    pub location: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRunParams {
    pub fetch_id: String,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedCurl {
    method: Option<String>,
    url: Url,
    headers: Vec<(String, String)>,
    body: Option<String>,
    follow_redirects: bool,
}

struct PreparedRequest {
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
    redacted_request: Value,
    credential_version: u64,
}

struct FetchResponse {
    status_code: u16,
    headers: Vec<FetchValueView>,
    body: Vec<u8>,
    final_url: String,
    duration_ms: u128,
    redirect_count: usize,
}

pub fn preview_curl(curl: &str) -> anyhow::Result<FetchImportPreview> {
    let draft = endpoint_draft_from_curl(
        curl,
        "fetch_preview".to_string(),
        "Preview".to_string(),
        None,
        Vec::new(),
        true,
    )?;
    Ok(FetchImportPreview {
        endpoint: endpoint_view(&draft.record),
        detected_sensitive_fields: detected_sensitive_fields(&draft.record),
        unsupported_warnings: Vec::new(),
    })
}

pub fn endpoint_draft_from_curl(
    curl: &str,
    fetch_id: String,
    name: String,
    description: Option<String>,
    tags: Vec<String>,
    enabled: bool,
) -> anyhow::Result<FetchEndpointDraft> {
    let parsed = parse_curl(curl)?;
    let method = parsed.method.unwrap_or_else(|| {
        if parsed.body.is_some() {
            "POST".to_string()
        } else {
            "GET".to_string()
        }
    });
    let method = method.trim().to_ascii_uppercase();
    Method::from_bytes(method.as_bytes())
        .with_context(|| format!("unsupported HTTP method {method}"))?;
    let mut plaintext_credentials = BTreeMap::new();
    let mut url = parsed.url.clone();
    let query_pairs = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    url.set_query(None);
    let query = value_slots(
        "query",
        query_pairs,
        &mut plaintext_credentials,
        is_sensitive_field_name,
    );
    let headers = parsed
        .headers
        .into_iter()
        .map(|(name, value)| {
            if is_controlled_header(&name) {
                anyhow::bail!("header {name} is controlled by LogAgent and cannot be imported");
            }
            Ok((name, value))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let headers = value_slots(
        "header",
        headers,
        &mut plaintext_credentials,
        is_sensitive_header_name,
    );
    let body = parsed
        .body
        .as_deref()
        .map(|body| body_record(body, &headers, &mut plaintext_credentials))
        .transpose()?;
    let now = Utc::now();
    Ok(FetchEndpointDraft {
        record: FetchEndpointRecord {
            schema_version: 1,
            fetch_id,
            name,
            description,
            tags,
            enabled,
            method,
            url_template: url.to_string(),
            query,
            headers,
            body,
            follow_redirects: parsed.follow_redirects,
            credential_set: FetchCredentialSet {
                version: 1,
                credentials: Vec::new(),
            },
            refresh_policy: None,
            created_at: now,
            updated_at: now,
            last_run_task_id: None,
        },
        plaintext_credentials,
    })
}

pub fn endpoint_view(endpoint: &FetchEndpointRecord) -> FetchEndpointView {
    FetchEndpointView {
        fetch_id: endpoint.fetch_id.clone(),
        name: endpoint.name.clone(),
        description: endpoint.description.clone(),
        tags: endpoint.tags.clone(),
        enabled: endpoint.enabled,
        method: endpoint.method.clone(),
        url_template: endpoint.url_template.clone(),
        query: endpoint.query.iter().map(value_view).collect(),
        headers: endpoint.headers.iter().map(value_view).collect(),
        body: endpoint.body.as_ref().map(|body| FetchBodyView {
            kind: body.kind,
            text: body.text.clone(),
            fields: body.fields.iter().map(value_view).collect(),
        }),
        follow_redirects: endpoint.follow_redirects,
        credential_version: endpoint.credential_set.version,
        refresh_policy: endpoint.refresh_policy.clone(),
        created_at: endpoint.created_at,
        updated_at: endpoint.updated_at,
        last_run_task_id: endpoint.last_run_task_id.clone(),
    }
}

pub async fn run_fetch_task(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: FetchRunParams = serde_json::from_value(task.tool_params.clone())
        .map_err(|err| AppError::bad_request(format!("invalid fetch params: {err}")))?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    tokio::fs::create_dir_all(&workspace)
        .await
        .map_err(|err| AppError::internal(format!("failed to create workspace: {err}")))?;
    let action_id = format!("act_fetch_{}", task.task_id);
    execute_fetch_to_artifacts(
        state.config.clone(),
        &state.fetch,
        &workspace,
        &action_id,
        params,
    )
    .await
    .map_err(|err| AppError::internal(format!("{err:#}")))
}

pub async fn execute_fetch_to_artifacts(
    config: Arc<AppConfig>,
    store: &FetchStore,
    workspace: &Path,
    action_id: &str,
    params: FetchRunParams,
) -> anyhow::Result<PathBuf> {
    if !config.fetch.enabled {
        anyhow::bail!("fetch is disabled by configuration");
    }
    validate_action_id(action_id)?;
    let resolved = store
        .get_resolved(&params.fetch_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown fetchId {}", params.fetch_id))?;
    if !resolved.endpoint.enabled {
        anyhow::bail!("fetch endpoint {} is disabled", resolved.endpoint.fetch_id);
    }
    let prepared = prepare_request(&config, &resolved, &params)?;
    let response = execute_prepared_request(&config, &resolved.endpoint, &prepared).await?;
    let result_dir = workspace.join("tool_results").join(action_id);
    tokio::fs::create_dir_all(&result_dir).await?;
    let body_path = result_dir.join("response_body.bin");
    tokio::fs::write(&body_path, &response.body).await?;
    let body_artifact_path = relative_string(workspace, &body_path)?;
    let body_preview = String::from_utf8_lossy(&response.body)
        .chars()
        .take(4000)
        .collect::<String>();
    let truncated = response.body.len() > config.fetch.max_response_bytes;
    let result_path = result_dir.join("result.json");
    let result_artifact_path = relative_string(workspace, &result_path)?;
    let result = json!({
        "schemaVersion": 3,
        "tool": FETCH_TOOL_ID,
        "actionId": action_id,
        "status": "OK",
        "exitCode": null,
        "durationMs": response.duration_ms,
        "command": [],
        "inputFile": null,
        "stdoutPath": "",
        "stderrPath": "",
        "summary": format!("{} {} -> HTTP {}", resolved.endpoint.method, resolved.endpoint.url_template, response.status_code),
        "findings": [],
        "error": null,
        "fetchId": resolved.endpoint.fetch_id,
        "httpOk": (200..=299).contains(&response.status_code),
        "statusCode": response.status_code,
        "redirectCount": response.redirect_count,
        "finalUrl": response.final_url,
        "request": prepared.redacted_request,
        "response": {
            "statusCode": response.status_code,
            "headers": response.headers,
            "bodyPreview": body_preview,
            "bodyArtifactPath": body_artifact_path,
            "truncated": truncated
        },
        "bodyArtifactPath": body_artifact_path,
        "truncated": truncated,
        "credentialVersion": prepared.credential_version,
        "evidenceRefs": [format!("{result_artifact_path}#response")]
    });
    write_json_atomic(result_path.clone(), &result).await?;
    Ok(result_path)
}

fn parse_curl(curl: &str) -> anyhow::Result<ParsedCurl> {
    let normalized = curl
        .trim()
        .trim_start_matches('$')
        .replace("\\\r\n", " ")
        .replace("\\\n", " ");
    if normalized.contains("`") || normalized.starts_with("curl.exe ") {
        anyhow::bail!("only bash-style curl commands are supported");
    }
    let argv = shell_words::split(&normalized).context("failed to parse bash curl command")?;
    if argv.is_empty() || !argv[0].ends_with("curl") {
        anyhow::bail!("curl command must start with curl");
    }
    let mut method = None;
    let mut url = None;
    let mut headers = Vec::new();
    let mut body = None;
    let mut follow_redirects = false;
    let mut index = 1;
    while index < argv.len() {
        let token = &argv[index];
        let next_value = |index: &mut usize, flag: &str| -> anyhow::Result<String> {
            *index += 1;
            argv.get(*index)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
        };
        match token.as_str() {
            "-X" | "--request" => method = Some(next_value(&mut index, token)?),
            "-H" | "--header" => headers.push(parse_header(&next_value(&mut index, token)?)?),
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                body = Some(next_value(&mut index, token)?)
            }
            "-b" | "--cookie" => {
                headers.push(("Cookie".to_string(), next_value(&mut index, token)?))
            }
            "-L" | "--location" => follow_redirects = true,
            "--compressed" => {}
            "-I" | "--head" => method = Some("HEAD".to_string()),
            value if value.starts_with("--request=") => {
                method = Some(value.trim_start_matches("--request=").to_string())
            }
            value if value.starts_with("--header=") => {
                headers.push(parse_header(value.trim_start_matches("--header="))?)
            }
            value if value.starts_with("--data=") => {
                body = Some(value.trim_start_matches("--data=").to_string())
            }
            value if value.starts_with("--data-raw=") => {
                body = Some(value.trim_start_matches("--data-raw=").to_string())
            }
            value if value.starts_with("--data-binary=") => {
                body = Some(value.trim_start_matches("--data-binary=").to_string())
            }
            value if value.starts_with("-X") && value.len() > 2 => {
                method = Some(value[2..].to_string())
            }
            value if value.starts_with("-H") && value.len() > 2 => {
                headers.push(parse_header(&value[2..])?)
            }
            value if value.starts_with("-d") && value.len() > 2 => body = Some(value[2..].to_string()),
            value if value.starts_with("-b") && value.len() > 2 => {
                headers.push(("Cookie".to_string(), value[2..].to_string()))
            }
            value if value.starts_with('-') => anyhow::bail!(
                "unsupported curl flag {value}; supported flags are -X, -H, --data, --cookie, --compressed and --location"
            ),
            value => {
                if url.is_some() {
                    anyhow::bail!("curl import contains more than one URL");
                }
                url = Some(value.to_string());
            }
        }
        index += 1;
    }
    let url = url.context("curl import is missing URL")?;
    let url = Url::parse(&url).context("curl URL must be absolute http/https URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("curl URL scheme must be http or https");
    }
    Ok(ParsedCurl {
        method,
        url,
        headers,
        body,
        follow_redirects,
    })
}

fn parse_header(value: &str) -> anyhow::Result<(String, String)> {
    let (name, header_value) = value
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("header must use Name: value syntax"))?;
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("header name must not be empty");
    }
    Ok((name.to_string(), header_value.trim_start().to_string()))
}

fn value_slots(
    location: &str,
    items: Vec<(String, String)>,
    credentials: &mut BTreeMap<String, String>,
    sensitive: fn(&str) -> bool,
) -> Vec<FetchValueSlot> {
    items
        .into_iter()
        .map(|(name, value)| {
            if sensitive(&name) {
                let key = credential_key(location, &name);
                credentials.insert(key.clone(), value);
                FetchValueSlot {
                    name,
                    value: None,
                    credential_key: Some(key),
                }
            } else {
                FetchValueSlot {
                    name,
                    value: Some(Value::String(value)),
                    credential_key: None,
                }
            }
        })
        .collect()
}

fn body_record(
    body: &str,
    headers: &[FetchValueSlot],
    credentials: &mut BTreeMap<String, String>,
) -> anyhow::Result<FetchBodyRecord> {
    let content_type = headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .and_then(|header| header.value.as_ref())
        .and_then(Value::as_str)
        .unwrap_or("");
    let trimmed = body.trim();
    if content_type.contains("application/json") || trimmed.starts_with('{') {
        if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(trimmed) {
            let fields = object
                .into_iter()
                .map(|(name, value)| {
                    if is_sensitive_field_name(&name) {
                        let key = credential_key("body", &name);
                        credentials.insert(key.clone(), json_scalar_to_string(&value));
                        FetchValueSlot {
                            name,
                            value: None,
                            credential_key: Some(key),
                        }
                    } else {
                        FetchValueSlot {
                            name,
                            value: Some(value),
                            credential_key: None,
                        }
                    }
                })
                .collect();
            return Ok(FetchBodyRecord {
                kind: FetchBodyKind::JsonObject,
                text: None,
                fields,
            });
        }
    }
    if content_type.contains("application/x-www-form-urlencoded")
        || (trimmed.contains('=') && !trimmed.contains('\n'))
    {
        let fields = trimmed
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (name, value) = part.split_once('=').unwrap_or((part, ""));
                let name = name.to_string();
                let value = value.to_string();
                if is_sensitive_field_name(&name) {
                    let key = credential_key("body", &name);
                    credentials.insert(key.clone(), value);
                    FetchValueSlot {
                        name,
                        value: None,
                        credential_key: Some(key),
                    }
                } else {
                    FetchValueSlot {
                        name,
                        value: Some(Value::String(value)),
                        credential_key: None,
                    }
                }
            })
            .collect();
        return Ok(FetchBodyRecord {
            kind: FetchBodyKind::Form,
            text: None,
            fields,
        });
    }
    Ok(FetchBodyRecord {
        kind: FetchBodyKind::Raw,
        text: Some(body.to_string()),
        fields: Vec::new(),
    })
}

fn prepare_request(
    config: &AppConfig,
    resolved: &FetchResolvedEndpoint,
    params: &FetchRunParams,
) -> anyhow::Result<PreparedRequest> {
    let endpoint = &resolved.endpoint;
    let url_template = apply_variables(&endpoint.url_template, &params.variables)?;
    let mut url = Url::parse(&url_template)?;
    ensure_url_allowed(config, &url)?;
    {
        let mut pairs = url.query_pairs_mut();
        for slot in &endpoint.query {
            pairs.append_pair(&slot.name, &slot_value_string(slot, &resolved.credentials)?);
        }
    }
    ensure_url_allowed(config, &url)?;
    let method = Method::from_bytes(endpoint.method.as_bytes())?;
    let mut headers = HeaderMap::new();
    for slot in &endpoint.headers {
        insert_header(
            &mut headers,
            &slot.name,
            &slot_value_string(slot, &resolved.credentials)?,
        )?;
    }
    for (name, value) in &params.headers {
        if is_controlled_header(name) {
            anyhow::bail!("header override {name} is controlled by LogAgent");
        }
        insert_header(&mut headers, name, value)?;
    }
    let body = match params.body.as_ref() {
        Some(body) => Some(body.as_bytes().to_vec()),
        None => endpoint
            .body
            .as_ref()
            .map(|body| render_body(body, &resolved.credentials))
            .transpose()?,
    };
    if body
        .as_ref()
        .map(|body| body.len() > config.fetch.max_request_bytes)
        .unwrap_or(false)
    {
        anyhow::bail!("fetch request body exceeds configured max_request_bytes");
    }
    let redacted_headers = headers
        .iter()
        .map(|(name, value)| FetchValueView {
            name: name.as_str().to_string(),
            value: if is_sensitive_header_name(name.as_str()) {
                Value::String(REDACTED.to_string())
            } else {
                Value::String(value.to_str().unwrap_or("<binary>").to_string())
            },
            sensitive: is_sensitive_header_name(name.as_str()),
        })
        .collect::<Vec<_>>();
    let redacted_query = endpoint.query.iter().map(value_view).collect::<Vec<_>>();
    Ok(PreparedRequest {
        method,
        url: url.clone(),
        headers,
        body,
        credential_version: endpoint.credential_set.version,
        redacted_request: json!({
            "method": endpoint.method,
            "url": redacted_url(&url, &redacted_query),
            "headers": redacted_headers,
            "query": redacted_query,
            "bodyPreview": endpoint.body.as_ref().map(redacted_body_preview)
        }),
    })
}

async fn execute_prepared_request(
    config: &AppConfig,
    endpoint: &FetchEndpointRecord,
    prepared: &PreparedRequest,
) -> anyhow::Result<FetchResponse> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.fetch.request_timeout_seconds,
        ))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let started = Instant::now();
    let original_host = prepared.url.host_str().map(ToString::to_string);
    let mut current_url = prepared.url.clone();
    let mut method = prepared.method.clone();
    let mut body = prepared.body.clone();
    let mut redirect_count = 0usize;
    loop {
        ensure_url_allowed(config, &current_url)?;
        let mut headers = prepared.headers.clone();
        if current_url.host_str().map(ToString::to_string) != original_host {
            headers.remove(reqwest::header::AUTHORIZATION);
            headers.remove(reqwest::header::COOKIE);
        }
        let mut request = client
            .request(method.clone(), current_url.clone())
            .headers(headers);
        if let Some(body) = body.clone() {
            request = request.body(body);
        }
        let mut response = request.send().await?;
        if endpoint.follow_redirects
            && response.status().is_redirection()
            && redirect_count < config.fetch.max_redirects
        {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("redirect response is missing Location header"))?;
            current_url = current_url.join(location)?;
            ensure_url_allowed(config, &current_url)?;
            redirect_count += 1;
            if response.status().as_u16() == 303 {
                method = Method::GET;
                body = None;
            }
            continue;
        }
        if endpoint.follow_redirects
            && response.status().is_redirection()
            && redirect_count >= config.fetch.max_redirects
        {
            anyhow::bail!("fetch redirect limit exceeded");
        }
        let status_code = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| FetchValueView {
                name: name.as_str().to_string(),
                value: if is_sensitive_header_name(name.as_str()) {
                    Value::String(REDACTED.to_string())
                } else {
                    Value::String(value.to_str().unwrap_or("<binary>").to_string())
                },
                sensitive: is_sensitive_header_name(name.as_str()),
            })
            .collect::<Vec<_>>();
        let body = read_limited_body(&mut response, config.fetch.max_response_bytes).await?;
        return Ok(FetchResponse {
            status_code,
            headers,
            body,
            final_url: redact_url_sensitive_query(&current_url),
            duration_ms: started.elapsed().as_millis(),
            redirect_count,
        });
    }
}

async fn read_limited_body(
    response: &mut reqwest::Response,
    max: usize,
) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = max.saturating_add(1).saturating_sub(body.len());
        if remaining == 0 {
            break;
        }
        if chunk.len() > remaining {
            body.extend_from_slice(&chunk[..remaining]);
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn ensure_url_allowed(config: &AppConfig, url: &Url) -> anyhow::Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("fetch URL scheme must be http or https");
    }
    if config
        .fetch
        .allowed_hosts
        .iter()
        .any(|allowed| allowed_host_matches(allowed, url))
    {
        Ok(())
    } else {
        anyhow::bail!("fetch URL {} is not allowed by fetch.allowed_hosts", url)
    }
}

fn allowed_host_matches(allowed: &FetchAllowedHost, url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if allowed
        .scheme
        .as_deref()
        .map(|scheme| scheme != url.scheme())
        .unwrap_or(false)
    {
        return false;
    }
    if allowed.host != host.to_ascii_lowercase() {
        return false;
    }
    match allowed.port {
        Some(port) => url.port_or_known_default() == Some(port),
        None => true,
    }
}

fn insert_header(headers: &mut HeaderMap, name: &str, value: &str) -> anyhow::Result<()> {
    if is_controlled_header(name) {
        anyhow::bail!("header {name} is controlled by LogAgent");
    }
    headers.insert(
        HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("invalid header name {name}"))?,
        HeaderValue::from_str(value).with_context(|| format!("invalid header value for {name}"))?,
    );
    Ok(())
}

fn render_body(
    body: &FetchBodyRecord,
    credentials: &HashMap<String, String>,
) -> anyhow::Result<Vec<u8>> {
    match body.kind {
        FetchBodyKind::Raw => Ok(body.text.clone().unwrap_or_default().into_bytes()),
        FetchBodyKind::Form => Ok(body
            .fields
            .iter()
            .map(|slot| {
                Ok(format!(
                    "{}={}",
                    slot.name,
                    slot_value_string(slot, credentials)?
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?
            .join("&")
            .into_bytes()),
        FetchBodyKind::JsonObject => {
            let mut object = serde_json::Map::new();
            for slot in &body.fields {
                let value = match slot.credential_key.as_ref() {
                    Some(key) => Value::String(
                        credentials
                            .get(key)
                            .ok_or_else(|| anyhow::anyhow!("missing fetch credential {key}"))?
                            .clone(),
                    ),
                    None => slot.value.clone().unwrap_or(Value::Null),
                };
                object.insert(slot.name.clone(), value);
            }
            Ok(serde_json::to_vec(&Value::Object(object))?)
        }
    }
}

fn slot_value_string(
    slot: &FetchValueSlot,
    credentials: &HashMap<String, String>,
) -> anyhow::Result<String> {
    if let Some(key) = slot.credential_key.as_deref() {
        return credentials
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing fetch credential {key}"));
    }
    Ok(slot
        .value
        .as_ref()
        .map(json_scalar_to_string)
        .unwrap_or_default())
}

fn value_view(slot: &FetchValueSlot) -> FetchValueView {
    FetchValueView {
        name: slot.name.clone(),
        value: if slot.credential_key.is_some() {
            Value::String(REDACTED.to_string())
        } else {
            slot.value.clone().unwrap_or(Value::Null)
        },
        sensitive: slot.credential_key.is_some(),
    }
}

fn redacted_body_preview(body: &FetchBodyRecord) -> Value {
    match body.kind {
        FetchBodyKind::Raw => Value::String(
            body.text
                .clone()
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect(),
        ),
        FetchBodyKind::Form | FetchBodyKind::JsonObject => Value::Array(
            body.fields
                .iter()
                .map(|slot| serde_json::to_value(value_view(slot)).unwrap_or(Value::Null))
                .collect(),
        ),
    }
}

fn redacted_url(url: &Url, redacted_query: &[FetchValueView]) -> String {
    let mut url = url.clone();
    url.set_query(None);
    if !redacted_query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for item in redacted_query {
            pairs.append_pair(&item.name, item.value.as_str().unwrap_or(REDACTED));
        }
    }
    url.to_string()
}

fn redact_url_sensitive_query(url: &Url) -> String {
    let mut redacted = url.clone();
    let query = redacted
        .query_pairs()
        .map(|(name, value)| {
            let value = if is_sensitive_field_name(&name) {
                REDACTED.to_string()
            } else {
                value.to_string()
            };
            (name.to_string(), value)
        })
        .collect::<Vec<_>>();
    redacted.set_query(None);
    if !query.is_empty() {
        let mut pairs = redacted.query_pairs_mut();
        for (name, value) in query {
            pairs.append_pair(&name, &value);
        }
    }
    redacted.to_string()
}

fn apply_variables(template: &str, variables: &BTreeMap<String, String>) -> anyhow::Result<String> {
    let mut output = template.to_string();
    for (key, value) in variables {
        validate_variable_name(key)?;
        output = output.replace(&format!("{{{key}}}"), value);
    }
    if output.contains('{') || output.contains('}') {
        anyhow::bail!("fetch URL template contains unresolved variables");
    }
    Ok(output)
}

fn validate_variable_name(name: &str) -> anyhow::Result<()> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid fetch variable name {name}")
    }
}

fn detected_sensitive_fields(endpoint: &FetchEndpointRecord) -> Vec<FetchSensitiveField> {
    let mut fields = Vec::new();
    for slot in &endpoint.headers {
        if slot.credential_key.is_some() {
            fields.push(FetchSensitiveField {
                location: "header".to_string(),
                name: slot.name.clone(),
            });
        }
    }
    for slot in &endpoint.query {
        if slot.credential_key.is_some() {
            fields.push(FetchSensitiveField {
                location: "query".to_string(),
                name: slot.name.clone(),
            });
        }
    }
    if let Some(body) = &endpoint.body {
        for slot in &body.fields {
            if slot.credential_key.is_some() {
                fields.push(FetchSensitiveField {
                    location: "body".to_string(),
                    name: slot.name.clone(),
                });
            }
        }
    }
    fields
}

fn credential_key(location: &str, name: &str) -> String {
    format!("{location}:{}", name.to_ascii_lowercase())
}

fn is_sensitive_header_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "authorization"
        || name == "cookie"
        || name == "proxy-authorization"
        || name == "set-cookie"
        || name.contains("token")
        || name.contains("secret")
        || name.contains("api-key")
        || name.contains("apikey")
        || name.contains("auth")
}

fn is_sensitive_field_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("token")
        || name.contains("api_key")
        || name.contains("apikey")
        || name.contains("access_key")
        || name.contains("secret")
        || name.contains("password")
        || name.contains("passwd")
        || name.contains("session")
        || name.contains("auth")
        || name.contains("cookie")
        || name.contains("csrf")
        || name.contains("xsrf")
        || name == "key"
}

fn is_controlled_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host" | "content-length" | "transfer-encoding" | "connection"
    )
}

fn json_scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn validate_action_id(action_id: &str) -> anyhow::Result<()> {
    let valid = action_id.starts_with("act_")
        && action_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid fetch actionId {action_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn parses_chrome_bash_curl_and_redacts_sensitive_values() {
        let curl = r#"curl 'https://api.example.com/v1/items?limit=10&api_key=secret-query' \
  -H 'Authorization: Bearer secret-token' \
  -H 'Content-Type: application/json' \
  --data-raw '{"token":"secret-body","name":"demo"}' \
  --compressed --location"#;
        let preview = preview_curl(curl).unwrap();

        assert_eq!(preview.endpoint.method, "POST");
        assert!(preview.endpoint.follow_redirects);
        assert_eq!(preview.endpoint.query[0].value, json!("10"));
        assert_eq!(preview.endpoint.query[1].value, json!(REDACTED));
        assert!(serde_json::to_string(&preview).unwrap().contains(REDACTED));
        assert!(!serde_json::to_string(&preview)
            .unwrap()
            .contains("secret-token"));
        assert!(!serde_json::to_string(&preview)
            .unwrap()
            .contains("secret-body"));
        assert_eq!(preview.detected_sensitive_fields.len(), 3);
    }

    #[test]
    fn rejects_unsupported_curl_flags() {
        let error = preview_curl("curl https://example.com --form file=@/tmp/a")
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported curl flag --form"));
    }

    #[tokio::test]
    async fn encrypts_and_decrypts_endpoint_credentials() {
        let root = test_root("fetch-store");
        let key = [7u8; 32];
        let store = FetchStore::load(
            root.join("fetch"),
            &crate::support::config::FetchSettings {
                enabled: true,
                secret_key_env: Some("FETCH_KEY".to_string()),
                secret_key: Some(key),
                allowed_hosts: Vec::new(),
                request_timeout_seconds: 1,
                max_request_bytes: 1024,
                max_response_bytes: 1024,
                max_redirects: 0,
            },
        )
        .unwrap();
        let draft = endpoint_draft_from_curl(
            "curl 'https://api.example.com?a=1&token=secret' -H 'Authorization: Bearer x'",
            "fetch_test".to_string(),
            "Test".to_string(),
            None,
            Vec::new(),
            true,
        )
        .unwrap();
        store.create(draft).await.unwrap();
        let raw = std::fs::read_to_string(root.join("fetch/fetch_test.json")).unwrap();
        assert!(!raw.contains("Bearer x"));
        assert!(!raw.contains("secret"));
        let resolved = store.get_resolved("fetch_test").await.unwrap().unwrap();
        assert_eq!(
            resolved
                .credentials
                .get("header:authorization")
                .map(String::as_str),
            Some("Bearer x")
        );
        assert_eq!(
            resolved.credentials.get("query:token").map(String::as_str),
            Some("secret")
        );
    }

    fn test_root(prefix: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}-{}",
            std::process::id(),
            BASE64.encode([1, 2, 3]).replace('=', ""),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
