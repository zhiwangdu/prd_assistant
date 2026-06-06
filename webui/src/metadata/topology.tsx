import type { Edge, Node } from "@xyflow/react";
import type {
  Diagnostic,
  MetadataViewModel,
  TopologyEntity,
  TopologyFilters
} from "./types";

type TopologyGraph = {
  nodes: Node[];
  edges: Edge[];
  entities: Map<string, TopologyEntity>;
};

export function buildTopology(vm: MetadataViewModel, filters: TopologyFilters): TopologyGraph {
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const entities = new Map<string, TopologyEntity>();
  const abnormalIds = diagnosticEntityIds(vm.diagnostics);
  const dataNodes = vm.nodes.filter((node) => node.kind === "data");
  const databases = vm.cluster.databases ?? [];
  const storedPartitions = vm.cluster.partitionViews ?? [];
  const partitionKeys = new Set(storedPartitions.map((pt) => `${pt.database}:${pt.ptId}`));
  const orphanPartitions = databases.flatMap((database) => {
    const referencedPtIds = new Set(
      (database.retentionPolicies ?? []).flatMap((rp) => [
        ...(rp.shardGroups ?? []).flatMap((group) => (group.shards ?? []).flatMap((shard) => shard.owners ?? [])),
        ...(rp.indexGroups ?? []).flatMap((group) => (group.indexes ?? []).flatMap((index) => index.owners ?? []))
      ])
    );
    return [...referencedPtIds]
      .filter((ptId) => !partitionKeys.has(`${database.name}:${ptId}`))
      .map((ptId) => ({
        database: database.name,
        ptId,
        ownerNodeId: null,
        status: null,
        statusText: "missing",
        version: null,
        replicaGroupId: null
      }));
  });
  const partitions = [...storedPartitions, ...orphanPartitions];
  const visibleDataNodes = dataNodes.filter((node) => !filters.dataNodeId || String(node.rawNodeId) === filters.dataNodeId);
  const knownDataNodeIds = new Set(dataNodes.map((node) => node.rawNodeId));
  const missingOwnerIds = new Set(
    partitions
      .filter((pt) => pt.ownerNodeId == null || !knownDataNodeIds.has(pt.ownerNodeId))
      .map((pt) => pt.ownerNodeId ?? -1)
  );
  const containerSources = [
    ...visibleDataNodes.map((node) => ({ node, missingOwnerId: undefined as number | undefined })),
    ...(!filters.dataNodeId
      ? [...missingOwnerIds].map((ownerId) => ({
          node: {
            nodeId: `missing-data-${ownerId}`,
            rawNodeId: ownerId === -1 ? null : ownerId,
            kind: "data" as const,
            host: null,
            status: "missing"
          },
          missingOwnerId: ownerId
        }))
      : [])
  ];
  let containerColumn = 0;

  for (const { node: dataNode, missingOwnerId } of containerSources) {
    const ownedPartitions = partitions.filter((pt) =>
      (missingOwnerId === undefined
        ? pt.ownerNodeId === dataNode.rawNodeId
        : (pt.ownerNodeId ?? -1) === missingOwnerId) &&
      (!filters.database || pt.database === filters.database)
    );
    const nodeAbnormal = missingOwnerId !== undefined || abnormalIds.has(dataNode.nodeId);
    const visiblePartitions = filters.onlyAbnormal
      ? ownedPartitions.filter((pt) => laneIsAbnormal(vm.diagnostics, pt.database))
      : ownedPartitions;
    if (filters.onlyAbnormal && !nodeAbnormal && !visiblePartitions.length) continue;

    const containerId = dataNode.nodeId;
    const laneCount = Math.max(visiblePartitions.length, 1);
    const width = 1120;
    const height = 110 + laneCount * 250;
    addEntity(entities, {
      id: containerId,
      kind: "dataNode",
      title: missingOwnerId === undefined ? `DataNode ${dataNode.rawNodeId}` : `Missing DataNode ${missingOwnerId === -1 ? "unknown" : missingOwnerId}`,
      subtitle: dataNode.host ?? (missingOwnerId === undefined ? undefined : "PT owner does not exist"),
      abnormal: nodeAbnormal || !ownedPartitions.length,
      fields: { ...dataNode, missingOwner: missingOwnerId !== undefined },
      relations: ownedPartitions.map((pt) => ({ type: "owns", target: `pt:${pt.database}:${pt.ptId}` }))
    });
    nodes.push({
      id: containerId,
      position: { x: containerColumn * (width + 80), y: 0 },
      data: { label: entityLabel(entities.get(containerId)!) },
      className: className("dataNode", entities.get(containerId)!.abnormal),
      style: { width, height },
      selectable: true,
      draggable: true
    });
    containerColumn += 1;

    if (!visiblePartitions.length) continue;

    const databaseOffsets = new Map<string, number>();
    visiblePartitions.forEach((pt, laneIndex) => {
      const database = databases.find((item) => item.name === pt.database);
      if (!database) return;
      const laneY = 70 + laneIndex * 250;
      const databaseId = `database:${containerId}:${database.name}`;
      if (!databaseOffsets.has(database.name)) {
        databaseOffsets.set(database.name, laneY);
        addChild(nodes, entities, {
          id: databaseId,
          kind: "database",
          title: database.name,
          subtitle: `default RP ${database.defaultRetentionPolicy ?? "-"}`,
          abnormal: abnormalIds.has(`db:${database.name}`),
          fields: database,
          relations: visiblePartitions
            .filter((item) => item.database === database.name)
            .map((item) => ({ type: "contains", target: `pt:${item.database}:${item.ptId}` }))
        }, containerId, 20, laneY, 170, 70);
      }

      const canonicalPtId = `pt:${pt.database}:${pt.ptId}`;
      const ptId = `dbpt:${containerId}:${pt.database}:${pt.ptId}`;
      addChild(nodes, entities, {
        id: ptId,
        kind: "dbpt",
        title: `PT ${pt.ptId}`,
        subtitle: `${pt.statusText ?? "unknown"} · ver ${pt.version ?? "-"}`,
        abnormal: abnormalIds.has(canonicalPtId),
        fields: { ...pt, database: database.name },
        relations: [{ type: "owner", target: containerId }]
      }, containerId, 220, laneY, 145, 70);
      addEdge(edges, databaseId, ptId);

      const policies = database.retentionPolicies ?? [];
      const shardGroups = policies.flatMap((rp) =>
        (rp.shardGroups ?? [])
          .filter((group) => inTimeRange(group.startTime, group.endTime, filters))
          .map((group) => ({ rp: rp.name, group }))
      );
      const indexGroups = policies.flatMap((rp) =>
        (rp.indexGroups ?? [])
          .filter((group) => inTimeRange(group.startTime, group.endTime, filters))
          .map((group) => ({ rp: rp.name, group }))
      );
      const ownedShardGroups = shardGroups.filter(({ group }) =>
        (group.shards ?? []).some((shard) => (shard.owners ?? []).includes(pt.ptId))
      );
      const ownedIndexGroups = indexGroups.filter(({ group }) =>
        (group.indexes ?? []).some((index) => (index.owners ?? []).includes(pt.ptId))
      );

      ownedShardGroups.forEach(({ rp, group }, groupIndex) => {
        const sgId = `sg:${containerId}:${pt.database}:${pt.ptId}:${rp}:${group.id}`;
        const canonicalSgId = `sg:${pt.database}:${rp}:${group.id}`;
        addChild(nodes, entities, {
          id: sgId,
          kind: "shardGroup",
          title: `ShardGroup ${group.id}`,
          subtitle: timeLabel(group.startTime, group.endTime),
          abnormal: abnormalIds.has(canonicalSgId) || rangeMismatch(group.startTime, group.endTime, ownedIndexGroups.map((item) => item.group)),
          fields: { ...group, database: pt.database, retentionPolicy: rp },
          relations: (group.shards ?? []).map((shard) => ({ type: "contains", target: `shard:${pt.database}:${rp}:${shard.id}` }))
        }, containerId, 405, laneY + groupIndex * 76, 170, 66);
        addEdge(edges, ptId, sgId);

        if (filters.showShards) {
          (group.shards ?? []).filter((shard) => (shard.owners ?? []).includes(pt.ptId)).forEach((shard, shardIndex) => {
            const shardId = `shard:${containerId}:${pt.database}:${pt.ptId}:${rp}:${shard.id}`;
            const canonicalShardId = `shard:${pt.database}:${rp}:${shard.id}`;
            addChild(nodes, entities, {
              id: shardId,
              kind: "shard",
              title: `Shard ${shard.id}`,
              subtitle: `Index ${shard.indexId ?? "-"}`,
              abnormal: abnormalIds.has(canonicalShardId),
              fields: { ...shard, database: pt.database, retentionPolicy: rp, shardGroupId: group.id },
              relations: [
                { type: "owner PT", target: canonicalPtId },
                ...(shard.indexId == null ? [] : [{ type: "index", target: `index:${pt.database}:${rp}:${shard.indexId}` }])
              ]
            }, containerId, 610, laneY + (groupIndex + shardIndex) * 76, 140, 66);
            addEdge(edges, sgId, shardId);
          });
        }
      });

      ownedIndexGroups.forEach(({ rp, group }, groupIndex) => {
        const igId = `ig:${containerId}:${pt.database}:${pt.ptId}:${rp}:${group.id}`;
        const canonicalIgId = `ig:${pt.database}:${rp}:${group.id}`;
        addChild(nodes, entities, {
          id: igId,
          kind: "indexGroup",
          title: `IndexGroup ${group.id}`,
          subtitle: timeLabel(group.startTime, group.endTime),
          abnormal: abnormalIds.has(canonicalIgId) || rangeMismatch(group.startTime, group.endTime, ownedShardGroups.map((item) => item.group)),
          fields: { ...group, database: pt.database, retentionPolicy: rp },
          relations: (group.indexes ?? []).map((index) => ({ type: "contains", target: `index:${pt.database}:${rp}:${index.id}` }))
        }, containerId, 790, laneY + groupIndex * 76, 170, 66);
        addEdge(edges, ptId, igId);

        if (filters.showIndexes) {
          (group.indexes ?? []).filter((index) => (index.owners ?? []).includes(pt.ptId)).forEach((index, indexIndex) => {
            const indexId = `index:${containerId}:${pt.database}:${pt.ptId}:${rp}:${index.id}`;
            const canonicalIndexId = `index:${pt.database}:${rp}:${index.id}`;
            addChild(nodes, entities, {
              id: indexId,
              kind: "index",
              title: `Index ${index.id}`,
              subtitle: `tier ${index.tier ?? "-"}`,
              abnormal: abnormalIds.has(canonicalIndexId),
              fields: { ...index, database: pt.database, retentionPolicy: rp, indexGroupId: group.id },
              relations: [{ type: "owner PT", target: canonicalPtId }]
            }, containerId, 990, laneY + (groupIndex + indexIndex) * 76, 115, 66);
            addEdge(edges, igId, indexId);
            if (filters.showShards) {
              for (const { rp: shardRp, group: shardGroup } of ownedShardGroups) {
                if (shardRp !== rp) continue;
                for (const shard of shardGroup.shards ?? []) {
                  if ((shard.owners ?? []).includes(pt.ptId) && shard.indexId === index.id) {
                    addEdge(edges, `shard:${containerId}:${pt.database}:${pt.ptId}:${rp}:${shard.id}`, indexId, "IndexID");
                  }
                }
              }
            }
          });
        }
      });
    });
  }

  return { nodes, edges, entities };
}

function addChild(
  nodes: Node[],
  entities: Map<string, TopologyEntity>,
  entity: TopologyEntity,
  parentId: string,
  x: number,
  y: number,
  width: number,
  height: number
) {
  if (entities.has(entity.id)) return;
  addEntity(entities, entity);
  nodes.push({
    id: entity.id,
    parentId,
    extent: "parent",
    position: { x, y },
    data: { label: entityLabel(entity) },
    className: className(entity.kind, entity.abnormal),
    style: { width, height }
  });
}

function addEdge(edges: Edge[], source: string, target: string, label?: string) {
  const id = `${source}->${target}`;
  if (edges.some((edge) => edge.id === id)) return;
  edges.push({ id, source, target, label, style: { stroke: "#94a3b8" } });
}

function addEntity(entities: Map<string, TopologyEntity>, entity: TopologyEntity) {
  entities.set(entity.id, entity);
}

function entityLabel(entity: TopologyEntity) {
  return <div><strong>{entity.title}</strong>{entity.subtitle && <small>{entity.subtitle}</small>}</div>;
}

function className(kind: TopologyEntity["kind"], abnormal: boolean) {
  return `topology-node topology-${kind}${abnormal ? " topology-abnormal" : ""}`;
}

function diagnosticEntityIds(diagnostics: Diagnostic[]) {
  return new Set(diagnostics.map((diagnostic) => diagnostic.entityId));
}

function laneIsAbnormal(diagnostics: Diagnostic[], database: string) {
  return diagnostics.some((item) =>
    item.entityId.startsWith(`pt:${database}:`) ||
    item.entityId.startsWith(`rp:${database}:`) ||
    item.entityId.startsWith(`shard:${database}:`) ||
    item.entityId.startsWith(`index:${database}:`)
  );
}

function inTimeRange(start: string | null | undefined, end: string | null | undefined, filters: TopologyFilters) {
  const filterStart = filters.startTime ? Date.parse(filters.startTime) : Number.NEGATIVE_INFINITY;
  const filterEnd = filters.endTime ? Date.parse(filters.endTime) : Number.POSITIVE_INFINITY;
  const itemStart = start ? Date.parse(start) : Number.NEGATIVE_INFINITY;
  const itemEnd = end ? Date.parse(end) : Number.POSITIVE_INFINITY;
  return itemStart <= filterEnd && itemEnd >= filterStart;
}

function timeLabel(start?: string | null, end?: string | null) {
  return `${start?.slice(0, 10) ?? "-"} → ${end?.slice(0, 10) ?? "-"}`;
}

function rangeMismatch(start: string | null | undefined, end: string | null | undefined, groups: Array<{ startTime?: string | null; endTime?: string | null }>) {
  return groups.length > 0 && !groups.some((group) => group.startTime === start && group.endTime === end);
}
