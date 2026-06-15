import {
  AlertTriangle,
  Boxes,
  Braces,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleX,
  FileJson,
  GitBranch,
  Link,
  Network,
  RefreshCw,
  Search,
  Server,
  TableProperties,
  UploadCloud
} from "lucide-react";
import { isValidElement, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input, Tabs, TabsContent, TabsList, TabsTrigger } from "../components/ui";
import { formatDuration, valueOrDash } from "../lib/utils";
import { confirmImport, fetchImportedInstances, fetchSnapshot, fetchStoredInstance, previewImport, previewTemplateImport, type ImportPreview } from "./api";
import { openGeminiFieldTypeLabel } from "./field-types";
import { buildTopologyIndex, filterTopologyRows } from "./topology";
import type { DatabaseDto, Diagnostic, MetadataInstanceSummary, MetadataViewModel, NodeDto, RetentionPolicyDto, TopologyFilters, TopologySummaryRow } from "./types";
import { buildViewModel } from "./view-model";

type Props = { apiKey: string };
type MetadataImportMode = "live" | "file" | "text";

export function MetadataDashboard({ apiKey }: Props) {
  const [url, setUrl] = useState("http://127.0.0.1:8091/getdata");
  const [instanceId, setInstanceId] = useState("");
  const [instanceRemark, setInstanceRemark] = useState("");
  const [importMode, setImportMode] = useState<MetadataImportMode>("live");
  const [jsonFile, setJsonFile] = useState<File | null>(null);
  const [jsonText, setJsonText] = useState("");
  const [instances, setInstances] = useState<MetadataInstanceSummary[]>([]);
  const [listStatus, setListStatus] = useState("等待加载已导入列表");
  const [vm, setVm] = useState<MetadataViewModel | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [importMessage, setImportMessage] = useState("");
  const [instancesCollapsed, setInstancesCollapsed] = useState(false);

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
    if (importMode === "live" && !instanceId.trim()) {
      setError("请先填写 InstanceID");
      return;
    }
    setLoading(true);
    setError("");
    try {
      if (!importPreview) {
        const preview = await createImportPreview();
        setImportPreview(preview);
        setImportMessage(`预览完成：${preview.summary.instances} instances / ${preview.summary.nodes} nodes / ${preview.summary.databases} databases。再次点击确认写入。`);
      } else {
        await confirmImport(importPreview.importId, apiKey);
        setImportMessage(`已写入 Server Metadata Store：${importPreview.importId}`);
        setImportPreview(null);
        await refreshInstances();
        if (instanceId.trim()) await load("stored");
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function createImportPreview() {
    if (importMode === "live") {
      return previewImport(url, instanceId.trim(), instanceRemark, apiKey);
    }
    if (importMode === "file") {
      if (!jsonFile) throw new Error("请先选择 JSON 文件");
      const content = await jsonFile.text();
      return previewTemplateImport({
        templateType: "json",
        filename: jsonFile.name,
        instanceId,
        remark: instanceRemark,
        content
      }, apiKey);
    }
    if (!jsonText.trim()) throw new Error("请先输入 JSON 文本");
    return previewTemplateImport({
      templateType: "json",
      filename: "metadata-manual.json",
      instanceId,
      remark: instanceRemark,
      content: jsonText
    }, apiKey);
  }

  function resetImportPreview() {
    setImportPreview(null);
    setImportMessage("");
  }

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader>
          <CardTitle>Metadata import</CardTitle>
          <CardDescription>支持实时加载、JSON 文件上传和手动 JSON 文本三种导入方式</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid min-w-0 gap-2 sm:grid-cols-[minmax(0,1fr)_150px]">
            <Input value={instanceId} onChange={(event) => { setInstanceId(event.target.value); resetImportPreview(); }} aria-label="Instance ID" placeholder={importMode === "live" ? "InstanceID（实时加载必填）" : "InstanceID（openGemini JSON 必填，模板 JSON 可选）"} />
            <Input value={instanceRemark} onChange={(event) => { setInstanceRemark(event.target.value); resetImportPreview(); }} aria-label="Instance remark" maxLength={120} placeholder="备注名" />
          </div>
          <div className="flex flex-wrap gap-2">
            <ImportModeButton active={importMode === "live"} icon={Link} label="实时加载" onClick={() => { setImportMode("live"); resetImportPreview(); }} />
            <ImportModeButton active={importMode === "file"} icon={UploadCloud} label="JSON 文件" onClick={() => { setImportMode("file"); resetImportPreview(); }} />
            <ImportModeButton active={importMode === "text"} icon={FileJson} label="JSON 文本" onClick={() => { setImportMode("text"); resetImportPreview(); }} />
          </div>
          <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_auto_auto]">
            <ImportSourceControls
              mode={importMode}
              url={url}
              jsonFile={jsonFile}
              jsonText={jsonText}
              onUrlChange={(value) => { setUrl(value); resetImportPreview(); }}
              onFileChange={(file) => { setJsonFile(file); resetImportPreview(); }}
              onTextChange={(value) => { setJsonText(value); resetImportPreview(); }}
            />
            <Button onClick={() => void load("live")} disabled={loading || importMode !== "live"}>
              <RefreshCw className={`mr-2 h-4 w-4 ${loading && importMode === "live" ? "animate-spin" : ""}`} />
              实时加载
            </Button>
            <Button variant="outline" onClick={() => void persistSnapshot()} disabled={loading}>
              {importPreview ? "确认写入" : "预览导入"}
            </Button>
          </div>
          <div className="flex justify-end">
            <Button variant="outline" onClick={() => void load("stored")} disabled={loading}>读取已存 Instance</Button>
          </div>
        </CardContent>
      </Card>

      {error && <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</div>}
      {importMessage && <div className="rounded-lg border border-teal-200 bg-teal-50 p-3 text-sm text-teal-800">{importMessage}</div>}
      <div className={`grid gap-5 ${instancesCollapsed ? "xl:grid-cols-[64px_minmax(0,1fr)]" : "xl:grid-cols-[360px_minmax(0,1fr)]"}`}>
        <ImportedInstancesPanel
          instances={instances}
          loading={loading}
          selectedInstanceId={instanceId}
          status={listStatus}
          collapsed={instancesCollapsed}
          onToggleCollapsed={() => setInstancesCollapsed((value) => !value)}
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
              <Tab value="explorer" icon={Network} label="Explorer" />
              <Tab value="schemas" icon={TableProperties} label="Schemas" />
              <Tab value="diagnostics" icon={AlertTriangle} label={`Diagnostics ${vm.diagnostics.length}`} />
              <Tab value="raw" icon={Braces} label="Raw JSON" />
            </TabsList>
            <TabsContent value="overview"><Overview vm={vm} /></TabsContent>
            <TabsContent value="nodes"><NodesView nodes={vm.nodes} /></TabsContent>
            <TabsContent value="partitions"><PartitionsView vm={vm} /></TabsContent>
            <TabsContent value="explorer"><MetadataExplorerView vm={vm} /></TabsContent>
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

function ImportModeButton({ active, icon: Icon, label, onClick }: { active: boolean; icon: typeof Boxes; label: string; onClick: () => void }) {
  return (
    <Button className={active ? "border-primary bg-teal-50 text-teal-800 hover:bg-teal-50" : ""} variant="outline" onClick={onClick} type="button">
      <Icon className="mr-2 h-4 w-4" />
      {label}
    </Button>
  );
}

function ImportSourceControls({
  mode,
  url,
  jsonFile,
  jsonText,
  onUrlChange,
  onFileChange,
  onTextChange
}: {
  mode: MetadataImportMode;
  url: string;
  jsonFile: File | null;
  jsonText: string;
  onUrlChange: (value: string) => void;
  onFileChange: (file: File | null) => void;
  onTextChange: (value: string) => void;
}) {
  if (mode === "live") {
    return <Input value={url} onChange={(event) => onUrlChange(event.target.value)} aria-label="Metadata URL" placeholder="http://127.0.0.1:8091/getdata" />;
  }
  if (mode === "file") {
    return (
      <label className="flex min-h-10 w-full cursor-pointer items-center justify-between gap-3 rounded-md border border-border bg-white px-3 text-sm hover:bg-slate-50">
        <span className="min-w-0 truncate">{jsonFile ? jsonFile.name : "选择 .json 元数据文件"}</span>
        <span className="shrink-0 text-xs text-muted-foreground">Browse</span>
        <input className="hidden" type="file" accept=".json,application/json" onChange={(event) => onFileChange(event.target.files?.[0] ?? null)} />
      </label>
    );
  }
  return (
    <textarea
      className="min-h-[132px] w-full rounded-md border border-border bg-white px-3 py-2 font-mono text-sm outline-none focus:ring-2 focus:ring-teal-600/20"
      value={jsonText}
      onChange={(event) => onTextChange(event.target.value)}
      aria-label="Metadata JSON text"
      placeholder='{"ClusterID":1,"MetaNodes":[],"DataNodes":[],"SqlNodes":[]}'
    />
  );
}

function Tab({ value, icon: Icon, label }: { value: string; icon: typeof Boxes; label: string }) {
  return <TabsTrigger value={value}><Icon className="mr-2 inline h-4 w-4" />{label}</TabsTrigger>;
}

function ImportedInstancesPanel({
  instances,
  loading,
  selectedInstanceId,
  status,
  collapsed,
  onToggleCollapsed,
  onRefresh,
  onSelect
}: {
  instances: MetadataInstanceSummary[];
  loading: boolean;
  selectedInstanceId: string;
  status: string;
  collapsed: boolean;
  onToggleCollapsed: () => void;
  onRefresh: () => void;
  onSelect: (item: MetadataInstanceSummary) => void;
}) {
  if (collapsed) {
    const selected = instances.find((item) => item.instanceId === selectedInstanceId);
    return (
      <Card className="h-fit">
        <CardContent className="flex flex-col items-center gap-2 p-2">
          <Button className="h-9 w-9 px-0" variant="outline" onClick={onToggleCollapsed} title="展开 Imported Instances">
            <ChevronRight className="h-4 w-4" />
          </Button>
          <Button className="h-9 w-9 px-0" variant="outline" onClick={onRefresh} disabled={loading} title="刷新 Imported Instances">
            <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
          </Button>
          <div className="mt-2 max-h-40 overflow-hidden text-xs text-muted-foreground [writing-mode:vertical-rl]" title={selected?.instanceId ?? status}>
            {selected ? selected.instanceId.slice(0, 18) : "Instances"}
          </div>
        </CardContent>
      </Card>
    );
  }
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <div>
            <CardTitle>Imported Instances</CardTitle>
            <CardDescription>{status}</CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <Button className="h-8 px-3" variant="outline" onClick={onToggleCollapsed} title="收缩 Imported Instances">
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <Button className="h-8 px-3" variant="outline" onClick={onRefresh} disabled={loading}>
              <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
            </Button>
          </div>
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

type ExplorerMode = "relations" | "metadata";

function MetadataExplorerView({ vm }: { vm: MetadataViewModel }) {
  const [mode, setMode] = useState<ExplorerMode>("relations");
  const [filters, setFilters] = useState<TopologyFilters>({
    database: "",
    dataNodeId: "",
    startTime: "",
    endTime: "",
    onlyAbnormal: false,
    showShards: true,
    showIndexes: true
  });
  const [selectedRow, setSelectedRow] = useState<TopologySummaryRow | null>(null);
  const index = useMemo(() => buildTopologyIndex(vm), [vm]);
  const rows = useMemo(() => filterTopologyRows(index.rows, filters), [index.rows, filters]);
  const groupedRows = useMemo(() => groupTopologyRows(rows), [rows]);
  const filteredDatabases = useMemo(() => filterDatabasesForExplorer(vm.cluster.databases ?? [], filters), [vm.cluster.databases, filters]);
  const databaseMetrics = useMemo(() => databaseCascadeMetrics(filteredDatabases), [filteredDatabases]);

  function patchFilter<K extends keyof TopologyFilters>(key: K, value: TopologyFilters[K]) {
    setFilters((current) => ({ ...current, [key]: value }));
  }

  function resetFilters() {
    setFilters({ database: "", dataNodeId: "", startTime: "", endTime: "", onlyAbnormal: false, showShards: true, showIndexes: true });
    setSelectedRow(null);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Metadata Explorer</CardTitle>
        <CardDescription>用一个入口查看 Node / DBPT / Shard 归属关系，以及 DB / RP / Shard / Index 元数据详情。</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="mb-4 flex flex-wrap gap-2">
          <Button className={mode === "relations" ? "border-primary bg-teal-50 text-teal-800 hover:bg-teal-50" : ""} variant="outline" onClick={() => setMode("relations")}>Node / DBPT / Shards</Button>
          <Button className={mode === "metadata" ? "border-primary bg-teal-50 text-teal-800 hover:bg-teal-50" : ""} variant="outline" onClick={() => setMode("metadata")}>DB / RP / Shards / Indexes</Button>
        </div>
        <div className="mb-4 grid gap-3 rounded-lg border border-border bg-slate-50 p-3 md:grid-cols-2 xl:grid-cols-4">
          <FilterSelect label="Database" value={filters.database} onChange={(value) => patchFilter("database", value)} options={index.databases} />
          {mode === "relations" ? <FilterSelect label="DataNode" value={filters.dataNodeId} onChange={(value) => patchFilter("dataNodeId", value)} options={index.dataNodes} /> : null}
          <FilterInput label="Start time" type="datetime-local" value={filters.startTime} onChange={(value) => patchFilter("startTime", value)} />
          <FilterInput label="End time" type="datetime-local" value={filters.endTime} onChange={(value) => patchFilter("endTime", value)} />
          {mode === "relations" ? <FilterCheck label="Only abnormal" checked={filters.onlyAbnormal} onChange={(value) => patchFilter("onlyAbnormal", value)} /> : null}
          <FilterCheck label="Show shard rows" checked={filters.showShards} onChange={(value) => patchFilter("showShards", value)} />
          <FilterCheck label="Show index info" checked={filters.showIndexes} onChange={(value) => patchFilter("showIndexes", value)} />
          <div className="flex items-end"><Button className="w-full" variant="outline" onClick={resetFilters}>Reset filters</Button></div>
        </div>
        {mode === "relations" ? (
          <>
            <div className="mb-4 grid gap-3 md:grid-cols-4">
              <Metric label="Visible PTs" value={rows.length} compact />
              <Metric label="Abnormal PTs" value={rows.filter((row) => row.abnormal).length} compact />
              <Metric label="ShardGroups" value={rows.reduce((total, row) => total + row.shardGroups, 0)} compact />
              <Metric label="Indexes" value={rows.reduce((total, row) => total + row.indexes, 0)} compact />
            </div>
            <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
              <TopologyCascade groups={groupedRows} selectedRow={selectedRow} showShards={filters.showShards} showIndexes={filters.showIndexes} onSelect={setSelectedRow} />
              <TopologyRowDetails row={selectedRow} diagnosticsByEntity={index.diagnosticsByEntity} />
            </div>
          </>
        ) : (
          <>
            <div className="mb-4 grid gap-3 md:grid-cols-4">
              <Metric label="Databases" value={filteredDatabases.length} compact />
              <Metric label="Retention policies" value={databaseMetrics.retentionPolicies} compact />
              <Metric label="ShardGroups" value={databaseMetrics.shardGroups} compact />
              <Metric label="Indexes" value={databaseMetrics.indexes} compact />
            </div>
            <DatabasesView databases={filteredDatabases} showShards={filters.showShards} showIndexes={filters.showIndexes} />
          </>
        )}
      </CardContent>
    </Card>
  );
}

function TopologyCascade({
  groups,
  selectedRow,
  showShards,
  showIndexes,
  onSelect
}: {
  groups: Array<{ database: string; nodes: Array<{ ownerKey: string; ownerLabel: string; rows: TopologySummaryRow[] }> }>;
  selectedRow: TopologySummaryRow | null;
  showShards: boolean;
  showIndexes: boolean;
  onSelect: (row: TopologySummaryRow) => void;
}) {
  if (!groups.length) return <EmptyState>当前筛选条件下没有 PT 拓扑数据。</EmptyState>;
  return (
    <div className="space-y-3">
      {groups.map((databaseGroup) => (
        <details className="rounded-lg border border-border bg-white" key={databaseGroup.database} open={groups.length <= 3}>
          <summary className="cursor-pointer px-4 py-3 text-sm font-semibold">
            {databaseGroup.database}
            <span className="ml-2 text-xs font-normal text-muted-foreground">{databaseGroup.nodes.reduce((total, node) => total + node.rows.length, 0)} PT(s)</span>
          </summary>
          <div className="space-y-3 border-t border-border p-3">
            {databaseGroup.nodes.map((nodeGroup) => (
              <details className="rounded-lg border border-border bg-slate-50" key={`${databaseGroup.database}:${nodeGroup.ownerKey}`} open={databaseGroup.nodes.length <= 4}>
                <summary className="cursor-pointer px-3 py-2 text-sm font-medium">
                  {nodeGroup.ownerLabel}
                  <span className="ml-2 text-xs font-normal text-muted-foreground">{nodeGroup.rows.length} DBPT(s)</span>
                </summary>
                <div className="space-y-2 p-3">
                  {nodeGroup.rows.map((row) => (
                    <details className={`rounded-md border bg-white ${selectedRow?.id === row.id ? "border-primary" : "border-border"}`} key={row.id}>
                      <summary className="cursor-pointer px-3 py-2 text-sm">
                        <span className="font-medium">DBPT {row.ptId}</span>
                        <span className="ml-2 text-xs text-muted-foreground">{row.shards} shard(s) · {row.indexes} index(es) · {timeRange(row.startTime, row.endTime)}</span>
                        {row.abnormal && <span className="ml-2"><Badge variant="destructive">{row.diagnosticCount ? `${row.diagnosticCount} issue(s)` : "Abnormal"}</Badge></span>}
                      </summary>
                      <div className="space-y-3 border-t border-border p-3">
                        <div className="grid gap-2 sm:grid-cols-4">
                          <Metric label="ShardGroups" value={row.shardGroups} compact />
                          <Metric label="Shards" value={row.shards} compact />
                          <Metric label="IndexGroups" value={row.indexGroups} compact />
                          <Metric label="Indexes" value={row.indexes} compact />
                        </div>
                        <Button className="h-8 px-3" variant="outline" onClick={() => onSelect(row)}>Show details</Button>
                        {showShards ? <ShardDetailTable row={row} showIndexes={showIndexes} /> : <p className="text-sm text-muted-foreground">Shard rows hidden by filter.</p>}
                      </div>
                    </details>
                  ))}
                </div>
              </details>
            ))}
          </div>
        </details>
      ))}
    </div>
  );
}

function ShardDetailTable({ row, showIndexes }: { row: TopologySummaryRow; showIndexes: boolean }) {
  return <Table headers={showIndexes ? ["RP", "ShardGroup", "Time range", "Shard", "Owners (PT IDs)", "IndexID", "Index tier", "Index deleted"] : ["RP", "ShardGroup", "Time range", "Shard", "Owners (PT IDs)"]} rows={row.shardDetails.map((item) => showIndexes ? [
    item.rp,
    item.shardGroupId,
    timeRange(item.startTime, item.endTime),
    item.shardId,
    item.owners.join(", "),
    item.indexId,
    item.indexTier,
    String(item.indexMarkDelete ?? false)
  ] : [
    item.rp,
    item.shardGroupId,
    timeRange(item.startTime, item.endTime),
    item.shardId,
    item.owners.join(", ")
  ])} empty="No shard rows for this DBPT" />;
}

function groupTopologyRows(rows: TopologySummaryRow[]) {
  const databaseMap = new Map<string, Map<string, { ownerKey: string; ownerLabel: string; rows: TopologySummaryRow[] }>>();
  for (const row of rows) {
    const nodeMap = databaseMap.get(row.database) ?? new Map<string, { ownerKey: string; ownerLabel: string; rows: TopologySummaryRow[] }>();
    const ownerKey = String(row.ownerNodeId ?? "missing");
    const ownerLabel = row.ownerNodeId == null ? "Missing DataNode" : `DataNode ${row.ownerNodeId} · ${row.ownerHost ?? "-"}`;
    const group = nodeMap.get(ownerKey) ?? { ownerKey, ownerLabel, rows: [] };
    group.rows.push(row);
    nodeMap.set(ownerKey, group);
    databaseMap.set(row.database, nodeMap);
  }
  return [...databaseMap.entries()].map(([database, nodeMap]) => ({
    database,
    nodes: [...nodeMap.values()].map((node) => ({ ...node, rows: node.rows.sort((left, right) => left.ptId - right.ptId) }))
  }));
}

function FilterSelect({ label, value, options, onChange, allowAll = true }: { label: string; value: string; options: Array<{ value: string; label: string }>; onChange: (value: string) => void; allowAll?: boolean }) {
  return <label className="grid gap-1 text-xs font-medium text-muted-foreground">{label}<select className="h-10 rounded-md border border-border bg-white px-3 text-sm text-foreground" value={value} onChange={(event) => onChange(event.target.value)}>{allowAll ? <option value="">All</option> : null}{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>;
}

function FilterInput({ label, value, type, onChange }: { label: string; value: string; type: string; onChange: (value: string) => void }) {
  return <label className="grid gap-1 text-xs font-medium text-muted-foreground">{label}<Input type={type} value={value} onChange={(event) => onChange(event.target.value)} /></label>;
}

function FilterCheck({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="flex h-10 items-center gap-2 self-end rounded-md border border-border bg-white px-3 text-sm"><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />{label}</label>;
}

function TopologyRowDetails({ row, diagnosticsByEntity }: { row: TopologySummaryRow | null; diagnosticsByEntity: Map<string, Diagnostic[]> }) {
  if (!row) {
    return <aside className="rounded-lg border border-dashed border-border bg-white p-5 text-sm text-muted-foreground">选择级联树中的 DBPT 查看聚合指标、异常和时间范围。</aside>;
  }
  const diagnostics = diagnosticsByEntity.get(`pt:${row.database}:${row.ptId}`) ?? [];
  return (
    <aside className="max-h-[760px] overflow-auto rounded-lg border border-border bg-white">
      <div className="sticky top-0 border-b border-border bg-white p-4">
        <div className="flex items-center justify-between gap-2"><strong>{row.database} / PT {row.ptId}</strong>{row.abnormal && <Badge variant="destructive">Abnormal</Badge>}</div>
        <p className="mt-1 text-xs text-muted-foreground">Owner DataNode {valueOrDash(row.ownerNodeId)} · {valueOrDash(row.ownerHost)}</p>
      </div>
      <div className="space-y-5 p-4">
        <section className="grid gap-3 sm:grid-cols-2">
          <Metric label="ShardGroups" value={row.shardGroups} compact />
          <Metric label="Shards" value={row.shards} compact />
          <Metric label="IndexGroups" value={row.indexGroups} compact />
          <Metric label="Indexes" value={row.indexes} compact />
        </section>
        <section><h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Range</h4><p className="text-sm">{timeRange(row.startTime, row.endTime)}</p></section>
        <section><h4 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">Diagnostics</h4>{diagnostics.length ? <div className="space-y-2">{diagnostics.map((diagnostic) => <div key={`${diagnostic.code}:${diagnostic.detail}`} className="rounded-md border border-border p-2 text-xs"><Badge variant={diagnostic.severity === "error" ? "destructive" : "warning"}>{diagnostic.code}</Badge><p className="mt-1 text-slate-700">{diagnostic.title}</p><p className="mt-1 text-muted-foreground">{diagnostic.detail}</p></div>)}</div> : <p className="text-sm text-muted-foreground">No direct PT diagnostics</p>}</section>
      </div>
    </aside>
  );
}

function timeRange(start?: string | null, end?: string | null) {
  return `${start?.slice(0, 19) ?? "-"} -> ${end?.slice(0, 19) ?? "-"}`;
}

function filterDatabasesForExplorer(databases: DatabaseDto[], filters: TopologyFilters) {
  return databases
    .filter((database) => !filters.database || database.name === filters.database)
    .map((database) => ({
      ...database,
      retentionPolicies: (database.retentionPolicies ?? []).map((rp) => ({
        ...rp,
        shardGroups: (rp.shardGroups ?? []).filter((group) => rangeOverlapsFilter(group.startTime, group.endTime, filters)),
        indexGroups: (rp.indexGroups ?? []).filter((group) => rangeOverlapsFilter(group.startTime, group.endTime, filters))
      }))
    }));
}

function databaseCascadeMetrics(databases: DatabaseDto[]) {
  const retentionPolicies = databases.flatMap((database) => database.retentionPolicies ?? []);
  const shardGroups = retentionPolicies.flatMap((rp) => rp.shardGroups ?? []);
  const indexGroups = retentionPolicies.flatMap((rp) => rp.indexGroups ?? []);
  return {
    retentionPolicies: retentionPolicies.length,
    shardGroups: shardGroups.length,
    indexes: indexGroups.reduce((total, group) => total + (group.indexes ?? []).length, 0)
  };
}

function rangeOverlapsFilter(start: string | null | undefined, end: string | null | undefined, filters: TopologyFilters) {
  if (!filters.startTime && !filters.endTime) return true;
  const filterStart = filters.startTime ? Date.parse(filters.startTime) : Number.NEGATIVE_INFINITY;
  const filterEnd = filters.endTime ? Date.parse(filters.endTime) : Number.POSITIVE_INFINITY;
  const itemStart = start ? Date.parse(start) : Number.NEGATIVE_INFINITY;
  const itemEnd = end ? Date.parse(end) : Number.POSITIVE_INFINITY;
  return itemStart <= filterEnd && itemEnd >= filterStart;
}

function DatabasesView({ databases, showShards = true, showIndexes = true }: { databases: DatabaseDto[]; showShards?: boolean; showIndexes?: boolean }) {
  if (!databases.length) return <EmptyState>No databases</EmptyState>;
  return (
    <div className="space-y-3">
      {databases.map((database) => (
        <details className="rounded-lg border border-border bg-white" key={database.name}>
          <summary className="cursor-pointer px-4 py-3">
            <span className="font-semibold">{database.name}</span>
            <span className="ml-2 text-xs text-muted-foreground">default RP {valueOrDash(database.defaultRetentionPolicy)} · replica {valueOrDash(database.replicaN)} · {(database.retentionPolicies ?? []).length} RP(s)</span>
            {database.markDeleted && <span className="ml-2"><Badge variant="destructive">Marked deleted</Badge></span>}
          </summary>
          <div className="space-y-3 border-t border-border p-3">
            {(database.retentionPolicies ?? []).map((rp) => <RetentionPolicy key={rp.name} database={database.name} rp={rp} showShards={showShards} showIndexes={showIndexes} />)}
          </div>
        </details>
      ))}
    </div>
  );
}

function RetentionPolicy({ database, rp, showShards, showIndexes }: { database: string; rp: RetentionPolicyDto; showShards: boolean; showIndexes: boolean }) {
  const shardCount = (rp.shardGroups ?? []).reduce((total, group) => total + (group.shards ?? []).length, 0);
  const indexCount = (rp.indexGroups ?? []).reduce((total, group) => total + (group.indexes ?? []).length, 0);
  return (
    <details className="rounded-lg border border-border bg-slate-50">
      <summary className="cursor-pointer px-3 py-2">
        <span className="font-medium">{rp.name}</span>
        <span className="ml-2 text-xs text-muted-foreground">{(rp.shardGroups ?? []).length} shard group(s) · {shardCount} shard(s) · {(rp.indexGroups ?? []).length} index group(s) · {indexCount} index(es)</span>
      </summary>
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap gap-2">
          <Badge variant="outline">ReplicaN {valueOrDash(rp.replicaN)}</Badge>
          <Badge variant="secondary">Duration {formatDuration(rp.duration)}</Badge>
          <Badge variant="secondary">ShardGroup {formatDuration(rp.shardGroupDuration)}</Badge>
          <Badge variant="secondary">IndexGroup {formatDuration(rp.indexGroupDuration)}</Badge>
        </div>
        {showShards ? <details className="rounded-md border border-border bg-white">
          <summary className="cursor-pointer px-3 py-2 text-sm font-medium">ShardGroups and Shards</summary>
          <div className="space-y-2 border-t border-border p-3">
            {(rp.shardGroups ?? []).length ? (rp.shardGroups ?? []).map((group) => (
              <details className="rounded-md border border-border bg-slate-50" key={group.id}>
                <summary className="cursor-pointer px-3 py-2 text-sm">ShardGroup {group.id}<span className="ml-2 text-xs text-muted-foreground">{timeRange(group.startTime, group.endTime)} · {(group.shards ?? []).length} shard(s)</span></summary>
                <div className="border-t border-border p-3">
                  <Table headers={["Shard", "Owners (PT IDs)", "Tier", "IndexID", "ReadOnly", "MarkDelete"]} rows={(group.shards ?? []).map((shard) => [
                    shard.id, (shard.owners ?? []).join(", "), shard.tier, shard.indexId, String(shard.readOnly ?? false), String(shard.markDelete ?? false)
                  ])} empty={`No Shards in ${database}/${rp.name}/ShardGroup ${group.id}`} />
                </div>
              </details>
            )) : <EmptyState>No ShardGroups</EmptyState>}
          </div>
        </details> : <p className="text-sm text-muted-foreground">Shard groups hidden by filter.</p>}
        {showIndexes ? <details className="rounded-md border border-border bg-white">
          <summary className="cursor-pointer px-3 py-2 text-sm font-medium">IndexGroups and Indexes</summary>
          <div className="space-y-2 border-t border-border p-3">
            {(rp.indexGroups ?? []).length ? (rp.indexGroups ?? []).map((group) => (
              <details className="rounded-md border border-border bg-slate-50" key={group.id}>
                <summary className="cursor-pointer px-3 py-2 text-sm">IndexGroup {group.id}<span className="ml-2 text-xs text-muted-foreground">{timeRange(group.startTime, group.endTime)} · {(group.indexes ?? []).length} index(es)</span></summary>
                <div className="border-t border-border p-3">
                  <Table headers={["Index", "Owners (PT IDs)", "Tier", "MarkDelete"]} rows={(group.indexes ?? []).map((index) => [
                    index.id, (index.owners ?? []).join(", "), index.tier, String(index.markDelete ?? false)
                  ])} empty={`No Indexes in ${database}/${rp.name}/IndexGroup ${group.id}`} />
                </div>
              </details>
            )) : <EmptyState>No IndexGroups</EmptyState>}
          </div>
        </details> : <p className="text-sm text-muted-foreground">Index groups hidden by filter.</p>}
      </div>
    </details>
  );
}

function SchemasView({ databases }: { databases: DatabaseDto[] }) {
  const defaultDatabase = useMemo(() => preferredSchemaDatabase(databases), [databases]);
  const [databaseFilter, setDatabaseFilter] = useState(defaultDatabase);
  const selectedDatabase = useMemo(() => databases.find((database) => database.name === databaseFilter) ?? databases.find((database) => database.name === defaultDatabase), [databaseFilter, databases, defaultDatabase]);
  const rpOptions = useMemo(() => selectedDatabase?.retentionPolicies ?? [], [selectedDatabase]);
  const defaultRp = rpOptions[0]?.name ?? "";
  const [rpFilter, setRpFilter] = useState(defaultRp);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (defaultDatabase && !databases.some((database) => database.name === databaseFilter)) {
      setDatabaseFilter(defaultDatabase);
    }
  }, [databaseFilter, databases, defaultDatabase]);

  useEffect(() => {
    if (!rpOptions.some((rp) => rp.name === rpFilter)) {
      setRpFilter(defaultRp);
    }
  }, [defaultRp, rpFilter, rpOptions]);

  const measurements = databases.flatMap((database) => (database.retentionPolicies ?? []).flatMap((rp) => (rp.measurements ?? []).map((measurement) => ({ database: database.name, rp: rp.name, measurement }))));
  const normalizedQuery = query.trim().toLowerCase();
  const matched = databaseFilter && rpFilter ? measurements.filter(({ database, rp, measurement }) =>
    database === databaseFilter &&
    rp === rpFilter &&
    (!normalizedQuery ||
      database.toLowerCase().includes(normalizedQuery) ||
      rp.toLowerCase().includes(normalizedQuery) ||
      measurement.name.toLowerCase().includes(normalizedQuery) ||
      (measurement.logicalName ?? "").toLowerCase().includes(normalizedQuery) ||
      (measurement.schema ?? []).some((field) => field.name.toLowerCase().includes(normalizedQuery)))
  ) : [];
  const visible = matched.slice(0, 100);
  return (
    <div className="space-y-4">
      <div className="grid gap-3 rounded-lg border border-border bg-slate-50 p-3 md:grid-cols-3">
        <FilterSelect label="Database" value={databaseFilter} allowAll={false} onChange={(value) => {
          setDatabaseFilter(value);
          const nextDatabase = databases.find((database) => database.name === value);
          setRpFilter(nextDatabase?.retentionPolicies?.[0]?.name ?? "");
        }} options={databases.map((database) => ({ value: database.name, label: database.name }))} />
        <FilterSelect label="Retention policy" value={rpFilter} allowAll={false} onChange={setRpFilter} options={rpOptions.map((rp) => ({ value: rp.name, label: rp.name }))} />
        <FilterInput label="Measurement / field" type="search" value={query} onChange={setQuery} />
      </div>
      {!databaseFilter || !rpFilter ? <EmptyState>当前没有可展示的 Database / RP Schema。</EmptyState> : visible.length ? (
        <div className="space-y-3">
          {matched.length > visible.length && <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800">当前只展示前 {visible.length} 个匹配 schema；请继续缩小过滤条件。</div>}
          {visible.map(({ database, rp, measurement }) => (
            <details className="rounded-lg border border-border bg-white" key={`${database}:${rp}:${measurement.name}`}>
              <summary className="cursor-pointer px-4 py-3">
                <span className="font-semibold">{measurement.logicalName ?? measurement.name}</span>
                <span className="ml-2 text-xs text-muted-foreground">{database} / {rp} · physical {measurement.name} · {(measurement.schema ?? []).length} field(s)</span>
              </summary>
              <div className="border-t border-border p-3">
                <Table headers={["Field", "Type", "Type code", "EndTime"]} rows={(measurement.schema ?? []).map((field) => [
                  field.name, openGeminiFieldTypeLabel(field.typ), field.typ, field.endTime
                ])} empty="Measurement has no Schema" />
              </div>
            </details>
          ))}
        </div>
      ) : <EmptyState>没有匹配的 Schema。</EmptyState>}
    </div>
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
  return (
    <Card>
      <CardHeader><CardTitle>Raw openGemini JSON</CardTitle><CardDescription>按需展开原始 `/getdata` 响应，避免大 JSON 一次性渲染导致页面卡顿。</CardDescription></CardHeader>
      <CardContent>
        <div className="relative mb-3"><Search className="absolute left-3 top-3 h-4 w-4 text-slate-400" /><Input className="pl-9" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="筛选当前展开层级的 key/path" /></div>
        <div className="max-h-[720px] overflow-auto rounded-lg bg-slate-950 p-4 font-mono text-xs leading-5 text-slate-100">
          <JsonTreeNode name="root" path="$" value={value} query={query.trim().toLowerCase()} defaultExpanded />
        </div>
      </CardContent>
    </Card>
  );
}

function JsonTreeNode({ name, path, value, query, defaultExpanded = false }: { name: string; path: string; value: unknown; query: string; defaultExpanded?: boolean }) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [visibleCount, setVisibleCount] = useState(100);
  const expandable = value !== null && typeof value === "object";

  if (!expandable) {
    return (
      <div className="flex gap-2 py-0.5">
        <span className="text-slate-400">{name}</span>
        <span className="text-slate-500">:</span>
        <span className="break-all text-emerald-200">{formatJsonPrimitive(value)}</span>
      </div>
    );
  }

  const arrayValue = Array.isArray(value) ? value : null;
  const objectValue = !arrayValue ? value as Record<string, unknown> : null;
  const objectKeys = objectValue ? Object.keys(objectValue) : [];
  const totalKeys = arrayValue ? arrayValue.length : objectKeys.length;
  const candidateKeys = arrayValue ? Array.from({ length: Math.min(totalKeys, visibleCount) }, (_, index) => String(index)) : objectKeys;
  const filteredKeys = query ? candidateKeys.filter((key) => `${path}.${key}`.toLowerCase().includes(query) || key.toLowerCase().includes(query)) : candidateKeys;
  const visibleKeys = arrayValue ? filteredKeys : filteredKeys.slice(0, visibleCount);
  const hasMore = arrayValue ? visibleCount < totalKeys : filteredKeys.length > visibleKeys.length;
  const summary = arrayValue ? `Array(${arrayValue.length})` : `Object(${objectKeys.length})`;

  return (
    <div className="py-0.5">
      <button className="flex max-w-full items-center gap-2 text-left hover:text-white" onClick={() => setExpanded((current) => !current)} type="button">
        <span className="w-4 text-slate-400">{expanded ? "▾" : "▸"}</span>
        <span className="text-sky-200">{name}</span>
        <span className="text-slate-500">:</span>
        <span className="text-slate-300">{summary}</span>
        <span className="truncate text-slate-500">{path}</span>
      </button>
      {expanded ? (
        <div className="ml-5 border-l border-slate-800 pl-3">
          {visibleKeys.length ? visibleKeys.map((key) => (
            <JsonTreeNode
              key={`${path}.${key}`}
              name={arrayValue ? `[${key}]` : key}
              path={arrayValue ? `${path}[${key}]` : `${path}.${key}`}
              value={arrayValue ? arrayValue[Number(key)] : objectValue?.[key]}
              query={query}
            />
          )) : <div className="py-1 text-slate-500">No matching keys in this level.</div>}
          {hasMore ? (
            <button className="mt-1 rounded border border-slate-700 px-2 py-1 text-slate-300 hover:bg-slate-900" onClick={() => setVisibleCount((count) => count + 100)} type="button">
              Load more ({Math.min(visibleCount, totalKeys)}/{totalKeys})
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function Metric({ label, value, compact }: { label: string; value: unknown; compact?: boolean }) {
  return <div className={`rounded-lg border border-border bg-white ${compact ? "p-3" : "p-4"}`}><p className="text-xs uppercase tracking-wide text-muted-foreground">{label}</p><p className="mt-1 break-all text-xl font-semibold">{valueOrDash(value)}</p></div>;
}

function formatJsonPrimitive(value: unknown) {
  if (typeof value === "string") return JSON.stringify(value);
  if (value === null) return "null";
  return String(value);
}

function StatusBadge({ node }: { node: NodeDto }) {
  if (node.kind === "meta") {
    return <Badge variant="secondary">none</Badge>;
  }
  const status = dataSqlStatus(node.statusCode);
  return <Badge variant={status.variant}>{status.label}</Badge>;
}

function Table({ headers, rows, empty = "暂无数据" }: { headers: string[]; rows: ReactNode[][]; empty?: string }) {
  if (!rows.length) return <EmptyState>{empty}</EmptyState>;
  return (
    <div className="max-h-[560px] overflow-auto rounded-lg border border-border">
      <table className="w-full min-w-[760px] border-collapse text-left text-sm">
        <thead className="sticky top-0 z-10 bg-slate-50 text-xs uppercase tracking-wide text-muted-foreground shadow-[0_1px_0_hsl(var(--border))]"><tr>{headers.map((header) => <th key={header} className="px-3 py-2.5">{header}</th>)}</tr></thead>
        <tbody>{rows.map((row, rowIndex) => <tr key={rowIndex} className="border-b border-border last:border-0 hover:bg-slate-50/70">{row.map((cell, cellIndex) => <td key={cellIndex} className="px-3 py-2.5 align-top">{isValidElement(cell) ? cell : valueOrDash(cell)}</td>)}</tr>)}</tbody>
      </table>
    </div>
  );
}

function rawBoolean(vm: MetadataViewModel, key: string) {
  return (vm.cluster.rawSnapshot?.[key] as boolean | undefined) ?? false;
}

function dataSqlStatus(statusCode?: number | null): { label: string; variant: "success" | "warning" | "destructive" | "secondary" } {
  if (statusCode == null) return { label: "unknown", variant: "secondary" };
  const labels: Record<number, string> = {
    0: "none",
    1: "alive",
    2: "leaving",
    3: "left",
    4: "failed"
  };
  if (statusCode === 1) return { label: labels[statusCode], variant: "success" };
  if (statusCode === 3 || statusCode === 4) return { label: labels[statusCode], variant: "destructive" };
  return { label: labels[statusCode] ?? `status-${statusCode}`, variant: "warning" };
}

function preferredSchemaDatabase(databases: DatabaseDto[]) {
  return databases.find((database) => database.name !== "_internal")?.name ?? databases[0]?.name ?? "";
}
