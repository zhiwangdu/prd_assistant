export type NodeKind = "meta" | "data" | "sql";

export type MetadataSnapshotResponse = {
  instance?: InstanceDto | null;
  cluster: ClusterDto;
  nodes: NodeDto[];
};

export type InstanceDto = {
  instanceId: string;
  remark?: string | null;
  clusterId?: string | null;
  nodeId?: string | null;
  product?: string | null;
  version?: string | null;
  environment?: string | null;
  region?: string | null;
  owner?: string | null;
  tags?: Record<string, string>;
};

export type MetadataInstanceSummary = InstanceDto & {
  nodeCount: number;
  databaseCount: number;
  partitionViewCount: number;
};

export type ClusterDto = {
  clusterId: string;
  name?: string | null;
  product?: string | null;
  labels?: Record<string, string>;
  databases?: DatabaseDto[];
  partitionViews?: PartitionDto[];
  rawSnapshot?: Record<string, unknown> | null;
};

export type NodeDto = {
  nodeId: string;
  rawNodeId?: number | null;
  kind?: NodeKind | null;
  host?: string | null;
  tcpHost?: string | null;
  rpcAddr?: string | null;
  gossipAddr?: string | null;
  role?: string | null;
  zone?: string | null;
  status?: string | null;
  statusCode?: number | null;
  connId?: number | null;
  aliveConnId?: number | null;
  index?: number | null;
};

export type PartitionDto = {
  database: string;
  ptId: number;
  ownerNodeId?: number | null;
  status?: number | null;
  statusText?: string | null;
  version?: number | null;
  replicaGroupId?: number | null;
};

export type DatabaseDto = {
  name: string;
  defaultRetentionPolicy?: string | null;
  replicaN?: number | null;
  markDeleted?: boolean | null;
  retentionPolicies?: RetentionPolicyDto[];
};

export type RetentionPolicyDto = {
  name: string;
  replicaN?: number | null;
  duration?: number | null;
  shardGroupDuration?: number | null;
  indexGroupDuration?: number | null;
  markDeleted?: boolean | null;
  measurements?: MeasurementDto[];
  shardGroups?: ShardGroupDto[];
  indexGroups?: IndexGroupDto[];
};

export type MeasurementDto = {
  name: string;
  logicalName?: string | null;
  versionName?: string | null;
  version?: number | null;
  shardKeyType?: string | null;
  schema?: FieldDto[];
  markDeleted?: boolean | null;
  engineType?: number | null;
};

export type FieldDto = {
  name: string;
  typ?: number | string | null;
  type?: number | string | null;
  Typ?: number | string | null;
  Type?: number | string | null;
  endTime?: number | null;
};

export type ShardGroupDto = {
  id: number;
  startTime?: string | null;
  endTime?: string | null;
  deletedAt?: string | null;
  truncatedAt?: string | null;
  engineType?: number | null;
  version?: number | null;
  shards?: ShardDto[];
};

export type ShardDto = {
  id: number;
  owners?: number[];
  min?: string | null;
  max?: string | null;
  tier?: number | null;
  indexId?: number | null;
  downsampleId?: number | null;
  downsampleLevel?: number | null;
  readOnly?: boolean | null;
  markDelete?: boolean | null;
};

export type IndexGroupDto = {
  id: number;
  startTime?: string | null;
  endTime?: string | null;
  deletedAt?: string | null;
  engineType?: number | null;
  indexes?: IndexDto[];
};

export type IndexDto = {
  id: number;
  tier?: number | null;
  owners?: number[];
  markDelete?: boolean | null;
};

export type DiagnosticSeverity = "error" | "warning" | "info";
export type Diagnostic = {
  code: string;
  severity: DiagnosticSeverity;
  title: string;
  detail: string;
  entityId: string;
};

export type TopologyFilters = {
  database: string;
  dataNodeId: string;
  startTime: string;
  endTime: string;
  onlyAbnormal: boolean;
  showShards: boolean;
  showIndexes: boolean;
};

export type TopologySummaryRow = {
  id: string;
  database: string;
  ptId: number;
  ownerNodeId?: number | null;
  ownerHost?: string | null;
  statusText?: string | null;
  abnormal: boolean;
  diagnosticCount: number;
  shardGroups: number;
  shards: number;
  indexGroups: number;
  indexes: number;
  startTime?: string | null;
  endTime?: string | null;
  shardDetails: Array<{
    rp: string;
    shardGroupId: number;
    startTime?: string | null;
    endTime?: string | null;
    shardId: number;
    indexId?: number | null;
    owners: number[];
    tier?: number | null;
    readOnly?: boolean | null;
    markDelete?: boolean | null;
    indexTier?: number | null;
    indexMarkDelete?: boolean | null;
  }>;
};

export type MetadataViewModel = MetadataSnapshotResponse & {
  counts: {
    databases: number;
    retentionPolicies: number;
    partitions: number;
    shardGroups: number;
    shards: number;
    measurements: number;
    indexes: number;
  };
  diagnostics: Diagnostic[];
};
