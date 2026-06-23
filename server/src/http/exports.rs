use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Write},
    path::Path,
    sync::Arc,
};

use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use zip::{
    write::{SimpleFileOptions, ZipWriter},
    CompressionMethod,
};

use crate::{
    app::AppState,
    services::skill_registry::{SkillExportEntry, SkillExportFile},
    support::{config::ToolSettings, error::AppError},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillsPackageManifest {
    schema_version: u32,
    generated_at: DateTime<Utc>,
    skills: Vec<SkillManifestEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillManifestEntry {
    skill_id: String,
    display_name: String,
    revision: String,
    source_root: String,
    source_path: String,
    files: Vec<SkillManifestFile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillManifestFile {
    path: String,
    zip_path: String,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolsPackageManifest {
    schema_version: u32,
    generated_at: DateTime<Utc>,
    server_os: &'static str,
    server_arch: &'static str,
    tools: Vec<ToolManifestEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolManifestEntry {
    tool_id: String,
    display_name: String,
    configured_args: Vec<String>,
    match_rules: ToolMatchManifest,
    server_os: &'static str,
    server_arch: &'static str,
    binary_filename: Option<String>,
    sha256: Option<String>,
    size: Option<u64>,
    packaged: bool,
    skipped: bool,
    skip_reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolMatchManifest {
    file_patterns: Vec<String>,
    keywords: Vec<String>,
}

pub async fn skills_zip(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let bytes = build_skills_zip(&state)
        .map_err(|err| AppError::internal(format!("failed to build skills.zip: {err:#}")))?;
    Ok(zip_response("skills.zip", bytes))
}

pub async fn tools_zip(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let bytes = build_tools_zip(&state)
        .map_err(|err| AppError::internal(format!("failed to build tools.zip: {err:#}")))?;
    Ok(zip_response("tools.zip", bytes))
}

fn zip_response(filename: &'static str, bytes: Vec<u8>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(match filename {
            "skills.zip" => "attachment; filename=\"skills.zip\"",
            "tools.zip" => "attachment; filename=\"tools.zip\"",
            _ => "attachment",
        }),
    );
    (headers, bytes)
}

fn build_skills_zip(state: &AppState) -> anyhow::Result<Vec<u8>> {
    let entries = state.skills.export_entries()?;
    let mut manifest = SkillsPackageManifest {
        schema_version: 1,
        generated_at: Utc::now(),
        skills: Vec::new(),
    };
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let file_options = text_options();
    let mut used_paths = HashSet::new();

    for entry in entries {
        let mut manifest_files = Vec::new();
        for file in &entry.files {
            let zip_path = skill_zip_path(&entry, file)?;
            if !used_paths.insert(zip_path.clone()) {
                anyhow::bail!("duplicate skill export path {zip_path}");
            }
            writer.start_file(&zip_path, file_options)?;
            let bytes = std::fs::read(&file.absolute_path)?;
            writer.write_all(&bytes)?;
            manifest_files.push(SkillManifestFile {
                path: file.relative_path.clone(),
                zip_path,
                size: file.size,
            });
        }
        manifest.skills.push(SkillManifestEntry {
            skill_id: entry.skill_id,
            display_name: entry.display_name,
            revision: entry.revision,
            source_root: entry.source_root.display().to_string(),
            source_path: entry.source_path.display().to_string(),
            files: manifest_files,
        });
    }

    writer.start_file("manifest.json", file_options)?;
    writer.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    Ok(writer.finish()?.into_inner())
}

fn build_tools_zip(state: &AppState) -> anyhow::Result<Vec<u8>> {
    let display_names = crate::services::tools::descriptors(&state.config)
        .into_iter()
        .map(|descriptor| (descriptor.tool_id, descriptor.display_name))
        .collect::<BTreeMap<_, _>>();
    let mut manifest = ToolsPackageManifest {
        schema_version: 1,
        generated_at: Utc::now(),
        server_os: std::env::consts::OS,
        server_arch: std::env::consts::ARCH,
        tools: Vec::new(),
    };
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let file_options = text_options();
    let exec_options = executable_options();
    writer.start_file("README.md", file_options)?;
    writer.write_all(tools_package_readme().as_bytes())?;

    for tool in state
        .config
        .tools
        .tools
        .values()
        .filter(|tool| tool.enabled)
    {
        let display_name = display_names
            .get(&tool.name)
            .cloned()
            .unwrap_or_else(|| tool.name.clone());
        let mut entry = ToolManifestEntry {
            tool_id: tool.name.clone(),
            display_name,
            configured_args: tool.args.clone(),
            match_rules: ToolMatchManifest {
                file_patterns: tool.match_settings.file_patterns.clone(),
                keywords: tool.match_settings.keywords.clone(),
            },
            server_os: std::env::consts::OS,
            server_arch: std::env::consts::ARCH,
            binary_filename: None,
            sha256: None,
            size: None,
            packaged: false,
            skipped: true,
            skip_reason: None,
        };
        match package_tool(&mut writer, tool, exec_options) {
            Ok(package) => {
                entry.binary_filename = Some(package.binary_filename.clone());
                entry.sha256 = Some(package.sha256);
                entry.size = Some(package.size);
                entry.packaged = true;
                entry.skipped = false;
                entry.skip_reason = None;
                writer.start_file(format!("wrappers/{}.sh", tool.name), exec_options)?;
                writer.write_all(tool_wrapper(&tool.name, &package.binary_filename).as_bytes())?;
            }
            Err(err) => {
                entry.skip_reason = Some(format!("{err:#}"));
            }
        }
        writer.start_file(format!("config/examples/{}.yaml", tool.name), file_options)?;
        writer
            .write_all(tool_config_example(tool, entry.binary_filename.as_deref())?.as_bytes())?;
        manifest.tools.push(entry);
    }

    writer.start_file("tools-manifest.json", file_options)?;
    writer.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    Ok(writer.finish()?.into_inner())
}

struct PackagedTool {
    binary_filename: String,
    sha256: String,
    size: u64,
}

fn package_tool<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    tool: &ToolSettings,
    options: SimpleFileOptions,
) -> anyhow::Result<PackagedTool> {
    let resolved = std::fs::canonicalize(&tool.path)?;
    let metadata = std::fs::metadata(&resolved)?;
    if !metadata.is_file() {
        anyhow::bail!("resolved path is not a regular file");
    }
    if !is_executable(&metadata) {
        anyhow::bail!("resolved path is not executable");
    }
    let binary_filename = tool
        .path
        .file_name()
        .or_else(|| resolved.file_name())
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("tool path has no valid binary filename"))?
        .to_string();
    validate_zip_segment(&tool.name)?;
    validate_zip_segment(&binary_filename)?;
    let bytes = std::fs::read(&resolved)?;
    let sha256 = hex_sha256(&bytes);
    writer.start_file(format!("bin/{}/{}", tool.name, binary_filename), options)?;
    writer.write_all(&bytes)?;
    Ok(PackagedTool {
        binary_filename,
        sha256,
        size: metadata.len(),
    })
}

fn skill_zip_path(entry: &SkillExportEntry, file: &SkillExportFile) -> anyhow::Result<String> {
    validate_zip_path(&entry.zip_dir)?;
    validate_zip_path(&file.relative_path)?;
    Ok(format!("{}/{}", entry.zip_dir, file.relative_path))
}

fn text_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn executable_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_zip_path(path: &str) -> anyhow::Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        anyhow::bail!("unsafe zip path {path}");
    }
    Ok(())
}

fn validate_zip_segment(value: &str) -> anyhow::Result<()> {
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        anyhow::bail!("unsafe zip path segment {value}");
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    !metadata.permissions().readonly()
}

fn tool_wrapper(tool_id: &str, binary_filename: &str) -> String {
    format!(
        "#!/usr/bin/env sh\nset -eu\nSCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nexec \"$SCRIPT_DIR\"/../bin/{}/{} \"$@\"\n",
        shell_quote_segment(tool_id),
        shell_quote_segment(binary_filename)
    )
}

fn shell_quote_segment(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn tool_config_example(
    tool: &ToolSettings,
    packaged_binary: Option<&str>,
) -> anyhow::Result<String> {
    let path = packaged_binary
        .map(|binary| format!("./bin/{}/{}", tool.name, binary))
        .unwrap_or_else(|| "/absolute/path/to/tool".to_string());
    let mut tools = serde_json::Map::new();
    tools.insert(
        tool.name.clone(),
        serde_json::json!({
            "enabled": true,
            "path": path,
            "timeout_seconds": tool.timeout_seconds,
            "max_output_bytes": tool.max_output_bytes,
            "max_input_files": tool.max_input_files,
            "args": tool.args,
            "match": {
                "file_patterns": tool.match_settings.file_patterns,
                "keywords": tool.match_settings.keywords
            }
        }),
    );
    let value = serde_json::json!({ "tools": tools });
    Ok(serde_yaml::to_string(&value)?)
}

fn tools_package_readme() -> String {
    format!(
        "# LogAgent Tools Package\n\nThis package is a snapshot of enabled executable tools on the LogAgent Server host.\n\n- Server platform: {}/{}\n- Binaries are under `bin/<tool_id>/`.\n- Shell wrappers are under `wrappers/` for tools that were packaged successfully.\n- `tools-manifest.json` records configured args, match rules, sha256, size, and skipped tools.\n- No API keys, environment variable values, server config files, uploads, or workspaces are included.\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::{collections::BTreeMap, io::Read, path::PathBuf, sync::Arc};
    use zip::ZipArchive;

    use crate::support::config::{
        AppConfig, AuthSettings, LogAnalyzerSettings, McpSettings, ServerSettings, SkillSettings,
        StorageSettings, ToolMatchSettings, ToolSettings, ToolsSettings,
    };

    #[test]
    fn skills_zip_contains_skill_tree_manifest_and_skips_symlinks() {
        let root = temp_root("skills-zip");
        write_skill(&root);
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            root.join("outside-secret.md"),
            root.join("skills/opengemini-diagnosis/references/linked.md"),
        )
        .unwrap();
        std::fs::write(root.join("outside-secret.md"), "secret").unwrap();
        let state = test_state(&root, BTreeMap::new());

        let bytes = build_skills_zip(&state).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(zip.by_name("opengemini-diagnosis/SKILL.md").is_ok());
        assert!(zip
            .by_name("opengemini-diagnosis/references/topology.md")
            .is_ok());
        assert!(zip
            .by_name("opengemini-diagnosis/references/linked.md")
            .is_err());
        let manifest = read_zip_json(&mut zip, "manifest.json");
        assert_eq!(manifest["schemaVersion"], 1);
        assert_eq!(manifest["skills"][0]["skillId"], "opengemini-diagnosis");
        let files = manifest["skills"][0]["files"].as_array().unwrap();
        assert!(files.iter().any(|file| file["path"] == "SKILL.md"));
        assert!(files
            .iter()
            .all(|file| file["path"] != "references/linked.md"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tools_zip_packages_executable_and_marks_missing_tool_skipped() {
        let root = temp_root("tools-zip");
        write_skill(&root);
        let executable = write_executable(&root, "fake-tool");
        let missing = root.join("bin/missing-tool");
        let mut tools = BTreeMap::new();
        tools.insert(
            "fake_tool".to_string(),
            ToolSettings {
                name: "fake_tool".to_string(),
                enabled: true,
                path: executable,
                timeout_seconds: 5,
                max_output_bytes: 4096,
                max_input_files: 2,
                args: vec!["--input".to_string(), "{input_file}".to_string()],
                match_settings: ToolMatchSettings {
                    file_patterns: vec!["*.log".to_string()],
                    keywords: vec!["panic".to_string()],
                },
            },
        );
        tools.insert(
            "missing_tool".to_string(),
            ToolSettings {
                name: "missing_tool".to_string(),
                enabled: true,
                path: missing,
                timeout_seconds: 5,
                max_output_bytes: 4096,
                max_input_files: 1,
                args: Vec::new(),
                match_settings: ToolMatchSettings::default(),
            },
        );
        let state = test_state(&root, tools);

        let bytes = build_tools_zip(&state).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(zip.by_name("README.md").is_ok());
        assert!(zip.by_name("bin/fake_tool/fake-tool").is_ok());
        assert!(zip.by_name("wrappers/fake_tool.sh").is_ok());
        assert!(zip.by_name("config/examples/fake_tool.yaml").is_ok());
        assert!(zip.by_name("config/examples/missing_tool.yaml").is_ok());
        assert!(zip.by_name("bin/missing_tool/missing-tool").is_err());

        let manifest = read_zip_json(&mut zip, "tools-manifest.json");
        let tools = manifest["tools"].as_array().unwrap();
        assert!(tools
            .iter()
            .all(|tool| tool["toolId"] != "logagent.get_metadata_field_types"));
        assert!(tools
            .iter()
            .all(|tool| tool["toolId"] != "logagent.get_metadata_tag_fields"));
        let fake = tools
            .iter()
            .find(|tool| tool["toolId"] == "fake_tool")
            .unwrap();
        assert_eq!(fake["packaged"], true);
        assert_eq!(fake["skipped"], false);
        assert_eq!(fake["binaryFilename"], "fake-tool");
        assert_eq!(fake["sha256"].as_str().unwrap().len(), 64);
        assert!(fake["size"].as_u64().unwrap() > 0);
        let missing = tools
            .iter()
            .find(|tool| tool["toolId"] == "missing_tool")
            .unwrap();
        assert_eq!(missing["packaged"], false);
        assert_eq!(missing["skipped"], true);
        assert!(
            missing["skipReason"].as_str().unwrap().contains("No such")
                || missing["skipReason"]
                    .as_str()
                    .unwrap()
                    .contains("not found")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn read_zip_json(zip: &mut ZipArchive<Cursor<Vec<u8>>>, path: &str) -> serde_json::Value {
        let mut file = zip.by_name(path).unwrap();
        let mut raw = String::new();
        file.read_to_string(&mut raw).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    fn test_state(root: &PathBuf, tools: BTreeMap<String, ToolSettings>) -> Arc<AppState> {
        let config = Arc::new(AppConfig {
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 1,
                max_input_chars: 60_000,
            },
            auth: AuthSettings {
                api_keys: vec!["test-key".to_string()],
            },
            storage: StorageSettings {
                data_dir: root.join("data"),
                max_upload_bytes: 1024 * 1024,
                max_chunk_bytes: 512 * 1024,
            },
            skills: SkillSettings {
                enabled: true,
                roots: vec![root.join("skills")],
                max_skill_chars: 4000,
                max_reference_chars: 20_000,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings { tools },
            fetch: crate::support::config::FetchSettings::default(),
            huawei_cloud: crate::support::config::HuaweiCloudSettings::default(),
            remote_execution: crate::support::config::RemoteExecutionSettings::default(),
            mcp: McpSettings::default(),
            dev_selftest: crate::support::config::DevSelftestSettings::default(),
        });
        config.prepare_dirs().unwrap();
        AppState::new(config).unwrap()
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn write_skill(root: &PathBuf) {
        let skill = root.join("skills/opengemini-diagnosis");
        std::fs::create_dir_all(skill.join("references")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: openGemini Diagnosis\ndescription: Diagnose openGemini.\n---\nUse current evidence first.\n",
        )
        .unwrap();
        std::fs::write(
            skill.join("references/topology.md"),
            "Topology reference content.",
        )
        .unwrap();
        std::fs::write(
            skill.join("logagent.json"),
            r#"{"schemaVersion":1,"skillId":"opengemini-diagnosis","displayName":"openGemini diagnosis","products":["opengemini"],"taskKinds":["log_analysis"],"includeByDefault":true,"references":[{"path":"references/topology.md","title":"Topology","summary":"Topology rules"}]}"#,
        )
        .unwrap();
    }

    fn write_executable(root: &PathBuf, name: &str) -> PathBuf {
        let path = root.join("bin").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "#!/usr/bin/env sh\nprintf '{}\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        path
    }
}
