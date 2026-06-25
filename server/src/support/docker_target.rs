//! Shared docker target spec + validation. Used by both the dev_selftest inline docker
//! test target (`DevSelftestTestDocker`) and the managed docker executor record
//! (`RemoteExecutorRecord::docker`). Field names are single lowercase words, so the same
//! type deserializes identically from YAML config (snake_case) and JSON API (camelCase)
//! without a `rename_all`.

use std::collections::BTreeMap;

use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerTargetSpec {
    pub image: String,
    /// `None` (default) ⇒ `host`. Otherwise a safe network identifier.
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub workdir: Option<String>,
    /// `host:container[:ro|rw]`; the host side may be an absolute path, or — for dev_selftest
    /// inline targets — a `${DEVSELFTEST_*}` placeholder interpolated at run time.
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// Validate a docker target. `context` is the config/record path prefix for error messages.
/// `allow_devselftest_placeholders` permits `${DEVSELFTEST_*}` volume host placeholders
/// (dev_selftest inline targets, interpolated at run time); managed executor records pass
/// `false` (literal absolute hosts only).
pub fn validate_docker_target(
    spec: &DockerTargetSpec,
    context: &str,
    allow_devselftest_placeholders: bool,
) -> anyhow::Result<()> {
    if spec.image.is_empty() {
        bail!("{context}.image must not be empty");
    }
    if spec.image.starts_with('-') {
        bail!("{context}.image must not start with '-'");
    }
    if let Some(network) = spec.network.as_deref() {
        if !is_safe_docker_network(network) {
            bail!("{context}.network must be 'host' or a safe identifier");
        }
    }
    if let Some(workdir) = spec.workdir.as_deref() {
        if !is_safe_container_path(workdir) {
            bail!("{context}.workdir must be an absolute path without '..'");
        }
    }
    for volume in &spec.volumes {
        validate_docker_volume(context, volume, allow_devselftest_placeholders)?;
    }
    for key in spec.env.keys() {
        if !is_safe_env_name(key) {
            bail!("{context}.env has invalid key '{key}'");
        }
    }
    Ok(())
}

pub fn validate_docker_volume(
    context: &str,
    volume: &str,
    allow_devselftest_placeholders: bool,
) -> anyhow::Result<()> {
    let prefix = format!("{context}.volumes");
    let parts: Vec<&str> = volume.splitn(3, ':').collect();
    let (host, container, mode) = match parts.as_slice() {
        [host, container] => (*host, *container, None),
        [host, container, mode] => (*host, *container, Some(*mode)),
        _ => {
            bail!("{prefix}: '{volume}' must be host:container[:ro|rw]");
        }
    };
    if host.is_empty() {
        bail!("{prefix}: host must not be empty");
    }
    // Host side: an absolute path, or (for dev_selftest inline) a ${DEVSELFTEST_*} placeholder
    // interpolated at run time then re-checked absolute. Anything else (relative path,
    // flag-like token) is rejected to prevent over-broad host mounts.
    let host_ok = host.starts_with('/')
        || (allow_devselftest_placeholders && host.starts_with("${DEVSELFTEST_"));
    if !host_ok {
        if allow_devselftest_placeholders {
            bail!("{prefix}: host must be an absolute path or a ${{DEVSELFTEST_*}} placeholder");
        }
        bail!("{prefix}: host must be an absolute path");
    }
    if !is_safe_container_path(container) {
        bail!("{prefix}: container path must be absolute without '..'");
    }
    if let Some(m) = mode {
        if !matches!(m, "ro" | "rw") {
            bail!("{prefix}: mode must be ro or rw");
        }
    }
    Ok(())
}

fn is_safe_docker_network(value: &str) -> bool {
    if value == "host" {
        return true;
    }
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphanumeric())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
}

fn is_safe_container_path(value: &str) -> bool {
    value.starts_with('/') && !value.contains("..")
}

fn is_safe_env_name(key: &str) -> bool {
    let mut bytes = key.bytes();
    matches!(bytes.next(), Some(b) if b == b'_' || b.is_ascii_uppercase())
        && bytes.all(|b| b == b'_' || b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(image: &str, volumes: Vec<&str>) -> DockerTargetSpec {
        DockerTargetSpec {
            image: image.to_string(),
            network: None,
            workdir: None,
            volumes: volumes.into_iter().map(String::from).collect(),
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn accepts_valid_target() {
        let mut s = spec("alpine:3.20", vec!["/h:/c:ro"]);
        s.network = Some("host".to_string());
        s.workdir = Some("/work".to_string());
        s.env.insert("OK_VAR".to_string(), "v".to_string());
        assert!(validate_docker_target(&s, "ctx", false).is_ok());
    }

    #[test]
    fn rejects_image_issues() {
        assert!(validate_docker_target(&spec("", vec![]), "ctx", false)
            .unwrap_err()
            .to_string()
            .contains("image must not be empty"));
        assert!(validate_docker_target(&spec("-flag", vec![]), "ctx", false)
            .unwrap_err()
            .to_string()
            .contains("must not start with '-'"));
    }

    #[test]
    fn rejects_volume_issues() {
        assert!(
            validate_docker_target(&spec("alpine", vec!["relative:/c:ro"]), "ctx", false)
                .unwrap_err()
                .to_string()
                .contains("host must be an absolute path")
        );
        // dev_selftest mode allows the placeholder; managed mode rejects it.
        assert!(validate_docker_target(
            &spec("alpine", vec!["${DEVSELFTEST_X}/b:/b:ro"]),
            "ctx",
            true
        )
        .is_ok());
        assert!(validate_docker_target(
            &spec("alpine", vec!["${DEVSELFTEST_X}/b:/b:ro"]),
            "ctx",
            false
        )
        .unwrap_err()
        .to_string()
        .contains("host must be an absolute path"));
        assert!(
            validate_docker_target(&spec("alpine", vec!["/h:/c/../d"]), "ctx", false)
                .unwrap_err()
                .to_string()
                .contains("container path")
        );
        assert!(
            validate_docker_target(&spec("alpine", vec!["/h:/c:rx"]), "ctx", false)
                .unwrap_err()
                .to_string()
                .contains("mode must be ro or rw")
        );
        assert!(
            validate_docker_target(&spec("alpine", vec!["nope"]), "ctx", false)
                .unwrap_err()
                .to_string()
                .contains("must be host:container")
        );
    }

    #[test]
    fn rejects_bad_env_key() {
        let mut s = spec("alpine", vec![]);
        s.env.insert("lower".to_string(), "v".to_string());
        assert!(validate_docker_target(&s, "ctx", false)
            .unwrap_err()
            .to_string()
            .contains("invalid key"));
    }
}
