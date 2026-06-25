use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value as YamlValue};
use tokio::{process::Command, time::timeout};

use crate::{
    app::AppState,
    support::{
        config::{DevSelftestGitRepo, DevSelftestSettings},
        error::AppError,
    },
};

pub const CONFIG_RESOURCE_URI: &str = "logagent://dev_selftest/config";
pub const ALLOWLIST_UPDATE_TOOL_ID: &str = "logagent.dev_selftest.allowlist.update";

#[derive(Debug)]
pub struct DevSelftestGitAllowlist {
    repos: RwLock<Vec<DevSelftestGitRepo>>,
}

impl DevSelftestGitAllowlist {
    pub fn new(repos: Vec<DevSelftestGitRepo>) -> Self {
        Self {
            repos: RwLock::new(normalize_repos(repos)),
        }
    }

    pub fn snapshot(&self) -> Vec<DevSelftestGitRepo> {
        self.repos
            .read()
            .map(|repos| repos.clone())
            .unwrap_or_default()
    }

    pub fn update_and_persist(
        &self,
        config_path: &Path,
        repo_url: &str,
        git_ref: &str,
        set_default: bool,
    ) -> Result<(Vec<DevSelftestGitRepo>, bool), AppError> {
        let mut guard = self
            .repos
            .write()
            .map_err(|_| AppError::internal("dev_selftest git allowlist lock is poisoned"))?;
        let before = guard.clone();
        let after = merge_repo_ref(before.clone(), repo_url, git_ref, set_default);
        persist_repos(config_path, &after)?;
        let changed = after != before;
        *guard = after.clone();
        Ok((after, changed))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestConfigSummary {
    pub schema_version: u32,
    pub dev_selftest_enabled: bool,
    pub git_enabled: bool,
    pub git_repos: Vec<DevSelftestConfigRepoSummary>,
    pub default_git_repo: Option<String>,
    pub default_git_ref: Option<String>,
    pub build_profiles: Vec<String>,
    pub docker_profiles: Vec<String>,
    pub test_suites: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestConfigRepoSummary {
    pub url: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AllowlistUpdateRequest {
    pub repo_url: String,
    pub git_ref: String,
    #[serde(default = "default_set_default")]
    pub set_default: bool,
    #[serde(default)]
    pub confirmed_user_consent: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowlistUpdateResponse {
    pub schema_version: u32,
    pub updated: bool,
    pub repo_url: String,
    pub git_ref: String,
    pub set_default: bool,
    pub reason: Option<String>,
    pub config_path: Option<String>,
    pub summary: DevSelftestConfigSummary,
}

fn default_set_default() -> bool {
    true
}

pub fn summary_for_state(state: &AppState) -> DevSelftestConfigSummary {
    summary_for(
        &state.config.dev_selftest,
        &state.dev_selftest_git_allowlist.snapshot(),
    )
}

pub fn summary_for(
    settings: &DevSelftestSettings,
    repos: &[DevSelftestGitRepo],
) -> DevSelftestConfigSummary {
    let normalized = normalize_repos(repos.to_vec());
    let default_git_repo = normalized.first().map(|repo| redact_repo_url(&repo.url));
    let default_git_ref = normalized
        .first()
        .and_then(|repo| repo.refs.first().cloned());
    DevSelftestConfigSummary {
        schema_version: 1,
        dev_selftest_enabled: settings.enabled,
        git_enabled: settings.git.enabled,
        git_repos: normalized
            .into_iter()
            .map(|repo| DevSelftestConfigRepoSummary {
                url: redact_repo_url(&repo.url),
                refs: repo.refs,
            })
            .collect(),
        default_git_repo,
        default_git_ref,
        build_profiles: settings.builds.keys().cloned().collect(),
        docker_profiles: settings.docker.clusters.keys().cloned().collect(),
        test_suites: settings.test_suites.keys().cloned().collect(),
    }
}

pub async fn update_allowlist(
    state: &std::sync::Arc<AppState>,
    request: AllowlistUpdateRequest,
) -> Result<AllowlistUpdateResponse, AppError> {
    if !request.confirmed_user_consent {
        return Err(AppError::bad_request(
            "confirmedUserConsent must be true before updating the dev_selftest git allowlist",
        ));
    }
    if !state.config.dev_selftest.git.enabled {
        return Err(AppError::bad_request("dev_selftest.git is disabled"));
    }
    let repo_url = validate_repo_url(&request.repo_url)?;
    let git_ref = validate_git_ref(&request.git_ref)?;
    verify_ref_reachable(
        &state.config.dev_selftest.git.binary,
        &repo_url,
        &git_ref,
        state.config.dev_selftest.build_timeout_seconds,
    )
    .await?;
    let config_path = state
        .config_path
        .as_ref()
        .ok_or_else(|| AppError::bad_request("server config path is unavailable"))?;
    let (repos, updated) = state.dev_selftest_git_allowlist.update_and_persist(
        config_path,
        &repo_url,
        &git_ref,
        request.set_default,
    )?;
    Ok(AllowlistUpdateResponse {
        schema_version: 1,
        updated,
        repo_url,
        git_ref,
        set_default: request.set_default,
        reason: request
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        config_path: Some(config_path.display().to_string()),
        summary: summary_for(&state.config.dev_selftest, &repos),
    })
}

pub fn repo_ref_allowed(repos: &[DevSelftestGitRepo], repo: &str, git_ref: &str) -> bool {
    repos
        .iter()
        .any(|allowed| allowed.url == repo && allowed.refs.iter().any(|r| r == git_ref))
}

fn validate_repo_url(raw: &str) -> Result<String, AppError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(AppError::bad_request("repoUrl must not be empty"));
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| AppError::bad_request("repoUrl must be an absolute URL"))?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https" | "ssh" | "git") {
        return Err(AppError::bad_request(
            "repoUrl must use http, https, ssh or git",
        ));
    }
    if parsed.password().is_some() {
        return Err(AppError::bad_request(
            "repoUrl must not contain embedded credentials",
        ));
    }
    if matches!(scheme.as_str(), "http" | "https") && !parsed.username().is_empty() {
        return Err(AppError::bad_request(
            "http/https repoUrl must not contain embedded credentials",
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AppError::bad_request(
            "repoUrl must not include query or fragment",
        ));
    }
    Ok(value.to_string())
}

fn validate_git_ref(raw: &str) -> Result<String, AppError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(AppError::bad_request("gitRef must not be empty"));
    }
    let invalid = value.starts_with('-')
        || value.ends_with('/')
        || value.ends_with('.')
        || value.contains("..")
        || value.contains("//")
        || value.contains("@{")
        || value.contains('\\')
        || value.chars().any(|ch| {
            ch.is_control() || ch.is_whitespace() || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[')
        });
    if invalid {
        return Err(AppError::bad_request(
            "gitRef is not a safe branch or ref name",
        ));
    }
    Ok(value.to_string())
}

async fn verify_ref_reachable(
    git_binary: &Path,
    repo_url: &str,
    git_ref: &str,
    timeout_seconds: u64,
) -> Result<(), AppError> {
    let mut command = Command::new(git_binary);
    command.kill_on_drop(true);
    command
        .arg("ls-remote")
        .arg("--exit-code")
        .arg(repo_url)
        .arg(git_ref)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = timeout(
        Duration::from_secs(timeout_seconds.clamp(5, 60)),
        command.output(),
    )
    .await
    .map_err(|_| AppError::bad_request("git ls-remote timed out"))?
    .map_err(|err| AppError::bad_request(format!("failed to run git ls-remote: {err}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit code {:?}", output.status.code())
    };
    Err(AppError::bad_request(format!(
        "git repo/ref is not reachable: {detail}"
    )))
}

fn merge_repo_ref(
    repos: Vec<DevSelftestGitRepo>,
    repo_url: &str,
    git_ref: &str,
    set_default: bool,
) -> Vec<DevSelftestGitRepo> {
    let mut repos = normalize_repos(repos);
    let repo_index = repos.iter().position(|repo| repo.url == repo_url);
    let mut entry = match repo_index {
        Some(index) => repos.remove(index),
        None => DevSelftestGitRepo {
            url: repo_url.to_string(),
            refs: Vec::new(),
        },
    };
    if let Some(index) = entry.refs.iter().position(|value| value == git_ref) {
        if set_default {
            let value = entry.refs.remove(index);
            entry.refs.insert(0, value);
        }
    } else if set_default {
        entry.refs.insert(0, git_ref.to_string());
    } else {
        entry.refs.push(git_ref.to_string());
    }
    if set_default {
        repos.insert(0, entry);
    } else {
        match repo_index {
            Some(index) => repos.insert(index.min(repos.len()), entry),
            None => repos.push(entry),
        }
    }
    repos
}

fn normalize_repos(repos: Vec<DevSelftestGitRepo>) -> Vec<DevSelftestGitRepo> {
    let mut normalized: Vec<DevSelftestGitRepo> = Vec::new();
    for repo in repos {
        let url = repo.url.trim().to_string();
        if url.is_empty() {
            continue;
        }
        let refs = repo
            .refs
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .fold(Vec::<String>::new(), |mut acc, value| {
                if !acc.contains(&value) {
                    acc.push(value);
                }
                acc
            });
        if refs.is_empty() {
            continue;
        }
        if let Some(existing) = normalized.iter_mut().find(|existing| existing.url == url) {
            for git_ref in refs {
                if !existing.refs.contains(&git_ref) {
                    existing.refs.push(git_ref);
                }
            }
        } else {
            normalized.push(DevSelftestGitRepo { url, refs });
        }
    }
    normalized
}

fn persist_repos(config_path: &Path, repos: &[DevSelftestGitRepo]) -> Result<(), AppError> {
    let raw = fs::read_to_string(config_path).unwrap_or_default();
    let mut yaml = if raw.trim().is_empty() {
        YamlValue::Mapping(Mapping::new())
    } else {
        serde_yaml::from_str::<YamlValue>(&raw)
            .map_err(|err| AppError::bad_request(format!("failed to parse config YAML: {err}")))?
    };
    let root = ensure_mapping(&mut yaml);
    let dev_selftest = root
        .entry(yaml_key("dev_selftest"))
        .or_insert_with(|| YamlValue::Mapping(Mapping::new()));
    let dev_selftest = ensure_mapping(dev_selftest);
    let git = dev_selftest
        .entry(yaml_key("git"))
        .or_insert_with(|| YamlValue::Mapping(Mapping::new()));
    let git = ensure_mapping(git);
    git.insert(yaml_key("repos"), repos_yaml(repos));

    let encoded = serde_yaml::to_string(&yaml)
        .map_err(|err| AppError::internal(format!("failed to encode config YAML: {err}")))?;
    atomic_write(config_path, encoded.as_bytes())
}

fn ensure_mapping(value: &mut YamlValue) -> &mut Mapping {
    if !matches!(value, YamlValue::Mapping(_)) {
        *value = YamlValue::Mapping(Mapping::new());
    }
    value.as_mapping_mut().expect("mapping ensured")
}

fn yaml_key(value: &str) -> YamlValue {
    YamlValue::String(value.to_string())
}

fn repos_yaml(repos: &[DevSelftestGitRepo]) -> YamlValue {
    YamlValue::Sequence(
        repos
            .iter()
            .map(|repo| {
                let mut map = Mapping::new();
                map.insert(yaml_key("url"), YamlValue::String(repo.url.clone()));
                map.insert(
                    yaml_key("refs"),
                    YamlValue::Sequence(
                        repo.refs
                            .iter()
                            .map(|git_ref| YamlValue::String(git_ref.clone()))
                            .collect(),
                    ),
                );
                YamlValue::Mapping(map)
            })
            .collect(),
    )
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::internal(format!(
                "failed to create config directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("logagent.yaml");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let tmp_name = format!(".{file_name}.tmp-{}-{nanos}", std::process::id());
    let tmp_path = path
        .parent()
        .map(|parent| parent.join(&tmp_name))
        .unwrap_or_else(|| PathBuf::from(&tmp_name));
    fs::write(&tmp_path, bytes).map_err(|err| {
        AppError::internal(format!(
            "failed to write temp config {}: {err}",
            tmp_path.display()
        ))
    })?;
    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(AppError::internal(format!(
            "failed to replace config {}: {err}",
            path.display()
        )));
    }
    Ok(())
}

fn redact_repo_url(value: &str) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(value) else {
        return value.to_string();
    };
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("redacted"));
    }
    if matches!(parsed.scheme(), "http" | "https") && !parsed.username().is_empty() {
        let _ = parsed.set_username("redacted");
    }
    parsed.to_string()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{
        app::AppState,
        services::{dev_selftest::SYNC_WORKSPACE_ID, tools::build_tool_run_task},
        support::config::load_config,
    };
    use chrono::Utc;
    use serde_json::json;
    use std::{os::unix::fs::PermissionsExt, sync::Arc};

    #[tokio::test]
    async fn update_preserves_old_allowlist_sets_default_and_affects_sync_validation() {
        let (state, root, config_path) = test_state("allowlist-update", true);

        let rejected = build_tool_run_task(
            &state,
            SYNC_WORKSPACE_ID,
            Vec::new(),
            &json!({
                "gitRepo": "https://example.test/project.git",
                "gitRef": "feature"
            }),
        )
        .await;
        assert!(rejected
            .unwrap_err()
            .to_string()
            .contains("configured allowlist"));

        let response = update_allowlist(
            &state,
            AllowlistUpdateRequest {
                repo_url: "https://example.test/project.git".to_string(),
                git_ref: "feature".to_string(),
                set_default: true,
                confirmed_user_consent: true,
                reason: Some("test".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.updated);
        assert_eq!(response.summary.default_git_ref.as_deref(), Some("feature"));
        assert_eq!(
            response.summary.git_repos[0].refs,
            vec!["feature".to_string(), "main".to_string()]
        );

        build_tool_run_task(
            &state,
            SYNC_WORKSPACE_ID,
            Vec::new(),
            &json!({
                "gitRepo": "https://example.test/project.git",
                "gitRef": "feature"
            }),
        )
        .await
        .unwrap();

        let loaded = load_config(&config_path).unwrap();
        assert_eq!(
            loaded.dev_selftest.git.repos[0].url,
            "https://example.test/project.git"
        );
        assert_eq!(
            loaded.dev_selftest.git.repos[0].refs,
            vec!["feature".to_string(), "main".to_string()]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn update_rejects_missing_consent_bad_inputs_and_unreachable_ref() {
        let (state, root, _config_path) = test_state("allowlist-rejects", true);
        let no_consent = update_allowlist(
            &state,
            AllowlistUpdateRequest {
                repo_url: "https://example.test/project.git".to_string(),
                git_ref: "feature".to_string(),
                set_default: true,
                confirmed_user_consent: false,
                reason: None,
            },
        )
        .await;
        assert!(no_consent.unwrap_err().to_string().contains("Consent"));

        let bad_url = update_allowlist(
            &state,
            AllowlistUpdateRequest {
                repo_url: "file:///tmp/repo".to_string(),
                git_ref: "feature".to_string(),
                set_default: true,
                confirmed_user_consent: true,
                reason: None,
            },
        )
        .await;
        assert!(bad_url.unwrap_err().to_string().contains("http"));

        let bad_ref = update_allowlist(
            &state,
            AllowlistUpdateRequest {
                repo_url: "https://example.test/project.git".to_string(),
                git_ref: "bad ref".to_string(),
                set_default: true,
                confirmed_user_consent: true,
                reason: None,
            },
        )
        .await;
        assert!(bad_ref.unwrap_err().to_string().contains("gitRef"));

        let missing = update_allowlist(
            &state,
            AllowlistUpdateRequest {
                repo_url: "https://example.test/project.git".to_string(),
                git_ref: "missing".to_string(),
                set_default: true,
                confirmed_user_consent: true,
                reason: None,
            },
        )
        .await;
        assert!(missing.unwrap_err().to_string().contains("not reachable"));

        let _ = std::fs::remove_dir_all(root);
    }

    fn test_state(
        prefix: &str,
        dev_selftest_enabled: bool,
    ) -> (Arc<AppState>, std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let fake_git = write_fake_git(&root);
        let fake_docker =
            write_fake_binary(&root, "fake-docker.sh", "#!/usr/bin/env bash\nexit 0\n");
        let config_path = root.join("logagent.yaml");
        let env_name = format!(
            "LOGAGENT_TEST_KEY_{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        );
        unsafe {
            std::env::set_var(&env_name, "test-key");
        }
        std::fs::write(
            &config_path,
            format!(
                r#"
auth:
  api_keys:
    - name: test
      value_env: {env_name}
storage:
  data_dir: {}
mcp:
  transport: stdio
dev_selftest:
  enabled: {dev_selftest_enabled}
  git:
    enabled: true
    binary: {}
    repos:
      - url: https://example.test/project.git
        refs: [main]
  docker:
    binary: {}
"#,
                root.join("data").display(),
                fake_git.display(),
                fake_docker.display()
            ),
        )
        .unwrap();
        let config = load_config(&config_path).unwrap();
        config.prepare_dirs().unwrap();
        (
            AppState::new_with_config_path(config, Some(config_path.clone())).unwrap(),
            root,
            config_path,
        )
    }

    fn write_fake_git(root: &std::path::Path) -> std::path::PathBuf {
        write_fake_binary(
            root,
            "fake-git.sh",
            r#"#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "ls-remote" ]; then
  ref="${4:-}"
  if [ "$ref" = "missing" ]; then
    echo "missing ref" >&2
    exit 2
  fi
  echo "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa refs/heads/$ref"
  exit 0
fi
if [ "${1:-}" = "clone" ]; then
  dest="${@: -1}"
  mkdir -p "$dest/.git"
  exit 0
fi
exit 0
"#,
        )
    }

    fn write_fake_binary(root: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = root.join(name);
        std::fs::write(&path, content).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }
}
