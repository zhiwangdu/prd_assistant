import type { Edge, Node } from "@xyflow/react";
import type {
  DatabaseDto,
  Diagnostic,
  IndexGroupDto,
  MetadataViewModel,
  ShardGroupDto,
  TopologyEntity,
  TopologyFilters,
  TopologyFocus,
  TopologySummaryRow
} from "./types";

export const FOCUSED_GRAPH_LIMIT = 600;

type TopologyGraph = {
  nodes: Node[];
  edges: Edge[];
  entities: Map<string, TopologyEntity>;
  totalElements: number;
  limited: boolean;
};

export type TopologyIndex = {
  rows: TopologySummaryRow[];
  dataNodes: Array<{ value: string; label: string }>;
  databases: Array<{ value: string; label: string }>;
  diagnosticsByEntity: Map<string, Diagnostic[]>;
};

export function buildTopologyIndex(vm: MetadataViewModel): TopologyIndex {
  const diagnosticsByEntity = groupDiagnostics(vm.diagnostics);
  const dataNodes = vm.nodes.filter((node) => node.kind === "data");
  const dataNodeByRawId = new Map(dataNodes.map((node) => [node.rawNodeId, node]));
  const databases = vm.cluster.databases ?? [];
  const databaseByName = new Map(databases.map((database) => [database.name, database]));
  const partitions = completePartitions(vm, databases);

  const rows = partitions.map((pt) => {
    const database = databaseByName.get(pt.database);
    const groups = database ? collectOwnedGroups(database, pt.ptId, {}) : emptyOwnedGroups();
    const entityIds = [
      `pt:${pt.database}:${pt.ptId}`,
      ...groups.shardGroups.map(({ rp, group }) => `sg:${pt.database}:${rp}:${group.id}`),
      ...groups.indexGroups.map(({ rp, group }) => `ig:${pt.database}:${rp}:${group.id}`),
      ...groups.shards.map(({ rp, shard }) => `shard:${pt.database}:${rp}:${shard.id}`),
      ...groups.indexes.map(({ rp, index }) => `index:${pt.database}:${rp}:${index.id}`)
    ];
    const diagnosticCount = entityIds.reduce((total, entityId) => total + (diagnosticsByEntity.get(entityId)?.length ?? 0), 0);
    const ranges = [
      ...groups.shardGroups.map(({ group }) => [group.startTime, group.endTime] as const),
      ...groups.indexGroups.map(({ group }) => [group.startTime, group.endTime] as const)
    ];
    return {
      id: rowId(pt.database, pt.ptId, pt.ownerNodeId),
      database: pt.database,
      ptId: pt.ptId,
      ownerNodeId: pt.ownerNodeId,
      ownerHost: dataNodeByRawId.get(pt.ownerNodeId)?.host,
      statusText: pt.statusText,
      abnormal: diagnosticCount > 0 || pt.ownerNodeId == null || !dataNodeByRawId.has(pt.ownerNodeId),
      diagnosticCount,
      shardGroups: groups.shardGroups.length,
      shards: groups.shards.length,
      indexGroups: groups.indexGroups.length,
      indexes: groups.indexes.length,
      startTime: minTime(ranges.map(([start]) => start)),
      endTime: maxTime(ranges.map(([, end]) => end))
    };
  });

  rows.sort((left, right) => Number(right.abnormal) - Number(left.abnormal) || right.diagnosticCount - left.diagnosticCount || String(left.ownerNodeId ?? "").localeCompare(String(right.ownerNodeId ?? "")) || left.database.localeCompare(right.database) || left.ptId - right.ptId);

  return {
    rows,
    dataNodes: dataNodes.map((node) => ({ value: String(node.rawNodeId), label: `DataNode ${node.rawNodeId} · ${node.host ?? "-"}` })),
    databases: databases.map((database) => ({ value: database.name, label: database.name })),
    diagnosticsByEntity
  };
}

export function filterTopologyRows(rows: TopologySummaryRow[], filters: TopologyFilters) {
  return rows.filter((row) =>
    (!filters.database || row.database === filters.database) &&
    (!filters.dataNodeId || String(row.ownerNodeId) === filters.dataNodeId) &&
    (!filters.onlyAbnormal || row.abnormal) &&
    rowInTimeRange(row, filters)
  );
}

export function buildFocusedTopology(vm: MetadataViewModel, filters: TopologyFilters, focus: TopologyFocus): TopologyGraph {
  const entities = new Map<string, TopologyEntity>();
  const nodes: Node[] = [];
  const edges: Edge[] = [];
  const diagnosticsByEntity = groupDiagnostics(vm.diagnostics);
  const database = (vm.cluster.databases ?? []).find((item) => item.name === focus.database);
  const ptId = Number(focus.ptId);
  const ownerNodeId = Number(focus.dataNodeId);
  const dataNode = vm.nodes.find((node) => node.kind === "data" && node.rawNodeId === ownerNodeId);
  const pt = (vm.cluster.partitionViews ?? []).find((item) => item.database === focus.database && item.ptId === ptId);
  if (!database || !Number.isFinite(ptId) || !Number.isFinite(ownerNodeId)) {
    return { nodes, edges, entities, totalElements: 0, limited: false };
  }

  const groups = collectOwnedGroups(database, ptId, filters);
  const leafCount = (filters.showShards ? groups.shards.length : 0) + (filters.showIndexes ? groups.indexes.length : 0);
  const totalElements = 3 + groups.shardGroups.length + groups.indexGroups.length + leafCount;
  if (totalElements > FOCUSED_GRAPH_LIMIT) {
    return { nodes, edges, entities, totalElements, limited: true };
  }

  const containerId = dataNode?.nodeId ?? `missing-data-${focus.dataNodeId}`;
  addEntity(entities, {
    id: containerId,
    kind: "dataNode",
    title: dataNode ? `DataNode ${dataNode.rawNodeId}` : `Missing DataNode ${focus.dataNodeId}`,
    subtitle: dataNode?.host ?? "PT owner does not exist",
    abnormal: !dataNode || hasDiagnostics(diagnosticsByEntity, containerId),
    fields: dataNode ?? { rawNodeId: ownerNodeId, status: "missing" },
    relations: [{ type: "owns", target: `pt:${focus.database}:${ptId}` }]
  });
  nodes.push({
    id: containerId,
    position: { x: 0, y: 0 },
    data: { label: entityLabel(entities.get(containerId)!) },
    className: className("dataNode", entities.get(containerId)!.abnormal),
    style: { width: 1120, height: Math.max(360, 170 + Math.max(groups.shardGroups.length, groups.indexGroups.length, 1) * 90) },
    selectable: true,
    draggable: true
  });

  const databaseId = `database:${containerId}:${database.name}`;
  addChild(nodes, entities, {
    id: databaseId,
    kind: "database",
    title: database.name,
    subtitle: `default RP ${database.defaultRetentionPolicy ?? "-"}`,
    abnormal: hasDiagnostics(diagnosticsByEntity, `db:${database.name}`),
    fields: database as unknown as Record<string, unknown>,
    relations: [{ type: "contains", target: `pt:${database.name}:${ptId}` }]
  }, containerId, 20, 90, 170, 70);

  const canonicalPtId = `pt:${database.name}:${ptId}`;
  const localPtId = `dbpt:${containerId}:${database.name}:${ptId}`;
  addChild(nodes, entities, {
    id: localPtId,
    kind: "dbpt",
    title: `PT ${ptId}`,
    subtitle: `${pt?.statusText ?? "unknown"} · ver ${pt?.version ?? "-"}`,
    abnormal: hasDiagnostics(diagnosticsByEntity, canonicalPtId) || !dataNode,
    fields: { ...(pt ?? { database: database.name, ptId, ownerNodeId }), database: database.name },
    relations: [{ type: "owner", target: containerId }]
  }, containerId, 220, 90, 145, 70);
  addEdge(edges, databaseId, localPtId);

  groups.shardGroups.forEach(({ rp, group }, groupIndex) => {
    const sgId = `sg:${containerId}:${database.name}:${ptId}:${rp}:${group.id}`;
    const canonicalSgId = `sg:${database.name}:${rp}:${group.id}`;
    addChild(nodes, entities, {
      id: sgId,
      kind: "shardGroup",
      title: `ShardGroup ${group.id}`,
      subtitle: timeLabel(group.startTime, group.endTime),
      abnormal: hasDiagnostics(diagnosticsByEntity, canonicalSgId) || rangeMismatch(group.startTime, group.endTime, groups.indexGroups.map((item) => item.group)),
      fields: { ...group, database: database.name, retentionPolicy: rp },
      relations: (group.shards ?? []).map((shard) => ({ type: "contains", target: `shard:${database.name}:${rp}:${shard.id}` }))
    }, containerId, 405, 40 + groupIndex * 86, 170, 66);
    addEdge(edges, localPtId, sgId);
  });

  if (filters.showShards) {
    groups.shards.forEach(({ rp, groupId, shard }, shardIndex) => {
      const shardId = `shard:${containerId}:${database.name}:${ptId}:${rp}:${shard.id}`;
      const canonicalShardId = `shard:${database.name}:${rp}:${shard.id}`;
      const sgId = `sg:${containerId}:${database.name}:${ptId}:${rp}:${groupId}`;
      addChild(nodes, entities, {
        id: shardId,
        kind: "shard",
        title: `Shard ${shard.id}`,
        subtitle: `Index ${shard.indexId ?? "-"}`,
        abnormal: hasDiagnostics(diagnosticsByEntity, canonicalShardId),
        fields: { ...shard, database: database.name, retentionPolicy: rp, shardGroupId: groupId },
        relations: [{ type: "owner PT", target: canonicalPtId }]
      }, containerId, 610, 40 + shardIndex * 76, 140, 66);
      addEdge(edges, sgId, shardId);
    });
  }

  groups.indexGroups.forEach(({ rp, group }, groupIndex) => {
    const igId = `ig:${containerId}:${database.name}:${ptId}:${rp}:${group.id}`;
    const canonicalIgId = `ig:${database.name}:${rp}:${group.id}`;
    addChild(nodes, entities, {
      id: igId,
      kind: "indexGroup",
      title: `IndexGroup ${group.id}`,
      subtitle: timeLabel(group.startTime, group.endTime),
      abnormal: hasDiagnostics(diagnosticsByEntity, canonicalIgId) || rangeMismatch(group.startTime, group.endTime, groups.shardGroups.map((item) => item.group)),
      fields: { ...group, database: database.name, retentionPolicy: rp },
      relations: (group.indexes ?? []).map((index) => ({ type: "contains", target: `index:${database.name}:${rp}:${index.id}` }))
    }, containerId, 790, 40 + groupIndex * 86, 170, 66);
    addEdge(edges, localPtId, igId);
  });

  if (filters.showIndexes) {
    groups.indexes.forEach(({ rp, groupId, index }, indexIndex) => {
      const indexId = `index:${containerId}:${database.name}:${ptId}:${rp}:${index.id}`;
      const canonicalIndexId = `index:${database.name}:${rp}:${index.id}`;
      const igId = `ig:${containerId}:${database.name}:${ptId}:${rp}:${groupId}`;
      addChild(nodes, entities, {
        id: indexId,
        kind: "index",
        title: `Index ${index.id}`,
        subtitle: `tier ${index.tier ?? "-"}`,
        abnormal: hasDiagnostics(diagnosticsByEntity, canonicalIndexId),
        fields: { ...index, database: database.name, retentionPolicy: rp, indexGroupId: groupId },
        relations: [{ type: "owner PT", target: canonicalPtId }]
      }, containerId, 990, 40 + indexIndex * 76, 115, 66);
      addEdge(edges, igId, indexId);
      if (filters.showShards) {
        for (const { rp: shardRp, shard } of groups.shards) {
          if (shardRp === rp && shard.indexId === index.id) {
            addEdge(edges, `shard:${containerId}:${database.name}:${ptId}:${rp}:${shard.id}`, indexId, "IndexID");
          }
        }
      }
    });
  }

  return { nodes, edges, entities, totalElements, limited: false };
}

function completePartitions(vm: MetadataViewModel, databases: DatabaseDto[]) {
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
  return [...storedPartitions, ...orphanPartitions];
}

function collectOwnedGroups(database: DatabaseDto, ptId: number, filters: Partial<TopologyFilters>) {
  const shardGroups = (database.retentionPolicies ?? []).flatMap((rp) =>
    (rp.shardGroups ?? [])
      .filter((group) => inTimeRange(group.startTime, group.endTime, filters))
      .filter((group) => (group.shards ?? []).some((shard) => (shard.owners ?? []).includes(ptId)))
      .map((group) => ({ rp: rp.name, group }))
  );
  const indexGroups = (database.retentionPolicies ?? []).flatMap((rp) =>
    (rp.indexGroups ?? [])
      .filter((group) => inTimeRange(group.startTime, group.endTime, filters))
      .filter((group) => (group.indexes ?? []).some((index) => (index.owners ?? []).includes(ptId)))
      .map((group) => ({ rp: rp.name, group }))
  );
  return {
    shardGroups,
    indexGroups,
    shards: shardGroups.flatMap(({ rp, group }) => (group.shards ?? []).filter((shard) => (shard.owners ?? []).includes(ptId)).map((shard) => ({ rp, groupId: group.id, shard }))),
    indexes: indexGroups.flatMap(({ rp, group }) => (group.indexes ?? []).filter((index) => (index.owners ?? []).includes(ptId)).map((index) => ({ rp, groupId: group.id, index })))
  };
}

function emptyOwnedGroups(): ReturnType<typeof collectOwnedGroups> {
  return { shardGroups: [], indexGroups: [], shards: [], indexes: [] };
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

function groupDiagnostics(diagnostics: Diagnostic[]) {
  const grouped = new Map<string, Diagnostic[]>();
  for (const diagnostic of diagnostics) {
    const current = grouped.get(diagnostic.entityId) ?? [];
    current.push(diagnostic);
    grouped.set(diagnostic.entityId, current);
  }
  return grouped;
}

function hasDiagnostics(diagnosticsByEntity: Map<string, Diagnostic[]>, entityId: string) {
  return Boolean(diagnosticsByEntity.get(entityId)?.length);
}

function rowId(database: string, ptId: number, ownerNodeId?: number | null) {
  return `${database}:${ptId}:${ownerNodeId ?? "missing"}`;
}

function rowInTimeRange(row: TopologySummaryRow, filters: TopologyFilters) {
  if (!filters.startTime && !filters.endTime) return true;
  return inTimeRange(row.startTime, row.endTime, filters);
}

function inTimeRange(start: string | null | undefined, end: string | null | undefined, filters: Partial<TopologyFilters>) {
  const filterStart = filters.startTime ? Date.parse(filters.startTime) : Number.NEGATIVE_INFINITY;
  const filterEnd = filters.endTime ? Date.parse(filters.endTime) : Number.POSITIVE_INFINITY;
  const itemStart = start ? Date.parse(start) : Number.NEGATIVE_INFINITY;
  const itemEnd = end ? Date.parse(end) : Number.POSITIVE_INFINITY;
  return itemStart <= filterEnd && itemEnd >= filterStart;
}

function minTime(values: Array<string | null | undefined>) {
  const valid = values.filter((value): value is string => Boolean(value));
  if (!valid.length) return null;
  return valid.reduce((left, right) => Date.parse(left) <= Date.parse(right) ? left : right);
}

function maxTime(values: Array<string | null | undefined>) {
  const valid = values.filter((value): value is string => Boolean(value));
  if (!valid.length) return null;
  return valid.reduce((left, right) => Date.parse(left) >= Date.parse(right) ? left : right);
}

function timeLabel(start?: string | null, end?: string | null) {
  return `${start?.slice(0, 10) ?? "-"} -> ${end?.slice(0, 10) ?? "-"}`;
}

function rangeMismatch(start: string | null | undefined, end: string | null | undefined, groups: IndexGroupDto[] | ShardGroupDto[]) {
  return groups.length > 0 && !groups.some((group) => group.startTime === start && group.endTime === end);
}
