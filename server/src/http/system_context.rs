use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tracing::info;

use crate::{
    app::AppState,
    domain::models::{
        CreateSystemContextResourceRequest, CreateSystemContextVersionRequest,
        PatchSystemContextResourceRequest, PatchSystemContextVersionRequest,
        SystemContextBundleItem, SystemContextContentType, SystemContextKind,
        SystemContextListResponse, SystemContextPreviewRequest, SystemContextPreviewResponse,
        SystemContextResource, SystemContextResourceSummary, SystemContextScope, TaskKind,
    },
    services::metadata::{MetadataInstanceSummary, TaskMetadataContext},
    stores::system_context_store::{
        metadata_adapter_item, render_system_context_prompt, system_context_bundle,
        validate_context_id_for_api,
    },
    support::error::AppError,
};

pub async fn list_resources(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SystemContextListResponse>, AppError> {
    let mut resources = metadata_resource_summaries(&state).await;
    resources.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(Json(SystemContextListResponse { resources }))
}

pub async fn create_resource(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSystemContextResourceRequest>,
) -> Result<(StatusCode, Json<SystemContextResource>), AppError> {
    if req.kind == SystemContextKind::MetadataInstance {
        return Err(AppError::bad_request(
            "metadata_instance resources are managed by Metadata Store",
        ));
    }
    let resource = state.system_context.create(req).await?;
    info!(
        context_id = %resource.context_id,
        kind = ?resource.kind,
        active_version_id = ?resource.active_version_id,
        "system context resource created"
    );
    Ok((StatusCode::CREATED, Json(resource)))
}

pub async fn get_resource(
    State(state): State<Arc<AppState>>,
    Path(context_id): Path<String>,
) -> Result<Json<SystemContextResource>, AppError> {
    validate_context_id_for_api(&context_id)?;
    state
        .system_context
        .get(&context_id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown contextId {context_id}")))
}

pub async fn patch_resource(
    State(state): State<Arc<AppState>>,
    Path(context_id): Path<String>,
    Json(req): Json<PatchSystemContextResourceRequest>,
) -> Result<Json<SystemContextResource>, AppError> {
    let resource = state
        .system_context
        .update_resource(&context_id, req)
        .await?;
    info!(
        context_id = %resource.context_id,
        enabled = resource.enabled,
        "system context resource updated"
    );
    Ok(Json(resource))
}

pub async fn create_version(
    State(state): State<Arc<AppState>>,
    Path(context_id): Path<String>,
    Json(req): Json<CreateSystemContextVersionRequest>,
) -> Result<(StatusCode, Json<SystemContextResource>), AppError> {
    let resource = state
        .system_context
        .create_version(&context_id, req)
        .await?;
    info!(
        context_id = %resource.context_id,
        active_version_id = ?resource.active_version_id,
        version_count = resource.versions.len(),
        "system context version created"
    );
    Ok((StatusCode::CREATED, Json(resource)))
}

pub async fn patch_version(
    State(state): State<Arc<AppState>>,
    Path((context_id, version_id)): Path<(String, String)>,
    Json(req): Json<PatchSystemContextVersionRequest>,
) -> Result<Json<SystemContextResource>, AppError> {
    let resource = state
        .system_context
        .update_version(&context_id, &version_id, req)
        .await?;
    info!(
        context_id = %context_id,
        version_id = %version_id,
        "system context version updated"
    );
    Ok(Json(resource))
}

pub async fn activate_version(
    State(state): State<Arc<AppState>>,
    Path((context_id, version_id)): Path<(String, String)>,
) -> Result<Json<SystemContextResource>, AppError> {
    let resource = state
        .system_context
        .activate_version(&context_id, &version_id)
        .await?;
    info!(
        context_id = %context_id,
        version_id = %version_id,
        "system context version activated"
    );
    Ok(Json(resource))
}

pub async fn preview(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SystemContextPreviewRequest>,
) -> Result<Json<SystemContextPreviewResponse>, AppError> {
    let task_kind = req.task_kind.unwrap_or(TaskKind::LogAnalysis);
    let mut resources = state
        .system_context
        .resolve_items(
            &req.context_ids,
            task_kind,
            req.product.as_deref(),
            req.version.as_deref(),
            req.environment.as_deref(),
        )
        .await;
    if let Some(instance_id) = req.instance_id.as_deref() {
        let metadata = state
            .metadata
            .resolve_task_context(Some(instance_id.to_string()), None, None)
            .await?;
        resources.push(metadata_context_bundle_item(&metadata));
    }
    let bundle = system_context_bundle(resources.clone());
    Ok(Json(SystemContextPreviewResponse {
        resources,
        prompt: render_system_context_prompt(&bundle),
    }))
}

pub(crate) fn metadata_context_bundle_item(
    metadata: &TaskMetadataContext,
) -> SystemContextBundleItem {
    let instance = metadata.instance_id.as_deref().unwrap_or("unknown");
    let title = format!("Metadata instance {instance}");
    let summary = format!(
        "product={} version={} environment={} node={} clusterNodes={}",
        metadata.product.as_deref().unwrap_or("-"),
        metadata.version.as_deref().unwrap_or("-"),
        metadata.environment.as_deref().unwrap_or("-"),
        metadata.node_id.as_deref().unwrap_or("-"),
        metadata.cluster_nodes.len()
    );
    let databases = metadata
        .cluster
        .as_ref()
        .map(|cluster| {
            cluster
                .databases
                .iter()
                .take(20)
                .map(|database| database.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".to_string());
    let nodes = metadata
        .cluster_nodes
        .iter()
        .take(20)
        .map(|node| {
            format!(
                "{} kind={} status={} host={}",
                node.node_id,
                node.kind.as_deref().unwrap_or("-"),
                node.status.as_deref().unwrap_or("-"),
                node.host.as_deref().unwrap_or("-")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!(
        "instance={instance}\ncluster={}\nproduct={}\nversion={}\nenvironment={}\ndatabases={databases}\nnodes:\n{nodes}",
        metadata.cluster_id.as_deref().unwrap_or("-"),
        metadata.product.as_deref().unwrap_or("-"),
        metadata.version.as_deref().unwrap_or("-"),
        metadata.environment.as_deref().unwrap_or("-")
    );
    metadata_adapter_item(format!("meta_{instance}"), title, summary, content)
}

async fn metadata_resource_summaries(state: &AppState) -> Vec<SystemContextResourceSummary> {
    state
        .metadata
        .list_instances()
        .await
        .into_iter()
        .map(metadata_summary)
        .collect()
}

fn metadata_summary(instance: MetadataInstanceSummary) -> SystemContextResourceSummary {
    SystemContextResourceSummary {
        context_id: format!("meta_{}", instance.instance_id),
        kind: SystemContextKind::MetadataInstance,
        title: instance
            .remark
            .as_ref()
            .map(|remark| format!("{} ({remark})", instance.instance_id))
            .unwrap_or(instance.instance_id.clone()),
        description: Some(format!(
            "Metadata adapter: nodes={} databases={} partitionViews={}",
            instance.node_count, instance.database_count, instance.partition_view_count
        )),
        scope: SystemContextScope::LogAnalysis,
        enabled: true,
        tags: vec!["metadata".to_string()],
        product: instance.product,
        version: instance.version,
        environment: instance.environment,
        active_version_id: None,
        active_summary: instance
            .cluster_id
            .map(|cluster_id| format!("cluster={cluster_id}")),
        content_type: Some(SystemContextContentType::MetadataAdapter),
        source: "metadata_adapter".to_string(),
        updated_at: chrono::Utc::now(),
    }
}
