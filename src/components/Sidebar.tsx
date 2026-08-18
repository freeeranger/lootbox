import {
  Archive,
  ArchiveRestore,
  Bookmark,
  Box,
  Check,
  ChevronRight,
  ChevronsUpDown,
  Copy,
  File,
  FileArchive,
  FileCode2,
  Folder,
  FolderCog,
  FolderOpen,
  FolderPlus,
  Gamepad2,
  HeartPulse,
  Image,
  Keyboard,
  Layers3,
  Library,
  Music2,
  Plus,
  Pencil,
  RefreshCw,
  Search,
  Settings,
  Trash2,
  Type,
  Video,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn, collapseHomePath, sortByNatural } from "@/lib/utils";
import type {
  AssetType,
  LibrarySelection,
  LibrarySnapshot,
  PackSummary,
  ProjectSummary,
} from "../types";
import type { SavedAssetView } from "../savedViews";
import lootboxIcon from "../../src-tauri/icons/icon.svg";
import { memo, useMemo, useState } from "react";

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
  activeProjectId: number | null;
  activeProjectAttention: number;
  savedViews: SavedAssetView[];
  activeSavedViewId: string | null;
  onSelect: (selection: LibrarySelection) => void;
  onActivateProject: (project: ProjectSummary | null) => void;
  onRelocateProject: (project: ProjectSummary) => void;
  onOpenSavedView: (view: SavedAssetView) => void;
  onDeleteSavedView: (view: SavedAssetView) => void;
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
  if (current.kind === "all" || current.kind === "health" || current.kind === "duplicates") return true;
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
  count?: number;
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
        "relative w-full justify-start rounded-md px-2.5 text-xs font-normal text-muted-foreground transition-colors",
        description ? "h-auto min-h-10 py-1.5" : "h-8",
        active && "bg-sidebar-accent text-sidebar-accent-foreground font-medium before:absolute before:left-0 before:top-2 before:bottom-2 before:w-0.5 before:rounded-full before:bg-primary",
      )}
      onClick={onClick}
      title={title}
      aria-current={active ? "page" : undefined}
    >
      <Icon className={cn("size-3.5 shrink-0", warning && "text-destructive")} />
      <span className="min-w-0 flex-1 text-left">
        <span className="block truncate">{label}</span>
        {description && <span className="block truncate font-mono text-[11px] leading-4 text-muted-foreground">{description}</span>}
      </span>
      {count !== undefined && (
        <span className="font-mono text-xs text-muted-foreground tabular-nums">
          {count > 9999 ? "9k+" : count.toLocaleString()}
        </span>
      )}
    </Button>
  );
}

interface SectionHeaderProps {
  label: string;
  count?: number;
  collapsed?: boolean;
  onToggle?: () => void;
  action?: {
    icon: typeof Plus;
    label: string;
    onClick: () => void;
    disabled?: boolean;
  };
}

function SectionHeader({ label, count, collapsed, onToggle, action }: SectionHeaderProps) {
  return (
    <div className="mb-1 flex h-6 items-center justify-between px-2 text-[11px] font-medium text-muted-foreground select-none">
      {onToggle ? (
        <button
          type="button"
          onClick={onToggle}
          className="flex items-center gap-1 hover:text-foreground transition-colors cursor-pointer"
        >
          <ChevronRight className={cn("size-3 transition-transform duration-150", !collapsed && "rotate-90")} />
          <span className="uppercase tracking-wider">{label}</span>
          {count !== undefined && (
            <span className="font-mono text-[11px] text-muted-foreground/80 tabular-nums">({count})</span>
          )}
        </button>
      ) : (
        <span className="uppercase tracking-wider">{label}</span>
      )}

      {action && (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          className="size-5 rounded-sm text-muted-foreground hover:text-foreground"
          onClick={action.onClick}
          aria-label={action.label}
          title={action.label}
          disabled={action.disabled}
        >
          <action.icon className="size-3" />
        </Button>
      )}
    </div>
  );
}

function SidebarComponent({
  snapshot,
  selection,
  creatingCollection,
  activeProjectId,
  activeProjectAttention,
  savedViews,
  activeSavedViewId,
  onSelect,
  onActivateProject,
  onRelocateProject,
  onOpenSavedView,
  onDeleteSavedView,
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
  const [sidebarQuery, setSidebarQuery] = useState("");
  const [collapsedSections, setCollapsedSections] = useState<{
    packs?: boolean;
    collections?: boolean;
    types?: boolean;
    views?: boolean;
  }>({});

  const toggleSection = (key: keyof typeof collapsedSections) => {
    setCollapsedSections((current) => ({ ...current, [key]: !current[key] }));
  };

  const activeProject = useMemo(
    () => (activeProjectId !== null ? snapshot.projects.find((project) => project.id === activeProjectId) ?? null : null),
    [activeProjectId, snapshot.projects],
  );

  const normalizedSidebarQuery = sidebarQuery.trim().toLocaleLowerCase();
  const isFiltering = normalizedSidebarQuery.length > 0;

  const filteredPacks = useMemo(
    () =>
      sortByNatural(
        snapshot.packs.filter(
          (pack) =>
            !normalizedSidebarQuery ||
            `${pack.name} ${pack.rootPath}`.toLocaleLowerCase().includes(normalizedSidebarQuery),
        ),
        (pack) => pack.name,
      ),
    [normalizedSidebarQuery, snapshot.packs],
  );

  const filteredCollections = useMemo(
    () =>
      sortByNatural(
        snapshot.collections.filter(
          (collection) =>
            !normalizedSidebarQuery ||
            collection.name.toLocaleLowerCase().includes(normalizedSidebarQuery),
        ),
        (collection) => collection.name,
      ),
    [normalizedSidebarQuery, snapshot.collections],
  );

  const filteredSavedViews = useMemo(
    () =>
      sortByNatural(
        savedViews.filter(
          (view) =>
            !normalizedSidebarQuery ||
            view.name.toLocaleLowerCase().includes(normalizedSidebarQuery),
        ),
        (view) => view.name,
      ),
    [normalizedSidebarQuery, savedViews],
  );

  const sortedProjects = useMemo(
    () => sortByNatural(snapshot.projects, (project) => project.name),
    [snapshot.projects],
  );

  const showSidebarSearch = snapshot.packs.length + snapshot.collections.length + savedViews.length > 10;

  return (
    <aside className="flex h-full min-w-0 flex-col overflow-hidden border-r bg-sidebar text-sidebar-foreground">
      {/* Brand & Workspace Switcher */}
      <div className="shrink-0 space-y-2.5 px-3 pt-3 pb-2">
        <div className="flex h-7 items-center gap-2 px-1">
          <img src={lootboxIcon} alt="" className="size-5 rounded" />
          <p className="text-sm font-semibold tracking-[-0.01em]">Lootbox</p>
        </div>

        <DropdownMenu>
          <DropdownMenuTrigger className="group flex w-full items-center justify-between gap-2 rounded-lg border bg-sidebar-accent/30 p-2 text-left transition-colors hover:bg-sidebar-accent hover:border-border cursor-pointer focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring">
            <div className="flex min-w-0 items-center gap-2.5">
              <div className="flex size-7 shrink-0 items-center justify-center rounded-md border bg-background/80 text-primary shadow-xs">
                {activeProject ? (
                  <Gamepad2 className="size-3.5" />
                ) : (
                  <Library className="size-3.5 text-muted-foreground" />
                )}
              </div>
              <div className="min-w-0 flex-1">
                <p className="truncate text-xs font-semibold leading-tight text-foreground">
                  {activeProject ? activeProject.name : "Global Library"}
                </p>
                <p className="truncate font-mono text-[11px] text-muted-foreground leading-tight mt-0.5" title={activeProject ? collapseHomePath(activeProject.rootPath) : undefined}>
                  {activeProject ? collapseHomePath(activeProject.rootPath) : `${snapshot.totalAssets.toLocaleString()} assets`}
                </p>
              </div>
            </div>
            <ChevronsUpDown className="size-3.5 shrink-0 text-muted-foreground group-hover:text-foreground transition-colors" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-64 p-1 text-xs">
            <DropdownMenuGroup>
              <DropdownMenuLabel className="px-2 py-1 text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                Workspace
              </DropdownMenuLabel>
              <DropdownMenuItem
                className="flex items-center gap-2 px-2 py-1.5 cursor-pointer text-xs"
                onClick={() => onActivateProject(null)}
              >
                <Library className="size-3.5 text-muted-foreground" />
                <span className="flex-1">Global Library</span>
                {activeProjectId === null && <Check className="size-3.5 text-primary" />}
              </DropdownMenuItem>
            </DropdownMenuGroup>

            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuLabel className="px-2 py-1 text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                Godot Projects
              </DropdownMenuLabel>
              {sortedProjects.map((project) => (
                <DropdownMenuItem
                  key={project.id}
                  className="flex items-center gap-2 px-2 py-1.5 cursor-pointer text-xs"
                  onClick={() => onActivateProject(project)}
                >
                  <Gamepad2 className={cn("size-3.5 shrink-0", !project.available && "text-destructive", project.available && "text-primary")} />
                  <div className="min-w-0 flex-1 flex flex-col">
                    <span className="truncate">{project.name}</span>
                    <span className="truncate font-mono text-[11px] text-muted-foreground">{collapseHomePath(project.rootPath)}</span>
                  </div>
                  {activeProjectId === project.id && <Check className="size-3.5 shrink-0 text-primary" />}
                </DropdownMenuItem>
              ))}
              {snapshot.projects.length === 0 && (
                <p className="px-2 py-1 text-[11px] text-muted-foreground">No projects added yet.</p>
              )}
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              className="flex items-center gap-2 px-2 py-1.5 cursor-pointer text-xs text-muted-foreground hover:text-foreground"
              onClick={onAddProject}
            >
              <Plus className="size-3.5" />
              <span>Add Godot project…</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>

        {showSidebarSearch && (
          <div className="relative">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={sidebarQuery}
              onChange={(event) => setSidebarQuery(event.target.value)}
              className="h-7 bg-muted/15 pr-2 pl-7 text-xs"
              placeholder="Filter sidebar"
              aria-label="Filter packs, collections, and saved views"
            />
          </div>
        )}
      </div>

      {/* Main Navigation Scroll Area */}
      <nav className="quiet-scrollbar min-h-0 flex-1 space-y-4 overflow-y-auto px-2.5 py-2" aria-label="Asset library">
        {/* Project View (if active) or Global Catalog View */}
        {activeProject ? (
          <section>
            <SectionHeader label="Project" />
            <div className="space-y-0.5">
              <ContextMenu>
                <ContextMenuTrigger className="contents">
                  <NavItem
                    active={isSelected(selection, { kind: "project", projectId: activeProject.id })}
                    icon={activeProject.available ? Gamepad2 : FolderCog}
                    label="Project assets"
                    description={collapseHomePath(activeProject.rootPath)}
                    count={activeProject.assetCount}
                    warning={!activeProject.available}
                    onClick={() => onSelect({ kind: "project", projectId: activeProject.id })}
                  />
                </ContextMenuTrigger>
                <ContextMenuContent>
                  {activeProject.available ? (
                    <>
                      <ContextMenuItem onClick={() => onOpenProject(activeProject)}>
                        <FolderOpen /> Open project folder
                      </ContextMenuItem>
                    </>
                  ) : (
                    <ContextMenuItem onClick={() => onRelocateProject(activeProject)}>
                      <FolderCog /> Fix project location
                    </ContextMenuItem>
                  )}
                  <ContextMenuSeparator />
                  <ContextMenuItem variant="destructive" onClick={() => onForgetProject(activeProject)}>
                    <Trash2 /> Disconnect project…
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
              <NavItem
                active={isSelected(selection, { kind: "health" })}
                icon={HeartPulse}
                label="Project sync & health"
                count={activeProjectAttention}
                warning={activeProjectAttention > 0 || !activeProject.available}
                onClick={() => onSelect({ kind: "health" })}
              />
            </div>
          </section>
        ) : (
          <section>
            <SectionHeader label="Catalog" />
            <div className="space-y-0.5">
              <NavItem
                active={isSelected(selection, { kind: "all" })}
                icon={Library}
                label="All assets"
                count={snapshot.totalAssets}
                onClick={() => onSelect({ kind: "all" })}
              />
              <NavItem
                active={isSelected(selection, { kind: "health" })}
                icon={HeartPulse}
                label="Library health"
                count={snapshot.missingAssets + snapshot.removedAssets + snapshot.packs.filter((pack) => !pack.available).length + snapshot.projects.filter((project) => !project.available).length}
                warning={snapshot.missingAssets > 0 || snapshot.packs.some((pack) => !pack.available) || snapshot.projects.some((project) => !project.available)}
                onClick={() => onSelect({ kind: "health" })}
              />
              <NavItem
                active={isSelected(selection, { kind: "duplicates" })}
                icon={Copy}
                label="Duplicates"
                count={snapshot.duplicateAssets}
                title={snapshot.hashingAssets ? "Checking file contents…" : undefined}
                onClick={() => onSelect({ kind: "duplicates" })}
              />
            </div>
          </section>
        )}

        {/* Global Library browsing option when inside project */}
        {activeProject && (
          <section>
            <SectionHeader label="Library" />
            <div className="space-y-0.5">
              <NavItem
                active={isSelected(selection, { kind: "all" })}
                icon={Library}
                label="All library assets"
                count={snapshot.totalAssets}
                onClick={() => onSelect({ kind: "all" })}
              />
            </div>
          </section>
        )}

        {/* Packs Group */}
        <section>
          <SectionHeader
            label="Packs"
            count={snapshot.packs.length}
            collapsed={!isFiltering && collapsedSections.packs}
            onToggle={() => toggleSection("packs")}
            action={{
              icon: FolderPlus,
              label: "Import packs",
              onClick: onImport,
            }}
          />
          {(!collapsedSections.packs || isFiltering) && (
            <div className="space-y-0.5">
              {filteredPacks.map((pack) => {
                const candidate: LibrarySelection = { kind: "pack", packId: pack.id };
                return (
                  <ContextMenu key={pack.id}>
                    <ContextMenuTrigger className="contents">
                      <NavItem
                        active={isSelected(selection, candidate)}
                        icon={pack.available ? Folder : FolderCog}
                        label={pack.name}
                        count={pack.assetCount}
                        title={pack.available ? collapseHomePath(pack.rootPath) : `Missing · ${collapseHomePath(pack.rootPath)}`}
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
                      {pack.missingAssetCount > 0 && activeProjectId === null && (
                        <ContextMenuItem variant="destructive" onClick={() => onPurgeMissing(pack)}>
                          <Trash2 /> Purge missing records
                        </ContextMenuItem>
                      )}
                      {activeProjectId === null && (
                        <>
                          <ContextMenuSeparator />
                          <ContextMenuItem variant="destructive" onClick={() => onForgetPack(pack)}>
                            <Trash2 /> Forget pack
                          </ContextMenuItem>
                        </>
                      )}
                    </ContextMenuContent>
                  </ContextMenu>
                );
              })}
              {snapshot.packs.length === 0 && (
                <button
                  type="button"
                  className="w-full rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground hover:bg-sidebar-accent hover:text-foreground transition-colors cursor-pointer"
                  onClick={onImport}
                >
                  <span className="flex items-center gap-1.5">
                    <FolderPlus className="size-3.5" />
                    Import asset packs…
                  </span>
                </button>
              )}
            </div>
          )}
        </section>

        {/* Collections Group */}
        <section>
          <SectionHeader
            label="Collections"
            count={snapshot.collections.length}
            collapsed={!isFiltering && collapsedSections.collections}
            onToggle={() => toggleSection("collections")}
            action={{
              icon: Plus,
              label: "New collection",
              onClick: onStartCollection,
              disabled: creatingCollection,
            }}
          />
          {(!collapsedSections.collections || isFiltering) && (
            <div className="space-y-0.5">
              {filteredCollections.map((collection) => {
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
              {snapshot.collections.length === 0 && (
                <button
                  type="button"
                  className="w-full rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground hover:bg-sidebar-accent hover:text-foreground transition-colors cursor-pointer"
                  onClick={onStartCollection}
                  disabled={creatingCollection}
                >
                  <span className="flex items-center gap-1.5">
                    <Plus className="size-3.5" />
                    Create a collection…
                  </span>
                </button>
              )}
            </div>
          )}
        </section>



        {/* Saved Views (if any exist) */}
        {savedViews.length > 0 && (
          <section>
            <SectionHeader
              label="Saved Views"
              count={savedViews.length}
              collapsed={!isFiltering && collapsedSections.views}
              onToggle={() => toggleSection("views")}
            />
            {(!collapsedSections.views || isFiltering) && (
              <div className="space-y-0.5">
                {filteredSavedViews.map((view) => (
                  <ContextMenu key={view.id}>
                    <ContextMenuTrigger className="contents">
                      <NavItem active={activeSavedViewId === view.id} icon={Bookmark} label={view.name} onClick={() => onOpenSavedView(view)} />
                    </ContextMenuTrigger>
                    <ContextMenuContent>
                      <ContextMenuItem onClick={() => onOpenSavedView(view)}><Bookmark /> Open saved view</ContextMenuItem>
                      <ContextMenuSeparator />
                      <ContextMenuItem variant="destructive" onClick={() => onDeleteSavedView(view)}><Trash2 /> Delete saved view</ContextMenuItem>
                    </ContextMenuContent>
                  </ContextMenu>
                ))}
              </div>
            )}
          </section>
        )}
      </nav>

      {/* Footer */}
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

export const Sidebar = memo(SidebarComponent);
