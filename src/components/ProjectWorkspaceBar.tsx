import { Activity, FolderOpen, Gamepad2, History, RefreshCw, TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useState } from "react";
import type { ProjectStatus, ProjectSummary } from "../types";

function formatExportTime(value: string | null) {
  if (!value) return "Never exported";
  const date = new Date(`${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export function ProjectWorkspaceBar({
  project,
  status,
  loading,
  onOpen,
  onRefresh,
  onViewAssets,
}: {
  project: ProjectSummary;
  status?: ProjectStatus | null;
  loading: boolean;
  onOpen: () => void;
  onRefresh: () => void;
  onViewAssets: () => void;
}) {
  const [historyOpen, setHistoryOpen] = useState(false);
  const attention = status
    ? status.sourceChangedFiles + status.sourceMissingFiles + status.projectModifiedFiles + status.projectMissingFiles
    : 0;

  return (
    <>
      <div className="flex min-h-10 shrink-0 items-center gap-3 border-b bg-sidebar/45 px-4 text-xs">
        <Gamepad2 className="size-3.5 text-primary" />
        <button type="button" className="min-w-0 truncate font-medium hover:text-primary" onClick={onViewAssets} title={project.rootPath}>{project.name}</button>
        <span className="truncate font-mono text-xs text-muted-foreground">{status?.destination ?? "res://assets/lootbox"}</span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5 text-muted-foreground">
          {loading ? <RefreshCw className="size-3 animate-spin" /> : !project.available || !status || attention > 0 ? <TriangleAlert className="size-3 text-destructive" /> : <Activity className="size-3 text-primary" />}
          {loading ? "Checking…" : !project.available ? "Project unavailable" : !status ? "Status unavailable" : attention > 0 ? `${attention.toLocaleString()} need attention` : `${status.upToDateFiles.toLocaleString()} current`}
        </span>
        <Button type="button" variant="ghost" size="icon-xs" onClick={() => setHistoryOpen(true)} aria-label="Export history" title="Export history"><History /></Button>
        <Button type="button" variant="ghost" size="icon-xs" onClick={onRefresh} aria-label="Refresh project status" title="Refresh project status"><RefreshCw /></Button>
        <Button type="button" variant="ghost" size="icon-xs" onClick={onOpen} aria-label="Open project folder" title="Open project folder"><FolderOpen /></Button>
      </div>

      <Dialog open={historyOpen} onOpenChange={setHistoryOpen}>
        <DialogContent className="gap-4 sm:max-w-lg">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">{project.name} export history</DialogTitle>
            <DialogDescription className="select-text truncate font-mono text-xs" title={project.rootPath}>{project.rootPath}</DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-2 gap-x-5 gap-y-2 rounded-md border bg-muted/10 p-3 text-xs">
            <span className="text-muted-foreground">Tracked files</span><span className="font-mono tabular-nums">{status?.trackedFiles.toLocaleString() ?? "—"}</span>
            <span className="text-muted-foreground">Current</span><span className="font-mono tabular-nums">{status?.upToDateFiles.toLocaleString() ?? "—"}</span>
            <span className="text-muted-foreground">Source changed / missing</span><span className="font-mono tabular-nums">{status ? `${status.sourceChangedFiles} / ${status.sourceMissingFiles}` : "—"}</span>
            <span className="text-muted-foreground">Project modified / missing</span><span className="font-mono tabular-nums">{status ? `${status.projectModifiedFiles} / ${status.projectMissingFiles}` : "—"}</span>
            <span className="text-muted-foreground">Last export</span><span>{formatExportTime(status?.lastExportedAt ?? project.lastExportedAt)}</span>
          </div>
          <div>
            <h3 className="mb-2 text-xs font-medium">Recent exports</h3>
            <div className="quiet-scrollbar max-h-72 overflow-y-auto rounded-md border">
              {status?.runs.length ? status.runs.map((run) => (
                <div key={run.id} className="grid grid-cols-[minmax(0,1fr)_auto] gap-3 border-b px-3 py-2.5 text-xs last:border-b-0">
                  <div className="min-w-0">
                    <p className="font-medium">{run.selectedCount.toLocaleString()} selected · {(run.copiedCount + run.unchangedCount).toLocaleString()} files</p>
                    <p className="mt-0.5 text-muted-foreground">{formatExportTime(run.exportedAt)}{run.modelFormats.length ? ` · ${run.modelFormats.map((format) => format.toUpperCase()).join(", ")}` : ""}</p>
                  </div>
                  <span className="self-center font-mono text-xs tabular-nums text-muted-foreground">{run.copiedCount.toLocaleString()} copied</span>
                </div>
              )) : <p className="px-3 py-8 text-center text-xs text-muted-foreground">No exports recorded yet.</p>}
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
