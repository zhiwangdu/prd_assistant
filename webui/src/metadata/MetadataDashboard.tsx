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
import { isValidElement, useMemo, useState, type ReactNode } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input, Tabs, TabsContent, TabsList, TabsTrigger } from "../components/ui";
import { formatDuration, valueOrDash } from "../lib/utils";
import { confirmImport, fetchSnapshot, fetchStoredCluster, previewImport, type ImportPreview } from "./api";
import { buildTopology } from "./topology";
import type { DatabaseDto, Diagnostic, MetadataViewModel, NodeDto, RetentionPolicyDto } from "./types";
import { buildViewModel } from "./view-model";

type Props = { apiKey: string };

export function MetadataDashboard({ apiKey }: Props) {
  const [url, setUrl] = useState("http://127.0.0.1:8091/getdata");
  const [clusterId, setClusterId] = useState("6735497445922383781");
  const [vm, setVm] = useState<MetadataViewModel | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [importMessage, setImportMessage] = useState("");

  async function load(mode: "live" | "stored") {
    if (!apiKey.trim()) {
      setError("请先填写 API Key");
      return;
    }
    setLoading(true);
    setError("");
    try {
      const snapshot = mode === "live" ? await fetchSnapshot(url, apiKey) : await fetchStoredCluster(clusterId, apiKey);
      setVm(buildViewModel(snapshot));
      setClusterId(snapshot.cluster.clusterId);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function persistSnapshot() {
    setLoading(true);
    setError("");
    try {
      if (!importPreview) {
        const preview = await previewImport(url, apiKey);
        setImportPreview(preview);
        setImportMessage(`预览完成：${preview.summary.nodes} nodes / ${preview.summary.databases} databases。再次点击确认写入。`);
      } else {
        await confirmImport(importPreview.importId, apiKey);
        setImportMessage(`已写入 Server Metadata Store：${importPreview.importId}`);
        setImportPreview(null);
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
        <CardContent className="flex flex-col gap-3 pt-5 xl:flex-row">
          <div className="flex flex-1 gap-2">
            <Input value={url} onChange={(event) => setUrl(event.target.value)} aria-label="Metadata URL" />
            <Button onClick={() => void load("live")} disabled={loading}>
              <RefreshCw className={`mr-2 h-4 w-4 ${loading ? "animate-spin" : ""}`} />
              实时加载
            </Button>
            <Button variant="outline" onClick={() => void persistSnapshot()} disabled={loading}>
              {importPreview ? "确认写入" : "预览导入"}
            </Button>
          </div>
          <div className="flex gap-2 xl:w-[420px]">
            <Input value={clusterId} onChange={(event) => setClusterId(event.target.value)} aria-label="Cluster ID" />
            <Button variant="outline" onClick={() => void load("stored")} disabled={loading}>读取已存快照</Button>
          </div>
        </CardContent>
      </Card>

      {error && <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</div>}
      {importMessage && <div className="rounded-lg border border-teal-200 bg-teal-50 p-3 text-sm text-teal-800">{importMessage}</div>}
      {!vm ? (
        <EmptyState>从 openGemini `/getdata` 实时加载，或读取 Server 中已确认的 Metadata 快照。</EmptyState>
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
  );
}

function Tab({ value, icon: Icon, label }: { value: string; icon: typeof Boxes; label: string }) {
  return <TabsTrigger value={value}><Icon className="mr-2 inline h-4 w-4" />{label}</TabsTrigger>;
}

function Overview({ vm }: { vm: MetadataViewModel }) {
  const labels = vm.cluster.labels ?? {};
  const metrics = [
    ["Cluster ID", vm.cluster.clusterId],
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
  const graph = useMemo(() => buildTopology(vm), [vm]);
  return (
    <Card>
      <CardHeader><CardTitle>Cluster topology</CardTitle><CardDescription>DataNode → Database/PT → ShardGroup → Shard → IndexGroup → Index</CardDescription></CardHeader>
      <CardContent>
        <div className="h-[680px] overflow-hidden rounded-lg border border-border bg-slate-50">
          <ReactFlow nodes={graph.nodes} edges={graph.edges} fitView minZoom={0.2} maxZoom={1.8}>
            <Background color="#cbd5e1" gap={20} />
            <MiniMap pannable zoomable />
            <Controls />
          </ReactFlow>
        </div>
      </CardContent>
    </Card>
  );
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
