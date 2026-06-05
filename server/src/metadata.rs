use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{config::AppConfig, error::AppError, id::next_id};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMetadata {
    pub instance_id: String,
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMetadata {
    pub node_id: String,
    pub instance_id: Option<String>,
    pub hostname: Option<String>,
    pub host: Option<String>,
    pub ssh_alias: Option<String>,
    pub role: Option<String>,
    pub zone: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
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
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataFetchImportRequest {
    pub url: String,
    pub template_type: Option<String>,
    pub filename: Option<String>,
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

    pub async fn get_cluster(&self, cluster_id: &str) -> Option<ClusterMetadata> {
        self.records.read().await.clusters.get(cluster_id).cloned()
    }

    pub async fn list_cluster_nodes(&self, cluster_id: &str) -> Vec<NodeMetadata> {
        let records = self.records.read().await;
        records
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
            .collect()
    }

    pub async fn create_import_preview(
        &self,
        req: MetadataImportRequest,
    ) -> Result<MetadataImportPreview, AppError> {
        let template = parse_template(&req.template_type, &req.content)?;
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
        if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
            return Err(AppError::bad_request(
                "metadata fetch url must start with http:// or https://",
            ));
        }
        let content = reqwest::get(&req.url)
            .await
            .map_err(|err| AppError::bad_request(format!("failed to fetch metadata: {err}")))?
            .error_for_status()
            .map_err(|err| {
                AppError::bad_request(format!("metadata endpoint returned error: {err}"))
            })?
            .text()
            .await
            .map_err(|err| {
                AppError::bad_request(format!("failed to read metadata response: {err}"))
            })?;
        self.create_import_preview(MetadataImportRequest {
            template_type: req
                .template_type
                .unwrap_or_else(|| "opengemini".to_string()),
            filename: req.filename.or(Some(req.url)),
            content,
        })
        .await
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

fn parse_template(template_type: &str, content: &str) -> Result<MetadataTemplate, AppError> {
    match template_type.to_ascii_lowercase().as_str() {
        "json" => parse_metadata_json(content),
        "yaml" | "yml" => serde_yaml::from_str(content)
            .map_err(|err| AppError::bad_request(format!("invalid metadata YAML: {err}"))),
        "opengemini" | "opengemini-json" | "influxdb-meta" => parse_opengemini_snapshot(content),
        "csv" => Err(AppError::bad_request(
            "metadata CSV import is reserved but not implemented yet",
        )),
        other => Err(AppError::bad_request(format!(
            "unsupported metadata templateType {other}"
        ))),
    }
}

fn parse_metadata_json(content: &str) -> Result<MetadataTemplate, AppError> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|err| AppError::bad_request(format!("invalid metadata JSON: {err}")))?;
    if value.get("ClusterID").is_some()
        || value.get("MetaNodes").is_some()
        || value.get("DataNodes").is_some()
        || value.get("SqlNodes").is_some()
    {
        return normalize_opengemini_value(value);
    }
    serde_json::from_value(value)
        .map_err(|err| AppError::bad_request(format!("invalid metadata JSON: {err}")))
}

fn parse_opengemini_snapshot(content: &str) -> Result<MetadataTemplate, AppError> {
    let value = serde_json::from_str(content)
        .map_err(|err| AppError::bad_request(format!("invalid openGemini metadata JSON: {err}")))?;
    normalize_opengemini_value(value)
}

fn normalize_opengemini_value(value: serde_json::Value) -> Result<MetadataTemplate, AppError> {
    let cluster_id = value
        .get("ClusterID")
        .and_then(serde_json::Value::as_u64)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "opengemini-local".to_string());
    let mut labels = HashMap::new();
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

    let mut template = MetadataTemplate {
        clusters: vec![ClusterMetadata {
            cluster_id: cluster_id.clone(),
            name: Some(format!("opengemini-{cluster_id}")),
            product: Some("opengemini".to_string()),
            version: None,
            environment: None,
            nodes: Vec::new(),
            labels,
        }],
        ..MetadataTemplate::default()
    };

    append_opengemini_nodes(&mut template, &cluster_id, "meta", value.get("MetaNodes"));
    append_opengemini_nodes(&mut template, &cluster_id, "data", value.get("DataNodes"));
    append_opengemini_nodes(&mut template, &cluster_id, "sql", value.get("SqlNodes"));
    if let Some(cluster) = template.clusters.first_mut() {
        cluster.nodes = template
            .nodes
            .iter()
            .map(|node| node.node_id.clone())
            .collect();
    }
    Ok(template)
}

fn append_opengemini_nodes(
    template: &mut MetadataTemplate,
    cluster_id: &str,
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
        let node_id = format!("{node_kind}-{raw_id}");
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

        template.instances.push(InstanceMetadata {
            instance_id: node_id.clone(),
            cluster_id: Some(cluster_id.to_string()),
            node_id: Some(node_id.clone()),
            product: Some("opengemini".to_string()),
            version: None,
            environment: None,
            region: node
                .get("Az")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            owner: None,
            tags: labels.clone(),
        });
        template.nodes.push(NodeMetadata {
            node_id: node_id.clone(),
            instance_id: Some(node_id),
            hostname: host.as_deref().map(hostname_from_addr),
            host,
            ssh_alias: None,
            role: Some(role),
            zone: node
                .get("Az")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            status: Some(status_text(node.get("Status"))),
            labels,
        });
    }
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

fn status_text(value: Option<&serde_json::Value>) -> String {
    match value.and_then(serde_json::Value::as_i64) {
        Some(0) => "inactive".to_string(),
        Some(1) => "active".to_string(),
        Some(status) => format!("status-{status}"),
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

    use crate::config::{AuthSettings, LogAnalyzerSettings, ServerSettings, StorageSettings};

    #[tokio::test]
    async fn previews_confirms_and_queries_metadata_import() {
        let fixture = Fixture::new("metadata-store");
        let store = MetadataStore::new(fixture.config());
        let preview = store
            .create_import_preview(MetadataImportRequest {
                template_type: "yaml".to_string(),
                filename: Some("metadata.yaml".to_string()),
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
        assert!(fixture.root.join("metadata/instances.json").exists());
    }

    #[tokio::test]
    async fn detects_duplicate_ids_in_import_preview() {
        let fixture = Fixture::new("metadata-duplicates");
        let store = MetadataStore::new(fixture.config());
        let preview = store
            .create_import_preview(MetadataImportRequest {
                template_type: "json".to_string(),
                filename: None,
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
                        "mydb": { "Name": "mydb" }
                    },
                    "TakeOverEnabled": true,
                    "BalancerEnabled": true
                })
                .to_string(),
            })
            .await
            .unwrap();

        assert_eq!(preview.summary.clusters, 1);
        assert_eq!(preview.summary.instances, 3);
        assert_eq!(preview.summary.nodes, 3);
        assert_eq!(preview.summary.errors, 0);

        store.confirm_import(&preview.import_id).await.unwrap();
        let cluster = store.get_cluster("6735497445922383781").await.unwrap();
        assert_eq!(cluster.product.as_deref(), Some("opengemini"));
        assert_eq!(
            cluster.labels.get("databases").map(String::as_str),
            Some("mydb")
        );
        assert_eq!(cluster.nodes, vec!["meta-1", "data-2", "sql-3"]);

        let data_node = store
            .list_cluster_nodes("6735497445922383781")
            .await
            .into_iter()
            .find(|node| node.node_id == "data-2")
            .unwrap();
        assert_eq!(data_node.role.as_deref(), Some("data"));
        assert_eq!(data_node.status.as_deref(), Some("active"));
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
                server: ServerSettings {
                    bind: "127.0.0.1:0".to_string(),
                    public_base_url: "http://127.0.0.1:0".to_string(),
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
            })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
