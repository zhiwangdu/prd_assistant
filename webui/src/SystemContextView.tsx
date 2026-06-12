import { Boxes, BrainCircuit, FileText, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, EmptyState, Tabs, TabsContent, TabsList, TabsTrigger } from "./components/ui";
import { authHeaders, fetchJson } from "./metadata/api";
import { MetadataDashboard } from "./metadata/MetadataDashboard";

type SkillReference = {
  referenceId: string;
  path: string;
  title: string;
  summary: string;
};

type SkillSummary = {
  skillId: string;
  name: string;
  displayName: string;
  description: string;
  managed: boolean;
  includeByDefault: boolean;
  priority: number;
  products: string[];
  domainAdapters: string[];
  toolIds: string[];
  taskKinds: string[];
  revision: string;
  sourceRoot: string;
  sourcePath: string;
  references: SkillReference[];
  updatedAt: string;
};

type SkillDetail = SkillSummary & {
  injectionContent: string;
};

export function SystemContextView({ apiKey }: { apiKey: string }) {
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);
  const [selectedSkill, setSelectedSkill] = useState<SkillDetail | null>(null);
  const [status, setStatus] = useState("Skills ready");

  const refresh = useCallback(async () => {
    if (!apiKey.trim()) {
      setSkills([]);
      setSelectedSkill(null);
      return;
    }
    const response = await fetchJson<{ skills: SkillSummary[] }>("/api/skills", { headers: authHeaders(apiKey) });
    setSkills(response.skills);
    setStatus(`${response.skills.length} skill(s) loaded`);
    if (!selectedSkillId && response.skills[0]) {
      setSelectedSkillId(response.skills[0].skillId);
    }
  }, [apiKey, selectedSkillId]);

  const loadSkill = useCallback(async (skillId: string) => {
    if (!apiKey.trim()) return;
    const response = await fetchJson<SkillDetail>(`/api/skills/${encodeURIComponent(skillId)}`, { headers: authHeaders(apiKey) });
    setSelectedSkill(response);
    setSelectedSkillId(skillId);
    setStatus(`${response.displayName} loaded`);
  }, [apiKey]);

  useEffect(() => {
    void refresh().catch((reason) => setStatus(errorMessage(reason)));
  }, [refresh]);

  useEffect(() => {
    if (selectedSkillId && (!selectedSkill || selectedSkill.skillId !== selectedSkillId)) {
      void loadSkill(selectedSkillId).catch((reason) => setStatus(errorMessage(reason)));
    }
  }, [loadSkill, selectedSkill, selectedSkillId]);

  return (
    <div className="space-y-5">
      <Tabs defaultValue="skills">
        <TabsList>
          <TabsTrigger value="skills"><BrainCircuit className="mr-2 h-4 w-4" />Skills</TabsTrigger>
          <TabsTrigger value="metadata"><Boxes className="mr-2 h-4 w-4" />Metadata</TabsTrigger>
        </TabsList>

        <TabsContent value="skills">
          <div className="grid gap-5 xl:grid-cols-[380px_minmax(0,1fr)]">
            <Card>
              <CardHeader>
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <CardTitle>Diagnostic Skills</CardTitle>
                    <CardDescription>{status}</CardDescription>
                  </div>
                  <Button className="h-8 px-3" variant="outline" onClick={() => void refresh()}><RefreshCw className="h-4 w-4" /></Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-2">
                {skills.length ? skills.map((skill) => (
                  <button className={`w-full rounded-lg border p-3 text-left ${selectedSkillId === skill.skillId ? "border-primary bg-slate-50" : "border-border"}`} key={skill.skillId} onClick={() => void loadSkill(skill.skillId)}>
                    <div className="flex items-center justify-between gap-2">
                      <span className="truncate text-sm font-medium">{skill.displayName}</span>
                      <Badge variant={skill.managed ? "secondary" : "outline"}>{skill.managed ? "managed" : "external"}</Badge>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">{skill.skillId} · priority {skill.priority} · rev {skill.revision.slice(0, 8)}</p>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{skill.description}</p>
                  </button>
                )) : <EmptyState>暂无 Skill。</EmptyState>}
              </CardContent>
            </Card>

            <SkillDetailPanel skill={selectedSkill} />
          </div>
        </TabsContent>

        <TabsContent value="metadata">
          <MetadataDashboard apiKey={apiKey} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function SkillDetailPanel({ skill }: { skill: SkillDetail | null }) {
  if (!skill) {
    return <Card><CardHeader><CardTitle>Skill detail</CardTitle><CardDescription>Select a Skill.</CardDescription></CardHeader><CardContent><EmptyState>暂无选中 Skill。</EmptyState></CardContent></Card>;
  }
  const tags = [
    ...skill.products.map((value) => `product:${value}`),
    ...skill.domainAdapters.map((value) => `adapter:${value}`),
    ...skill.toolIds.map((value) => `tool:${value}`),
    ...skill.taskKinds.map((value) => `task:${value}`)
  ];
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle>{skill.displayName}</CardTitle>
            <CardDescription>{skill.skillId} · rev {skill.revision}</CardDescription>
          </div>
          <Badge variant={skill.includeByDefault ? "default" : "outline"}>{skill.includeByDefault ? "auto" : "explicit"}</Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">{skill.description}</p>
        <div className="flex flex-wrap gap-2">
          {tags.length ? tags.map((tag) => <Badge key={tag} variant="secondary">{tag}</Badge>) : <Badge variant="outline">no match metadata</Badge>}
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <DataBox label="Source" value={skill.sourcePath} />
          <DataBox label="Updated" value={new Date(skill.updatedAt).toLocaleString()} />
        </div>
        <div>
          <div className="mb-2 flex items-center gap-2 text-sm font-medium"><FileText className="h-4 w-4 text-primary" />References</div>
          <div className="space-y-2">
            {skill.references.length ? skill.references.map((reference) => (
              <div className="rounded-lg border border-border p-3" key={reference.referenceId}>
                <p className="text-sm font-medium">{reference.title}</p>
                <p className="mt-1 font-mono text-xs text-muted-foreground">{reference.path} · {reference.referenceId}</p>
                <p className="mt-1 text-xs text-muted-foreground">{reference.summary}</p>
              </div>
            )) : <EmptyState>该 Skill 未声明 references。</EmptyState>}
          </div>
        </div>
        <pre className="max-h-96 overflow-auto rounded-lg border border-border bg-slate-50 p-3 text-xs">{skill.injectionContent}</pre>
      </CardContent>
    </Card>
  );
}

function DataBox({ label, value }: { label: string; value: string }) {
  return <div className="rounded-lg border border-border p-3"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 break-all text-sm">{value || "-"}</p></div>;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
