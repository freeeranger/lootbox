import { Activity, FolderOpen, Gamepad2, History, RefreshCw, TriangleAlert, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { collapseHomePath } from "@/lib/utils";
import { memo, useState } from "react";
import type { ProjectStatus, ProjectSummary } from "../types";

function formatExportTime(value: string | null) {
  if (!value) return "Never exported";
  const date = new Date(`${value.replace(" ", "T")}Z`);
  return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

function ProjectWorkspaceBarComponent({
  project,
  status,
  loading,
  isProjectView,
  onOpen,
  onRefresh,
  onViewAssets,
  onClearTarget,
}: {
  project: ProjectSummary;
  status?: ProjectStatus | null;
  loading: boolean;
  isProjectView: boolean;
  onOpen: () => void;
  onRefresh: () => void;
  onViewAssets: () => void;
  onClearTarget?: () => void;
}) {
  const [historyOpen, setHistoryOpen] = useState(false);
  const attention = status
    ? status.sourceChangedFiles + status.sourceMissingFiles + status.projectModifiedFiles + status.projectMissingFiles
    : 0;

  return (
    <>
      <div className="flex min-h-9 shrink-0 items-center justify-between gap-3 border-b bg-sidebar/50 px-4 text-xs backdrop-blur-sm">
        <div className="flex min-w-0 items-center gap-2">
          <Gamepad2 className="size-3.5 shrink-0 text-primary" />
          {isProjectView ? (
            <span className="truncate font-mono text-[11px] text-muted-foreground" title={collapseHomePath(project.rootPath)}>
              {status?.destination ?? "res://assets/lootbox"}
            </span>
          ) : (
            <div className="flex min-w-0 items-center gap-1.5 truncate">
              <span className="text-[11px] text-muted-foreground">Export target:</span>
              <button
                type="button"
                className="truncate font-medium text-foreground hover:text-primary transition-colors cursor-pointer"
                onClick={onViewAssets}
                title={`View ${project.name} assets (${collapseHomePath(project.rootPath)})`}
              >
                {project.name}
              </button>
              <span className="hidden font-mono text-[11px] text-muted-foreground md:inline">
                ({status?.destination ?? "res://assets/lootbox"})
              </span>
            </div>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <span className="flex items-center gap-1 text-[11px] text-muted-foreground mr-1.5">
            {loading ? (
              <RefreshCw className="size-3 animate-spin" />
            ) : !project.available || !status || attention > 0 ? (
              <TriangleAlert className="size-3 text-destructive" />
            ) : (
              <Activity className="size-3 text-primary" />
            )}
            {loading
              ? "Checking…"
              : !project.available
              ? "Unavailable"
              : !status
              ? "Status unknown"
              : attention > 0
              ? `${attention.toLocaleString()} need attention`
              : `${status.upToDateFiles.toLocaleString()} current`}
          </span>

          {!isProjectView && (
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="h-6 rounded px-2 text-[11px] text-primary hover:text-primary mr-1"
              onClick={onViewAssets}
            >
              View project assets
            </Button>
          )}

          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="size-6 text-muted-foreground"
            onClick={() => setHistoryOpen(true)}
            aria-label="Export history"
            title="Export history"
          >
            <History className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="size-6 text-muted-foreground"
            onClick={onRefresh}
            aria-label="Refresh project status"
            title="Refresh project status"
          >
            <RefreshCw className="size-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            className="size-6 text-muted-foreground"
            onClick={onOpen}
            aria-label="Open project folder"
            title="Open project folder"
          >
            <FolderOpen className="size-3.5" />
          </Button>
          {onClearTarget && (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="size-6 text-muted-foreground hover:text-foreground"
              onClick={onClearTarget}
              aria-label="Clear export target"
              title="Clear active export target"
            >
              <X className="size-3.5" />
            </Button>
          )}
        </div>
      </div>

      <Dialog open={historyOpen} onOpenChange={setHistoryOpen}>
        <DialogContent className="gap-4 sm:max-w-2xl">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-base font-semibold">{project.name} export history</DialogTitle>
            <DialogDescription className="select-text truncate font-mono text-xs text-muted-foreground" title={collapseHomePath(project.rootPath)}>{collapseHomePath(project.rootPath)}</DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-2 gap-x-5 gap-y-2 min-w-0 w-full rounded-md border bg-muted/10 p-3.5 text-xs">
            <span className="text-muted-foreground">Tracked files</span><span className="font-mono tabular-nums">{status?.trackedFiles.toLocaleString() ?? "—"}</span>
            <span className="text-muted-foreground">Current</span><span className="font-mono tabular-nums">{status?.upToDateFiles.toLocaleString() ?? "—"}</span>
            <span className="text-muted-foreground">Source changed / missing</span><span className="font-mono tabular-nums">{status ? `${status.sourceChangedFiles} / ${status.sourceMissingFiles}` : "—"}</span>
            <span className="text-muted-foreground">Project modified / missing</span><span className="font-mono tabular-nums">{status ? `${status.projectModifiedFiles} / ${status.projectMissingFiles}` : "—"}</span>
            <span className="text-muted-foreground">Last export</span><span>{formatExportTime(status?.lastExportedAt ?? project.lastExportedAt)}</span>
          </div>
          <div className="min-w-0 w-full">
            <h3 className="mb-2 text-xs font-medium">Recent exports</h3>
            <div className="quiet-scrollbar max-h-72 min-w-0 w-full overflow-y-auto rounded-md border">
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

export const ProjectWorkspaceBar = memo(ProjectWorkspaceBarComponent);
