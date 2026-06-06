import { diagnose } from "./diagnostics";
import type { MetadataSnapshotResponse, MetadataViewModel } from "./types";

export function buildViewModel(snapshot: MetadataSnapshotResponse): MetadataViewModel {
  const databases = snapshot.cluster.databases ?? [];
  const retentionPolicies = databases.flatMap((database) => database.retentionPolicies ?? []);
  const shardGroups = retentionPolicies.flatMap((policy) => policy.shardGroups ?? []);
  const measurements = retentionPolicies.flatMap((policy) => policy.measurements ?? []);
  const indexes = retentionPolicies.flatMap((policy) => policy.indexGroups ?? []).flatMap((group) => group.indexes ?? []);
  return {
    ...snapshot,
    counts: {
      databases: databases.length,
      retentionPolicies: retentionPolicies.length,
      partitions: snapshot.cluster.partitionViews?.length ?? 0,
      shardGroups: shardGroups.length,
      shards: shardGroups.reduce((sum, group) => sum + (group.shards?.length ?? 0), 0),
      measurements: measurements.length,
      indexes: indexes.length
    },
    diagnostics: diagnose(snapshot)
  };
}
