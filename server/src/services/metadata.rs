use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::support::{config::AppConfig, error::AppError, id::next_id};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMetadata {
    pub instance_id: String,
    #[serde(default)]
    pub remark: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub region: Option<String>,
    pub owner: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterMetadata {
    pub cluster_id: String,
    pub name: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub databases: Vec<DatabaseMetadata>,
    #[serde(default)]
    pub partition_views: Vec<PartitionViewMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_snapshot: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMetadata {
    pub node_id: String,
    pub raw_node_id: Option<u64>,
    pub kind: Option<String>,
    pub instance_id: Option<String>,
    pub hostname: Option<String>,
    pub host: Option<String>,
    pub tcp_host: Option<String>,
    pub rpc_addr: Option<String>,
    pub gossip_addr: Option<String>,
    pub ssh_alias: Option<String>,
    pub role: Option<String>,
    pub zone: Option<String>,
    pub status: Option<String>,
    pub status_code: Option<i64>,
    pub conn_id: Option<u64>,
    pub alive_conn_id: Option<u64>,
    pub index: Option<u64>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseMetadata {
    pub name: String,
    pub default_retention_policy: Option<String>,
    pub replica_n: Option<u64>,
    pub mark_deleted: Option<bool>,
    #[serde(default)]
    pub retention_policies: Vec<RetentionPolicyMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicyMetadata {
    pub name: String,
    pub replica_n: Option<u64>,
    pub duration: Option<u64>,
    pub shard_group_duration: Option<u64>,
    pub index_group_duration: Option<u64>,
    pub mark_deleted: Option<bool>,
    #[serde(default)]
    pub measurements: Vec<MeasurementMetadata>,
    #[serde(default)]
    pub shard_groups: Vec<ShardGroupMetadata>,
    #[serde(default)]
    pub index_groups: Vec<IndexGroupMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasurementMetadata {
    pub name: String,
    pub logical_name: Option<String>,
    pub version_name: Option<String>,
    pub version: Option<u64>,
    pub shard_key_type: Option<String>,
    #[serde(default)]
    pub schema: Vec<FieldSchemaMetadata>,
    pub mark_deleted: Option<bool>,
    pub engine_type: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSchemaMetadata {
    pub name: String,
    pub typ: Option<u64>,
    pub end_time: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardGroupMetadata {
    pub id: u64,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    #[serde(default)]
    pub shard_ids: Vec<u64>,
    #[serde(default)]
    pub owners: Vec<u64>,
    #[serde(default)]
    pub shards: Vec<ShardMetadata>,
    pub deleted_at: Option<String>,
    pub truncated_at: Option<String>,
    pub engine_type: Option<u64>,
    pub version: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardMetadata {
    pub id: u64,
    #[serde(default)]
    pub owners: Vec<u64>,
    pub min: Option<String>,
    pub max: Option<String>,
    pub tier: Option<u64>,
    pub index_id: Option<u64>,
    pub downsample_id: Option<u64>,
    pub downsample_level: Option<u64>,
    pub read_only: Option<bool>,
    pub mark_delete: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexGroupMetadata {
    pub id: u64,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub deleted_at: Option<String>,
    pub engine_type: Option<u64>,
    #[serde(default)]
    pub indexes: Vec<IndexMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexMetadata {
    pub id: u64,
    pub tier: Option<u64>,
    #[serde(default)]
    pub owners: Vec<u64>,
    pub mark_delete: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionViewMetadata {
    pub database: String,
    pub pt_id: u64,
    pub owner_node_id: Option<u64>,
    pub status: Option<u64>,
    pub status_text: String,
    pub version: Option<u64>,
    pub replica_group_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMetadataContext {
    pub schema_version: u32,
    pub resolved_at: DateTime<Utc>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance: Option<InstanceMetadata>,
    pub cluster: Option<ClusterMetadata>,
    pub node: Option<NodeMetadata>,
    pub cluster_nodes: Vec<NodeMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataTemplate {
    #[serde(default)]
    pub instances: Vec<InstanceMetadata>,
    #[serde(default)]
    pub clusters: Vec<ClusterMetadata>,
    #[serde(default)]
    pub nodes: Vec<NodeMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataImportRequest {
    pub template_type: String,
    pub filename: Option<String>,
    pub instance_id: Option<String>,
    #[serde(default)]
    pub remark: Option<String>,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataFetchImportRequest {
    pub url: String,
    pub template_type: Option<String>,
    pub filename: Option<String>,
    pub instance_id: Option<String>,
    #[serde(default)]
    pub remark: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSnapshotResponse {
    pub instance: Option<InstanceMetadata>,
    pub cluster: ClusterMetadata,
    pub nodes: Vec<NodeMetadata>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataInstanceSummary {
    pub instance_id: String,
    pub remark: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub region: Option<String>,
    pub owner: Option<String>,
    pub node_count: usize,
    pub database_count: usize,
    pub partition_view_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataImportPreview {
    pub import_id: String,
    pub filename: Option<String>,
    pub template_type: String,
    pub summary: MetadataImportSummary,
    pub changes: Vec<MetadataChange>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataImportSummary {
    pub instances: usize,
    pub clusters: usize,
    pub nodes: usize,
    pub databases: usize,
    pub partition_views: usize,
    pub warnings: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataChange {
    pub kind: &'static str,
    pub id: String,
    pub action: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataConfirmResponse {
    pub import_id: String,
    pub applied: bool,
    pub summary: MetadataImportSummary,
}

#[derive(Debug, Default)]
struct MetadataRecords {
    instances: HashMap<String, InstanceMetadata>,
    clusters: HashMap<String, ClusterMetadata>,
    nodes: HashMap<String, NodeMetadata>,
    imports: HashMap<String, PendingImport>,
}

#[derive(Debug, Clone)]
struct PendingImport {
    preview: MetadataImportPreview,
    template: MetadataTemplate,
}

#[derive(Debug, Clone)]
pub struct MetadataStore {
    root: PathBuf,
    records: Arc<RwLock<MetadataRecords>>,
}

impl MetadataStore {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let root = config.storage.metadata_dir();
        let records = MetadataRecords {
            instances: load_map(root.join("instances.json"), |item: &InstanceMetadata| {
                item.instance_id.clone()
            }),
            clusters: load_map(root.join("clusters.json"), |item: &ClusterMetadata| {
                item.cluster_id.clone()
            }),
            nodes: load_map(root.join("nodes.json"), |item: &NodeMetadata| {
                item.node_id.clone()
            }),
            imports: HashMap::new(),
        };
        Self {
            root,
            records: Arc::new(RwLock::new(records)),
        }
    }

    pub async fn get_instance(&self, instance_id: &str) -> Option<InstanceMetadata> {
        self.records
            .read()
            .await
            .instances
            .get(instance_id)
            .cloned()
    }

    pub async fn list_instances(&self) -> Vec<MetadataInstanceSummary> {
        let records = self.records.read().await;
        let mut instances = records
            .instances
            .values()
            .map(|instance| {
                let cluster = instance
                    .cluster_id
                    .as_ref()
                    .and_then(|cluster_id| records.clusters.get(cluster_id));
                MetadataInstanceSummary {
                    instance_id: instance.instance_id.clone(),
                    remark: instance.remark.clone(),
                    cluster_id: instance.cluster_id.clone(),
                    node_id: instance.node_id.clone(),
                    product: instance.product.clone(),
                    version: instance.version.clone(),
                    environment: instance.environment.clone(),
                    region: instance.region.clone(),
                    owner: instance.owner.clone(),
                    node_count: instance
                        .cluster_id
                        .as_deref()
                        .map(|cluster_id| cluster_nodes(&records, cluster_id).len())
                        .unwrap_or_else(|| {
                            records
                                .nodes
                                .values()
                                .filter(|node| {
                                    node.instance_id.as_deref()
                                        == Some(instance.instance_id.as_str())
                                })
                                .count()
                        }),
                    database_count: cluster.map(|cluster| cluster.databases.len()).unwrap_or(0),
                    partition_view_count: cluster
                        .map(|cluster| cluster.partition_views.len())
                        .unwrap_or(0),
                }
            })
            .collect::<Vec<_>>();
        instances.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        instances
    }

    pub async fn get_instance_snapshot(
        &self,
        instance_id: &str,
    ) -> Result<MetadataSnapshotResponse, AppError> {
        let records = self.records.read().await;
        let instance = records
            .instances
            .get(instance_id)
            .cloned()
            .ok_or_else(|| AppError::bad_request("unknown instanceId"))?;
        let cluster_id = instance
            .cluster_id
            .as_deref()
            .unwrap_or(instance.instance_id.as_str());
        let cluster = records
            .clusters
            .get(cluster_id)
            .cloned()
            .ok_or_else(|| AppError::bad_request("instanceId has no metadata snapshot"))?;
        let nodes = cluster_nodes(&records, cluster_id);
        Ok(MetadataSnapshotResponse {
            instance: Some(instance),
            cluster,
            nodes,
        })
    }

    pub async fn get_cluster(&self, cluster_id: &str) -> Option<ClusterMetadata> {
        self.records.read().await.clusters.get(cluster_id).cloned()
    }

    pub async fn list_cluster_nodes(&self, cluster_id: &str) -> Vec<NodeMetadata> {
        let records = self.records.read().await;
        cluster_nodes(&records, cluster_id)
    }

    pub async fn resolve_task_context(
        &self,
        requested_instance_id: Option<String>,
        requested_cluster_id: Option<String>,
        requested_node_id: Option<String>,
    ) -> Result<TaskMetadataContext, AppError> {
        let records = self.records.read().await;
        let requested_instance = requested_instance_id
            .as_ref()
            .map(|instance_id| {
                records.instances.get(instance_id).cloned().ok_or_else(|| {
                    AppError::bad_request(format!("unknown instanceId {instance_id}"))
                })
            })
            .transpose()?;
        let node_id = merge_related_id(
            "nodeId",
            requested_node_id,
            requested_instance
                .as_ref()
                .and_then(|value| value.node_id.clone()),
        )?;
        let node = node_id
            .as_ref()
            .map(|node_id| {
                records
                    .nodes
                    .get(node_id)
                    .cloned()
                    .ok_or_else(|| AppError::bad_request(format!("unknown nodeId {node_id}")))
            })
            .transpose()?;
        let instance_id = merge_related_id(
            "instanceId",
            requested_instance_id,
            node.as_ref().and_then(|value| value.instance_id.clone()),
        )?;
        let instance = instance_id
            .as_ref()
            .map(|instance_id| {
                records.instances.get(instance_id).cloned().ok_or_else(|| {
                    AppError::bad_request(format!("unknown instanceId {instance_id}"))
                })
            })
            .transpose()?;
        let node_cluster_id = node.as_ref().and_then(|node| {
            let matches = records
                .clusters
                .values()
                .filter(|cluster| cluster.nodes.iter().any(|value| value == &node.node_id))
                .map(|cluster| cluster.cluster_id.clone())
                .collect::<Vec<_>>();
            (matches.len() == 1).then(|| matches[0].clone())
        });
        let derived_cluster_id = instance
            .as_ref()
            .and_then(|value| value.cluster_id.clone())
            .or(node_cluster_id);
        let cluster_id = merge_related_id("clusterId", requested_cluster_id, derived_cluster_id)?;
        let cluster =
            cluster_id
                .as_ref()
                .map(|cluster_id| {
                    records.clusters.get(cluster_id).cloned().ok_or_else(|| {
                        AppError::bad_request(format!("unknown clusterId {cluster_id}"))
                    })
                })
                .transpose()?;

        if let (Some(instance), Some(node)) = (&instance, &node) {
            if let Some(node_instance_id) = node.instance_id.as_ref() {
                if node_instance_id != &instance.instance_id {
                    return Err(AppError::bad_request(format!(
                        "nodeId {} belongs to instanceId {}, not {}",
                        node.node_id, node_instance_id, instance.instance_id
                    )));
                }
            }
        }
        if let (Some(cluster_id), Some(node)) = (&cluster_id, &node) {
            let belongs = cluster
                .as_ref()
                .map(|cluster| cluster.nodes.iter().any(|value| value == &node.node_id))
                .unwrap_or(false)
                || node
                    .instance_id
                    .as_ref()
                    .and_then(|instance_id| records.instances.get(instance_id))
                    .and_then(|instance| instance.cluster_id.as_ref())
                    == Some(cluster_id);
            if !belongs {
                return Err(AppError::bad_request(format!(
                    "nodeId {} does not belong to clusterId {}",
                    node.node_id, cluster_id
                )));
            }
        }

        let mut cluster = cluster;
        if let Some(cluster) = cluster.as_mut() {
            cluster.raw_snapshot = None;
        }
        let cluster_nodes = cluster_id
            .as_deref()
            .map(|cluster_id| cluster_nodes(&records, cluster_id))
            .unwrap_or_default();
        let product = instance
            .as_ref()
            .and_then(|value| value.product.clone())
            .or_else(|| cluster.as_ref().and_then(|value| value.product.clone()));
        let version = instance
            .as_ref()
            .and_then(|value| value.version.clone())
            .or_else(|| cluster.as_ref().and_then(|value| value.version.clone()));
        let environment = instance
            .as_ref()
            .and_then(|value| value.environment.clone())
            .or_else(|| cluster.as_ref().and_then(|value| value.environment.clone()));

        Ok(TaskMetadataContext {
            schema_version: 1,
            resolved_at: Utc::now(),
            instance_id,
            cluster_id,
            node_id,
            product,
            version,
            environment,
            instance,
            cluster,
            node,
            cluster_nodes,
        })
    }

    pub async fn create_import_preview(
        &self,
        req: MetadataImportRequest,
    ) -> Result<MetadataImportPreview, AppError> {
        let template = parse_template(
            &req.template_type,
            &req.content,
            req.instance_id.as_deref(),
            req.remark.as_deref(),
        )?;
        let mut records = self.records.write().await;
        let import_id = next_id("meta_imp");
        let preview = build_preview(&import_id, &req, &template, &records);
        records.imports.insert(
            import_id,
            PendingImport {
                preview: preview.clone(),
                template,
            },
        );
        Ok(preview)
    }

    pub async fn fetch_import_preview(
        &self,
        req: MetadataFetchImportRequest,
    ) -> Result<MetadataImportPreview, AppError> {
        let content = fetch_metadata_content(&req.url).await?;
        self.create_import_preview(MetadataImportRequest {
            template_type: req
                .template_type
                .unwrap_or_else(|| "opengemini".to_string()),
            filename: req.filename.or(Some(req.url)),
            instance_id: req.instance_id,
            remark: req.remark,
            content,
        })
        .await
    }

    pub async fn fetch_snapshot(
        &self,
        req: MetadataFetchImportRequest,
    ) -> Result<MetadataSnapshotResponse, AppError> {
        let content = fetch_metadata_content(&req.url).await?;
        let template = parse_template(
            req.template_type.as_deref().unwrap_or("opengemini"),
            &content,
            req.instance_id.as_deref(),
            req.remark.as_deref(),
        )?;
        let instance = template.instances.into_iter().next();
        let cluster = template
            .clusters
            .into_iter()
            .next()
            .ok_or_else(|| AppError::bad_request("metadata snapshot has no cluster"))?;
        Ok(MetadataSnapshotResponse {
            instance,
            cluster,
            nodes: template.nodes,
        })
    }

    pub async fn get_import_preview(&self, import_id: &str) -> Option<MetadataImportPreview> {
        self.records
            .read()
            .await
            .imports
            .get(import_id)
            .map(|pending| pending.preview.clone())
    }

    pub async fn confirm_import(
        &self,
        import_id: &str,
    ) -> Result<MetadataConfirmResponse, AppError> {
        let mut records = self.records.write().await;
        let pending = records
            .imports
            .remove(import_id)
            .ok_or_else(|| AppError::bad_request("unknown metadata import"))?;
        if pending.preview.summary.errors > 0 {
            return Err(AppError::bad_request(
                "metadata import has validation errors",
            ));
        }

        for instance in pending.template.instances {
            records
                .instances
                .insert(instance.instance_id.clone(), instance);
        }
        for cluster in pending.template.clusters {
            records.clusters.insert(cluster.cluster_id.clone(), cluster);
        }
        for node in pending.template.nodes {
            records.nodes.insert(node.node_id.clone(), node);
        }

        persist_records(&self.root, &records)?;
        Ok(MetadataConfirmResponse {
            import_id: pending.preview.import_id,
            applied: true,
            summary: pending.preview.summary,
        })
    }
}

fn merge_related_id(
    name: &str,
    requested: Option<String>,
    derived: Option<String>,
) -> Result<Option<String>, AppError> {
    match (requested, derived) {
        (Some(requested), Some(derived)) if requested != derived => Err(AppError::bad_request(
            format!("{name} {requested} conflicts with instance metadata value {derived}"),
        )),
        (Some(requested), _) => Ok(Some(requested)),
        (None, derived) => Ok(derived),
    }
}

fn cluster_nodes(records: &MetadataRecords, cluster_id: &str) -> Vec<NodeMetadata> {
    let mut nodes = records
        .nodes
        .values()
        .filter(|node| {
            records
                .clusters
                .get(cluster_id)
                .map(|cluster| cluster.nodes.iter().any(|node_id| node_id == &node.node_id))
                .unwrap_or(false)
                || node
                    .instance_id
                    .as_ref()
                    .and_then(|instance_id| records.instances.get(instance_id))
                    .and_then(|instance| instance.cluster_id.as_ref())
                    .map(|node_cluster_id| node_cluster_id == cluster_id)
                    .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    nodes
}

async fn fetch_metadata_content(url: &str) -> Result<String, AppError> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::bad_request(
            "metadata fetch url must start with http:// or https://",
        ));
    }
    reqwest::get(url)
        .await
        .map_err(|err| AppError::bad_request(format!("failed to fetch metadata: {err}")))?
        .error_for_status()
        .map_err(|err| AppError::bad_request(format!("metadata endpoint returned error: {err}")))?
        .text()
        .await
        .map_err(|err| AppError::bad_request(format!("failed to read metadata response: {err}")))
}

fn parse_template(
    template_type: &str,
    content: &str,
    instance_id: Option<&str>,
    remark: Option<&str>,
) -> Result<MetadataTemplate, AppError> {
    match template_type.to_ascii_lowercase().as_str() {
        "json" => parse_metadata_json(content, instance_id, remark),
        "yaml" | "yml" => serde_yaml::from_str(content)
            .map_err(|err| AppError::bad_request(format!("invalid metadata YAML: {err}"))),
        "opengemini" | "opengemini-json" | "influxdb-meta" => {
            parse_opengemini_snapshot(content, instance_id, remark)
        }
        "csv" => Err(AppError::bad_request(
            "metadata CSV import is reserved but not implemented yet",
        )),
        other => Err(AppError::bad_request(format!(
            "unsupported metadata templateType {other}"
        ))),
    }
}

fn parse_metadata_json(
    content: &str,
    instance_id: Option<&str>,
    remark: Option<&str>,
) -> Result<MetadataTemplate, AppError> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|err| AppError::bad_request(format!("invalid metadata JSON: {err}")))?;
    if value.get("ClusterID").is_some()
        || value.get("MetaNodes").is_some()
        || value.get("DataNodes").is_some()
        || value.get("SqlNodes").is_some()
    {
        return normalize_opengemini_value(value, instance_id, remark);
    }
    serde_json::from_value(value)
        .map_err(|err| AppError::bad_request(format!("invalid metadata JSON: {err}")))
}

fn parse_opengemini_snapshot(
    content: &str,
    instance_id: Option<&str>,
    remark: Option<&str>,
) -> Result<MetadataTemplate, AppError> {
    let value = serde_json::from_str(content)
        .map_err(|err| AppError::bad_request(format!("invalid openGemini metadata JSON: {err}")))?;
    normalize_opengemini_value(value, instance_id, remark)
}

fn normalize_opengemini_value(
    value: serde_json::Value,
    instance_id: Option<&str>,
    remark: Option<&str>,
) -> Result<MetadataTemplate, AppError> {
    let instance_id = clean_required_instance_id(instance_id)?;
    let remark = clean_optional_remark(remark)?;
    let source_cluster_id = value
        .get("ClusterID")
        .and_then(serde_json::Value::as_u64)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "opengemini-local".to_string());
    let mut labels = HashMap::new();
    labels.insert("sourceClusterId".to_string(), source_cluster_id.clone());
    insert_u64_label(&mut labels, "term", value.get("Term"));
    insert_u64_label(&mut labels, "index", value.get("Index"));
    insert_u64_label(&mut labels, "clusterPtNum", value.get("ClusterPtNum"));
    insert_u64_label(&mut labels, "ptNumPerNode", value.get("PtNumPerNode"));
    insert_u64_label(&mut labels, "numOfShards", value.get("NumOfShards"));
    insert_u64_label(&mut labels, "maxNodeId", value.get("MaxNodeID"));
    insert_u64_label(&mut labels, "maxShardGroupId", value.get("MaxShardGroupID"));
    insert_u64_label(&mut labels, "maxShardId", value.get("MaxShardID"));
    labels.insert(
        "takeOverEnabled".to_string(),
        bool_label(value.get("TakeOverEnabled")),
    );
    labels.insert(
        "balancerEnabled".to_string(),
        bool_label(value.get("BalancerEnabled")),
    );
    if let Some(databases) = value
        .get("Databases")
        .and_then(serde_json::Value::as_object)
    {
        labels.insert("databaseCount".to_string(), databases.len().to_string());
        labels.insert(
            "databases".to_string(),
            databases.keys().cloned().collect::<Vec<_>>().join(","),
        );
    }
    let databases = normalize_opengemini_databases(value.get("Databases"));
    let partition_views = normalize_opengemini_pt_view(value.get("PtView"));

    let mut template = MetadataTemplate {
        instances: vec![InstanceMetadata {
            instance_id: instance_id.clone(),
            remark,
            cluster_id: Some(instance_id.clone()),
            node_id: None,
            product: Some("opengemini".to_string()),
            version: None,
            environment: None,
            region: None,
            owner: None,
            tags: HashMap::from([("sourceClusterId".to_string(), source_cluster_id.clone())]),
        }],
        clusters: vec![ClusterMetadata {
            cluster_id: instance_id.clone(),
            name: Some(format!("opengemini-{instance_id}")),
            product: Some("opengemini".to_string()),
            version: None,
            environment: None,
            nodes: Vec::new(),
            labels,
            databases,
            partition_views,
            raw_snapshot: Some(value.clone()),
        }],
        ..MetadataTemplate::default()
    };

    append_opengemini_nodes(&mut template, &instance_id, "meta", value.get("MetaNodes"));
    append_opengemini_nodes(&mut template, &instance_id, "data", value.get("DataNodes"));
    append_opengemini_nodes(&mut template, &instance_id, "sql", value.get("SqlNodes"));
    if let Some(cluster) = template.clusters.first_mut() {
        cluster.nodes = template
            .nodes
            .iter()
            .map(|node| node.node_id.clone())
            .collect();
    }
    Ok(template)
}

fn clean_required_instance_id(instance_id: Option<&str>) -> Result<String, AppError> {
    let value = instance_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::bad_request("instanceId is required for openGemini metadata import")
        })?;
    let valid = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'));
    if !valid {
        return Err(AppError::bad_request(
            "instanceId may only contain letters, numbers, '.', ':', '_' or '-'",
        ));
    }
    Ok(value.to_string())
}

fn clean_optional_remark(remark: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = remark.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > 120 {
        return Err(AppError::bad_request(
            "remark must be at most 120 characters",
        ));
    }
    Ok(Some(value.to_string()))
}

fn normalize_opengemini_databases(value: Option<&serde_json::Value>) -> Vec<DatabaseMetadata> {
    let Some(databases) = value.and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut result = databases
        .iter()
        .map(|(name, database)| DatabaseMetadata {
            name: database
                .get("Name")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(name)
                .to_string(),
            default_retention_policy: database
                .get("DefaultRetentionPolicy")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            replica_n: database.get("ReplicaN").and_then(serde_json::Value::as_u64),
            mark_deleted: database
                .get("MarkDeleted")
                .and_then(serde_json::Value::as_bool),
            retention_policies: normalize_opengemini_retention_policies(
                database.get("RetentionPolicies"),
            ),
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.name.cmp(&right.name));
    result
}

fn normalize_opengemini_retention_policies(
    value: Option<&serde_json::Value>,
) -> Vec<RetentionPolicyMetadata> {
    let Some(policies) = value.and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut result = policies
        .iter()
        .map(|(name, policy)| RetentionPolicyMetadata {
            name: policy
                .get("Name")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(name)
                .to_string(),
            replica_n: policy.get("ReplicaN").and_then(serde_json::Value::as_u64),
            duration: policy.get("Duration").and_then(serde_json::Value::as_u64),
            shard_group_duration: policy
                .get("ShardGroupDuration")
                .and_then(serde_json::Value::as_u64),
            index_group_duration: policy
                .get("IndexGroupDuration")
                .and_then(serde_json::Value::as_u64),
            mark_deleted: policy
                .get("MarkDeleted")
                .and_then(serde_json::Value::as_bool),
            measurements: normalize_opengemini_measurements(policy),
            shard_groups: normalize_opengemini_shard_groups(policy.get("ShardGroups")),
            index_groups: normalize_opengemini_index_groups(policy.get("IndexGroups")),
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.name.cmp(&right.name));
    result
}

fn normalize_opengemini_measurements(policy: &serde_json::Value) -> Vec<MeasurementMetadata> {
    let versions = policy
        .get("MstVersions")
        .and_then(serde_json::Value::as_object);
    let Some(measurements) = policy
        .get("Measurements")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let mut result = measurements
        .iter()
        .map(|(name, measurement)| MeasurementMetadata {
            name: measurement
                .get("Name")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(name)
                .to_string(),
            logical_name: measurement_logical_name(versions, name),
            version_name: measurement_version_name(versions, name),
            version: measurement_version(versions, name),
            shard_key_type: measurement
                .get("ShardKeys")
                .and_then(serde_json::Value::as_array)
                .and_then(|keys| keys.first())
                .and_then(|key| key.get("Type"))
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            schema: normalize_opengemini_schema(measurement.get("Schema")),
            mark_deleted: measurement
                .get("MarkDeleted")
                .and_then(serde_json::Value::as_bool),
            engine_type: measurement
                .get("EngineType")
                .and_then(serde_json::Value::as_u64),
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.name.cmp(&right.name));
    result
}

fn measurement_version_entry<'a>(
    versions: Option<&'a serde_json::Map<String, serde_json::Value>>,
    measurement_name: &str,
) -> Option<(&'a str, &'a serde_json::Value)> {
    versions.and_then(|versions| {
        versions
            .iter()
            .find(|(_, version)| {
                version
                    .get("NameWithVersion")
                    .and_then(serde_json::Value::as_str)
                    == Some(measurement_name)
            })
            .map(|(name, version)| (name.as_str(), version))
            .or_else(|| {
                versions
                    .get_key_value(measurement_name)
                    .map(|(name, value)| (name.as_str(), value))
            })
    })
}

fn measurement_logical_name(
    versions: Option<&serde_json::Map<String, serde_json::Value>>,
    measurement_name: &str,
) -> Option<String> {
    measurement_version_entry(versions, measurement_name)
        .map(|(name, _)| name.to_string())
        .or_else(|| {
            measurement_name
                .rsplit_once('_')
                .map(|(base, _)| base.to_string())
        })
}

fn measurement_version_name(
    versions: Option<&serde_json::Map<String, serde_json::Value>>,
    measurement_name: &str,
) -> Option<String> {
    measurement_version_entry(versions, measurement_name)
        .map(|(_, version)| version)
        .and_then(|version| version.get("NameWithVersion"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn measurement_version(
    versions: Option<&serde_json::Map<String, serde_json::Value>>,
    measurement_name: &str,
) -> Option<u64> {
    measurement_version_entry(versions, measurement_name)
        .and_then(|(_, version)| version.get("Version"))
        .and_then(serde_json::Value::as_u64)
}

fn normalize_opengemini_schema(value: Option<&serde_json::Value>) -> Vec<FieldSchemaMetadata> {
    let Some(schema) = value.and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut result = schema
        .iter()
        .map(|(name, field)| FieldSchemaMetadata {
            name: name.to_string(),
            typ: field.get("Typ").and_then(serde_json::Value::as_u64),
            end_time: field.get("EndTime").and_then(serde_json::Value::as_u64),
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.name.cmp(&right.name));
    result
}

fn normalize_opengemini_shard_groups(value: Option<&serde_json::Value>) -> Vec<ShardGroupMetadata> {
    let Some(groups) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut result = groups
        .iter()
        .filter_map(|group| {
            let id = group.get("ID").and_then(serde_json::Value::as_u64)?;
            let mut shard_ids = Vec::new();
            let mut owners = Vec::new();
            let mut normalized_shards = Vec::new();
            if let Some(shards) = group.get("Shards").and_then(serde_json::Value::as_array) {
                for shard in shards {
                    if let Some(shard_id) = shard.get("ID").and_then(serde_json::Value::as_u64) {
                        shard_ids.push(shard_id);
                        let shard_owners = shard
                            .get("Owners")
                            .and_then(serde_json::Value::as_array)
                            .map(|owners| {
                                owners
                                    .iter()
                                    .filter_map(serde_json::Value::as_u64)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        owners.extend(shard_owners.iter().copied());
                        normalized_shards.push(ShardMetadata {
                            id: shard_id,
                            owners: shard_owners,
                            min: json_string(shard.get("Min")),
                            max: json_string(shard.get("Max")),
                            tier: shard.get("Tier").and_then(serde_json::Value::as_u64),
                            index_id: shard.get("IndexID").and_then(serde_json::Value::as_u64),
                            downsample_id: shard
                                .get("DownSampleID")
                                .and_then(serde_json::Value::as_u64),
                            downsample_level: shard
                                .get("DownSampleLevel")
                                .and_then(serde_json::Value::as_u64),
                            read_only: shard.get("ReadOnly").and_then(serde_json::Value::as_bool),
                            mark_delete: shard
                                .get("MarkDelete")
                                .and_then(serde_json::Value::as_bool),
                        });
                    }
                }
            }
            shard_ids.sort_unstable();
            shard_ids.dedup();
            owners.sort_unstable();
            owners.dedup();
            Some(ShardGroupMetadata {
                id,
                start_time: json_string(group.get("StartTime")),
                end_time: json_string(group.get("EndTime")),
                shard_ids,
                owners,
                shards: normalized_shards,
                deleted_at: json_string(group.get("DeletedAt")),
                truncated_at: json_string(group.get("TruncatedAt")),
                engine_type: group.get("EngineType").and_then(serde_json::Value::as_u64),
                version: group.get("Version").and_then(serde_json::Value::as_u64),
            })
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|group| group.id);
    result
}

fn normalize_opengemini_index_groups(value: Option<&serde_json::Value>) -> Vec<IndexGroupMetadata> {
    let Some(groups) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut result = groups
        .iter()
        .filter_map(|group| {
            let id = group.get("ID").and_then(serde_json::Value::as_u64)?;
            let indexes = group
                .get("Indexes")
                .and_then(serde_json::Value::as_array)
                .map(|indexes| {
                    indexes
                        .iter()
                        .filter_map(|index| {
                            Some(IndexMetadata {
                                id: index.get("ID").and_then(serde_json::Value::as_u64)?,
                                tier: index.get("Tier").and_then(serde_json::Value::as_u64),
                                owners: index
                                    .get("Owners")
                                    .and_then(serde_json::Value::as_array)
                                    .map(|owners| {
                                        owners
                                            .iter()
                                            .filter_map(serde_json::Value::as_u64)
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                mark_delete: index
                                    .get("MarkDelete")
                                    .and_then(serde_json::Value::as_bool),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(IndexGroupMetadata {
                id,
                start_time: json_string(group.get("StartTime")),
                end_time: json_string(group.get("EndTime")),
                deleted_at: json_string(group.get("DeletedAt")),
                engine_type: group.get("EngineType").and_then(serde_json::Value::as_u64),
                indexes,
            })
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|group| group.id);
    result
}

fn normalize_opengemini_pt_view(value: Option<&serde_json::Value>) -> Vec<PartitionViewMetadata> {
    let Some(databases) = value.and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for (database, partitions) in databases {
        let Some(partitions) = partitions.as_array() else {
            continue;
        };
        for partition in partitions {
            let Some(pt_id) = partition.get("PtId").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            let status = partition.get("Status").and_then(serde_json::Value::as_u64);
            result.push(PartitionViewMetadata {
                database: database.to_string(),
                pt_id,
                owner_node_id: partition
                    .get("Owner")
                    .and_then(|owner| owner.get("NodeID"))
                    .and_then(serde_json::Value::as_u64),
                status,
                status_text: partition_status_text(status),
                version: partition.get("Ver").and_then(serde_json::Value::as_u64),
                replica_group_id: partition.get("RGID").and_then(serde_json::Value::as_u64),
            });
        }
    }
    result.sort_by(|left, right| {
        left.database
            .cmp(&right.database)
            .then(left.pt_id.cmp(&right.pt_id))
    });
    result
}

fn append_opengemini_nodes(
    template: &mut MetadataTemplate,
    instance_id: &str,
    node_kind: &str,
    nodes: Option<&serde_json::Value>,
) {
    let Some(nodes) = nodes.and_then(serde_json::Value::as_array) else {
        return;
    };
    for node in nodes {
        let raw_id = node
            .get("ID")
            .and_then(serde_json::Value::as_u64)
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let node_id = format!("{instance_id}:{node_kind}-{raw_id}");
        let role = node
            .get("Role")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(node_kind)
            .to_string();
        let host = first_non_empty_str(node, &["Host", "TCPHost", "RPCAddr", "GossipAddr"]);
        let mut labels = HashMap::new();
        labels.insert("kind".to_string(), node_kind.to_string());
        labels.insert("rawNodeId".to_string(), raw_id.clone());
        insert_string_label(&mut labels, "rpcAddr", node.get("RPCAddr"));
        insert_string_label(&mut labels, "tcpHost", node.get("TCPHost"));
        insert_string_label(&mut labels, "gossipAddr", node.get("GossipAddr"));
        insert_string_label(&mut labels, "az", node.get("Az"));
        insert_u64_label(&mut labels, "statusCode", node.get("Status"));
        labels.insert("statusText".to_string(), status_text(node.get("Status")));
        insert_u64_label(&mut labels, "lTime", node.get("LTime"));
        insert_u64_label(&mut labels, "connId", node.get("ConnID"));
        insert_u64_label(&mut labels, "aliveConnId", node.get("AliveConnID"));
        insert_u64_label(&mut labels, "index", node.get("Index"));
        insert_u64_label(&mut labels, "segregateStatus", node.get("SegregateStatus"));

        template.nodes.push(NodeMetadata {
            node_id: node_id.clone(),
            raw_node_id: node.get("ID").and_then(serde_json::Value::as_u64),
            kind: Some(node_kind.to_string()),
            instance_id: Some(instance_id.to_string()),
            hostname: host.as_deref().map(hostname_from_addr),
            host,
            tcp_host: optional_string(node.get("TCPHost")),
            rpc_addr: optional_string(node.get("RPCAddr")),
            gossip_addr: optional_string(node.get("GossipAddr")),
            ssh_alias: None,
            role: Some(role),
            zone: node
                .get("Az")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            status: Some(status_text(node.get("Status"))),
            status_code: node.get("Status").and_then(serde_json::Value::as_i64),
            conn_id: node.get("ConnID").and_then(serde_json::Value::as_u64),
            alive_conn_id: node.get("AliveConnID").and_then(serde_json::Value::as_u64),
            index: node.get("Index").and_then(serde_json::Value::as_u64),
            labels,
        });
    }
}

fn optional_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn first_non_empty_str(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn hostname_from_addr(value: &str) -> String {
    value
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(value)
        .to_string()
}

fn insert_string_label(
    labels: &mut HashMap<String, String>,
    key: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
    {
        labels.insert(key.to_string(), value.to_string());
    }
}

fn insert_u64_label(
    labels: &mut HashMap<String, String>,
    key: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value.and_then(serde_json::Value::as_u64) {
        labels.insert(key.to_string(), value.to_string());
    }
}

fn bool_label(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(serde_json::Value::as_bool)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "false".to_string())
}

fn json_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn status_text(value: Option<&serde_json::Value>) -> String {
    match value.and_then(serde_json::Value::as_i64) {
        Some(0) => "inactive".to_string(),
        Some(1) => "active".to_string(),
        Some(status) => format!("status-{status}"),
        None => "unknown".to_string(),
    }
}

fn partition_status_text(status: Option<u64>) -> String {
    match status {
        Some(0) => "online".to_string(),
        Some(1) => "offline".to_string(),
        Some(2) => "pre-offline".to_string(),
        Some(value) => format!("status-{value}"),
        None => "unknown".to_string(),
    }
}

fn build_preview(
    import_id: &str,
    req: &MetadataImportRequest,
    template: &MetadataTemplate,
    records: &MetadataRecords,
) -> MetadataImportPreview {
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    collect_instance_changes(template, records, &mut changes, &mut warnings, &mut errors);
    collect_cluster_changes(template, records, &mut changes, &mut warnings, &mut errors);
    collect_node_changes(template, records, &mut changes, &mut warnings, &mut errors);

    MetadataImportPreview {
        import_id: import_id.to_string(),
        filename: req.filename.clone(),
        template_type: req.template_type.clone(),
        summary: MetadataImportSummary {
            instances: template.instances.len(),
            clusters: template.clusters.len(),
            nodes: template.nodes.len(),
            databases: template
                .clusters
                .iter()
                .map(|cluster| cluster.databases.len())
                .sum(),
            partition_views: template
                .clusters
                .iter()
                .map(|cluster| cluster.partition_views.len())
                .sum(),
            warnings: warnings.len(),
            errors: errors.len(),
        },
        changes,
        warnings,
        errors,
    }
}

fn collect_instance_changes(
    template: &MetadataTemplate,
    records: &MetadataRecords,
    changes: &mut Vec<MetadataChange>,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let mut seen = HashMap::<&str, usize>::new();
    for instance in &template.instances {
        if instance.instance_id.trim().is_empty() {
            errors.push("instance missing instanceId".to_string());
            continue;
        }
        *seen.entry(instance.instance_id.as_str()).or_default() += 1;
        if let Some(cluster_id) = instance.cluster_id.as_ref() {
            let exists = records.clusters.contains_key(cluster_id)
                || template
                    .clusters
                    .iter()
                    .any(|cluster| cluster.cluster_id == *cluster_id);
            if !exists {
                warnings.push(format!(
                    "instance {} references unknown cluster {}",
                    instance.instance_id, cluster_id
                ));
            }
        }
        changes.push(MetadataChange {
            kind: "instance",
            id: instance.instance_id.clone(),
            action: if records.instances.contains_key(&instance.instance_id) {
                "update"
            } else {
                "create"
            },
            message: format!("upsert instance {}", instance.instance_id),
        });
    }
    for (instance_id, count) in seen {
        if count > 1 {
            errors.push(format!("duplicate instanceId {instance_id} in import"));
        }
    }
}

fn collect_cluster_changes(
    template: &MetadataTemplate,
    records: &MetadataRecords,
    changes: &mut Vec<MetadataChange>,
    _warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let mut seen = HashMap::<&str, usize>::new();
    for cluster in &template.clusters {
        if cluster.cluster_id.trim().is_empty() {
            errors.push("cluster missing clusterId".to_string());
            continue;
        }
        *seen.entry(cluster.cluster_id.as_str()).or_default() += 1;
        changes.push(MetadataChange {
            kind: "cluster",
            id: cluster.cluster_id.clone(),
            action: if records.clusters.contains_key(&cluster.cluster_id) {
                "update"
            } else {
                "create"
            },
            message: format!("upsert cluster {}", cluster.cluster_id),
        });
    }
    for (cluster_id, count) in seen {
        if count > 1 {
            errors.push(format!("duplicate clusterId {cluster_id} in import"));
        }
    }
}

fn collect_node_changes(
    template: &MetadataTemplate,
    records: &MetadataRecords,
    changes: &mut Vec<MetadataChange>,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let mut seen = HashMap::<&str, usize>::new();
    for node in &template.nodes {
        if node.node_id.trim().is_empty() {
            errors.push("node missing nodeId".to_string());
            continue;
        }
        *seen.entry(node.node_id.as_str()).or_default() += 1;
        if let Some(instance_id) = node.instance_id.as_ref() {
            let exists = records.instances.contains_key(instance_id)
                || template
                    .instances
                    .iter()
                    .any(|instance| instance.instance_id == *instance_id);
            if !exists {
                warnings.push(format!(
                    "node {} references unknown instance {}",
                    node.node_id, instance_id
                ));
            }
        }
        changes.push(MetadataChange {
            kind: "node",
            id: node.node_id.clone(),
            action: if records.nodes.contains_key(&node.node_id) {
                "update"
            } else {
                "create"
            },
            message: format!("upsert node {}", node.node_id),
        });
    }
    for (node_id, count) in seen {
        if count > 1 {
            errors.push(format!("duplicate nodeId {node_id} in import"));
        }
    }
}

fn load_map<T, F>(path: PathBuf, key: F) -> HashMap<String, T>
where
    T: for<'de> Deserialize<'de>,
    F: Fn(&T) -> String,
{
    let Ok(raw) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(values) = serde_json::from_str::<Vec<T>>(&raw) else {
        return HashMap::new();
    };
    values
        .into_iter()
        .map(|value| (key(&value), value))
        .collect()
}

fn persist_records(root: &std::path::Path, records: &MetadataRecords) -> Result<(), AppError> {
    fs::create_dir_all(root)
        .map_err(|err| AppError::internal(format!("failed to create metadata dir: {err}")))?;
    write_json_array(
        root.join("instances.json"),
        records.instances.values().cloned().collect::<Vec<_>>(),
    )?;
    write_json_array(
        root.join("clusters.json"),
        records.clusters.values().cloned().collect::<Vec<_>>(),
    )?;
    write_json_array(
        root.join("nodes.json"),
        records.nodes.values().cloned().collect::<Vec<_>>(),
    )?;
    Ok(())
}

fn write_json_array<T: Serialize>(path: PathBuf, mut values: Vec<T>) -> Result<(), AppError> {
    values.sort_by_key(|value| serde_json::to_string(value).unwrap_or_default());
    let file = fs::File::create(path)
        .map_err(|err| AppError::internal(format!("failed to write metadata store: {err}")))?;
    serde_json::to_writer_pretty(file, &values)
        .map_err(|err| AppError::internal(format!("failed to encode metadata store: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::support::config::{
        AnalysisSettings, AuthSettings, EmbeddingSettings, LlmProvider, LlmSettings,
        LogAnalyzerSettings, ServerSettings, StorageSettings, ToolsSettings,
    };

    #[tokio::test]
    async fn previews_confirms_and_queries_metadata_import() {
        let fixture = Fixture::new("metadata-store");
        let store = MetadataStore::new(fixture.config());
        let preview = store
            .create_import_preview(MetadataImportRequest {
                template_type: "yaml".to_string(),
                filename: Some("metadata.yaml".to_string()),
                instance_id: None,
                remark: None,
                content: r#"
instances:
  - instanceId: i-123
    clusterId: c-1
    nodeId: n-1
    product: redis
clusters:
  - clusterId: c-1
    name: cache-prod
    nodes:
      - n-1
nodes:
  - nodeId: n-1
    instanceId: i-123
    role: primary
"#
                .to_string(),
            })
            .await
            .unwrap();

        assert_eq!(preview.summary.instances, 1);
        assert_eq!(preview.summary.clusters, 1);
        assert_eq!(preview.summary.nodes, 1);
        assert_eq!(preview.summary.errors, 0);

        let response = store.confirm_import(&preview.import_id).await.unwrap();
        assert!(response.applied);
        assert_eq!(
            store
                .get_instance("i-123")
                .await
                .unwrap()
                .product
                .as_deref(),
            Some("redis")
        );
        assert_eq!(store.get_cluster("c-1").await.unwrap().nodes, vec!["n-1"]);
        assert_eq!(store.list_cluster_nodes("c-1").await.len(), 1);
        let context = store
            .resolve_task_context(None, None, Some("n-1".to_string()))
            .await
            .unwrap();
        assert_eq!(context.instance_id.as_deref(), Some("i-123"));
        assert_eq!(context.cluster_id.as_deref(), Some("c-1"));
        assert_eq!(context.node_id.as_deref(), Some("n-1"));
        assert_eq!(context.product.as_deref(), Some("redis"));
        assert_eq!(context.cluster_nodes.len(), 1);
        assert!(context
            .cluster
            .as_ref()
            .and_then(|cluster| cluster.raw_snapshot.as_ref())
            .is_none());
        assert!(fixture.root.join("metadata/instances.json").exists());
    }

    #[tokio::test]
    async fn rejects_conflicting_task_metadata_selection() {
        let fixture = Fixture::new("metadata-task-conflict");
        let store = MetadataStore::new(fixture.config());
        let preview = store
            .create_import_preview(MetadataImportRequest {
                template_type: "yaml".to_string(),
                filename: None,
                instance_id: None,
                remark: None,
                content: r#"
instances:
  - instanceId: i-1
    clusterId: c-1
clusters:
  - clusterId: c-1
  - clusterId: c-2
"#
                .to_string(),
            })
            .await
            .unwrap();
        store.confirm_import(&preview.import_id).await.unwrap();

        let error = store
            .resolve_task_context(Some("i-1".to_string()), Some("c-2".to_string()), None)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("conflicts"));
    }

    #[tokio::test]
    async fn detects_duplicate_ids_in_import_preview() {
        let fixture = Fixture::new("metadata-duplicates");
        let store = MetadataStore::new(fixture.config());
        let preview = store
            .create_import_preview(MetadataImportRequest {
                template_type: "json".to_string(),
                filename: None,
                instance_id: None,
                remark: None,
                content: serde_json::json!({
                    "instances": [
                        { "instanceId": "i-dup" },
                        { "instanceId": "i-dup" }
                    ]
                })
                .to_string(),
            })
            .await
            .unwrap();

        assert_eq!(preview.summary.errors, 1);
        assert!(store.confirm_import(&preview.import_id).await.is_err());
    }

    #[tokio::test]
    async fn normalizes_opengemini_getdata_snapshot() {
        let fixture = Fixture::new("metadata-opengemini");
        let store = MetadataStore::new(fixture.config());
        let preview = store
            .create_import_preview(MetadataImportRequest {
                template_type: "opengemini".to_string(),
                filename: Some("getdata.json".to_string()),
                instance_id: Some("prod-a".to_string()),
                remark: Some("生产集群 A".to_string()),
                content: serde_json::json!({
                    "ClusterID": 6735497445922383781_u64,
                    "Term": 2,
                    "Index": 51711,
                    "NumOfShards": 3,
                    "MetaNodes": [
                        {
                            "ID": 1,
                            "Host": "127.0.0.1:8091",
                            "RPCAddr": "127.0.0.1:8092",
                            "TCPHost": "127.0.0.1:8088",
                            "Status": 0
                        }
                    ],
                    "DataNodes": [
                        {
                            "ID": 2,
                            "Host": "127.0.0.1:8400",
                            "TCPHost": "127.0.0.1:8401",
                            "Status": 1,
                            "ConnID": 1,
                            "AliveConnID": 1,
                            "Index": 51700,
                            "Az": ""
                        }
                    ],
                    "SqlNodes": [
                        {
                            "ID": 3,
                            "TCPHost": ":8086",
                            "Status": 1,
                            "GossipAddr": ":8012"
                        }
                    ],
                    "Databases": {
                        "mydb": {
                            "Name": "mydb",
                            "DefaultRetentionPolicy": "autogen",
                            "ReplicaN": 1,
                            "RetentionPolicies": {
                                "autogen": {
                                    "Name": "autogen",
                                    "ReplicaN": 1,
                                    "Duration": 0_u64,
                                    "ShardGroupDuration": 604800000000000_u64,
                                    "IndexGroupDuration": 604800000000000_u64,
                                    "Measurements": {
                                        "testmst_0000": {
                                            "Name": "testmst_0000",
                                            "ShardKeys": [
                                                { "Type": "hash", "ShardGroup": 1 }
                                            ],
                                            "Schema": {
                                                "tagk": { "Typ": 6, "EndTime": 414642691 },
                                                "value": { "Typ": 3, "EndTime": 414642691 }
                                            },
                                            "MarkDeleted": false,
                                            "EngineType": 0
                                        }
                                    },
                                    "MstVersions": {
                                        "testmst": {
                                            "NameWithVersion": "testmst_0000",
                                            "Version": 0
                                        }
                                    },
                                    "ShardGroups": [
                                        {
                                            "ID": 1,
                                            "StartTime": "2026-06-01T00:00:00Z",
                                            "EndTime": "2026-06-08T00:00:00Z",
                                            "Shards": [
                                                { "ID": 1, "Owners": [0], "IndexID": 1 }
                                            ],
                                            "EngineType": 0,
                                            "Version": 0
                                        }
                                    ],
                                    "IndexGroups": [
                                        {
                                            "ID": 1,
                                            "StartTime": "2026-06-01T00:00:00Z",
                                            "EndTime": "2026-06-08T00:00:00Z",
                                            "Indexes": [
                                                { "ID": 1, "Owners": [0], "Tier": 0, "MarkDelete": false }
                                            ],
                                            "EngineType": 0
                                        }
                                    ]
                                }
                            },
                            "MarkDeleted": false
                        }
                    },
                    "PtView": {
                        "mydb": [
                            {
                                "Owner": { "NodeID": 2 },
                                "Status": 0,
                                "PtId": 0,
                                "Ver": 1,
                                "RGID": 0
                            }
                        ]
                    },
                    "TakeOverEnabled": true,
                    "BalancerEnabled": true
                })
                .to_string(),
            })
            .await
            .unwrap();

        assert_eq!(preview.summary.clusters, 1);
        assert_eq!(preview.summary.instances, 1);
        assert_eq!(preview.summary.nodes, 3);
        assert_eq!(preview.summary.databases, 1);
        assert_eq!(preview.summary.partition_views, 1);
        assert_eq!(preview.summary.errors, 0);

        store.confirm_import(&preview.import_id).await.unwrap();
        let instances = store.list_instances().await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].instance_id, "prod-a");
        assert_eq!(instances[0].remark.as_deref(), Some("生产集群 A"));
        assert_eq!(instances[0].cluster_id.as_deref(), Some("prod-a"));
        assert_eq!(instances[0].node_count, 3);
        let snapshot = store.get_instance_snapshot("prod-a").await.unwrap();
        let instance = snapshot.instance.unwrap();
        assert_eq!(instance.instance_id, "prod-a");
        assert_eq!(instance.remark.as_deref(), Some("生产集群 A"));
        let cluster = snapshot.cluster;
        assert_eq!(cluster.product.as_deref(), Some("opengemini"));
        assert_eq!(
            cluster.labels.get("sourceClusterId").map(String::as_str),
            Some("6735497445922383781")
        );
        assert_eq!(
            cluster.labels.get("databases").map(String::as_str),
            Some("mydb")
        );
        assert_eq!(
            cluster.nodes,
            vec!["prod-a:meta-1", "prod-a:data-2", "prod-a:sql-3"]
        );
        assert_eq!(cluster.partition_views.len(), 1);
        assert_eq!(cluster.partition_views[0].database, "mydb");
        assert_eq!(cluster.partition_views[0].owner_node_id, Some(2));
        assert_eq!(cluster.partition_views[0].status_text, "online");
        assert_eq!(cluster.databases.len(), 1);
        let database = &cluster.databases[0];
        assert_eq!(database.name, "mydb");
        assert_eq!(
            database.default_retention_policy.as_deref(),
            Some("autogen")
        );
        assert_eq!(
            database.retention_policies[0].measurements[0].name,
            "testmst_0000"
        );
        assert_eq!(
            database.retention_policies[0].measurements[0]
                .schema
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["tagk", "value"]
        );
        assert_eq!(
            database.retention_policies[0].shard_groups[0].shard_ids,
            vec![1]
        );
        assert_eq!(
            database.retention_policies[0].shard_groups[0].shards[0].owners,
            vec![0]
        );
        assert_eq!(
            database.retention_policies[0].measurements[0]
                .logical_name
                .as_deref(),
            Some("testmst")
        );
        assert_eq!(
            database.retention_policies[0].index_groups[0].indexes[0].id,
            1
        );
        assert!(cluster.raw_snapshot.is_some());

        let data_node = store
            .list_cluster_nodes("prod-a")
            .await
            .into_iter()
            .find(|node| node.node_id == "prod-a:data-2")
            .unwrap();
        assert_eq!(data_node.role.as_deref(), Some("data"));
        assert_eq!(data_node.status.as_deref(), Some("active"));
        assert_eq!(data_node.raw_node_id, Some(2));
        assert_eq!(data_node.conn_id, Some(1));
        assert_eq!(
            data_node.labels.get("tcpHost").map(String::as_str),
            Some("127.0.0.1:8401")
        );
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("logagent-{name}-{now}"));
            fs::create_dir_all(root.join("metadata/imports")).unwrap();
            Self { root }
        }

        fn config(&self) -> Arc<AppConfig> {
            Arc::new(AppConfig {
                config_path: self.root.join("logagent-test.yaml"),
                server: ServerSettings {
                    bind: "127.0.0.1:0".to_string(),
                    public_base_url: "http://127.0.0.1:0".to_string(),
                    max_concurrent_tasks: 2,
                },
                auth: AuthSettings { api_keys: vec![] },
                storage: StorageSettings {
                    data_dir: self.root.clone(),
                    max_upload_bytes: 1024 * 1024,
                    max_chunk_bytes: 512 * 1024,
                },
                log_analyzer: LogAnalyzerSettings {
                    keywords: vec!["error".to_string()],
                    max_matches: 20,
                },
                tools: ToolsSettings::default(),
                llm: LlmSettings {
                    provider: LlmProvider::Stub,
                    base_url: None,
                    api_key: None,
                    binary_path: None,
                    binary_max_output_bytes: 1024 * 1024,
                    model: "stub".to_string(),
                    request_timeout_seconds: 1,
                    max_input_chars: 60_000,
                    max_output_tokens: 100,
                },
                claude_code: crate::support::config::ClaudeCodeSettings::default(),
                mcp: crate::support::config::McpSettings::default(),
                analysis: test_analysis_settings(),
                embedding: test_embedding_settings(),
            })
        }
    }

    fn test_analysis_settings() -> AnalysisSettings {
        AnalysisSettings {
            max_rounds: 4,
            max_llm_calls: 4,
            max_actions: 6,
            max_repeated_action_fingerprints: 1,
        }
    }

    fn test_embedding_settings() -> EmbeddingSettings {
        EmbeddingSettings {
            enabled: false,
            provider: "openai_compatible".to_string(),
            model: "text-embedding-3-small".to_string(),
            api_key_env: None,
            store: "sqlite".to_string(),
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
