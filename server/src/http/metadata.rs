use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use tracing::info;

use crate::{
    app::AppState,
    services::metadata::{
        MetadataConfirmResponse, MetadataDeleteResponse, MetadataFetchImportRequest,
        MetadataImportPreview, MetadataImportRequest, MetadataInstanceSummary,
        MetadataSnapshotResponse,
    },
    support::error::AppError,
};

pub async fn list_instances(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let instances: Vec<MetadataInstanceSummary> = state.metadata.list_instances().await;
    Ok(Json(serde_json::json!({ "instances": instances })))
}

pub async fn get_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let instance = state
        .metadata
        .get_instance(&instance_id)
        .await
        .ok_or_else(|| AppError::bad_request("unknown instanceId"))?;
    Ok(Json(serde_json::json!({ "instance": instance })))
}

pub async fn get_instance_snapshot(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> Result<Json<MetadataSnapshotResponse>, AppError> {
    Ok(Json(
        state.metadata.get_instance_snapshot(&instance_id).await?,
    ))
}

pub async fn refresh_instance_snapshot(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> Result<Json<MetadataSnapshotResponse>, AppError> {
    let response = state
        .metadata
        .refresh_instance_from_raw_snapshot(&instance_id)
        .await?;
    info!(
        instance_id = ?response.instance.as_ref().map(|instance| instance.instance_id.as_str()),
        cluster_id = %response.cluster.cluster_id,
        node_count = response.nodes.len(),
        "metadata instance refreshed from raw snapshot"
    );
    Ok(Json(response))
}

pub async fn delete_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> Result<Json<MetadataDeleteResponse>, AppError> {
    let response = state.metadata.delete_instance(&instance_id).await?;
    info!(
        instance_id = %response.instance_id,
        cluster_id = ?response.cluster_id,
        deleted_nodes = response.deleted_nodes,
        "metadata instance deleted"
    );
    Ok(Json(response))
}

pub async fn fetch_snapshot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MetadataFetchImportRequest>,
) -> Result<Json<MetadataSnapshotResponse>, AppError> {
    let response = state.metadata.fetch_snapshot(req).await?;
    info!(
        instance_id = ?response.instance.as_ref().map(|instance| instance.instance_id.as_str()),
        cluster_id = %response.cluster.cluster_id,
        node_count = response.nodes.len(),
        "metadata snapshot fetched"
    );
    Ok(Json(response))
}

pub async fn get_cluster(
    State(state): State<Arc<AppState>>,
    Path(cluster_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let cluster = state
        .metadata
        .get_cluster(&cluster_id)
        .await
        .ok_or_else(|| AppError::bad_request("unknown clusterId"))?;
    Ok(Json(serde_json::json!({ "cluster": cluster })))
}

pub async fn list_cluster_nodes(
    State(state): State<Arc<AppState>>,
    Path(cluster_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let nodes = state.metadata.list_cluster_nodes(&cluster_id).await;
    Ok(Json(
        serde_json::json!({ "clusterId": cluster_id, "nodes": nodes }),
    ))
}

pub async fn create_import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MetadataImportRequest>,
) -> Result<Json<MetadataImportPreview>, AppError> {
    let preview = state.metadata.create_import_preview(req).await?;
    info!(
        import_id = %preview.import_id,
        template_type = %preview.template_type,
        "metadata import preview created"
    );
    Ok(Json(preview))
}

pub async fn fetch_import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MetadataFetchImportRequest>,
) -> Result<Json<MetadataImportPreview>, AppError> {
    let preview = state.metadata.fetch_import_preview(req).await?;
    info!(
        import_id = %preview.import_id,
        template_type = %preview.template_type,
        "metadata import preview fetched"
    );
    Ok(Json(preview))
}

pub async fn get_import_preview(
    State(state): State<Arc<AppState>>,
    Path(import_id): Path<String>,
) -> Result<Json<MetadataImportPreview>, AppError> {
    let preview = state
        .metadata
        .get_import_preview(&import_id)
        .await
        .ok_or_else(|| AppError::bad_request("unknown metadata import"))?;
    Ok(Json(preview))
}

pub async fn confirm_import(
    State(state): State<Arc<AppState>>,
    Path(import_id): Path<String>,
) -> Result<Json<MetadataConfirmResponse>, AppError> {
    let response = state.metadata.confirm_import(&import_id).await?;
    info!(
        import_id = %import_id,
        applied = response.applied,
        instances = response.summary.instances,
        clusters = response.summary.clusters,
        nodes = response.summary.nodes,
        "metadata import confirmed"
    );
    Ok(Json(response))
}
