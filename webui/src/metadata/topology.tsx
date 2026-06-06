import type { Edge, Node } from "@xyflow/react";
import type { MetadataViewModel } from "./types";

const columns = {
  data: 0,
  dbpt: 1,
  shardGroup: 2,
  shard: 3,
  indexGroup: 4,
  index: 5
};

export function buildTopology(vm: MetadataViewModel): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const nodeIds = new Set<string>();
  const edgeIds = new Set<string>();
  const rows = new Map<number, number>();

  const addNode = (id: string, type: keyof typeof columns, label: string, subtitle?: string) => {
    if (nodeIds.has(id)) return;
    nodeIds.add(id);
    const column = columns[type];
    const y = (rows.get(column) ?? 0) * 100;
    rows.set(column, (rows.get(column) ?? 0) + 1);
    nodes.push({
      id,
      position: { x: column * 250, y },
      data: {
        label: <div><strong>{label}</strong>{subtitle && <small>{subtitle}</small>}</div>
      },
      className: `topology-node topology-${type}`
    });
  };

  const addEdge = (source: string, target: string, label?: string) => {
    if (!nodeIds.has(source) || !nodeIds.has(target)) return;
    const id = `${source}->${target}`;
    if (edgeIds.has(id)) return;
    edgeIds.add(id);
    edges.push({
      id,
      source,
      target,
      label,
      style: { stroke: "#94a3b8" }
    });
  };

  const dataNodes = vm.nodes.filter((node) => node.kind === "data");
  dataNodes.forEach((node) => {
    addNode(node.nodeId, "data", `DataNode ${node.rawNodeId}`, node.host ?? undefined);
  });

  for (const database of vm.cluster.databases ?? []) {
    const partitions = (vm.cluster.partitionViews ?? []).filter((pt) => pt.database === database.name);

    for (const pt of partitions) {
      const dbPtId = `dbpt:${database.name}:${pt.ptId}`;
      addNode(dbPtId, "dbpt", `${database.name} / PT ${pt.ptId}`, `${pt.statusText ?? "unknown"} · ver ${pt.version ?? "-"}`);
      const ownerNode = dataNodes.find((node) => node.rawNodeId === pt.ownerNodeId);
      if (ownerNode) addEdge(ownerNode.nodeId, dbPtId, "owns");
    }

    for (const rp of database.retentionPolicies ?? []) {
      const indexGroupByIndexId = new Map<number, number>();

      for (const indexGroup of rp.indexGroups ?? []) {
        const indexGroupId = `ig:${database.name}:${rp.name}:${indexGroup.id}`;
        addNode(indexGroupId, "indexGroup", `IndexGroup ${indexGroup.id}`, `${database.name} / ${rp.name}`);
        for (const index of indexGroup.indexes ?? []) {
          indexGroupByIndexId.set(index.id, indexGroup.id);
          const indexId = `index:${database.name}:${rp.name}:${index.id}`;
          addNode(indexId, "index", `Index ${index.id}`, `PT ${(index.owners ?? []).join(", ") || "-"}`);
          addEdge(indexGroupId, indexId);
        }
      }

      for (const shardGroup of rp.shardGroups ?? []) {
        const shardGroupId = `sg:${database.name}:${rp.name}:${shardGroup.id}`;
        addNode(shardGroupId, "shardGroup", `ShardGroup ${shardGroup.id}`, `${database.name} / ${rp.name}`);

        const ownerPtIds = new Set(
          (shardGroup.shards ?? []).flatMap((shard) => shard.owners ?? [])
        );
        for (const ownerPtId of ownerPtIds) {
          addEdge(`dbpt:${database.name}:${ownerPtId}`, shardGroupId, "contains");
        }

        for (const shard of shardGroup.shards ?? []) {
          const shardId = `shard:${database.name}:${rp.name}:${shard.id}`;
          addNode(shardId, "shard", `Shard ${shard.id}`, `PT ${(shard.owners ?? []).join(", ") || "-"}`);
          addEdge(shardGroupId, shardId);

          if (shard.indexId != null) {
            const indexGroupId = indexGroupByIndexId.get(shard.indexId);
            if (indexGroupId != null) {
              addEdge(shardId, `ig:${database.name}:${rp.name}:${indexGroupId}`, `Index ${shard.indexId}`);
            }
          }
        }
      }
    }
  }

  return { nodes, edges };
}
