import {
  Archive,
  ArchiveRestore,
  Box,
  Copy,
  File,
  FileArchive,
  FileCode2,
  Folder,
  FolderCog,
  FolderOpen,
  FolderPlus,
  Gamepad2,
  Image,
  Keyboard,
  Layers3,
  Library,
  Music2,
  Plus,
  Pencil,
  RefreshCw,
  Settings,
  Trash2,
  Type,
  Video,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import type {
  AssetType,
  LibrarySelection,
  LibrarySnapshot,
  PackSummary,
  ProjectSummary,
} from "../types";
import lootboxIcon from "../../src-tauri/icons/icon.svg";

const typeMetadata: Record<AssetType, { label: string; icon: typeof Image }> = {
  image: { label: "Images", icon: Image },
  texture: { label: "Textures", icon: Image },
  model: { label: "Models", icon: Box },
  audio: { label: "Audio", icon: Music2 },
  video: { label: "Video", icon: Video },
  font: { label: "Fonts", icon: Type },
  shader: { label: "Shaders", icon: FileCode2 },
  material: { label: "Materials", icon: Layers3 },
  archive: { label: "Archives", icon: FileArchive },
  other: { label: "Other", icon: File },
};

interface Props {
  snapshot: LibrarySnapshot;
  selection: LibrarySelection;
  creatingCollection: boolean;
  onSelect: (selection: LibrarySelection) => void;
  onImport: () => void;
  onStartCollection: () => void;
  onRenamePack: (pack: PackSummary) => void;
  onRescanPack: (pack: PackSummary) => void;
  onOpenPack: (pack: PackSummary) => void;
  onRelocatePack: (pack: PackSummary) => void;
  onForgetPack: (pack: PackSummary) => void;
  onViewRemoved: (pack: PackSummary) => void;
  onViewMissing: (pack: PackSummary) => void;
  onPurgeMissing: (pack: PackSummary) => void;
  onAddProject: () => void;
  onOpenProject: (project: ProjectSummary) => void;
  onForgetProject: (project: ProjectSummary) => void;
  onSettings: () => void;
  onShortcuts: () => void;
}

function isSelected(current: LibrarySelection, candidate: LibrarySelection) {
  if (current.kind !== candidate.kind) return false;
  if (current.kind === "all" || current.kind === "duplicates") return true;
  if (current.kind === "type") {
    return current.assetType === (candidate as typeof current).assetType;
  }
  if (current.kind === "pack") {
    return current.packId === (candidate as typeof current).packId;
  }
  if (current.kind === "removed") {
    return current.packId ===
      (candidate as Extract<LibrarySelection, { kind: "removed" }>).packId;
  }
  if (current.kind === "missing") {
    return current.packId ===
      (candidate as Extract<LibrarySelection, { kind: "missing" }>).packId;
  }
  if (current.kind === "project") {
    return current.projectId ===
      (candidate as Extract<LibrarySelection, { kind: "project" }>).projectId;
  }
  return current.collectionId ===
    (candidate as Extract<LibrarySelection, { kind: "collection" }>).collectionId;
}

interface NavItemProps {
  active: boolean;
  icon: typeof Library;
  label: string;
  description?: string;
  count: number;
  title?: string;
  onClick: () => void;
  warning?: boolean;
}

function NavItem({ active, icon: Icon, label, description, count, title, onClick, warning }: NavItemProps) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      className={cn(
        "relative w-full justify-start rounded-md px-2.5 text-xs font-normal text-muted-foreground",
        description ? "h-auto min-h-10 py-1.5" : "h-8",
        active && "bg-sidebar-accent text-sidebar-accent-foreground",
      )}
      onClick={onClick}
      title={title}
      aria-current={active ? "page" : undefined}
    >
      <Icon className={cn("size-3.5", warning && "text-destructive")} />
      <span className="min-w-0 flex-1 text-left">
        <span className="block truncate">{label}</span>
        {description && <span className="block truncate font-mono text-[11px] leading-3 text-muted-foreground/70">{description}</span>}
      </span>
      <span className="font-mono text-[11px] text-muted-foreground/70">
        {count > 9999 ? "9k+" : count.toLocaleString()}
      </span>
    </Button>
  );
}

export function Sidebar({
  snapshot,
  selection,
  creatingCollection,
  onSelect,
  onImport,
  onStartCollection,
  onRenamePack,
  onRescanPack,
  onOpenPack,
  onRelocatePack,
  onForgetPack,
  onViewRemoved,
  onViewMissing,
  onPurgeMissing,
  onAddProject,
  onOpenProject,
  onForgetProject,
  onSettings,
  onShortcuts,
}: Props) {
  return (
    <aside className="flex min-w-0 flex-col overflow-hidden border-r bg-sidebar text-sidebar-foreground">
      <div className="shrink-0 px-3 pt-3 pb-2">
        <div className="flex h-9 items-center gap-2.5 px-1.5">
          <img src={lootboxIcon} alt="" className="size-6 rounded-md" />
          <p className="text-sm font-semibold tracking-[-0.01em]">Lootbox</p>
        </div>

        <Button
          type="button"
          size="sm"
          className="mt-3 h-8 w-full justify-start rounded-md px-2.5 text-xs font-medium"
          onClick={onImport}
        >
          <FolderPlus className="size-3.5" />
          Import packs
        </Button>
      </div>

      <nav className="quiet-scrollbar min-h-0 flex-1 space-y-5 overflow-y-auto px-2.5 py-3" aria-label="Asset library">
        <section>
          <h2 className="mb-1 px-2 text-[11px] font-medium text-muted-foreground">Library</h2>
          <div className="space-y-0.5">
            <NavItem
              active={isSelected(selection, { kind: "all" })}
              icon={Library}
              label="All assets"
              count={snapshot.totalAssets}
              onClick={() => onSelect({ kind: "all" })}
            />
            <NavItem
              active={isSelected(selection, { kind: "duplicates" })}
              icon={Copy}
              label="Duplicates"
              count={snapshot.duplicateAssets}
              title={snapshot.hashingAssets ? "Checking file contents…" : undefined}
              onClick={() => onSelect({ kind: "duplicates" })}
            />
            {snapshot.typeCounts.map(({ assetType, count }) => {
              const item = typeMetadata[assetType];
              const candidate: LibrarySelection = { kind: "type", assetType };
              return (
                <NavItem
                  key={assetType}
                  active={isSelected(selection, candidate)}
                  icon={item.icon}
                  label={item.label}
                  count={count}
                  onClick={() => onSelect(candidate)}
                />
              );
            })}
          </div>
        </section>

        <section>
          <div className="mb-1 flex h-6 items-center justify-between px-2">
            <h2 className="text-[11px] font-medium text-muted-foreground">Godot projects</h2>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="rounded-sm text-muted-foreground"
              onClick={onAddProject}
              aria-label="Add Godot project"
              title="Add Godot project"
            >
              <Plus className="size-3" />
            </Button>
          </div>
          <div className="space-y-0.5">
            {snapshot.projects.map((project) => {
              const candidate: LibrarySelection = { kind: "project", projectId: project.id };
              return (
                <ContextMenu key={project.id}>
                  <ContextMenuTrigger className="contents">
                    <NavItem
                      active={isSelected(selection, candidate)}
                      icon={project.available ? Gamepad2 : FolderCog}
                      label={project.name}
                      description={project.rootPath}
                      count={project.assetCount}
                      title={project.available ? project.rootPath : `Missing · ${project.rootPath}`}
                      warning={!project.available}
                      onClick={() => onSelect(candidate)}
                    />
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <ContextMenuItem
                      disabled={!project.available}
                      onClick={() => onOpenProject(project)}
                    >
                      <FolderOpen /> Open project folder
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem
                      variant="destructive"
                      onClick={() => onForgetProject(project)}
                    >
                      <Trash2 /> Forget project
                    </ContextMenuItem>
                  </ContextMenuContent>
                </ContextMenu>
              );
            })}
            {snapshot.projects.length === 0 && (
              <button type="button" className="w-full rounded-md px-2 py-2 text-left text-[11px] text-muted-foreground hover:bg-sidebar-accent hover:text-foreground" onClick={onAddProject}>
                Add a Godot project…
              </button>
            )}
          </div>
        </section>

        <section>
          <h2 className="mb-1 px-2 text-[11px] font-medium text-muted-foreground">Packs</h2>
          <div className="space-y-0.5">
            {snapshot.packs.map((pack) => {
              const candidate: LibrarySelection = { kind: "pack", packId: pack.id };
              return (
                <ContextMenu key={pack.id}>
                  <ContextMenuTrigger className="contents">
                    <NavItem
                      active={isSelected(selection, candidate)}
                      icon={pack.available ? Folder : FolderCog}
                      label={pack.name}
                      count={pack.assetCount}
                      title={pack.available ? pack.rootPath : `Missing · ${pack.rootPath}`}
                      warning={!pack.available}
                      onClick={() => onSelect(candidate)}
                    />
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    {pack.available ? (
                      <>
                        <ContextMenuItem onClick={() => onOpenPack(pack)}>
                          <FolderOpen /> Open folder
                        </ContextMenuItem>
                        <ContextMenuItem onClick={() => onRescanPack(pack)}>
                          <RefreshCw /> Rescan
                        </ContextMenuItem>
                      </>
                    ) : (
                      <ContextMenuItem onClick={() => onRelocatePack(pack)}>
                        <FolderCog /> Fix location
                      </ContextMenuItem>
                    )}
                    <ContextMenuItem onClick={() => onRenamePack(pack)}>
                      <Pencil /> Rename
                    </ContextMenuItem>
                    <ContextMenuItem
                      disabled={pack.removedAssetCount === 0}
                      onClick={() => onViewRemoved(pack)}
                    >
                      <ArchiveRestore /> Removed items
                      {pack.removedAssetCount > 0 && (
                        <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                          {pack.removedAssetCount}
                        </span>
                      )}
                    </ContextMenuItem>
                    <ContextMenuItem
                      disabled={pack.missingAssetCount === 0}
                      onClick={() => onViewMissing(pack)}
                    >
                      <FolderCog /> Missing items
                      {pack.missingAssetCount > 0 && (
                        <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                          {pack.missingAssetCount}
                        </span>
                      )}
                    </ContextMenuItem>
                    {pack.missingAssetCount > 0 && (
                      <ContextMenuItem variant="destructive" onClick={() => onPurgeMissing(pack)}>
                        <Trash2 /> Purge missing records
                      </ContextMenuItem>
                    )}
                    <ContextMenuSeparator />
                    <ContextMenuItem variant="destructive" onClick={() => onForgetPack(pack)}>
                      <Trash2 /> Forget pack
                    </ContextMenuItem>
                  </ContextMenuContent>
                </ContextMenu>
              );
            })}
          </div>
        </section>

        <section>
          <div className="mb-1 flex h-6 items-center justify-between px-2">
            <h2 className="text-[11px] font-medium text-muted-foreground">Collections</h2>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="rounded-sm text-muted-foreground"
              onClick={onStartCollection}
              aria-label="New collection"
              title="New collection"
              disabled={creatingCollection}
            >
              <Plus className="size-3" />
            </Button>
          </div>
          <div className="space-y-0.5">
            {snapshot.collections.map((collection) => {
              const candidate: LibrarySelection = {
                kind: "collection",
                collectionId: collection.id,
              };
              return (
                <NavItem
                  key={collection.id}
                  active={isSelected(selection, candidate)}
                  icon={Archive}
                  label={collection.name}
                  count={collection.assetCount}
                  onClick={() => onSelect(candidate)}
                />
              );
            })}
            {snapshot.collections.length === 0 && snapshot.totalAssets > 0 && (
              <button type="button" className="w-full rounded-md px-2 py-2 text-left text-[11px] text-muted-foreground hover:bg-sidebar-accent hover:text-foreground" onClick={onStartCollection}>
                Create a collection…
              </button>
            )}
          </div>
        </section>
      </nav>

      <footer className="flex shrink-0 items-center gap-1 border-t px-2.5 py-2">
        <Button type="button" variant="ghost" size="sm" className="h-8 flex-1 justify-start rounded-md px-2 text-xs font-normal text-muted-foreground" onClick={onSettings}>
          <Settings className="size-3.5" /> Maintenance
        </Button>
        <Button type="button" variant="ghost" size="icon-sm" className="rounded-md text-muted-foreground" onClick={onShortcuts} aria-label="Keyboard shortcuts" title="Keyboard shortcuts">
          <Keyboard className="size-3.5" />
        </Button>
      </footer>
    </aside>
  );
}
