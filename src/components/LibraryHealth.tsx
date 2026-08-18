import {
  ArchiveRestore,
  Check,
  FolderCog,
  FolderOpen,
  Gamepad2,
  RefreshCw,
  TriangleAlert,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { collapseHomePath, sortByNatural } from "@/lib/utils";
import { memo, useMemo } from "react";
import type { LibrarySnapshot, ProjectStatus } from "../types";

interface Props {
  snapshot: LibrarySnapshot;
  activeProjectName?: string;
  projectStatus?: ProjectStatus | null;
  projectStatusLoading?: boolean;
  onViewMissing: () => void;
  onViewRemoved: () => void;
  onRelocatePack: (packId: number) => void;
  onRelocateProject: (projectId: number) => void;
  onViewProject: () => void;
  onRefreshProject: () => void;
}

function HealthRow({
  icon: Icon,
  title,
  detail,
  count,
  action,
  onAction,
  attention = false,
}: {
  icon: typeof Check;
  title: string;
  detail: string;
  count?: number;
  action?: string;
  onAction?: () => void;
  attention?: boolean;
}) {
  return (
    <div className="grid min-h-14 grid-cols-[24px_minmax(0,1fr)_auto] items-center gap-3 border-b px-4 py-2.5 last:border-b-0">
      <Icon className={attention ? "size-4 text-destructive" : "size-4 text-muted-foreground"} />
      <div className="min-w-0">
        <p className="text-xs font-medium">{title}</p>
        <p className="mt-0.5 truncate text-xs text-muted-foreground">{detail}</p>
      </div>
      <div className="flex items-center gap-3">
        {count !== undefined && <span className="font-mono text-xs tabular-nums text-muted-foreground">{count.toLocaleString()}</span>}
        {action && onAction && <Button type="button" variant="ghost" size="sm" onClick={onAction}>{action}</Button>}
      </div>
    </div>
  );
}

function LibraryHealthComponent({
  snapshot,
  activeProjectName,
  projectStatus,
  projectStatusLoading,
  onViewMissing,
  onViewRemoved,
  onRelocatePack,
  onRelocateProject,
  onViewProject,
  onRefreshProject,
}: Props) {
  const unavailablePacks = useMemo(
    () => sortByNatural(snapshot.packs.filter((pack) => !pack.available), (pack) => pack.name),
    [snapshot.packs],
  );
  const unavailableProjects = useMemo(
    () => sortByNatural(snapshot.projects.filter((project) => !project.available), (project) => project.name),
    [snapshot.projects],
  );
  const projectAttention = projectStatus
    ? projectStatus.sourceChangedFiles + projectStatus.sourceMissingFiles + projectStatus.projectModifiedFiles + projectStatus.projectMissingFiles
    : 0;
  const attentionCount = snapshot.missingAssets + unavailablePacks.length + unavailableProjects.length + projectAttention;


  return (
    <div className="quiet-scrollbar h-full overflow-y-auto p-5">
      <div className="mx-auto max-w-3xl">
        <header className="mb-5 flex items-start justify-between gap-4">
          <div>
            <h2 className="text-base font-semibold tracking-[-0.01em]">Library health</h2>
            <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
              {attentionCount === 0
                ? snapshot.removedAssets > 0
                  ? `Sources and project copies are in order. ${snapshot.removedAssets.toLocaleString()} removals remain available to restore.`
                  : "Sources, recoverable records, and the active project are in order."
                : `${attentionCount.toLocaleString()} records or locations need attention. Source files are never changed by repair actions.`}
            </p>
          </div>
          <span className={attentionCount > 0 ? "rounded-md bg-destructive/10 px-2 py-1 font-mono text-xs text-destructive" : "rounded-md bg-primary/10 px-2 py-1 font-mono text-xs text-primary"}>
            {attentionCount > 0 ? `${attentionCount.toLocaleString()} attention` : "Clear"}
          </span>
        </header>

        <section className="overflow-hidden rounded-md border bg-card/25">
          <HealthRow icon={FolderCog} title="Missing source files" detail="Indexed files no longer found at their source locations." count={snapshot.missingAssets} action={snapshot.missingAssets > 0 ? "Review" : undefined} onAction={onViewMissing} attention={snapshot.missingAssets > 0} />
          <HealthRow icon={ArchiveRestore} title="Recoverable removals" detail="Assets hidden from Lootbox that can be restored without reimporting." count={snapshot.removedAssets} action={snapshot.removedAssets > 0 ? "Review" : undefined} onAction={onViewRemoved} />
          <HealthRow icon={FolderOpen} title="Disconnected packs" detail="Pack roots that are unavailable or have moved." count={unavailablePacks.length} attention={unavailablePacks.length > 0} />
          <HealthRow icon={Gamepad2} title="Disconnected projects" detail="Registered Godot projects whose project.godot cannot be found." count={unavailableProjects.length} attention={unavailableProjects.length > 0} />
        </section>

        {unavailablePacks.length > 0 && (
          <section className="mt-6">
            <h3 className="mb-2 text-xs font-semibold">Reconnect packs</h3>
            <div className="overflow-hidden rounded-md border">
              {unavailablePacks.map((pack) => (
                <HealthRow key={pack.id} icon={FolderCog} title={pack.name} detail={collapseHomePath(pack.rootPath)} action="Locate" onAction={() => onRelocatePack(pack.id)} attention />
              ))}
            </div>
          </section>
        )}

        {unavailableProjects.length > 0 && (
          <section className="mt-6">
            <h3 className="mb-2 text-xs font-semibold">Reconnect projects</h3>
            <div className="overflow-hidden rounded-md border">
              {unavailableProjects.map((project) => (
                <HealthRow key={project.id} icon={Gamepad2} title={project.name} detail={collapseHomePath(project.rootPath)} action="Locate" onAction={() => onRelocateProject(project.id)} attention />
              ))}
            </div>
          </section>
        )}

        {activeProjectName && (
          <section className="mt-6">
            <div className="mb-2 flex items-center justify-between">
              <h3 className="text-xs font-semibold">{activeProjectName}</h3>
              <Button type="button" variant="ghost" size="xs" onClick={onRefreshProject} disabled={projectStatusLoading}>
                <RefreshCw className={projectStatusLoading ? "animate-spin" : ""} /> Refresh status
              </Button>
            </div>
            <div className="overflow-hidden rounded-md border">
              {projectStatusLoading && !projectStatus ? (
                <HealthRow icon={RefreshCw} title="Checking project copies" detail="Comparing tracked source and destination files." />
              ) : projectStatus ? (
                <>
                  <HealthRow icon={projectAttention > 0 ? TriangleAlert : Check} title={projectAttention > 0 ? "Project needs attention" : "Project copies are current"} detail={`${projectStatus.upToDateFiles.toLocaleString()} of ${projectStatus.trackedFiles.toLocaleString()} tracked files are current.`} count={projectAttention} action="Open project view" onAction={onViewProject} attention={projectAttention > 0} />
                  <HealthRow icon={FolderCog} title="Source changed or missing" detail="Re-export changed sources or reconnect missing packs." count={projectStatus.sourceChangedFiles + projectStatus.sourceMissingFiles} attention={projectStatus.sourceChangedFiles + projectStatus.sourceMissingFiles > 0} />
                  <HealthRow icon={Gamepad2} title="Project modified or missing" detail="Project edits are preserved; missing copies can be re-exported." count={projectStatus.projectModifiedFiles + projectStatus.projectMissingFiles} attention={projectStatus.projectModifiedFiles + projectStatus.projectMissingFiles > 0} />
                </>
              ) : null}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

export const LibraryHealth = memo(LibraryHealthComponent);
