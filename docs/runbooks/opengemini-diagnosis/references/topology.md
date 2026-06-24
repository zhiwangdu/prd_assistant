# openGemini Topology Notes

`Shard.Owners` and `Index.Owners` are PT IDs, not node IDs. Resolve them through the same database's `PtView` before naming a DataNode.

Use this order for topology checks:

1. Identify Instance, Cluster, and selected Node from Metadata context.
2. Check node status and connection fields for MetaNode, DataNode, and SqlNode separately.
3. Map Database -> PT -> owner DataNode before interpreting Shard and Index owners.
4. Compare ShardGroup and IndexGroup time ranges when query or compaction symptoms mention missing data, stale indexes, or uneven load.
5. Treat missing PT, missing DataNode, orphan Index, and owner mismatch as higher-priority topology anomalies than generic timeout text.

Do not use this reference as final evidence. Cite task artifacts that show the actual affected node, PT, shard, index, or log line.
