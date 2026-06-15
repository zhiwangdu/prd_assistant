use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    domain::models::{
        SkillReferenceSummary, SystemContextBundle, SystemContextBundleItem,
        SystemContextContentType, SystemContextKind, TaskKind,
    },
    support::{config::SkillSettings, error::AppError},
};

#[derive(Debug, Clone)]
pub struct SkillRegistry {
    enabled: bool,
    max_reference_chars: usize,
    skills: BTreeMap<String, DiagnosticSkill>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListResponse {
    pub skills: Vec<SkillSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub skill_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub managed: bool,
    pub include_by_default: bool,
    pub priority: i32,
    pub products: Vec<String>,
    pub domain_adapters: Vec<String>,
    pub tool_ids: Vec<String>,
    pub task_kinds: Vec<String>,
    pub revision: String,
    pub source_root: String,
    pub source_path: String,
    pub references: Vec<SkillReferenceSummary>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetailResponse {
    #[serde(flatten)]
    pub summary: SkillSummary,
    pub injection_content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreviewRequest {
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreviewResponse {
    pub resources: Vec<SystemContextBundleItem>,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct SkillReferenceRead {
    pub skill_id: String,
    pub skill_revision: String,
    pub reference: SkillReferenceSummary,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct SkillExportEntry {
    pub skill_id: String,
    pub display_name: String,
    pub revision: String,
    pub source_root: PathBuf,
    pub source_path: PathBuf,
    pub zip_dir: String,
    pub files: Vec<SkillExportFile>,
}

#[derive(Debug, Clone)]
pub struct SkillExportFile {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ResolveSkillsInput<'a> {
    pub explicit_skill_ids: &'a [String],
    pub task_kind: TaskKind,
    pub product: Option<&'a str>,
    pub version: Option<&'a str>,
    pub environment: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct DiagnosticSkill {
    skill_id: String,
    name: String,
    display_name: String,
    description: String,
    managed: bool,
    include_by_default: bool,
    priority: i32,
    max_prompt_chars: usize,
    products: Vec<String>,
    domain_adapters: Vec<String>,
    tool_ids: Vec<String>,
    task_kinds: Vec<String>,
    revision: String,
    source_root: PathBuf,
    skill_dir: PathBuf,
    skill_md_path: PathBuf,
    injection_content: String,
    references: Vec<SkillReference>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct SkillReference {
    summary: SkillReferenceSummary,
    absolute_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct SkillFrontMatter {
    name: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogAgentSkillManifest {
    schema_version: u32,
    skill_id: String,
    display_name: Option<String>,
    #[serde(default)]
    products: Vec<String>,
    #[serde(default)]
    domain_adapters: Vec<String>,
    #[serde(default)]
    tool_ids: Vec<String>,
    #[serde(default)]
    task_kinds: Vec<String>,
    #[serde(default)]
    include_by_default: bool,
    #[serde(default)]
    priority: i32,
    max_prompt_chars: Option<usize>,
    #[serde(default)]
    references: Vec<LogAgentSkillReference>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogAgentSkillReference {
    path: String,
    title: String,
    summary: String,
}

impl SkillRegistry {
    pub fn load(settings: SkillSettings) -> anyhow::Result<Self> {
        let mut skills = BTreeMap::new();
        if settings.enabled {
            for root in &settings.roots {
                if !root.exists() {
                    warn!(root = %root.display(), "skill root does not exist");
                    continue;
                }
                let canonical_root = root.canonicalize().with_context(|| {
                    format!("failed to canonicalize skill root {}", root.display())
                })?;
                for skill_dir in discover_skill_dirs(&canonical_root)? {
                    let skill =
                        load_skill_dir(&canonical_root, &skill_dir, settings.max_skill_chars)
                            .with_context(|| {
                                format!("failed to load skill {}", skill_dir.display())
                            })?;
                    if skills.insert(skill.skill_id.clone(), skill).is_some() {
                        anyhow::bail!("duplicate skillId in configured skill roots");
                    }
                }
            }
        }
        Ok(Self {
            enabled: settings.enabled,
            max_reference_chars: settings.max_reference_chars,
            skills,
        })
    }

    pub fn list(&self) -> Vec<SkillSummary> {
        self.skills.values().map(DiagnosticSkill::summary).collect()
    }

    pub fn get(&self, skill_id: &str) -> Option<SkillDetailResponse> {
        self.skills.get(skill_id).map(|skill| SkillDetailResponse {
            summary: skill.summary(),
            injection_content: skill.injection_content.clone(),
        })
    }

    pub fn export_entries(&self) -> anyhow::Result<Vec<SkillExportEntry>> {
        let mut entries = Vec::new();
        for skill in self.skills.values() {
            let mut files = Vec::new();
            collect_skill_export_files(&skill.skill_dir, &skill.skill_dir, &mut files)?;
            files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
            entries.push(SkillExportEntry {
                skill_id: skill.skill_id.clone(),
                display_name: skill.display_name.clone(),
                revision: skill.revision.clone(),
                source_root: skill.source_root.clone(),
                source_path: skill.skill_dir.clone(),
                zip_dir: skill_zip_dir(skill)?,
                files,
            });
        }
        Ok(entries)
    }

    pub fn resolve_items(
        &self,
        input: ResolveSkillsInput<'_>,
    ) -> Result<Vec<SystemContextBundleItem>, AppError> {
        if !self.enabled {
            if input.explicit_skill_ids.is_empty() {
                return Ok(Vec::new());
            }
            return Err(AppError::bad_request(
                "skills are disabled by configuration",
            ));
        }
        let mut selected = Vec::<DiagnosticSkill>::new();
        let mut seen = HashSet::<String>::new();
        for skill_id in input.explicit_skill_ids {
            validate_skill_id(skill_id)?;
            let skill = self
                .skills
                .get(skill_id)
                .ok_or_else(|| AppError::bad_request(format!("unknown skillId {skill_id}")))?;
            if seen.insert(skill.skill_id.clone()) {
                selected.push(skill.clone());
            }
        }

        let allow_auto =
            input.product.is_some() || input.version.is_some() || input.environment.is_some();
        if allow_auto {
            let mut automatic = self
                .skills
                .values()
                .filter(|skill| {
                    skill.managed
                        && skill.include_by_default
                        && matches_task_kind(&skill.task_kinds, input.task_kind)
                        && metadata_filters_match(
                            skill,
                            input.product,
                            input.version,
                            input.environment,
                        )
                })
                .cloned()
                .collect::<Vec<_>>();
            automatic.sort_by(|left, right| {
                right
                    .priority
                    .cmp(&left.priority)
                    .then_with(|| left.display_name.cmp(&right.display_name))
            });
            for skill in automatic {
                if seen.insert(skill.skill_id.clone()) {
                    selected.push(skill);
                }
            }
        }

        selected.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.display_name.cmp(&right.display_name))
        });
        Ok(selected
            .into_iter()
            .map(|skill| skill.to_system_context_item())
            .collect())
    }

    pub async fn read_reference_from_snapshot(
        &self,
        bundle: &SystemContextBundle,
        skill_id: &str,
        reference_id: Option<&str>,
        reference_path: Option<&str>,
    ) -> anyhow::Result<SkillReferenceRead> {
        validate_skill_id_anyhow(skill_id)?;
        let snapshot_item = bundle
            .resources
            .iter()
            .find(|item| {
                item.kind == SystemContextKind::DiagnosticSkill
                    && item.skill_id.as_deref() == Some(skill_id)
            })
            .ok_or_else(|| anyhow::anyhow!("skill {skill_id} was not selected for this task"))?;
        let skill = self
            .skills
            .get(skill_id)
            .ok_or_else(|| anyhow::anyhow!("skill {skill_id} is not available in registry"))?;
        if snapshot_item.revision.as_deref() != Some(skill.revision.as_str()) {
            anyhow::bail!(
                "skill {skill_id} revision differs from the task snapshot; rerun the task to refresh skill references"
            );
        }
        let requested = match (reference_id, reference_path) {
            (Some(id), _) => snapshot_item
                .references
                .iter()
                .find(|reference| reference.reference_id == id)
                .ok_or_else(|| {
                    anyhow::anyhow!("referenceId {id} is not declared by selected skill")
                })?,
            (None, Some(path)) => {
                validate_reference_path(path)?;
                snapshot_item
                    .references
                    .iter()
                    .find(|reference| reference.path == path)
                    .ok_or_else(|| {
                        anyhow::anyhow!("reference path {path} is not declared by selected skill")
                    })?
            }
            (None, None) => anyhow::bail!("referenceId or path is required"),
        };
        let reference = skill
            .references
            .iter()
            .find(|reference| reference.summary.reference_id == requested.reference_id)
            .ok_or_else(|| {
                anyhow::anyhow!("reference {} is not available", requested.reference_id)
            })?;
        let raw = tokio::fs::read_to_string(&reference.absolute_path)
            .await
            .with_context(|| format!("failed to read skill reference {}", requested.path))?;
        let (content, truncated) = truncate_chars_with_flag(raw.trim(), self.max_reference_chars);
        Ok(SkillReferenceRead {
            skill_id: skill.skill_id.clone(),
            skill_revision: skill.revision.clone(),
            reference: reference.summary.clone(),
            content,
            truncated,
        })
    }

    pub async fn read_reference(
        &self,
        skill_id: &str,
        reference_id: Option<&str>,
        reference_path: Option<&str>,
    ) -> anyhow::Result<SkillReferenceRead> {
        validate_skill_id_anyhow(skill_id)?;
        let skill = self
            .skills
            .get(skill_id)
            .ok_or_else(|| anyhow::anyhow!("skill {skill_id} is not available in registry"))?;
        let reference = match (reference_id, reference_path) {
            (Some(id), _) => skill
                .references
                .iter()
                .find(|reference| reference.summary.reference_id == id)
                .ok_or_else(|| anyhow::anyhow!("referenceId {id} is not declared by skill"))?,
            (None, Some(path)) => {
                validate_reference_path(path)?;
                skill
                    .references
                    .iter()
                    .find(|reference| reference.summary.path == path)
                    .ok_or_else(|| {
                        anyhow::anyhow!("reference path {path} is not declared by skill")
                    })?
            }
            (None, None) => anyhow::bail!("referenceId or path is required"),
        };
        let raw = tokio::fs::read_to_string(&reference.absolute_path)
            .await
            .with_context(|| {
                format!("failed to read skill reference {}", reference.summary.path)
            })?;
        let (content, truncated) = truncate_chars_with_flag(raw.trim(), self.max_reference_chars);
        Ok(SkillReferenceRead {
            skill_id: skill.skill_id.clone(),
            skill_revision: skill.revision.clone(),
            reference: reference.summary.clone(),
            content,
            truncated,
        })
    }
}

impl DiagnosticSkill {
    fn summary(&self) -> SkillSummary {
        SkillSummary {
            skill_id: self.skill_id.clone(),
            name: self.name.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            managed: self.managed,
            include_by_default: self.include_by_default,
            priority: self.priority,
            products: self.products.clone(),
            domain_adapters: self.domain_adapters.clone(),
            tool_ids: self.tool_ids.clone(),
            task_kinds: self.task_kinds.clone(),
            revision: self.revision.clone(),
            source_root: self.source_root.display().to_string(),
            source_path: self.skill_dir.display().to_string(),
            references: self
                .references
                .iter()
                .map(|reference| reference.summary.clone())
                .collect(),
            updated_at: self.updated_at,
        }
    }

    fn to_system_context_item(self) -> SystemContextBundleItem {
        SystemContextBundleItem {
            context_id: format!("diagnostic_skill:{}", self.skill_id),
            version_id: Some(self.revision.clone()),
            kind: SystemContextKind::DiagnosticSkill,
            title: self.display_name,
            content_type: SystemContextContentType::Markdown,
            summary: Some(self.description),
            content: self.injection_content,
            source: "skill_registry".to_string(),
            prompt_priority: self.priority,
            prompt_chars: self.max_prompt_chars,
            skill_id: Some(self.skill_id),
            revision: Some(self.revision),
            source_root: Some(self.source_root.display().to_string()),
            source_path: Some(self.skill_md_path.display().to_string()),
            references: self
                .references
                .into_iter()
                .map(|reference| reference.summary)
                .collect(),
        }
    }
}

fn discover_skill_dirs(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    if root.join("SKILL.md").is_file() {
        dirs.push(root.to_path_buf());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if path.join("SKILL.md").is_file() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn load_skill_dir(
    root: &Path,
    skill_dir: &Path,
    max_skill_chars: usize,
) -> anyhow::Result<DiagnosticSkill> {
    let skill_dir = skill_dir.canonicalize()?;
    if !skill_dir.starts_with(root) {
        anyhow::bail!("skill directory escapes configured root");
    }
    let skill_md_path = skill_dir.join("SKILL.md");
    let skill_md_raw = fs::read_to_string(&skill_md_path)?;
    let (frontmatter, body) = parse_skill_frontmatter(&skill_md_raw)?;
    let manifest_path = skill_dir.join("logagent.json");
    let (manifest, manifest_raw) = if manifest_path.is_file() {
        let raw = fs::read_to_string(&manifest_path)?;
        let manifest: LogAgentSkillManifest = serde_json::from_str(&raw)?;
        validate_manifest(&manifest)?;
        (Some(manifest), raw)
    } else {
        (None, String::new())
    };
    let fallback_id = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .map(slug_id)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("skill directory name cannot be used as skillId"))?;
    let skill_id = manifest
        .as_ref()
        .map(|manifest| manifest.skill_id.clone())
        .unwrap_or(fallback_id);
    validate_skill_id_anyhow(&skill_id)?;
    let references = load_references(&skill_id, &skill_dir, manifest.as_ref())?;
    let references_manifest = references
        .iter()
        .map(|reference| &reference.summary)
        .collect::<Vec<_>>();
    let revision = stable_hash_hex(&[
        skill_md_raw.as_bytes(),
        manifest_raw.as_bytes(),
        serde_json::to_string(&references_manifest)?.as_bytes(),
    ]);
    let max_prompt_chars = manifest
        .as_ref()
        .and_then(|manifest| manifest.max_prompt_chars)
        .unwrap_or(max_skill_chars)
        .clamp(200, max_skill_chars);
    let (injection_content, _) = truncate_chars_with_flag(body.trim(), max_prompt_chars);
    let metadata = fs::metadata(&skill_md_path)?;
    let updated_at = metadata
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(Utc::now);
    Ok(DiagnosticSkill {
        skill_id,
        name: frontmatter.name.clone(),
        display_name: manifest
            .as_ref()
            .and_then(|manifest| manifest.display_name.clone())
            .unwrap_or(frontmatter.name),
        description: frontmatter.description,
        managed: manifest.is_some(),
        include_by_default: manifest
            .as_ref()
            .map(|manifest| manifest.include_by_default)
            .unwrap_or(false),
        priority: manifest
            .as_ref()
            .map(|manifest| manifest.priority)
            .unwrap_or(0),
        max_prompt_chars,
        products: manifest
            .as_ref()
            .map(|manifest| normalize_list(manifest.products.clone()))
            .unwrap_or_default(),
        domain_adapters: manifest
            .as_ref()
            .map(|manifest| normalize_list(manifest.domain_adapters.clone()))
            .unwrap_or_default(),
        tool_ids: manifest
            .as_ref()
            .map(|manifest| normalize_list(manifest.tool_ids.clone()))
            .unwrap_or_default(),
        task_kinds: manifest
            .as_ref()
            .map(|manifest| normalize_list(manifest.task_kinds.clone()))
            .unwrap_or_default(),
        revision,
        source_root: root.to_path_buf(),
        skill_dir,
        skill_md_path,
        injection_content,
        references,
        updated_at,
    })
}

fn parse_skill_frontmatter(raw: &str) -> anyhow::Result<(SkillFrontMatter, String)> {
    let mut lines = raw.lines();
    if lines.next().map(str::trim) != Some("---") {
        anyhow::bail!("SKILL.md must start with YAML frontmatter");
    }
    let mut yaml = String::new();
    let mut body = String::new();
    let mut in_frontmatter = true;
    for line in lines {
        if in_frontmatter && line.trim() == "---" {
            in_frontmatter = false;
            continue;
        }
        if in_frontmatter {
            yaml.push_str(line);
            yaml.push('\n');
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    if in_frontmatter {
        anyhow::bail!("SKILL.md frontmatter is not closed");
    }
    let frontmatter: SkillFrontMatter = serde_yaml::from_str(&yaml)?;
    if frontmatter.name.trim().is_empty() || frontmatter.description.trim().is_empty() {
        anyhow::bail!("SKILL.md frontmatter requires name and description");
    }
    Ok((
        SkillFrontMatter {
            name: frontmatter.name.trim().to_string(),
            description: frontmatter.description.trim().to_string(),
        },
        body,
    ))
}

fn validate_manifest(manifest: &LogAgentSkillManifest) -> anyhow::Result<()> {
    if manifest.schema_version != 1 {
        anyhow::bail!(
            "unsupported logagent.json schemaVersion {}",
            manifest.schema_version
        );
    }
    validate_skill_id_anyhow(&manifest.skill_id)?;
    for reference in &manifest.references {
        validate_reference_path(&reference.path)?;
        if reference.title.trim().is_empty() || reference.summary.trim().is_empty() {
            anyhow::bail!("skill reference title and summary are required");
        }
    }
    Ok(())
}

fn load_references(
    skill_id: &str,
    skill_dir: &Path,
    manifest: Option<&LogAgentSkillManifest>,
) -> anyhow::Result<Vec<SkillReference>> {
    let Some(manifest) = manifest else {
        return Ok(Vec::new());
    };
    let mut references = Vec::new();
    let mut seen = HashSet::new();
    for reference in &manifest.references {
        validate_reference_path(&reference.path)?;
        if !seen.insert(reference.path.clone()) {
            anyhow::bail!("duplicate skill reference path {}", reference.path);
        }
        let absolute_path = skill_dir
            .join(&reference.path)
            .canonicalize()
            .with_context(|| format!("failed to resolve skill reference {}", reference.path))?;
        if !absolute_path.starts_with(skill_dir) {
            anyhow::bail!("skill reference {} escapes skill directory", reference.path);
        }
        if !absolute_path.is_file() {
            anyhow::bail!("skill reference {} is not a file", reference.path);
        }
        references.push(SkillReference {
            summary: SkillReferenceSummary {
                reference_id: stable_reference_id(skill_id, &reference.path),
                path: reference.path.trim().to_string(),
                title: reference.title.trim().to_string(),
                summary: reference.summary.trim().to_string(),
            },
            absolute_path,
        });
    }
    Ok(references)
}

fn collect_skill_export_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<SkillExportFile>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_skill_export_files(root, &path, out)?;
        } else if file_type.is_file() {
            out.push(SkillExportFile {
                relative_path: safe_relative_path(root, &path)?,
                absolute_path: path,
                size: metadata.len(),
            });
        }
    }
    Ok(())
}

fn skill_zip_dir(skill: &DiagnosticSkill) -> anyhow::Result<String> {
    let relative = safe_relative_path(&skill.source_root, &skill.skill_dir).unwrap_or_default();
    if relative.is_empty() {
        Ok(skill.skill_id.clone())
    } else {
        Ok(relative)
    }
}

fn safe_relative_path(root: &Path, path: &Path) -> anyhow::Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} escapes {}", path.display(), root.display()))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("path contains invalid UTF-8"))?;
                if value.is_empty() {
                    anyhow::bail!("path contains empty segment");
                }
                parts.push(value.to_string());
            }
            Component::CurDir => {}
            _ => anyhow::bail!("path contains unsafe segment"),
        }
    }
    Ok(parts.join("/"))
}

fn validate_reference_path(path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    let valid = !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if valid {
        Ok(())
    } else {
        anyhow::bail!("skill reference path must be workspace-relative without traversal");
    }
}

fn validate_skill_id(skill_id: &str) -> Result<(), AppError> {
    validate_skill_id_anyhow(skill_id).map_err(|err| AppError::bad_request(err.to_string()))
}

fn validate_skill_id_anyhow(skill_id: &str) -> anyhow::Result<()> {
    let valid = !skill_id.is_empty()
        && skill_id.len() <= 120
        && skill_id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
        });
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid skillId");
    }
}

fn metadata_filters_match(
    skill: &DiagnosticSkill,
    product: Option<&str>,
    _version: Option<&str>,
    environment: Option<&str>,
) -> bool {
    list_filter_matches(&skill.products, product) && optional_environment_matches(environment)
}

fn list_filter_matches(filters: &[String], value: Option<&str>) -> bool {
    filters.is_empty()
        || value
            .map(|value| {
                filters
                    .iter()
                    .any(|filter| filter.eq_ignore_ascii_case(value))
            })
            .unwrap_or(false)
}

fn optional_environment_matches(_environment: Option<&str>) -> bool {
    true
}

fn matches_task_kind(task_kinds: &[String], task_kind: TaskKind) -> bool {
    task_kinds.is_empty()
        || task_kinds
            .iter()
            .any(|value| value == task_kind_label(task_kind))
}

fn task_kind_label(task_kind: TaskKind) -> &'static str {
    match task_kind {
        TaskKind::LogAnalysis => "log_analysis",
        TaskKind::ToolRun => "tool_run",
        TaskKind::RemoteCommandRun => "remote_command_run",
    }
}

fn normalize_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

fn slug_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn stable_reference_id(skill_id: &str, path: &str) -> String {
    format!(
        "ref_{}",
        stable_hash_hex(&[skill_id.as_bytes(), b"\0", path.as_bytes()])
    )
}

fn stable_hash_hex(parts: &[&[u8]]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

fn truncate_chars_with_flag(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        (value.to_string(), false)
    } else {
        let mut out = value.chars().take(max_chars).collect::<String>();
        out.push_str("\n[truncated]");
        (out, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-skill-registry-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn settings(root: PathBuf) -> SkillSettings {
        SkillSettings {
            enabled: true,
            roots: vec![root],
            max_skill_chars: 2000,
            max_reference_chars: 2000,
        }
    }

    #[test]
    fn loads_skill_with_manifest_and_revision() {
        let root = root("load");
        let skill = root.join("opengemini");
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: openGemini Diagnosis\ndescription: Diagnose openGemini clusters.\n---\nUse metadata and logs.\n",
        )
        .unwrap();
        fs::write(skill.join("references/topology.md"), "Topology reference").unwrap();
        fs::write(
            skill.join("logagent.json"),
            r#"{"schemaVersion":1,"skillId":"opengemini-diagnosis","displayName":"openGemini diagnosis","products":["opengemini"],"taskKinds":["log_analysis"],"includeByDefault":true,"priority":20,"references":[{"path":"references/topology.md","title":"Topology","summary":"Topology rules"}]}"#,
        )
        .unwrap();

        let registry = SkillRegistry::load(settings(root.clone())).unwrap();
        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].skill_id, "opengemini-diagnosis");
        assert_eq!(list[0].references.len(), 1);
        assert!(!list[0].revision.is_empty());
        let items = registry
            .resolve_items(ResolveSkillsInput {
                explicit_skill_ids: &[],
                task_kind: TaskKind::LogAnalysis,
                product: Some("opengemini"),
                version: None,
                environment: None,
            })
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, SystemContextKind::DiagnosticSkill);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_skill_without_manifest_is_explicit_only() {
        let root = root("external");
        let skill = root.join("external");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: External\ndescription: Explicit skill.\n---\nExternal only.\n",
        )
        .unwrap();
        let registry = SkillRegistry::load(settings(root.clone())).unwrap();
        assert!(registry
            .resolve_items(ResolveSkillsInput {
                explicit_skill_ids: &[],
                task_kind: TaskKind::LogAnalysis,
                product: Some("opengemini"),
                version: None,
                environment: None,
            })
            .unwrap()
            .is_empty());
        assert_eq!(
            registry
                .resolve_items(ResolveSkillsInput {
                    explicit_skill_ids: &["external".to_string()],
                    task_kind: TaskKind::LogAnalysis,
                    product: None,
                    version: None,
                    environment: None,
                })
                .unwrap()
                .len(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_path_traversal_reference() {
        let root = root("traversal");
        let skill = root.join("bad");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: Bad\ndescription: Bad reference.\n---\nBad.\n",
        )
        .unwrap();
        fs::write(
            skill.join("logagent.json"),
            r#"{"schemaVersion":1,"skillId":"bad","references":[{"path":"../secret.md","title":"Bad","summary":"Bad"}]}"#,
        )
        .unwrap();
        assert!(SkillRegistry::load(settings(root.clone())).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_duplicate_skill_ids() {
        let root = root("duplicate");
        for name in ["one", "two"] {
            let skill = root.join(name);
            fs::create_dir_all(&skill).unwrap();
            fs::write(
                skill.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Duplicate.\n---\nBody.\n"),
            )
            .unwrap();
            fs::write(
                skill.join("logagent.json"),
                r#"{"schemaVersion":1,"skillId":"duplicate"}"#,
            )
            .unwrap();
        }
        assert!(SkillRegistry::load(settings(root.clone())).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
