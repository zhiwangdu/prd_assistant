use serde::Serialize;

#[derive(Debug, Clone)]
pub struct DomainAdapterRegistry {
    adapters: Vec<DomainAdapterSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainAdapterSummary {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub products: Vec<String>,
    pub evidence_kinds: Vec<String>,
    pub planned_tools: Vec<String>,
    pub notes: Vec<String>,
}

impl DomainAdapterRegistry {
    pub fn builtin() -> Self {
        Self {
            adapters: vec![
                DomainAdapterSummary {
                    id: "opengemini_influxdb".to_string(),
                    display_name: "openGemini / InfluxDB".to_string(),
                    status: "active".to_string(),
                    products: vec![
                        "opengemini".to_string(),
                        "influxdb".to_string(),
                        "influxql".to_string(),
                    ],
                    evidence_kinds: vec![
                        "metadata_context".to_string(),
                        "log_patterns".to_string(),
                        "query_tool_results".to_string(),
                        "case_context".to_string(),
                    ],
                    planned_tools: vec![
                        "influxql_analyzer".to_string(),
                        "flux_query_analyzer".to_string(),
                        "pprof_analyzer".to_string(),
                    ],
                    notes: vec![
                        "Current default adapter; owns openGemini metadata, PT/shard/index views, and Influx query diagnostics.".to_string(),
                    ],
                },
                DomainAdapterSummary {
                    id: "cassandra".to_string(),
                    display_name: "Cassandra".to_string(),
                    status: "skeleton".to_string(),
                    products: vec!["cassandra".to_string()],
                    evidence_kinds: vec![
                        "system_log".to_string(),
                        "schema_and_ring".to_string(),
                        "nodetool_output".to_string(),
                        "ci_pipeline_logs".to_string(),
                    ],
                    planned_tools: vec![
                        "nodetool_status".to_string(),
                        "nodetool_tpstats".to_string(),
                        "nodetool_compactionstats".to_string(),
                    ],
                    notes: vec![
                        "Future adapter for repair, compaction, tombstone, read/write latency, and ring ownership diagnostics.".to_string(),
                    ],
                },
                DomainAdapterSummary {
                    id: "rocksdb".to_string(),
                    display_name: "RocksDB".to_string(),
                    status: "skeleton".to_string(),
                    products: vec!["rocksdb".to_string()],
                    evidence_kinds: vec![
                        "rocksdb_log".to_string(),
                        "manifest_options".to_string(),
                        "sst_metadata".to_string(),
                        "perf_context".to_string(),
                    ],
                    planned_tools: vec![
                        "ldb".to_string(),
                        "sst_dump".to_string(),
                        "rocksdb_log_parser".to_string(),
                    ],
                    notes: vec![
                        "Future adapter for compaction, write stalls, flush, MANIFEST/OPTIONS, and SST-level analysis.".to_string(),
                    ],
                },
            ],
        }
    }

    pub fn summaries(&self) -> Vec<DomainAdapterSummary> {
        self.adapters.clone()
    }
}
