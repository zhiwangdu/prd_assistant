use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::Arc,
};

use chrono::Utc;
use tokio::sync::RwLock;

use crate::{
    domain::models::{
        CreateSystemContextResourceRequest, CreateSystemContextVersionRequest,
        PatchSystemContextResourceRequest, PatchSystemContextVersionRequest, SystemContextBundle,
        SystemContextBundleItem, SystemContextContentType, SystemContextKind,
        SystemContextPromptPolicy, SystemContextResource, SystemContextResourceSummary,
        SystemContextScope, SystemContextVersion, SystemContextVersionStatus, TaskKind,
    },
    support::{error::AppError, id::next_id},
};

#[derive(Debug, Clone)]
pub struct SystemContextStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, SystemContextResource>>>,
}

impl SystemContextStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut resources = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let resource: SystemContextResource = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid system context {}: {err}", path.display())
            })?;
            validate_resource(&resource).map_err(|err| {
                anyhow::anyhow!("invalid system context {}: {err}", path.display())
            })?;
            if resources
                .insert(resource.context_id.clone(), resource)
                .is_some()
            {
                anyhow::bail!("duplicate system context record in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(resources)),
        })
    }

    pub async fn list(&self) -> Vec<SystemContextResource> {
        let mut resources = self
            .inner
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        resources.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        resources
    }

    pub async fn get(&self, context_id: &str) -> Option<SystemContextResource> {
        self.inner.read().await.get(context_id).cloned()
    }

    pub async fn create(
        &self,
        req: CreateSystemContextResourceRequest,
    ) -> Result<SystemContextResource, AppError> {
        let title = clean_required(req.title, "title")?;
        let content = clean_content(req.content)?;
        let context_id = next_id("ctx");
        let version_id = next_id("ctxver");
        let now = Utc::now();
        let version = SystemContextVersion {
            version_id: version_id.clone(),
            revision: 1,
            status: SystemContextVersionStatus::Active,
            content_type: req.content_type,
            content,
            summary: clean_optional(req.summary),
            prompt_policy: normalize_prompt_policy(req.prompt_policy),
            created_at: now,
            updated_at: now,
        };
        let resource = SystemContextResource {
            schema_version: 1,
            context_id: context_id.clone(),
            kind: req.kind,
            title,
            description: clean_optional(req.description),
            scope: req.scope,
            enabled: req.enabled,
            tags: normalize_tags(req.tags),
            product: clean_optional(req.product),
            version: clean_optional(req.version),
            environment: clean_optional(req.environment),
            active_version_id: Some(version_id),
            versions: vec![version],
            created_at: now,
            updated_at: now,
        };
        validate_resource(&resource).map_err(|err| AppError::bad_request(err.to_string()))?;
        let mut resources = self.inner.write().await;
        self.persist(&resource)?;
        resources.insert(context_id, resource.clone());
        Ok(resource)
    }

    pub async fn update_resource(
        &self,
        context_id: &str,
        req: PatchSystemContextResourceRequest,
    ) -> Result<SystemContextResource, AppError> {
        validate_context_id_for_api(context_id)?;
        let mut resources = self.inner.write().await;
        let mut resource = resources
            .get(context_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("unknown contextId {context_id}")))?;
        if let Some(title) = req.title {
            resource.title = clean_required(title, "title")?;
        }
        if let Some(description) = req.description {
            resource.description = clean_optional(description);
        }
        if let Some(scope) = req.scope {
            resource.scope = scope;
        }
        if let Some(enabled) = req.enabled {
            resource.enabled = enabled;
        }
        if let Some(tags) = req.tags {
            resource.tags = normalize_tags(tags);
        }
        if let Some(product) = req.product {
            resource.product = clean_optional(product);
        }
        if let Some(version) = req.version {
            resource.version = clean_optional(version);
        }
        if let Some(environment) = req.environment {
            resource.environment = clean_optional(environment);
        }
        resource.updated_at = Utc::now();
        validate_resource(&resource).map_err(|err| AppError::bad_request(err.to_string()))?;
        self.persist(&resource)?;
        resources.insert(context_id.to_string(), resource.clone());
        Ok(resource)
    }

    pub async fn create_version(
        &self,
        context_id: &str,
        req: CreateSystemContextVersionRequest,
    ) -> Result<SystemContextResource, AppError> {
        validate_context_id_for_api(context_id)?;
        let mut resources = self.inner.write().await;
        let mut resource = resources
            .get(context_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("unknown contextId {context_id}")))?;
        let now = Utc::now();
        let revision = resource
            .versions
            .iter()
            .map(|version| version.revision)
            .max()
            .unwrap_or(0)
            + 1;
        let version_id = next_id("ctxver");
        if req.activate {
            archive_active_versions(&mut resource);
        }
        resource.versions.push(SystemContextVersion {
            version_id: version_id.clone(),
            revision,
            status: if req.activate {
                SystemContextVersionStatus::Active
            } else {
                SystemContextVersionStatus::Draft
            },
            content_type: req.content_type,
            content: clean_content(req.content)?,
            summary: clean_optional(req.summary),
            prompt_policy: normalize_prompt_policy(req.prompt_policy),
            created_at: now,
            updated_at: now,
        });
        if req.activate {
            resource.active_version_id = Some(version_id);
        }
        resource.updated_at = now;
        validate_resource(&resource).map_err(|err| AppError::bad_request(err.to_string()))?;
        self.persist(&resource)?;
        resources.insert(context_id.to_string(), resource.clone());
        Ok(resource)
    }

    pub async fn update_version(
        &self,
        context_id: &str,
        version_id: &str,
        req: PatchSystemContextVersionRequest,
    ) -> Result<SystemContextResource, AppError> {
        validate_context_id_for_api(context_id)?;
        validate_context_version_id_for_api(version_id)?;
        let mut resources = self.inner.write().await;
        let mut resource = resources
            .get(context_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("unknown contextId {context_id}")))?;
        let active_requested = req.status == Some(SystemContextVersionStatus::Active);
        if active_requested {
            archive_active_versions(&mut resource);
        }
        let version_index = resource
            .versions
            .iter()
            .position(|version| version.version_id == version_id)
            .ok_or_else(|| AppError::not_found(format!("unknown versionId {version_id}")))?;
        let version = &mut resource.versions[version_index];
        if let Some(content_type) = req.content_type {
            version.content_type = content_type;
        }
        if let Some(content) = req.content {
            version.content = clean_content(content)?;
        }
        if let Some(summary) = req.summary {
            version.summary = clean_optional(summary);
        }
        if let Some(policy) = req.prompt_policy {
            version.prompt_policy = normalize_prompt_policy(policy);
        }
        if let Some(status) = req.status {
            version.status = status;
        }
        if active_requested {
            resource.active_version_id = Some(version_id.to_string());
        } else if resource.active_version_id.as_deref() == Some(version_id)
            && resource.versions[version_index].status != SystemContextVersionStatus::Active
        {
            resource.active_version_id = resource
                .versions
                .iter()
                .find(|candidate| candidate.status == SystemContextVersionStatus::Active)
                .map(|candidate| candidate.version_id.clone());
        }
        let now = Utc::now();
        resource.versions[version_index].updated_at = now;
        resource.updated_at = now;
        validate_resource(&resource).map_err(|err| AppError::bad_request(err.to_string()))?;
        self.persist(&resource)?;
        resources.insert(context_id.to_string(), resource.clone());
        Ok(resource)
    }

    pub async fn activate_version(
        &self,
        context_id: &str,
        version_id: &str,
    ) -> Result<SystemContextResource, AppError> {
        self.update_version(
            context_id,
            version_id,
            PatchSystemContextVersionRequest {
                content_type: None,
                content: None,
                summary: None,
                prompt_policy: None,
                status: Some(SystemContextVersionStatus::Active),
            },
        )
        .await
    }

    pub async fn resolve_items(
        &self,
        explicit_context_ids: &[String],
        task_kind: TaskKind,
        product: Option<&str>,
        version: Option<&str>,
        environment: Option<&str>,
    ) -> Vec<SystemContextBundleItem> {
        let explicit = explicit_context_ids.iter().collect::<HashSet<_>>();
        let mut items = self
            .inner
            .read()
            .await
            .values()
            .filter_map(|resource| {
                let active = resource.active_version()?;
                let include = explicit.contains(&resource.context_id)
                    || (resource.enabled
                        && active.prompt_policy.include_by_default
                        && scope_allows_task(resource.scope, task_kind)
                        && policy_allows_task(&active.prompt_policy, task_kind)
                        && metadata_filters_match(resource, product, version, environment));
                include.then(|| bundle_item(resource, active, "system_context"))
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .prompt_priority
                .cmp(&left.prompt_priority)
                .then_with(|| left.title.cmp(&right.title))
        });
        items
    }

    fn persist(&self, resource: &SystemContextResource) -> Result<(), AppError> {
        let path = self.resource_path(&resource.context_id);
        let temp = self.dir.join(format!(".{}.json.tmp", resource.context_id));
        fs::write(
            &temp,
            serde_json::to_vec_pretty(resource).map_err(|err| {
                AppError::internal(format!("failed to encode system context: {err}"))
            })?,
        )
        .map_err(|err| AppError::internal(format!("failed to write system context: {err}")))?;
        fs::rename(&temp, &path).map_err(|err| {
            AppError::internal(format!("failed to persist system context: {err}"))
        })?;
        Ok(())
    }

    fn resource_path(&self, context_id: &str) -> PathBuf {
        self.dir.join(format!("{context_id}.json"))
    }
}

pub fn system_context_bundle(items: Vec<SystemContextBundleItem>) -> SystemContextBundle {
    SystemContextBundle {
        schema_version: 1,
        resolved_at: Utc::now(),
        resources: items,
    }
}

pub fn render_system_context_prompt(bundle: &SystemContextBundle) -> String {
    if bundle.resources.is_empty() {
        return "no system context resources selected\n".to_string();
    }
    let mut out = String::new();
    for item in &bundle.resources {
        out.push_str(&format!(
            "- [{}] {} source={} version={} summary={}\n",
            context_kind_label(item.kind),
            item.title,
            item.source,
            item.version_id.as_deref().unwrap_or("-"),
            item.summary.as_deref().unwrap_or("-")
        ));
        if !item.content.trim().is_empty() {
            out.push_str(&truncate_chars(&item.content, item.prompt_chars));
            out.push('\n');
        }
    }
    out
}

pub fn metadata_adapter_item(
    context_id: String,
    title: String,
    summary: String,
    content: String,
) -> SystemContextBundleItem {
    SystemContextBundleItem {
        context_id,
        version_id: None,
        kind: SystemContextKind::MetadataInstance,
        title,
        content_type: SystemContextContentType::MetadataAdapter,
        summary: Some(summary),
        content: truncate_chars(&content, 4000),
        source: "metadata_adapter".to_string(),
        prompt_priority: 50,
        prompt_chars: 4000,
    }
}

pub fn validate_context_id_for_api(context_id: &str) -> Result<(), AppError> {
    if valid_prefixed_id(context_id, "ctx_") || valid_prefixed_id(context_id, "meta_") {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid contextId"))
    }
}

fn validate_context_version_id_for_api(version_id: &str) -> Result<(), AppError> {
    if valid_prefixed_id(version_id, "ctxver_") {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid versionId"))
    }
}

fn validate_resource(resource: &SystemContextResource) -> anyhow::Result<()> {
    if resource.schema_version != 1 {
        anyhow::bail!("unsupported schemaVersion {}", resource.schema_version);
    }
    if !valid_prefixed_id(&resource.context_id, "ctx_") {
        anyhow::bail!("invalid contextId");
    }
    if resource.title.trim().is_empty() {
        anyhow::bail!("missing title");
    }
    if resource.versions.is_empty() {
        anyhow::bail!("system context resource must contain at least one version");
    }
    let mut active_count = 0usize;
    let mut ids = HashSet::new();
    for version in &resource.versions {
        if !valid_prefixed_id(&version.version_id, "ctxver_") {
            anyhow::bail!("invalid versionId");
        }
        if !ids.insert(&version.version_id) {
            anyhow::bail!("duplicate versionId");
        }
        if version.content.trim().is_empty() {
            anyhow::bail!("empty version content");
        }
        if version.status == SystemContextVersionStatus::Active {
            active_count += 1;
        }
    }
    if active_count > 1 {
        anyhow::bail!("multiple active versions");
    }
    if let Some(active_version_id) = resource.active_version_id.as_deref() {
        let active = resource
            .versions
            .iter()
            .find(|version| version.version_id == active_version_id)
            .ok_or_else(|| anyhow::anyhow!("activeVersionId does not exist"))?;
        if active.status != SystemContextVersionStatus::Active {
            anyhow::bail!("activeVersionId does not point to an active version");
        }
    }
    Ok(())
}

fn archive_active_versions(resource: &mut SystemContextResource) {
    for version in &mut resource.versions {
        if version.status == SystemContextVersionStatus::Active {
            version.status = SystemContextVersionStatus::Archived;
        }
    }
}

fn bundle_item(
    resource: &SystemContextResource,
    version: &SystemContextVersion,
    source: &str,
) -> SystemContextBundleItem {
    let content = truncate_chars(&version.content, version.prompt_policy.max_chars);
    SystemContextBundleItem {
        context_id: resource.context_id.clone(),
        version_id: Some(version.version_id.clone()),
        kind: resource.kind,
        title: resource.title.clone(),
        content_type: version.content_type,
        summary: version.summary.clone(),
        content,
        source: source.to_string(),
        prompt_priority: version.prompt_policy.priority,
        prompt_chars: version.prompt_policy.max_chars,
    }
}

fn scope_allows_task(scope: SystemContextScope, task_kind: TaskKind) -> bool {
    match (scope, task_kind) {
        (SystemContextScope::Global, _) => true,
        (SystemContextScope::LogAnalysis, TaskKind::LogAnalysis) => true,
        (SystemContextScope::ToolRun, TaskKind::ToolRun) => true,
        (SystemContextScope::CaseImport, _) => false,
        _ => false,
    }
}

fn policy_allows_task(policy: &SystemContextPromptPolicy, task_kind: TaskKind) -> bool {
    policy.allowed_task_kinds.is_empty() || policy.allowed_task_kinds.contains(&task_kind)
}

fn metadata_filters_match(
    resource: &SystemContextResource,
    product: Option<&str>,
    version: Option<&str>,
    environment: Option<&str>,
) -> bool {
    optional_filter_matches(resource.product.as_deref(), product)
        && optional_filter_matches(resource.version.as_deref(), version)
        && optional_filter_matches(resource.environment.as_deref(), environment)
}

fn optional_filter_matches(filter: Option<&str>, value: Option<&str>) -> bool {
    filter.is_none_or(|filter| {
        value
            .map(|value| value.eq_ignore_ascii_case(filter))
            .unwrap_or(false)
    })
}

pub fn resource_summaries_with_source(
    resources: Vec<SystemContextResource>,
    source: &'static str,
) -> Vec<SystemContextResourceSummary> {
    resources
        .into_iter()
        .map(|resource| resource.summary(source))
        .collect()
}

fn normalize_prompt_policy(mut policy: SystemContextPromptPolicy) -> SystemContextPromptPolicy {
    policy.max_chars = policy.max_chars.clamp(200, 20_000);
    policy
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .filter_map(|tag| clean_optional(Some(tag)))
        .filter(|tag| seen.insert(tag.to_ascii_lowercase()))
        .take(32)
        .collect()
}

fn clean_required(value: String, field: &str) -> Result<String, AppError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::bad_request(format!("{field} is required")));
    }
    if value.chars().count() > 200 {
        return Err(AppError::bad_request(format!("{field} is too long")));
    }
    Ok(value)
}

fn clean_content(value: String) -> Result<String, AppError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::bad_request("content is required"));
    }
    if value.chars().count() > 200_000 {
        return Err(AppError::bad_request("content is too long"));
    }
    Ok(value)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn valid_prefixed_id(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let mut out = value.chars().take(max_chars).collect::<String>();
        out.push_str("\n[truncated]");
        out
    }
}

fn context_kind_label(kind: SystemContextKind) -> &'static str {
    match kind {
        SystemContextKind::PromptPack => "prompt_pack",
        SystemContextKind::ArchitectureDoc => "architecture_doc",
        SystemContextKind::Runbook => "runbook",
        SystemContextKind::Glossary => "glossary",
        SystemContextKind::ToolCapability => "tool_capability",
        SystemContextKind::MetadataInstance => "metadata_instance",
        SystemContextKind::KnowledgeNote => "knowledge_note",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-system-context-store-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn create_request() -> CreateSystemContextResourceRequest {
        CreateSystemContextResourceRequest {
            kind: SystemContextKind::ArchitectureDoc,
            title: "openGemini architecture".to_string(),
            description: Some("cluster layout".to_string()),
            scope: SystemContextScope::LogAnalysis,
            enabled: true,
            tags: vec!["opengemini".to_string()],
            product: Some("opengemini".to_string()),
            version: None,
            environment: None,
            content_type: SystemContextContentType::Mermaid,
            content: "flowchart LR\nA-->B".to_string(),
            summary: Some("Meta and Data nodes".to_string()),
            prompt_policy: SystemContextPromptPolicy::default(),
        }
    }

    #[tokio::test]
    async fn persists_resource_and_versions() {
        let root = temp_dir("basic");
        let store = SystemContextStore::load(root.clone()).unwrap();
        let resource = store.create(create_request()).await.unwrap();
        assert!(resource.context_id.starts_with("ctx_"));
        assert_eq!(resource.versions.len(), 1);

        let updated = store
            .create_version(
                &resource.context_id,
                CreateSystemContextVersionRequest {
                    content_type: SystemContextContentType::Markdown,
                    content: "# Updated".to_string(),
                    summary: Some("updated".to_string()),
                    prompt_policy: SystemContextPromptPolicy::default(),
                    activate: true,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.versions.len(), 2);
        assert_eq!(
            updated
                .active_version()
                .and_then(|version| version.summary.as_deref()),
            Some("updated")
        );

        let reloaded = SystemContextStore::load(root.clone()).unwrap();
        assert_eq!(reloaded.list().await.len(), 1);
        let items = reloaded
            .resolve_items(&[], TaskKind::LogAnalysis, Some("opengemini"), None, None)
            .await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].summary.as_deref(), Some("updated"));
        let _ = std::fs::remove_dir_all(root);
    }
}
