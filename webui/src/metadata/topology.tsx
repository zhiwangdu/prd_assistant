import type { Edge, Node } from "@xyflow/react";
import type { MetadataViewModel } from "./types";

const columns = { sql: 0, database: 1, rp: 2, pt: 2, measurement: 3, shardGroup: 3, shard: 4, index: 5, data: 5 };

export function buildTopology(vm: MetadataViewModel): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const row = new Map<number, number>();
  const partitionNodeIds = new Set((vm.cluster.partitionViews ?? []).map((pt) => `pt:${pt.database}:${pt.ptId}`));
  const addNode = (id: string, type: keyof typeof columns, label: string, subtitle?: string) => {
    const column = columns[type];
    const y = (row.get(column) ?? 0) * 92;
    row.set(column, (row.get(column) ?? 0) + 1);
    nodes.push({
      id,
      position: { x: column * 250, y },
      data: { label: <div><strong>{label}</strong>{subtitle && <small>{subtitle}</small>}</div> },
      className: `topology-node topology-${type}`
    });
  };
  const addEdge = (source: string, target: string, label?: string) => edges.push({
    id: `${source}->${target}`,
    source,
    target,
    label,
    animated: false,
    style: { stroke: "#94a3b8" }
  });

  const sqlNodes = vm.nodes.filter((node) => node.kind === "sql");
  const dataNodes = vm.nodes.filter((node) => node.kind === "data");
  sqlNodes.forEach((node) => addNode(node.nodeId, "sql", `SQL ${node.rawNodeId}`, node.tcpHost ?? undefined));
  dataNodes.forEach((node) => addNode(node.nodeId, "data", `Data ${node.rawNodeId}`, node.host ?? undefined));

  for (const database of vm.cluster.databases ?? []) {
    const dbId = `db:${database.name}`;
    addNode(dbId, "database", database.name, "Database");
    sqlNodes.forEach((node) => addEdge(node.nodeId, dbId, "query"));
    for (const pt of (vm.cluster.partitionViews ?? []).filter((item) => item.database === database.name)) {
      const ptId = `pt:${database.name}:${pt.ptId}`;
      addNode(ptId, "pt", `PT ${pt.ptId}`, pt.statusText ?? undefined);
      addEdge(dbId, ptId);
      const owner = dataNodes.find((node) => node.rawNodeId === pt.ownerNodeId);
      if (owner) addEdge(ptId, owner.nodeId, "owner");
    }
    for (const rp of database.retentionPolicies ?? []) {
      const rpId = `rp:${database.name}:${rp.name}`;
      addNode(rpId, "rp", rp.name, "Retention Policy");
      addEdge(dbId, rpId);
      for (const measurement of rp.measurements ?? []) {
        const mstId = `measurement:${database.name}:${rp.name}:${measurement.name}`;
        addNode(mstId, "measurement", measurement.logicalName ?? measurement.name, measurement.name);
        addEdge(rpId, mstId);
      }
      for (const group of rp.shardGroups ?? []) {
        const groupId = `sg:${database.name}:${rp.name}:${group.id}`;
        addNode(groupId, "shardGroup", `ShardGroup ${group.id}`);
        addEdge(rpId, groupId);
        for (const shard of group.shards ?? []) {
          const shardId = `shard:${database.name}:${rp.name}:${shard.id}`;
          addNode(shardId, "shard", `Shard ${shard.id}`, `PT ${(shard.owners ?? []).join(", ")}`);
          addEdge(groupId, shardId);
          for (const ownerPt of shard.owners ?? []) {
            const ownerId = `pt:${database.name}:${ownerPt}`;
            if (partitionNodeIds.has(ownerId)) addEdge(shardId, ownerId, "PT owner");
          }
          if (shard.indexId != null) addEdge(shardId, `index:${database.name}:${rp.name}:${shard.indexId}`);
        }
      }
      for (const group of rp.indexGroups ?? []) {
        for (const index of group.indexes ?? []) {
          addNode(`index:${database.name}:${rp.name}:${index.id}`, "index", `Index ${index.id}`, `group ${group.id}`);
        }
      }
    }
  }
  return { nodes, edges };
}
