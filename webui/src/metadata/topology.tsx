import type { DatabaseDto, Diagnostic, MetadataViewModel, TopologyFilters, TopologySummaryRow } from "./types";

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
    const shardDetails = groups.shards.map(({ rp, groupId, groupStartTime, groupEndTime, shard }) => {
      const index = groups.indexes.find((item) => item.rp === rp && item.index.id === shard.indexId)?.index;
      return {
        rp,
        shardGroupId: groupId,
        startTime: groupStartTime,
        endTime: groupEndTime,
        shardId: shard.id,
        indexId: shard.indexId,
        owners: shard.owners ?? [],
        tier: shard.tier,
        readOnly: shard.readOnly,
        markDelete: shard.markDelete,
        indexTier: index?.tier,
        indexMarkDelete: index?.markDelete
      };
    });
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
      endTime: maxTime(ranges.map(([, end]) => end)),
      shardDetails
    };
  });

  rows.sort((left, right) => Number(right.abnormal) - Number(left.abnormal) || right.diagnosticCount - left.diagnosticCount || left.database.localeCompare(right.database) || String(left.ownerNodeId ?? "").localeCompare(String(right.ownerNodeId ?? "")) || left.ptId - right.ptId);

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
    shards: shardGroups.flatMap(({ rp, group }) => (group.shards ?? []).filter((shard) => (shard.owners ?? []).includes(ptId)).map((shard) => ({ rp, groupId: group.id, groupStartTime: group.startTime, groupEndTime: group.endTime, shard }))),
    indexes: indexGroups.flatMap(({ rp, group }) => (group.indexes ?? []).filter((index) => (index.owners ?? []).includes(ptId)).map((index) => ({ rp, groupId: group.id, index })))
  };
}

function emptyOwnedGroups(): ReturnType<typeof collectOwnedGroups> {
  return { shardGroups: [], indexGroups: [], shards: [], indexes: [] };
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
