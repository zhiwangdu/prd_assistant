import { Background, Controls, MiniMap, ReactFlow } from "@xyflow/react";
import {
  AlertTriangle,
  Boxes,
  Braces,
  CheckCircle2,
  CircleX,
  Database,
  GitBranch,
  Network,
  RefreshCw,
  Search,
  Server,
  TableProperties
} from "lucide-react";
import { isValidElement, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input, Tabs, TabsContent, TabsList, TabsTrigger } from "../components/ui";
import { formatDuration, valueOrDash } from "../lib/utils";
import { confirmImport, fetchImportedInstances, fetchSnapshot, fetchStoredInstance, previewImport, type ImportPreview } from "./api";
import { buildTopology } from "./topology";
import type { DatabaseDto, Diagnostic, MetadataInstanceSummary, MetadataViewModel, NodeDto, RetentionPolicyDto, TopologyEntity, TopologyFilters } from "./types";
import { buildViewModel } from "./view-model";

type Props = { apiKey: string };

export function MetadataDashboard({ apiKey }: Props) {
  const [url, setUrl] = useState("http://127.0.0.1:8091/getdata");
  const [instanceId, setInstanceId] = useState("");
  const [instanceRemark, setInstanceRemark] = useState("");
  const [instances, setInstances] = useState<MetadataInstanceSummary[]>([]);
  const [listStatus, setListStatus] = useState("等待加载已导入列表");
  const [vm, setVm] = useState<MetadataViewModel | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [importMessage, setImportMessage] = useState("");

  const refreshInstances = useCallback(async () => {
    if (!apiKey.trim()) {
      setInstances([]);
      setListStatus("请先填写 API Key");
      return;
    }
    try {
      const result = await fetchImportedInstances(apiKey);
      setInstances(result.instances);
      setListStatus(`${result.instances.length} 个已导入 Instance`);
    } catch (reason) {
      setListStatus(reason instanceof Error ? reason.message : String(reason));
    }
  }, [apiKey]);

  useEffect(() => {
    void refreshInstances();
  }, [refreshInstances]);

  async function load(mode: "live" | "stored") {
    if (!apiKey.trim()) {
      setError("请先填写 API Key");
      return;
    }
    if (!instanceId.trim()) {
      setError("请先填写 InstanceID");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const snapshot = mode === "live" ? await fetchSnapshot(url, instanceId.trim(), instanceRemark, apiKey) : await fetchStoredInstance(instanceId.trim(), apiKey);
      setVm(buildViewModel(snapshot));
      setInstanceId(snapshot.instance?.instanceId ?? instanceId.trim());
      setInstanceRemark(snapshot.instance?.remark ?? "");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function persistSnapshot() {
    if (!apiKey.trim()) {
      setError("请先填写 API Key");
      return;
    }
    if (!instanceId.trim()) {
      setError("请先填写 InstanceID");
      return;
    }
    setLoading(true);
    setError("");
    try {
      if (!importPreview) {
        const preview = await previewImport(url, instanceId.trim(), instanceRemark, apiKey);
        setImportPreview(preview);
        setImportMessage(`预览完成：${preview.summary.nodes} nodes / ${preview.summary.databases} databases。再次点击确认写入。`);
      } else {
        await confirmImport(importPreview.importId, apiKey);
        setImportMessage(`已写入 Server Metadata Store：${importPreview.importId}`);
        setImportPreview(null);
        await refreshInstances();
        await load("stored");
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardContent className="grid gap-3 pt-5 xl:grid-cols-[440px_minmax(0,1fr)_auto]">
          <div className="grid min-w-0 gap-2 sm:grid-cols-[minmax(0,1fr)_150px]">
            <Input value={instanceId} onChange={(event) => { setInstanceId(event.target.value); setImportPreview(null); }} aria-label="Instance ID" placeholder="InstanceID（手工输入，唯一键）" />
            <Input value={instanceRemark} onChange={(event) => { setInstanceRemark(event.target.value); setImportPreview(null); }} aria-label="Instance remark" maxLength={120} placeholder="备注名" />
          </div>
          <div className="flex min-w-0 gap-2">
            <Input value={url} onChange={(event) => { setUrl(event.target.value); setImportPreview(null); }} aria-label="Metadata URL" />
            <Button onClick={() => void load("live")} disabled={loading}>
              <RefreshCw className={`mr-2 h-4 w-4 ${loading ? "animate-spin" : ""}`} />
              实时加载
            </Button>
            <Button variant="outline" onClick={() => void persistSnapshot()} disabled={loading}>
              {importPreview ? "确认写入" : "预览导入"}
            </Button>
          </div>
          <Button variant="outline" onClick={() => void load("stored")} disabled={loading}>读取已存 Instance</Button>
        </CardContent>
      </Card>

      {error && <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</div>}
      {importMessage && <div className="rounded-lg border border-teal-200 bg-teal-50 p-3 text-sm text-teal-800">{importMessage}</div>}
      <div className="grid gap-5 xl:grid-cols-[360px_minmax(0,1fr)]">
        <ImportedInstancesPanel
          instances={instances}
          loading={loading}
          selectedInstanceId={instanceId}
          status={listStatus}
          onRefresh={() => void refreshInstances()}
          onSelect={(item) => {
            setInstanceId(item.instanceId);
            setInstanceRemark(item.remark ?? "");
            setImportPreview(null);
            void loadStoredInstance(item.instanceId);
          }}
        />
        {!vm ? (
          <EmptyState>输入 InstanceID 后从 openGemini `/getdata` 实时加载，或从左侧读取已确认的 Metadata 快照。</EmptyState>
        ) : (
          <Tabs defaultValue="overview">
            <TabsList>
              <Tab value="overview" icon={Boxes} label="Overview" />
              <Tab value="nodes" icon={Server} label="Nodes" />
              <Tab value="partitions" icon={GitBranch} label="Partitions" />
              <Tab value="topology" icon={Network} label="Topology" />
              <Tab value="databases" icon={Database} label="Databases" />
              <Tab value="schemas" icon={TableProperties} label="Schemas" />
              <Tab value="diagnostics" icon={AlertTriangle} label={`Diagnostics ${vm.diagnostics.length}`} />
              <Tab value="raw" icon={Braces} label="Raw JSON" />
            </TabsList>
            <TabsContent value="overview"><Overview vm={vm} /></TabsContent>
            <TabsContent value="nodes"><NodesView nodes={vm.nodes} /></TabsContent>
            <TabsContent value="partitions"><PartitionsView vm={vm} /></TabsContent>
            <TabsContent value="topology"><TopologyView vm={vm} /></TabsContent>
            <TabsContent value="databases"><DatabasesView databases={vm.cluster.databases ?? []} /></TabsContent>
            <TabsContent value="schemas"><SchemasView databases={vm.cluster.databases ?? []} /></TabsContent>
            <TabsContent value="diagnostics"><DiagnosticsView diagnostics={vm.diagnostics} /></TabsContent>
            <TabsContent value="raw"><RawJsonView value={vm.cluster.rawSnapshot ?? vm.cluster} /></TabsContent>
          </Tabs>
        )}
      </div>
    </div>
  );

  async function loadStoredInstance(nextInstanceId: string) {
    if (!apiKey.trim()) {
      setError("请先填写 API Key");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const snapshot = await fetchStoredInstance(nextInstanceId, apiKey);
      setVm(buildViewModel(snapshot));
      setInstanceId(snapshot.instance?.instanceId ?? nextInstanceId);
      setInstanceRemark(snapshot.instance?.remark ?? "");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }
}

function Tab({ value, icon: Icon, label }: { value: string; icon: typeof Boxes; label: string }) {
  return <TabsTrigger value={value}><Icon className="mr-2 inline h-4 w-4" />{label}</TabsTrigger>;
}

function ImportedInstancesPanel({
  instances,
  loading,
  selectedInstanceId,
  status,
  onRefresh,
  onSelect
}: {
  instances: MetadataInstanceSummary[];
  loading: boolean;
  selectedInstanceId: string;
  status: string;
  onRefresh: () => void;
  onSelect: (item: MetadataInstanceSummary) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <div>
            <CardTitle>Imported Instances</CardTitle>
            <CardDescription>{status}</CardDescription>
          </div>
          <Button className="h-8 px-3" variant="outline" onClick={onRefresh} disabled={loading}>
            <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-2">
        {instances.length ? instances.map((item) => (
          <button
            className={`w-full rounded-lg border p-3 text-left transition ${selectedInstanceId === item.instanceId ? "border-primary bg-slate-50" : "border-border hover:bg-slate-50"}`}
            key={item.instanceId}
            onClick={() => onSelect(item)}
            type="button"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex min-w-0 items-center gap-2">
                  <p className="min-w-0 flex-1 truncate text-sm font-medium" title={item.instanceId}>{item.instanceId}</p>
                  {item.remark && <span className="max-w-[150px] truncate rounded-md border border-border bg-white px-1.5 py-0.5 text-[11px] text-muted-foreground" title={item.remark}>{item.remark}</span>}
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{item.product ?? "unknown"} {item.version ?? ""} · {item.environment ?? "env -"}</p>
              </div>
              <Badge variant="secondary">{item.nodeCount} nodes</Badge>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">{item.databaseCount} databases · {item.partitionViewCount} PT views</p>
          </button>
        )) : <EmptyState>暂无已导入 Instance。</EmptyState>}
      </CardContent>
    </Card>
  );
}

function Overview({ vm }: { vm: MetadataViewModel }) {
  const labels = vm.cluster.labels ?? {};
  const metrics = [
    ["Instance ID", vm.instance?.instanceId ?? vm.cluster.clusterId],
    ["Remark", vm.instance?.remark],
    ["Source Cluster ID", labels.sourceClusterId],
    ["Term", labels.term],
    ["Index", labels.index],
    ["Nodes", vm.nodes.length],
    ["Databases", vm.counts.databases],
    ["Partitions", vm.counts.partitions],
    ["Shards", vm.counts.shards],
    ["Measurements", vm.counts.measurements]
  ];
  const rawWatermarks = Object.entries(vm.cluster.rawSnapshot ?? {}).filter(([key]) => key.startsWith("Max"));
  const watermarks = rawWatermarks.length ? rawWatermarks : Object.entries(labels).filter(([key]) => key.startsWith("max"));
  const switches: Array<[string, string | boolean | undefined]> = [
    ["TakeOver", labels.takeOverEnabled],
    ["Balancer", labels.balancerEnabled],
    ["Expand Shards", rawBoolean(vm, "ExpandShardsEnable")]
  ];
  return (
    <div className="space-y-5">
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {metrics.map(([label, value]) => <Metric key={label} label={String(label)} value={value} />)}
      </div>
      <div className="grid gap-5 xl:grid-cols-2">
        <Card>
          <CardHeader><CardTitle>Node composition</CardTitle><CardDescription>按 openGemini 节点职责统计</CardDescription></CardHeader>
          <CardContent className="grid grid-cols-3 gap-3">
            {(["meta", "data", "sql"] as const).map((kind) => <Metric key={kind} label={`${kind.toUpperCase()} nodes`} value={vm.nodes.filter((node) => node.kind === kind).length} />)}
          </CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>Feature switches</CardTitle></CardHeader>
          <CardContent className="flex flex-wrap gap-3">
            {switches.map(([name, enabled]) => <Badge key={name} variant={String(enabled) === "true" ? "success" : "secondary"}>{name}: {String(enabled)}</Badge>)}
          </CardContent>
        </Card>
      </div>
      <Card>
        <CardHeader><CardTitle>MaxID watermarks</CardTitle><CardDescription>元数据 ID 分配高水位</CardDescription></CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {watermarks.map(([name, value]) => <Metric key={name} label={name} value={value} compact />)}
        </CardContent>
      </Card>
    </div>
  );
}

function NodesView({ nodes }: { nodes: NodeDto[] }) {
  return (
    <div className="space-y-5">
      {(["meta", "data", "sql"] as const).map((kind) => {
        const grouped = nodes.filter((node) => node.kind === kind);
        return (
          <Card key={kind}>
            <CardHeader><CardTitle>{kind.toUpperCase()} Nodes</CardTitle><CardDescription>{grouped.length} node(s)</CardDescription></CardHeader>
            <CardContent>{grouped.length ? <Table headers={["ID", "Host", "TCPHost", "RPCAddr", "GossipAddr", "Status", "ConnID", "AliveConnID", "Index", "AZ", "Role"]} rows={grouped.map((node) => [
              node.rawNodeId, node.host, node.tcpHost, node.rpcAddr, node.gossipAddr,
              <StatusBadge key="status" node={node} />, node.connId, node.aliveConnId, node.index, node.zone, node.role
            ])} /> : <EmptyState>暂无 {kind} node</EmptyState>}</CardContent>
          </Card>
        );
      })}
    </div>
  );
}

function PartitionsView({ vm }: { vm: MetadataViewModel }) {
  const dataNodes = new Map(vm.nodes.filter((node) => node.kind === "data").map((node) => [node.rawNodeId, node]));
  return (
    <Card>
      <CardHeader><CardTitle>DB Partition allocation</CardTitle><CardDescription>Owner 指向 DataNode；Shard Owners 则指向这里的 PT ID</CardDescription></CardHeader>
      <CardContent>
        <Table headers={["Database", "PtId", "Owner NodeID", "Owner Host", "Status", "Ver", "RGID"]} rows={(vm.cluster.partitionViews ?? []).map((pt) => [
          pt.database, pt.ptId, pt.ownerNodeId, dataNodes.get(pt.ownerNodeId)?.host,
          <Badge key="status" variant={pt.status === 0 ? "success" : "warning"}>{pt.statusText ?? pt.status}</Badge>,
          pt.version, pt.replicaGroupId
        ])} />
      </CardContent>
    </Card>
  );
}

function TopologyView({ vm }: { vm: MetadataViewModel }) {
  const [filters, setFilters] = useState<TopologyFilters>({
    database: "",
    dataNodeId: "",
    startTime: "",
    endTime: "",
    onlyAbnormal: false,
    showShards: true,
    showIndexes: true
  });
  const [selected, setSelected] = useState<TopologyEntity | null>(null);
  const graph = useMemo(() => buildTopology(vm, filters), [vm, filters]);
  const databases = vm.cluster.databases ?? [];
  const dataNodes = vm.nodes.filter((node) => node.kind === "data");

  function patchFilter<K extends keyof TopologyFilters>(key: K, value: TopologyFilters[K]) {
    setFilters((current) => ({ ...current, [key]: value }));
    setSelected(null);
  }

  return (
    <Card>
      <CardHeader><CardTitle>DataNode-centric DBPT topology</CardTitle><CardDescription>DataNode 容器内按 Database / PT 展示 Shard 和 Index 分配；Owners 数字均为 PT ID</CardDescription></CardHeader>
      <CardContent>
        <div className="mb-4 grid gap-3 rounded-lg border border-border bg-slate-50 p-3 md:grid-cols-2 xl:grid-cols-4">
          <FilterSelect label="Database" value={filters.database} onChange={(value) => patchFilter("database", value)} options={databases.map((database) => ({ value: database.name, label: database.name }))} />
          <FilterSelect label="DataNode" value={filters.dataNodeId} onChange={(value) => patchFilter("dataNodeId", value)} options={dataNodes.map((node) => ({ value: String(node.rawNodeId), label: `DataNode ${node.rawNodeId} · ${node.host ?? "-"}` }))} />
          <FilterInput label="Start time" type="datetime-local" value={filters.startTime} onChange={(value) => patchFilter("startTime", value)} />
          <FilterInput label="End time" type="datetime-local" value={filters.endTime} onChange={(value) => patchFilter("endTime", value)} />
          <FilterCheck label="Only abnormal" checked={filters.onlyAbnormal} onChange={(value) => patchFilter("onlyAbnormal", value)} />
          <FilterCheck label="Show shards" checked={filters.showShards} onChange={(value) => patchFilter("showShards", value)} />
          <FilterCheck label="Show indexes" checked={filters.showIndexes} onChange={(value) => patchFilter("showIndexes", value)} />
          <div className="flex items-end"><Button className="w-full" variant="outline" onClick={() => setFilters({ database: "", dataNodeId: "", startTime: "", endTime: "", onlyAbnormal: false, showShards: true, showIndexes: true })}>Reset filters</Button></div>
        </div>
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
          <div className="h-[760px] overflow-hidden rounded-lg border border-border bg-slate-50">
            <ReactFlow nodes={graph.nodes} edges={graph.edges} fitView minZoom={0.15} maxZoom={1.8} nodesDraggable onNodeClick={(_, node) => setSelected(graph.entities.get(node.id) ?? null)}>
              <Background color="#cbd5e1" gap={20} />
              <MiniMap pannable zoomable />
              <Controls />
            </ReactFlow>
          </div>
          <TopologyDetails entity={selected} />
        </div>
      </CardContent>
    </Card>
  );
}

function FilterSelect({ label, value, options, onChange }: { label: string; value: string; options: Array<{ value: string; label: string }>; onChange: (value: string) => void }) {
  return <label className="grid gap-1 text-xs font-medium text-muted-foreground">{label}<select className="h-10 rounded-md border border-border bg-white px-3 text-sm text-foreground" value={value} onChange={(event) => onChange(event.target.value)}><option value="">All</option>{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>;
}

function FilterInput({ label, value, type, onChange }: { label: string; value: string; type: string; onChange: (value: string) => void }) {
  return <label className="grid gap-1 text-xs font-medium text-muted-foreground">{label}<Input type={type} value={value} onChange={(event) => onChange(event.target.value)} /></label>;
}

function FilterCheck({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="flex h-10 items-center gap-2 self-end rounded-md border border-border bg-white px-3 text-sm"><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />{label}</label>;
}

function TopologyDetails({ entity }: { entity: TopologyEntity | null }) {
  if (!entity) {
    return <aside className="rounded-lg border border-dashed border-border bg-white p-5 text-sm text-muted-foreground">点击 DataNode、Database、DBPT、ShardGroup、Shard、IndexGroup 或 Index 查看完整字段和关联对象。</aside>;
  }
  return (
    <aside className="max-h-[760px] overflow-auto rounded-lg border border-border bg-white">
      <div className="sticky top-0 border-b border-border bg-white p-4">
        <div className="flex items-center justify-between gap-2"><strong>{entity.title}</strong>{entity.abnormal && <Badge variant="destructive">Abnormal</Badge>}</div>
        <p className="mt-1 text-xs text-muted-foreground">{entity.kind} · {entity.subtitle ?? entity.id}</p>
      </div>
      <div className="space-y-5 p-4">
        <section><h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Fields</h4><KeyValueList value={entity.fields} /></section>
        <section><h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Relations</h4>{entity.relations.length ? <div className="space-y-2">{entity.relations.map((relation, index) => <div key={`${relation.type}:${relation.target}:${index}`} className="rounded-md border border-border p-2 text-xs"><Badge variant="outline">{relation.type}</Badge><code className="mt-1 block break-all">{relation.target}</code></div>)}</div> : <p className="text-sm text-muted-foreground">No relations</p>}</section>
      </div>
    </aside>
  );
}

function KeyValueList({ value }: { value: Record<string, unknown> }) {
  return <div className="space-y-2">{Object.entries(value).map(([key, item]) => <div key={key} className="grid gap-1 rounded-md bg-slate-50 p-2"><span className="text-xs text-muted-foreground">{key}</span><code className="whitespace-pre-wrap break-all text-xs">{typeof item === "object" ? JSON.stringify(item, null, 2) : valueOrDash(item)}</code></div>)}</div>;
}

function DatabasesView({ databases }: { databases: DatabaseDto[] }) {
  return <div className="space-y-5">{databases.map((database) => (
    <Card key={database.name}>
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <div><CardTitle>{database.name}</CardTitle><CardDescription>Default RP: {valueOrDash(database.defaultRetentionPolicy)} · ReplicaN: {valueOrDash(database.replicaN)}</CardDescription></div>
          {database.markDeleted && <Badge variant="destructive">Marked deleted</Badge>}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">{(database.retentionPolicies ?? []).map((rp) => <RetentionPolicy key={rp.name} database={database.name} rp={rp} />)}</CardContent>
    </Card>
  ))}</div>;
}

function RetentionPolicy({ database, rp }: { database: string; rp: RetentionPolicyDto }) {
  const shards = (rp.shardGroups ?? []).flatMap((group) => group.shards ?? []);
  return (
    <div className="rounded-lg border border-border p-4">
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <strong>{rp.name}</strong>
        <Badge variant="outline">ReplicaN {valueOrDash(rp.replicaN)}</Badge>
        <Badge variant="secondary">Duration {formatDuration(rp.duration)}</Badge>
        <Badge variant="secondary">ShardGroup {formatDuration(rp.shardGroupDuration)}</Badge>
        <Badge variant="secondary">IndexGroup {formatDuration(rp.indexGroupDuration)}</Badge>
      </div>
      <div className="space-y-4">
        <div>
          <h4 className="mb-2 text-sm font-semibold">ShardGroups and Shards</h4>
          <Table headers={["Group", "Range", "Shard", "Owners (PT IDs)", "Tier", "IndexID", "ReadOnly", "MarkDelete"]} rows={(rp.shardGroups ?? []).flatMap((group) => (group.shards ?? []).map((shard) => [
            group.id, `${group.startTime ?? "-"} → ${group.endTime ?? "-"}`, shard.id,
            (shard.owners ?? []).join(", "), shard.tier, shard.indexId, String(shard.readOnly ?? false), String(shard.markDelete ?? false)
          ]))} empty={`No Shards in ${database}/${rp.name}`} />
        </div>
        <div>
          <h4 className="mb-2 text-sm font-semibold">Indexes</h4>
          <Table headers={["IndexGroup", "Range", "Index", "Owners (PT IDs)", "Tier", "MarkDelete"]} rows={(rp.indexGroups ?? []).flatMap((group) => (group.indexes ?? []).map((index) => [
            group.id, `${group.startTime ?? "-"} → ${group.endTime ?? "-"}`, index.id, (index.owners ?? []).join(", "), index.tier, String(index.markDelete ?? false)
          ]))} empty={shards.length ? "Shards exist but no IndexGroups" : "No IndexGroups"} />
        </div>
      </div>
    </div>
  );
}

function SchemasView({ databases }: { databases: DatabaseDto[] }) {
  const measurements = databases.flatMap((database) => (database.retentionPolicies ?? []).flatMap((rp) => (rp.measurements ?? []).map((measurement) => ({ database: database.name, rp: rp.name, measurement }))));
  return (
    <div className="space-y-5">{measurements.map(({ database, rp, measurement }) => (
      <Card key={`${database}:${rp}:${measurement.name}`}>
        <CardHeader>
          <CardTitle>{measurement.logicalName ?? measurement.name}</CardTitle>
          <CardDescription>{database} / {rp} · physical: {measurement.name} · version: {valueOrDash(measurement.version)} · shard key: {valueOrDash(measurement.shardKeyType)}</CardDescription>
        </CardHeader>
        <CardContent>
          <Table headers={["Field", "Type", "Type code", "EndTime"]} rows={(measurement.schema ?? []).map((field) => [
            field.name, fieldType(field.typ), field.typ, field.endTime
          ])} empty="Measurement has no Schema" />
        </CardContent>
      </Card>
    ))}</div>
  );
}

function DiagnosticsView({ diagnostics }: { diagnostics: Diagnostic[] }) {
  if (!diagnostics.length) return <EmptyState><CheckCircle2 className="mx-auto mb-2 h-8 w-8 text-emerald-600" />未发现已知元数据异常</EmptyState>;
  return (
    <div className="space-y-3">{diagnostics.map((item, index) => (
      <Card key={`${item.code}:${item.entityId}:${index}`} className={item.severity === "error" ? "border-red-200" : item.severity === "warning" ? "border-amber-200" : ""}>
        <CardContent className="flex gap-3 pt-5">
          {item.severity === "error" ? <CircleX className="h-5 w-5 shrink-0 text-red-600" /> : <AlertTriangle className="h-5 w-5 shrink-0 text-amber-600" />}
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2"><strong>{item.title}</strong><Badge variant={item.severity === "error" ? "destructive" : item.severity === "warning" ? "warning" : "secondary"}>{item.code}</Badge></div>
            <p className="mt-1 text-sm text-muted-foreground">{item.detail}</p>
            <code className="mt-2 block text-xs text-slate-500">{item.entityId}</code>
          </div>
        </CardContent>
      </Card>
    ))}</div>
  );
}

function RawJsonView({ value }: { value: unknown }) {
  const [query, setQuery] = useState("");
  const text = JSON.stringify(value, null, 2);
  const visible = query ? text.split("\n").filter((line) => line.toLowerCase().includes(query.toLowerCase())).join("\n") : text;
  return (
    <Card>
      <CardHeader><CardTitle>Raw openGemini JSON</CardTitle><CardDescription>保留 `/getdata` 原始响应，便于字段核对和故障排查</CardDescription></CardHeader>
      <CardContent>
        <div className="relative mb-3"><Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" /><Input className="pl-9" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="筛选字段或值" /></div>
        <pre className="max-h-[720px] overflow-auto rounded-lg bg-slate-950 p-4 text-xs leading-5 text-slate-100">{visible}</pre>
      </CardContent>
    </Card>
  );
}

function Metric({ label, value, compact }: { label: string; value: unknown; compact?: boolean }) {
  return <div className={`rounded-lg border border-border bg-white ${compact ? "p-3" : "p-4"}`}><p className="text-xs uppercase tracking-wide text-muted-foreground">{label}</p><p className="mt-1 break-all text-xl font-semibold">{valueOrDash(value)}</p></div>;
}

function StatusBadge({ node }: { node: NodeDto }) {
  const healthy = node.kind === "meta" ? node.statusCode === 0 || node.statusCode === 1 : node.statusCode === 1;
  return <Badge variant={healthy ? "success" : "destructive"}>{node.status ?? node.statusCode ?? "unknown"}</Badge>;
}

function Table({ headers, rows, empty = "暂无数据" }: { headers: string[]; rows: ReactNode[][]; empty?: string }) {
  if (!rows.length) return <EmptyState>{empty}</EmptyState>;
  return (
    <div className="overflow-x-auto rounded-lg border border-border">
      <table className="w-full min-w-[760px] border-collapse text-left text-sm">
        <thead className="bg-slate-50 text-xs uppercase tracking-wide text-muted-foreground"><tr>{headers.map((header) => <th key={header} className="border-b border-border px-3 py-2.5">{header}</th>)}</tr></thead>
        <tbody>{rows.map((row, rowIndex) => <tr key={rowIndex} className="border-b border-border last:border-0 hover:bg-slate-50/70">{row.map((cell, cellIndex) => <td key={cellIndex} className="px-3 py-2.5 align-top">{isValidElement(cell) ? cell : valueOrDash(cell)}</td>)}</tr>)}</tbody>
      </table>
    </div>
  );
}

function rawBoolean(vm: MetadataViewModel, key: string) {
  return (vm.cluster.rawSnapshot?.[key] as boolean | undefined) ?? false;
}

function fieldType(type?: number | null) {
  if (type == null) return "unknown";
  return type === 6 ? "tag" : `type-${type}`;
}
