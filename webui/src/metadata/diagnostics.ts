import type { Diagnostic, MetadataSnapshotResponse } from "./types";

export function diagnose(snapshot: MetadataSnapshotResponse): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  const dataNodeIds = new Set(snapshot.nodes.filter((node) => node.kind === "data").map((node) => node.rawNodeId));
  const dataNodes = snapshot.nodes.filter((node) => node.kind === "data");
  const partitionsByDatabase = new Map<string, Set<number>>();

  for (const node of snapshot.nodes) {
    const offline = (node.kind === "data" || node.kind === "sql") && node.statusCode != null && node.statusCode !== 1;
    if (offline) {
      diagnostics.push(issue("NODE_OFFLINE", "error", `${node.kind} node ${node.rawNodeId} offline`, `status=${node.statusCode ?? "unknown"}`, node.nodeId));
    }
    if (node.connId != null && node.aliveConnId != null && node.connId !== node.aliveConnId) {
      diagnostics.push(issue("NODE_CONNECTION_STALE", "warning", `Node ${node.rawNodeId} connection stale`, `ConnID ${node.connId} != AliveConnID ${node.aliveConnId}`, node.nodeId));
    }
  }

  for (const pt of snapshot.cluster.partitionViews ?? []) {
    const ptIds = partitionsByDatabase.get(pt.database) ?? new Set<number>();
    ptIds.add(pt.ptId);
    partitionsByDatabase.set(pt.database, ptIds);
    if (pt.ownerNodeId == null || !dataNodeIds.has(pt.ownerNodeId)) {
      diagnostics.push(issue("PT_OWNER_MISSING_NODE", "error", `${pt.database}/PT ${pt.ptId} owner missing`, `DataNode ${pt.ownerNodeId ?? "unknown"} does not exist`, `pt:${pt.database}:${pt.ptId}`));
    }
  }

  for (const node of dataNodes) {
    if (!(snapshot.cluster.partitionViews ?? []).some((pt) => pt.ownerNodeId === node.rawNodeId)) {
      diagnostics.push(issue("DATANODE_WITHOUT_PT", "warning", `DataNode ${node.rawNodeId} has no PT`, node.host ?? node.nodeId, node.nodeId));
    }
  }

  for (const database of snapshot.cluster.databases ?? []) {
    const policies = database.retentionPolicies ?? [];
    if (!database.defaultRetentionPolicy || !policies.some((rp) => rp.name === database.defaultRetentionPolicy)) {
      diagnostics.push(issue("DATABASE_DEFAULT_RP_MISSING", "error", `${database.name} has no valid default RP`, `Configured value: ${database.defaultRetentionPolicy ?? "empty"}`, `db:${database.name}`));
    }
    for (const rp of policies) {
      if (!(rp.shardGroups?.length)) {
        diagnostics.push(issue("RP_WITHOUT_SHARD_GROUP", "warning", `${database.name}/${rp.name} has no ShardGroup`, "No time range is currently allocated", `rp:${database.name}:${rp.name}`));
      }
      const indexes = new Set((rp.indexGroups ?? []).flatMap((group) => group.indexes ?? []).map((index) => index.id));
      const referencedIndexes = new Set<number>();
      const ptIds = partitionsByDatabase.get(database.name) ?? new Set<number>();
      const shardOwnerPtIds = new Set<number>();
      const indexOwnerPtIds = new Set<number>();
      for (const group of rp.shardGroups ?? []) {
        for (const shard of group.shards ?? []) {
          for (const ownerPtId of shard.owners ?? []) {
            shardOwnerPtIds.add(ownerPtId);
            if (!ptIds.has(ownerPtId)) {
              diagnostics.push(issue("SHARD_OWNER_MISSING_PT", "error", `Shard ${shard.id} owner PT missing`, `${ownerPtId} is a PT ID, but is absent from PtView.${database.name}`, `shard:${database.name}:${rp.name}:${shard.id}`));
            }
          }
          if (shard.indexId != null) {
            referencedIndexes.add(shard.indexId);
            if (!indexes.has(shard.indexId)) {
              diagnostics.push(issue("SHARD_INDEX_MISSING", "error", `Shard ${shard.id} index missing`, `Index ${shard.indexId} does not exist`, `shard:${database.name}:${rp.name}:${shard.id}`));
            }
          }
        }
      }
      for (const group of rp.indexGroups ?? []) {
        for (const index of group.indexes ?? []) {
          for (const ownerPtId of index.owners ?? []) {
            indexOwnerPtIds.add(ownerPtId);
            if (!ptIds.has(ownerPtId)) {
              diagnostics.push(issue("INDEX_OWNER_MISSING_PT", "error", `Index ${index.id} owner PT missing`, `${ownerPtId} is a PT ID, but is absent from PtView.${database.name}`, `index:${database.name}:${rp.name}:${index.id}`));
            }
          }
        }
      }
      for (const ptId of ptIds) {
        if (!shardOwnerPtIds.has(ptId)) {
          diagnostics.push(issue("PT_WITHOUT_SHARD", "warning", `${database.name}/PT ${ptId} has no Shard`, rp.name, `pt:${database.name}:${ptId}`));
        }
        if (!indexOwnerPtIds.has(ptId)) {
          diagnostics.push(issue("PT_WITHOUT_INDEX", "warning", `${database.name}/PT ${ptId} has no Index`, rp.name, `pt:${database.name}:${ptId}`));
        }
      }
      const shardRanges = new Set((rp.shardGroups ?? []).map((group) => `${group.startTime ?? ""}|${group.endTime ?? ""}`));
      const indexRanges = new Set((rp.indexGroups ?? []).map((group) => `${group.startTime ?? ""}|${group.endTime ?? ""}`));
      for (const range of new Set([...shardRanges, ...indexRanges])) {
        if (!shardRanges.has(range) || !indexRanges.has(range)) {
          diagnostics.push(issue("GROUP_TIME_RANGE_MISMATCH", "warning", `${database.name}/${rp.name} group time range mismatch`, range.replace("|", " -> "), `rp:${database.name}:${rp.name}`));
        }
      }
      for (const index of indexes) {
        if (!referencedIndexes.has(index)) {
          diagnostics.push(issue("ORPHAN_INDEX", "info", `Index ${index} is not referenced`, `${database.name}/${rp.name}`, `index:${database.name}:${rp.name}:${index}`));
        }
      }
      for (const measurement of rp.measurements ?? []) {
        if (!(measurement.schema?.length)) {
          diagnostics.push(issue("MEASUREMENT_WITHOUT_SCHEMA", "warning", `${measurement.logicalName ?? measurement.name} has no Schema`, measurement.name, `measurement:${database.name}:${rp.name}:${measurement.name}`));
        }
        if (measurement.versionName && measurement.versionName !== measurement.name) {
          diagnostics.push(issue("MST_VERSION_TARGET_MISSING", "warning", `${measurement.logicalName ?? measurement.name} version target mismatch`, `${measurement.versionName} != ${measurement.name}`, `measurement:${database.name}:${rp.name}:${measurement.name}`));
        }
      }
    }
  }
  return diagnostics;
}

function issue(code: string, severity: Diagnostic["severity"], title: string, detail: string, entityId: string): Diagnostic {
  return { code, severity, title, detail, entityId };
}
