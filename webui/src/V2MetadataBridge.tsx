import { Boxes, CheckCircle2, RefreshCw, Trash2, UploadCloud } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Input } from "./components/ui";
import {
  confirmV2MetadataImport,
  deleteV2MetadataInstance,
  getV2MetadataSnapshot,
  importV2Metadata,
  importV2MetadataFromUrl,
  listV2MetadataImports,
  listV2MetadataInstances,
  previewV2MetadataFetchImport,
  previewV2MetadataImport,
  refreshV2MetadataInstance,
  type V2MetadataImport,
  type V2MetadataImportResponse,
  type V2MetadataInstanceSummary
} from "./v2-api";

type TemplateType = "json" | "yaml" | "opengemini";

export function V2MetadataBridge({ apiKey }: { apiKey: string }) {
  const [instances, setInstances] = useState<V2MetadataInstanceSummary[]>([]);
  const [imports, setImports] = useState<V2MetadataImport[]>([]);
  const [selectedInstanceId, setSelectedInstanceId] = useState("");
  const [snapshot, setSnapshot] = useState<Record<string, unknown> | null>(null);
  const [preview, setPreview] = useState<V2MetadataImportResponse | null>(null);
  const [instanceId, setInstanceId] = useState("");
  const [remark, setRemark] = useState("");
  const [templateType, setTemplateType] = useState<TemplateType>("json");
  const [url, setUrl] = useState("http://127.0.0.1:8091/getdata");
  const [content, setContent] = useState("");
  const [status, setStatus] = useState("V2 Metadata waiting to load");
  const [loading, setLoading] = useState(false);

  const selectedInstance = useMemo(() => instances.find((item) => item.instanceId === selectedInstanceId) ?? null, [instances, selectedInstanceId]);

  const refresh = useCallback(async () => {
    if (!apiKey.trim()) {
      setInstances([]);
      setImports([]);
      setStatus("API Key required");
      return;
    }
    setLoading(true);
    try {
      const [instanceResponse, importResponse] = await Promise.all([
        listV2MetadataInstances(apiKey),
        listV2MetadataImports(apiKey)
      ]);
      setInstances(instanceResponse.instances);
      setImports(importResponse.imports);
      if (!selectedInstanceId && instanceResponse.instances[0]) {
        setSelectedInstanceId(instanceResponse.instances[0].instanceId);
      }
      setStatus(`V2 loaded ${instanceResponse.instances.length} instances and ${importResponse.imports.length} imports`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [apiKey, selectedInstanceId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function loadSnapshot(nextInstanceId: string) {
    setSelectedInstanceId(nextInstanceId);
    setLoading(true);
    try {
      const nextSnapshot = await getV2MetadataSnapshot(apiKey, nextInstanceId);
      setSnapshot(nextSnapshot);
      setStatus(`V2 snapshot loaded for ${nextInstanceId}`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function previewContentImport() {
    if (!instanceId.trim() || !content.trim()) {
      setStatus("Instance ID and content are required");
      return;
    }
    setLoading(true);
    try {
      const response = await previewV2MetadataImport(apiKey, payloadBase({ content }));
      setPreview(response);
      setStatus(`V2 preview ${response.import.importId}: ${response.import.nodeCount} nodes, ${response.import.databaseCount} DBs`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function previewFetchImport() {
    if (!instanceId.trim() || !url.trim()) {
      setStatus("Instance ID and URL are required");
      return;
    }
    setLoading(true);
    try {
      const response = await previewV2MetadataFetchImport(apiKey, payloadBase({ url }));
      setPreview(response);
      setStatus(`V2 fetch preview ${response.import.importId}: ${response.import.nodeCount} nodes, ${response.import.databaseCount} DBs`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function confirmPreview() {
    if (!preview) return;
    setLoading(true);
    try {
      const response = await confirmV2MetadataImport(apiKey, preview.import.importId);
      setPreview(response);
      setSelectedInstanceId(response.import.instanceId);
      setSnapshot(response.snapshot);
      setStatus(`V2 confirmed ${response.import.importId}`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function directImport(fromUrl: boolean) {
    if (!instanceId.trim()) {
      setStatus("Instance ID is required");
      return;
    }
    setLoading(true);
    try {
      const response = fromUrl
        ? await importV2MetadataFromUrl(apiKey, payloadBase({ url }))
        : await importV2Metadata(apiKey, payloadBase({ content }));
      setSelectedInstanceId(response.instance.instanceId);
      setSnapshot(response.snapshot);
      setStatus(`V2 imported ${response.instance.instanceId}`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function deleteInstance(instance: V2MetadataInstanceSummary) {
    if (!window.confirm(`Delete V2 metadata instance ${instance.instanceId}?`)) return;
    setLoading(true);
    try {
      await deleteV2MetadataInstance(apiKey, instance.instanceId);
      if (selectedInstanceId === instance.instanceId) {
        setSelectedInstanceId("");
        setSnapshot(null);
      }
      setStatus(`V2 deleted ${instance.instanceId}`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function refreshInstance(instance: V2MetadataInstanceSummary) {
    setLoading(true);
    try {
      const response = await refreshV2MetadataInstance(apiKey, instance.instanceId);
      setSelectedInstanceId(instance.instanceId);
      setSnapshot(response.snapshot);
      setStatus(`V2 refreshed ${instance.instanceId}`);
      await refresh();
    } catch (reason) {
      setStatus(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }

  async function handleFileSelected(file?: File) {
    if (!file) return;
    try {
      setContent(await file.text());
      if (!instanceId.trim()) setInstanceId(file.name.replace(/\.[^.]+$/, ""));
      setStatus(`${file.name} loaded`);
    } catch (reason) {
      setStatus(errorMessage(reason));
    }
  }

  function payloadBase<T extends { content: string } | { url: string }>(extra: T) {
    return {
      instanceId: instanceId.trim(),
      templateType,
      remark: remark.trim() || null,
      ...extra
    };
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Boxes className="h-5 w-5 text-primary" />
              <CardTitle>V2 Metadata Workbench</CardTitle>
            </div>
            <CardDescription>V2 metadata import preview/confirm, direct import, instance listing, delete, and snapshot inspection</CardDescription>
          </div>
          <Button className="h-8 px-3" disabled={loading || !apiKey.trim()} variant="outline" onClick={() => void refresh()}><RefreshCw className="mr-2 h-4 w-4" />刷新</Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid gap-5 xl:grid-cols-[420px_minmax(0,1fr)_420px]">
          <div className="space-y-4 rounded-lg border border-border p-4">
            <div className="grid gap-3 md:grid-cols-2">
              <Input value={instanceId} onChange={(event) => setInstanceId(event.target.value)} placeholder="Instance ID" />
              <Input value={remark} onChange={(event) => setRemark(event.target.value)} placeholder="Remark" />
            </div>
            <label className="rounded-lg border border-border p-3 text-sm">
              <span className="mb-2 block text-xs text-muted-foreground">Template type</span>
              <select className="w-full bg-transparent text-sm outline-none" value={templateType} onChange={(event) => setTemplateType(event.target.value as TemplateType)}>
                <option value="json">json</option>
                <option value="yaml">yaml</option>
                <option value="opengemini">opengemini</option>
              </select>
            </label>
            <Input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="Metadata URL" />
            <label className="flex min-h-20 cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-border bg-slate-50 px-3 text-center text-sm text-muted-foreground transition hover:border-primary">
              <UploadCloud className="mb-2 h-5 w-5" />
              Upload metadata JSON/YAML
              <input className="hidden" type="file" accept=".json,.yaml,.yml,text/*,application/json" onChange={(event) => void handleFileSelected(event.target.files?.[0])} />
            </label>
            <textarea className="min-h-56 w-full resize-y rounded-md border border-border bg-white p-3 font-mono text-xs outline-none focus:ring-2 focus:ring-teal-600/20" spellCheck={false} value={content} onChange={(event) => setContent(event.target.value)} placeholder="Metadata JSON/YAML content" />
            <div className="grid gap-2 sm:grid-cols-2">
              <Button disabled={loading || !instanceId.trim() || !content.trim()} variant="outline" onClick={() => void previewContentImport()}>Preview content</Button>
              <Button disabled={loading || !instanceId.trim() || !url.trim()} variant="outline" onClick={() => void previewFetchImport()}>Preview URL</Button>
              <Button disabled={loading || !instanceId.trim() || !content.trim()} onClick={() => void directImport(false)}>Import content</Button>
              <Button disabled={loading || !instanceId.trim() || !url.trim()} onClick={() => void directImport(true)}>Import URL</Button>
            </div>
            <p className="text-xs text-muted-foreground">{status}</p>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <h3 className="text-sm font-semibold">Import preview</h3>
              <Button disabled={loading || !preview} onClick={() => void confirmPreview()}><CheckCircle2 className="mr-2 h-4 w-4" />Confirm</Button>
            </div>
            {preview ? (
              <>
                <div className="grid gap-3 md:grid-cols-4">
                  <Metric label="Import" value={preview.import.importId} />
                  <Metric label="Instance" value={preview.import.instanceId} />
                  <Metric label="Nodes" value={String(preview.import.nodeCount)} />
                  <Metric label="DBs" value={String(preview.import.databaseCount)} />
                </div>
                <JsonBlock title="Snapshot preview" value={preview.snapshot} />
              </>
            ) : <EmptyState>Preview content or URL before confirming.</EmptyState>}

            <div>
              <h3 className="mb-2 text-sm font-semibold">Recent imports</h3>
              <div className="max-h-52 space-y-2 overflow-auto">
                {imports.length ? imports.map((item) => (
                  <div className="rounded-lg border border-border p-3" key={item.importId}>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant={item.status === "confirmed" ? "success" : "secondary"}>{item.status}</Badge>
                      <span className="font-mono text-xs">{item.importId}</span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">{item.instanceId} · {item.templateType} · {item.nodeCount} nodes · {item.databaseCount} DBs</p>
                  </div>
                )) : <EmptyState>No V2 metadata imports.</EmptyState>}
              </div>
            </div>
          </div>

          <div className="space-y-4 rounded-lg border border-border p-4">
            <h3 className="text-sm font-semibold">V2 instances</h3>
            <div className="max-h-80 space-y-2 overflow-auto">
              {instances.length ? instances.map((instance) => (
                <div className={`rounded-lg border p-3 ${selectedInstanceId === instance.instanceId ? "border-primary bg-slate-50" : "border-border"}`} key={instance.instanceId}>
                  <button className="w-full text-left" onClick={() => void loadSnapshot(instance.instanceId)}>
                    <p className="break-all text-sm font-medium">{instance.instanceId}</p>
                    <p className="mt-1 text-xs text-muted-foreground">{instance.product ?? "-"} {instance.version ?? ""} · {instance.environment ?? "-"} · {instance.templateType}</p>
                    <p className="mt-1 text-xs text-muted-foreground">{instance.nodeCount} nodes · {instance.databaseCount} DBs · {instance.remark ?? "no remark"}</p>
                  </button>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button className="h-8 px-3" disabled={loading} variant="outline" onClick={() => void refreshInstance(instance)}><RefreshCw className="mr-2 h-4 w-4" />Refresh raw</Button>
                    <Button className="h-8 px-3 text-red-600" disabled={loading} variant="outline" onClick={() => void deleteInstance(instance)}><Trash2 className="mr-2 h-4 w-4" />Delete</Button>
                  </div>
                </div>
              )) : <EmptyState>No V2 metadata instances.</EmptyState>}
            </div>
            <div>
              <h3 className="mb-2 text-sm font-semibold">{selectedInstance ? selectedInstance.instanceId : "Snapshot"}</h3>
              {snapshot ? <JsonBlock title="Snapshot JSON" value={snapshot} /> : <EmptyState>Select an instance to load snapshot.</EmptyState>}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value}</p></div>;
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  return <div><p className="mb-2 text-xs text-muted-foreground">{title}</p><pre className="max-h-96 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{JSON.stringify(value, null, 2)}</pre></div>;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
