use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value as YamlValue};

use crate::{
    app::AppState,
    support::{
        config::{
            normalize_dev_selftest_docker_target, validate_dev_selftest_profile_id,
            DevSelftestBuildProfile, DevSelftestTestDocker, DevSelftestTestSuite,
        },
        docker_target::{validate_docker_target, DockerTargetSpec},
        error::AppError,
    },
};

pub const PROFILE_UPSERT_TOOL_ID: &str = "logagent.dev_selftest.profiles.upsert";

#[derive(Debug)]
pub struct DevSelftestProfileRegistry {
    profiles: RwLock<DevSelftestProfilesSnapshot>,
}

impl DevSelftestProfileRegistry {
    pub fn new(
        builds: BTreeMap<String, DevSelftestBuildProfile>,
        test_suites: BTreeMap<String, DevSelftestTestSuite>,
    ) -> Self {
        Self {
            profiles: RwLock::new(DevSelftestProfilesSnapshot {
                builds,
                test_suites,
            }),
        }
    }

    pub fn snapshot(&self) -> DevSelftestProfilesSnapshot {
        self.profiles
            .read()
            .map(|profiles| profiles.clone())
            .unwrap_or_default()
    }

    fn update_and_persist(
        &self,
        config_path: &Path,
        request: &ProfileUpsertRequest,
        profile: UpsertedProfile,
    ) -> Result<(DevSelftestProfilesSnapshot, bool), AppError> {
        let mut guard = self
            .profiles
            .write()
            .map_err(|_| AppError::internal("dev_selftest profile registry lock is poisoned"))?;
        let before = guard.clone();
        let mut after = before.clone();
        match profile {
            UpsertedProfile::Build(build) => {
                after.builds.insert(request.id.clone(), build);
                persist_profile(
                    config_path,
                    request.kind,
                    &request.id,
                    after.builds.get(&request.id),
                    None,
                )?;
            }
            UpsertedProfile::Test(suite) => {
                after.test_suites.insert(request.id.clone(), suite);
                persist_profile(
                    config_path,
                    request.kind,
                    &request.id,
                    None,
                    after.test_suites.get(&request.id),
                )?;
            }
        }
        let changed = after != before;
        *guard = after.clone();
        Ok((after, changed))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestProfilesSnapshot {
    pub builds: BTreeMap<String, DevSelftestBuildProfile>,
    pub test_suites: BTreeMap<String, DevSelftestTestSuite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    Build,
    Test,
}

impl ProfileKind {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "build" => Ok(Self::Build),
            "test" | "test_suite" | "tests" => Ok(Self::Test),
            _ => Err(AppError::bad_request("profile kind must be build or test")),
        }
    }

    fn as_path_key(self) -> &'static str {
        match self {
            Self::Build => "builds",
            Self::Test => "test_suites",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileUpsertRequest {
    pub kind: ProfileKind,
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub image: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub artifact_globs: Vec<String>,
    #[serde(default)]
    pub confirmed_user_consent: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileUpsertBody {
    #[serde(default)]
    pub display_name: Option<String>,
    pub image: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub artifact_globs: Vec<String>,
    #[serde(default)]
    pub confirmed_user_consent: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

impl ProfileUpsertBody {
    pub fn into_request(self, kind: ProfileKind, id: String) -> ProfileUpsertRequest {
        ProfileUpsertRequest {
            kind,
            id,
            display_name: self.display_name,
            image: self.image,
            argv: self.argv,
            timeout_seconds: self.timeout_seconds,
            network: self.network,
            workdir: self.workdir,
            volumes: self.volumes,
            env: self.env,
            artifact_globs: self.artifact_globs,
            confirmed_user_consent: self.confirmed_user_consent,
            reason: self.reason,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpsertResponse {
    pub schema_version: u32,
    pub updated: bool,
    pub kind: ProfileKind,
    pub id: String,
    pub reason: Option<String>,
    pub config_path: Option<String>,
    pub profiles: DevSelftestProfilesResponse,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestProfilesResponse {
    pub schema_version: u32,
    pub build_profiles: Vec<DevSelftestProfileSummary>,
    pub test_suites: Vec<DevSelftestProfileSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestProfileSummary {
    pub id: String,
    pub kind: String,
    pub enabled: bool,
    pub image: Option<String>,
    pub display_name: String,
    pub timeout_seconds: Option<u64>,
}

enum UpsertedProfile {
    Build(DevSelftestBuildProfile),
    Test(DevSelftestTestSuite),
}

pub fn profiles_response(snapshot: &DevSelftestProfilesSnapshot) -> DevSelftestProfilesResponse {
    DevSelftestProfilesResponse {
        schema_version: 1,
        build_profiles: build_summaries(snapshot),
        test_suites: test_summaries(snapshot),
    }
}

pub fn build_summaries(snapshot: &DevSelftestProfilesSnapshot) -> Vec<DevSelftestProfileSummary> {
    snapshot
        .builds
        .iter()
        .map(|(id, profile)| DevSelftestProfileSummary {
            id: id.clone(),
            kind: if profile.docker.is_some() {
                "docker".to_string()
            } else {
                "host".to_string()
            },
            enabled: true,
            image: profile.docker.as_ref().map(|docker| docker.image.clone()),
            display_name: profile.display_name.clone(),
            timeout_seconds: profile.timeout_seconds,
        })
        .collect()
}

pub fn test_summaries(snapshot: &DevSelftestProfilesSnapshot) -> Vec<DevSelftestProfileSummary> {
    snapshot
        .test_suites
        .iter()
        .map(|(id, suite)| DevSelftestProfileSummary {
            id: id.clone(),
            kind: if suite.docker.is_some() {
                "docker".to_string()
            } else {
                "host".to_string()
            },
            enabled: true,
            image: suite.docker.as_ref().map(|docker| docker.image.clone()),
            display_name: suite.display_name.clone(),
            timeout_seconds: suite.timeout_seconds,
        })
        .collect()
}

pub fn get_profiles(state: &AppState) -> DevSelftestProfilesResponse {
    profiles_response(&state.dev_selftest_profiles.snapshot())
}

pub async fn upsert_profile(
    state: &std::sync::Arc<AppState>,
    request: ProfileUpsertRequest,
) -> Result<ProfileUpsertResponse, AppError> {
    if !request.confirmed_user_consent {
        return Err(AppError::bad_request(
            "confirmedUserConsent must be true before updating dev_selftest profiles",
        ));
    }
    validate_dev_selftest_profile_id(&request.id)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    let profile = build_profile_from_request(&request)?;
    let config_path = state
        .config_path
        .as_ref()
        .ok_or_else(|| AppError::bad_request("server config path is unavailable"))?;
    let (snapshot, updated) =
        state
            .dev_selftest_profiles
            .update_and_persist(config_path, &request, profile)?;
    Ok(ProfileUpsertResponse {
        schema_version: 1,
        updated,
        kind: request.kind,
        id: request.id,
        reason: request
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        config_path: Some(config_path.display().to_string()),
        profiles: profiles_response(&snapshot),
    })
}

fn build_profile_from_request(request: &ProfileUpsertRequest) -> Result<UpsertedProfile, AppError> {
    let argv = request
        .argv
        .iter()
        .map(|arg| arg.trim().to_string())
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    if argv.is_empty() {
        return Err(AppError::bad_request("argv must not be empty"));
    }
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| request.id.clone());
    let docker = normalize_dev_selftest_docker_target(DockerTargetSpec {
        image: request.image.clone(),
        network: request.network.clone(),
        workdir: request.workdir.clone(),
        volumes: request.volumes.clone(),
        env: request.env.clone(),
    });
    let context = match request.kind {
        ProfileKind::Build => format!("dev_selftest.builds.{}.docker", request.id),
        ProfileKind::Test => format!("dev_selftest.test_suites.{}.docker", request.id),
    };
    validate_docker_target(&docker, &context, true)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    let timeout_seconds = request.timeout_seconds.map(|value| value.max(1));
    Ok(match request.kind {
        ProfileKind::Build => UpsertedProfile::Build(DevSelftestBuildProfile {
            display_name,
            command: argv,
            working_dir: String::new(),
            artifact_globs: request
                .artifact_globs
                .iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
            timeout_seconds,
            docker: Some(docker),
        }),
        ProfileKind::Test => UpsertedProfile::Test(DevSelftestTestSuite {
            display_name,
            argv,
            command: None,
            timeout_seconds,
            env: BTreeMap::new(),
            docker: Some(docker),
        }),
    })
}

fn persist_profile(
    config_path: &Path,
    kind: ProfileKind,
    id: &str,
    build: Option<&DevSelftestBuildProfile>,
    suite: Option<&DevSelftestTestSuite>,
) -> Result<(), AppError> {
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
    let profiles = dev_selftest
        .entry(yaml_key(kind.as_path_key()))
        .or_insert_with(|| YamlValue::Mapping(Mapping::new()));
    let profiles = ensure_mapping(profiles);
    let value = match kind {
        ProfileKind::Build => build_yaml(build.expect("build profile provided")),
        ProfileKind::Test => test_yaml(suite.expect("test suite provided")),
    };
    profiles.insert(yaml_key(id), value);

    let encoded = serde_yaml::to_string(&yaml)
        .map_err(|err| AppError::internal(format!("failed to encode config YAML: {err}")))?;
    atomic_write(config_path, encoded.as_bytes())
}

fn build_yaml(profile: &DevSelftestBuildProfile) -> YamlValue {
    let mut map = Mapping::new();
    map.insert(
        yaml_key("display_name"),
        YamlValue::String(profile.display_name.clone()),
    );
    map.insert(yaml_key("argv"), string_sequence(&profile.command));
    map.insert(yaml_key("working_dir"), YamlValue::String(String::new()));
    map.insert(
        yaml_key("artifact_globs"),
        string_sequence(&profile.artifact_globs),
    );
    if let Some(timeout) = profile.timeout_seconds {
        map.insert(
            yaml_key("timeout_seconds"),
            YamlValue::Number(timeout.into()),
        );
    }
    if let Some(docker) = &profile.docker {
        map.insert(yaml_key("docker"), docker_yaml(docker));
    }
    YamlValue::Mapping(map)
}

fn test_yaml(suite: &DevSelftestTestSuite) -> YamlValue {
    let mut map = Mapping::new();
    map.insert(
        yaml_key("display_name"),
        YamlValue::String(suite.display_name.clone()),
    );
    map.insert(yaml_key("argv"), string_sequence(&suite.argv));
    if let Some(timeout) = suite.timeout_seconds {
        map.insert(
            yaml_key("timeout_seconds"),
            YamlValue::Number(timeout.into()),
        );
    }
    if let Some(docker) = &suite.docker {
        map.insert(yaml_key("docker"), docker_yaml(docker));
    }
    YamlValue::Mapping(map)
}

fn docker_yaml(docker: &DevSelftestTestDocker) -> YamlValue {
    let mut map = Mapping::new();
    map.insert(yaml_key("image"), YamlValue::String(docker.image.clone()));
    if let Some(network) = &docker.network {
        map.insert(yaml_key("network"), YamlValue::String(network.clone()));
    }
    if let Some(workdir) = &docker.workdir {
        map.insert(yaml_key("workdir"), YamlValue::String(workdir.clone()));
    }
    if !docker.volumes.is_empty() {
        map.insert(yaml_key("volumes"), string_sequence(&docker.volumes));
    }
    if !docker.env.is_empty() {
        let mut env = Mapping::new();
        for (key, value) in &docker.env {
            env.insert(yaml_key(key), YamlValue::String(value.clone()));
        }
        map.insert(yaml_key("env"), YamlValue::Mapping(env));
    }
    YamlValue::Mapping(map)
}

fn string_sequence(values: &[String]) -> YamlValue {
    YamlValue::Sequence(
        values
            .iter()
            .map(|value| YamlValue::String(value.clone()))
            .collect(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::AppState,
        services::{dev_selftest::BUILD_ID, tools::build_tool_run_task},
        support::config::load_config,
    };
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn upsert_rejects_missing_consent_and_bad_volume() {
        let (state, root, _config_path) = test_state("profile-upsert-rejects");

        let no_consent = upsert_profile(
            &state,
            ProfileUpsertRequest {
                kind: ProfileKind::Build,
                id: "dyn_build".to_string(),
                display_name: None,
                image: "alpine:3.20".to_string(),
                argv: vec!["build".to_string()],
                timeout_seconds: None,
                network: None,
                workdir: None,
                volumes: Vec::new(),
                env: BTreeMap::new(),
                artifact_globs: Vec::new(),
                confirmed_user_consent: false,
                reason: None,
            },
        )
        .await;
        assert!(no_consent.unwrap_err().to_string().contains("Consent"));

        let bad_volume = upsert_profile(
            &state,
            ProfileUpsertRequest {
                kind: ProfileKind::Test,
                id: "dyn_test".to_string(),
                display_name: None,
                image: "alpine:3.20".to_string(),
                argv: vec!["sh".to_string(), "/tests/smoke.sh".to_string()],
                timeout_seconds: Some(30),
                network: Some("host".to_string()),
                workdir: None,
                volumes: vec!["relative:/tests:ro".to_string()],
                env: BTreeMap::new(),
                artifact_globs: Vec::new(),
                confirmed_user_consent: true,
                reason: None,
            },
        )
        .await;
        assert!(bad_volume.unwrap_err().to_string().contains("volumes"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn upsert_persists_build_profile_and_affects_validation() {
        let (state, root, config_path) = test_state("profile-upsert-build");

        let response = upsert_profile(
            &state,
            ProfileUpsertRequest {
                kind: ProfileKind::Build,
                id: "dyn_build".to_string(),
                display_name: Some("Dynamic build".to_string()),
                image: "builder:latest".to_string(),
                argv: vec!["/usr/local/bin/build".to_string()],
                timeout_seconds: Some(90),
                network: Some("host".to_string()),
                workdir: Some("/workspace/source".to_string()),
                volumes: vec!["${DEVSELFTEST_SOURCE_DIR}/cache:/cache:rw".to_string()],
                env: BTreeMap::from([("BUILD_MODE".to_string(), "ci".to_string())]),
                artifact_globs: vec!["build/app".to_string()],
                confirmed_user_consent: true,
                reason: Some("test".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.updated);
        assert!(response
            .profiles
            .build_profiles
            .iter()
            .any(|profile| profile.id == "dyn_build" && profile.kind == "docker"));

        let task = build_tool_run_task(
            &state,
            BUILD_ID,
            Vec::new(),
            &json!({"runId":"devselftest_1","buildProfile":"dyn_build"}),
        )
        .await
        .unwrap();
        assert_eq!(
            task.tool_params["profileSnapshot"]["docker"]["image"],
            "builder:latest"
        );

        let loaded = load_config(&config_path).unwrap();
        let loaded_profile = loaded.dev_selftest.builds.get("dyn_build").unwrap();
        assert_eq!(loaded_profile.command, vec!["/usr/local/bin/build"]);
        assert_eq!(loaded_profile.artifact_globs, vec!["build/app"]);
        assert_eq!(
            loaded_profile.docker.as_ref().unwrap().workdir.as_deref(),
            Some("/workspace/source")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn test_state(prefix: &str) -> (Arc<AppState>, std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
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
  enabled: true
  git:
    enabled: false
  docker:
    binary: /usr/bin/docker
"#,
                root.join("data").display(),
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
}
