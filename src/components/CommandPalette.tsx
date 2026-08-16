import {
  FolderArchive,
  Boxes,
  Bookmark,
  Gamepad2,
  Settings,
  Grid,
  List,
  Plus,
  RefreshCw,
  ShieldAlert,
  ArrowUpDown,
  DatabaseBackup,
  Music,
  Box,
  Image,
  Video,
  CornerDownLeft,
  X,
  PanelLeft,
  PanelRight,
  CheckSquare,
  Layers,
  HardDrive,
  HelpCircle,
} from "lucide-react";
import {
  CommandDialog,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
} from "@/components/ui/command";
import type {
  LibrarySnapshot,
  ProjectSummary,
  AssetType,
  LibrarySelection,
} from "@/types";
import type { SavedAssetView } from "@/savedViews";
import { Kbd } from "@/components/ui/kbd";
import { cn } from "@/lib/utils";

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  snapshot: LibrarySnapshot;
  activeProject: ProjectSummary | null;
  savedViews: SavedAssetView[];
  selectedCount: number;
  view: "grid" | "list";
  leftPanelCollapsed: boolean;
  rightPanelCollapsed: boolean;
  onSelectScope: (selection: LibrarySelection) => void;
  onOpenSavedView: (view: SavedAssetView) => void;
  onActivateProject: (project: ProjectSummary | null) => void;
  onImportPack: () => void;
  onStartCollection: () => void;
  onSaveCurrentView: () => void;
  onAddProject: () => void;
  onExportToActiveProject: () => void;
  onOpenSettings: () => void;
  onOpenShortcuts: () => void;
  onSetView: (view: "grid" | "list") => void;
  onToggleSidebar: () => void;
  onToggleDetailPanel: () => void;
  onSelectAll: () => void;
  onClearSelection: () => void;
  onSetFilterType: (type: AssetType | null) => void;
  onSetSort: (sort: string) => void;
  onCleanCache: () => void;
  onClearCache: () => void;
}

export function CommandPalette({
  open,
  onOpenChange,
  snapshot,
  activeProject,
  savedViews,
  selectedCount,
  view,
  leftPanelCollapsed,
  rightPanelCollapsed,
  onSelectScope,
  onOpenSavedView,
  onActivateProject,
  onImportPack,
  onStartCollection,
  onSaveCurrentView,
  onAddProject,
  onExportToActiveProject,
  onOpenSettings,
  onOpenShortcuts,
  onSetView,
  onToggleSidebar,
  onToggleDetailPanel,
  onSelectAll,
  onClearSelection,
  onSetFilterType,
  onSetSort,
  onCleanCache,
  onClearCache,
}: CommandPaletteProps) {
  const runCommand = (action: () => void) => {
    onOpenChange(false);
    action();
  };

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Command Palette"
      description="Search actions, packs, projects, and views"
    >
      <CommandInput placeholder="Type a command or search..." />
      <CommandList>
        <CommandEmpty>No matching commands found.</CommandEmpty>

        {/* Quick Actions */}
        <CommandGroup heading="Actions">
          {selectedCount > 0 && activeProject?.available && (
            <CommandItem
              keywords={["export", "send", "godot", "sync", "project", activeProject.name]}
              onSelect={() => runCommand(onExportToActiveProject)}
              className="gap-2.5"
            >
              <CornerDownLeft className="size-4 text-primary" />
              <span>
                Export {selectedCount.toLocaleString()} {selectedCount === 1 ? "asset" : "assets"} to{" "}
                <span className="text-foreground font-medium">{activeProject.name}</span>
              </span>
              <CommandShortcut>⌘E</CommandShortcut>
            </CommandItem>
          )}

          {selectedCount > 0 && (
            <CommandItem
              keywords={["collection", "group", "selection", "new", "create"]}
              onSelect={() => runCommand(onStartCollection)}
              className="gap-2.5"
            >
              <Plus className="size-4 text-muted-foreground" />
              <span>Create collection from selection ({selectedCount.toLocaleString()})</span>
              <CommandShortcut>⌘⇧C</CommandShortcut>
            </CommandItem>
          )}

          {selectedCount > 0 ? (
            <CommandItem
              keywords={["deselect", "clear", "unselect", "none"]}
              onSelect={() => runCommand(onClearSelection)}
              className="gap-2.5"
            >
              <X className="size-4 text-muted-foreground" />
              <span>Clear selection ({selectedCount.toLocaleString()})</span>
              <CommandShortcut>⌘⇧A</CommandShortcut>
            </CommandItem>
          ) : (
            <CommandItem
              keywords={["select", "all", "highlight", "batch"]}
              onSelect={() => runCommand(onSelectAll)}
              className="gap-2.5"
            >
              <CheckSquare className="size-4 text-muted-foreground" />
              <span>Select all assets</span>
              <CommandShortcut>⌘A</CommandShortcut>
            </CommandItem>
          )}

          <CommandItem
            keywords={["import", "add", "pack", "folder", "index", "scan"]}
            onSelect={() => runCommand(onImportPack)}
            className="gap-2.5"
          >
            <FolderArchive className="size-4 text-muted-foreground" />
            <span>Import asset pack...</span>
          </CommandItem>

          <CommandItem
            keywords={["godot", "project", "link", "add", "workspace"]}
            onSelect={() => runCommand(onAddProject)}
            className="gap-2.5"
          >
            <Gamepad2 className="size-4 text-muted-foreground" />
            <span>Add Godot project...</span>
          </CommandItem>

          <CommandItem
            keywords={["collection", "new", "create", "empty"]}
            onSelect={() => runCommand(onStartCollection)}
            className="gap-2.5"
          >
            <Boxes className="size-4 text-muted-foreground" />
            <span>New collection...</span>
          </CommandItem>

          <CommandItem
            keywords={["save", "view", "preset", "filter", "bookmark", "pin"]}
            onSelect={() => runCommand(onSaveCurrentView)}
            className="gap-2.5"
          >
            <Bookmark className="size-4 text-muted-foreground" />
            <span>Save current view...</span>
          </CommandItem>
        </CommandGroup>

        {/* Projects */}
        <CommandGroup heading="Projects">
          <CommandItem
            keywords={["global", "library", "clear", "target", "none", "all"]}
            onSelect={() => runCommand(() => onActivateProject(null))}
            className="gap-2.5"
          >
            <HardDrive className="size-4 text-muted-foreground" />
            <span>Global Library</span>
            {!activeProject && (
              <span className="ml-auto font-mono text-[11px] text-primary">Active</span>
            )}
          </CommandItem>

          {snapshot.projects.map((project) => {
            const isTarget = activeProject?.id === project.id;
            return (
              <CommandItem
                key={project.id}
                keywords={["project", "godot", project.name, project.rootPath]}
                onSelect={() =>
                  runCommand(() => {
                    onActivateProject(project);
                    onSelectScope({ kind: "project", projectId: project.id });
                  })
                }
                className="gap-2.5"
              >
                <Gamepad2 className={cn("size-4", isTarget ? "text-primary" : "text-muted-foreground")} />
                <span className="truncate">{project.name}</span>
                {isTarget ? (
                  <span className="ml-auto font-mono text-[11px] text-primary">Active</span>
                ) : (
                  <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                    {project.assetCount.toLocaleString()} exports
                  </span>
                )}
              </CommandItem>
            );
          })}
        </CommandGroup>

        {/* Packs */}
        {snapshot.packs.length > 0 && (
          <CommandGroup heading="Packs">
            {snapshot.packs.map((pack) => (
              <CommandItem
                key={pack.id}
                keywords={["pack", "folder", pack.name, pack.rootPath]}
                onSelect={() =>
                  runCommand(() => onSelectScope({ kind: "pack", packId: pack.id }))
                }
                className="gap-2.5"
              >
                <FolderArchive className="size-4 text-muted-foreground" />
                <span className="truncate">{pack.name}</span>
                <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                  {pack.assetCount.toLocaleString()} assets
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {/* Collections */}
        {snapshot.collections.length > 0 && (
          <CommandGroup heading="Collections">
            {snapshot.collections.map((col) => (
              <CommandItem
                key={col.id}
                keywords={["collection", col.name]}
                onSelect={() =>
                  runCommand(() => onSelectScope({ kind: "collection", collectionId: col.id }))
                }
                className="gap-2.5"
              >
                <Boxes className="size-4 text-muted-foreground" />
                <span className="truncate">{col.name}</span>
                <span className="ml-auto font-mono text-[11px] text-muted-foreground">
                  {col.assetCount.toLocaleString()}
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {/* Saved Views */}
        {savedViews.length > 0 && (
          <CommandGroup heading="Saved Views">
            {savedViews.map((viewItem) => (
              <CommandItem
                key={viewItem.id}
                keywords={["saved", "view", viewItem.name, viewItem.query || ""]}
                onSelect={() => runCommand(() => onOpenSavedView(viewItem))}
                className="gap-2.5"
              >
                <Bookmark className="size-4 text-muted-foreground" />
                <span className="truncate">{viewItem.name}</span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {/* Navigation & Ledgers */}
        <CommandGroup heading="Views & Ledgers">
          <CommandItem
            keywords={["all", "assets", "everything", "catalog", "library", "home"]}
            onSelect={() => runCommand(() => onSelectScope({ kind: "all" }))}
            className="gap-2.5"
          >
            <HardDrive className="size-4 text-muted-foreground" />
            <span>All Assets</span>
            <span className="ml-auto font-mono text-[11px] text-muted-foreground">
              {snapshot.totalAssets.toLocaleString()}
            </span>
          </CommandItem>

          <CommandItem
            keywords={["duplicates", "copies", "hash", "sha256", "redundant"]}
            onSelect={() => runCommand(() => onSelectScope({ kind: "duplicates" }))}
            className="gap-2.5"
          >
            <Layers className="size-4 text-muted-foreground" />
            <span>Duplicates</span>
            {snapshot.duplicateAssets > 0 && (
              <span className="ml-auto font-mono text-[11px] text-amber-400">
                {snapshot.duplicateAssets.toLocaleString()}
              </span>
            )}
          </CommandItem>

          <CommandItem
            keywords={["health", "diagnostics", "missing", "disconnected", "integrity", "status"]}
            onSelect={() => runCommand(() => onSelectScope({ kind: "health" }))}
            className="gap-2.5"
          >
            <ShieldAlert className="size-4 text-muted-foreground" />
            <span>Library Health & Diagnostics</span>
          </CommandItem>
        </CommandGroup>

        {/* Filter by Type */}
        <CommandGroup heading="Filter by Type">
          <CommandItem
            keywords={["3d", "models", "gltf", "glb", "obj", "fbx", "mesh", "poly"]}
            onSelect={() => runCommand(() => onSetFilterType("model"))}
            className="gap-2.5"
          >
            <Box className="size-4 text-muted-foreground" />
            <span>3D Models</span>
          </CommandItem>
          <CommandItem
            keywords={["textures", "pbr", "albedo", "normal", "roughness", "metallic", "height", "ao", "map"]}
            onSelect={() => runCommand(() => onSetFilterType("texture"))}
            className="gap-2.5"
          >
            <Layers className="size-4 text-muted-foreground" />
            <span>Textures</span>
          </CommandItem>
          <CommandItem
            keywords={["audio", "sfx", "sound", "music", "wav", "ogg", "mp3", "tracks"]}
            onSelect={() => runCommand(() => onSetFilterType("audio"))}
            className="gap-2.5"
          >
            <Music className="size-4 text-muted-foreground" />
            <span>Audio & SFX</span>
          </CommandItem>
          <CommandItem
            keywords={["images", "sprites", "2d", "png", "svg", "jpg", "picture"]}
            onSelect={() => runCommand(() => onSetFilterType("image"))}
            className="gap-2.5"
          >
            <Image className="size-4 text-muted-foreground" />
            <span>2D Images & Sprites</span>
          </CommandItem>
          <CommandItem
            keywords={["videos", "mp4", "webm", "sequences", "motion"]}
            onSelect={() => runCommand(() => onSetFilterType("video"))}
            className="gap-2.5"
          >
            <Video className="size-4 text-muted-foreground" />
            <span>Videos</span>
          </CommandItem>
          <CommandItem
            keywords={["clear", "reset", "all", "filter", "types"]}
            onSelect={() => runCommand(() => onSetFilterType(null))}
            className="gap-2.5 text-muted-foreground"
          >
            <X className="size-4" />
            <span>Clear type filter</span>
          </CommandItem>
        </CommandGroup>

        {/* Sort Order */}
        <CommandGroup heading="Sort">
          <CommandItem
            keywords={["sort", "name", "alphabetical", "a-z"]}
            onSelect={() => runCommand(() => onSetSort("name"))}
            className="gap-2.5"
          >
            <ArrowUpDown className="size-4 text-muted-foreground" />
            <span>Sort by Name (A–Z)</span>
          </CommandItem>
          <CommandItem
            keywords={["sort", "newest", "recent", "modified", "date"]}
            onSelect={() => runCommand(() => onSetSort("newest"))}
            className="gap-2.5"
          >
            <ArrowUpDown className="size-4 text-muted-foreground" />
            <span>Sort by Newest Modified</span>
          </CommandItem>
          <CommandItem
            keywords={["sort", "size", "largest", "file", "bytes", "mb"]}
            onSelect={() => runCommand(() => onSetSort("largest"))}
            className="gap-2.5"
          >
            <ArrowUpDown className="size-4 text-muted-foreground" />
            <span>Sort by File Size</span>
          </CommandItem>
          <CommandItem
            keywords={["sort", "type", "kind", "format", "category"]}
            onSelect={() => runCommand(() => onSetSort("type"))}
            className="gap-2.5"
          >
            <ArrowUpDown className="size-4 text-muted-foreground" />
            <span>Sort by Asset Type</span>
          </CommandItem>
        </CommandGroup>

        {/* Workspace & Controls */}
        <CommandGroup heading="Workspace">
          <CommandItem
            keywords={["layout", "view", "grid", "list", "toggle", "columns"]}
            onSelect={() => runCommand(() => onSetView(view === "grid" ? "list" : "grid"))}
            className="gap-2.5"
          >
            {view === "grid" ? <List className="size-4 text-muted-foreground" /> : <Grid className="size-4 text-muted-foreground" />}
            <span>{view === "grid" ? "Switch to List view" : "Switch to Grid view"}</span>
            <CommandShortcut>{view === "grid" ? "L" : "G"}</CommandShortcut>
          </CommandItem>

          <CommandItem
            keywords={["sidebar", "navigation", "left", "panel", "toggle", "hide", "show"]}
            onSelect={() => runCommand(onToggleSidebar)}
            className="gap-2.5"
          >
            <PanelLeft className="size-4 text-muted-foreground" />
            <span>{leftPanelCollapsed ? "Show sidebar" : "Hide sidebar"}</span>
            <CommandShortcut>⌘B</CommandShortcut>
          </CommandItem>

          <CommandItem
            keywords={["inspector", "details", "metadata", "preview", "right", "panel", "toggle", "hide", "show"]}
            onSelect={() => runCommand(onToggleDetailPanel)}
            className="gap-2.5"
          >
            <PanelRight className="size-4 text-muted-foreground" />
            <span>{rightPanelCollapsed ? "Show inspector" : "Hide inspector"}</span>
            <CommandShortcut>⌘I</CommandShortcut>
          </CommandItem>

          <CommandItem
            keywords={["shortcuts", "hotkeys", "cheat", "sheet", "keys", "help"]}
            onSelect={() => runCommand(onOpenShortcuts)}
            className="gap-2.5"
          >
            <HelpCircle className="size-4 text-muted-foreground" />
            <span>Keyboard shortcuts</span>
            <CommandShortcut>?</CommandShortcut>
          </CommandItem>
        </CommandGroup>

        {/* System & Cache */}
        <CommandGroup heading="System">
          <CommandItem
            keywords={["settings", "maintenance", "backup", "restore", "database", "safety"]}
            onSelect={() => runCommand(onOpenSettings)}
            className="gap-2.5"
          >
            <Settings className="size-4 text-muted-foreground" />
            <span>Maintenance & Backup settings...</span>
          </CommandItem>

          <CommandItem
            keywords={["cache", "clean", "thumbnails", "purge", "optimize", "stale", "orphaned"]}
            onSelect={() => runCommand(onCleanCache)}
            className="gap-2.5"
          >
            <RefreshCw className="size-4 text-muted-foreground" />
            <span>Clean thumbnail cache</span>
          </CommandItem>

          <CommandItem
            keywords={["cache", "clear", "erase", "reset", "thumbnails", "delete", "all"]}
            onSelect={() => runCommand(onClearCache)}
            className="gap-2.5 text-destructive"
          >
            <DatabaseBackup className="size-4 text-destructive" />
            <span>Clear all thumbnail cache...</span>
          </CommandItem>
        </CommandGroup>
      </CommandList>

      {/* Quiet Toolbench Footer */}
      <div className="flex items-center justify-between border-t border-border/60 bg-muted/20 px-4 py-2 text-[11px] text-muted-foreground">
        <span className="font-mono text-muted-foreground/60">Lootbox</span>
        <div className="flex items-center gap-3 font-mono text-[11px]">
          <span className="inline-flex items-center gap-1">
            <Kbd>↑↓</Kbd>
            <span>navigate</span>
          </span>
          <span className="inline-flex items-center gap-1">
            <Kbd>↵</Kbd>
            <span>select</span>
          </span>
          <span className="inline-flex items-center gap-1">
            <Kbd>esc</Kbd>
            <span>close</span>
          </span>
        </div>
      </div>
    </CommandDialog>
  );
}
