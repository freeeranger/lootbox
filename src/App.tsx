import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useHotkeys } from "@tanstack/react-hotkeys";
import { Kbd } from "@/components/ui/kbd";
import {
  AlertCircle,
  Activity,
  ArchiveRestore,
  ArrowUpDown,
  BookmarkPlus,
  Box,
  Check,
  ChevronLeft,
  ChevronRight,
  ChevronsUpDown,
  Copy,
  DatabaseBackup,
  ExternalLink,
  File,
  FileArchive,
  FileCode2,
  FolderCog,
  FolderMinus,
  FolderOpen,
  FolderPlus,
  Gamepad2,
  HardDrive,
  Image,
  Layers3,
  SlidersHorizontal,
  Grid2X2,
  List,
  LoaderCircle,
  MoreHorizontal,
  Music2,
  Pencil,
  Plus,
  RefreshCw,
  Command,
  Search,
  SearchX,
  Trash2,
  Type,
  Video,
  X,
} from "lucide-react";
import { api } from "./api";
import { AssetCard } from "./components/AssetCard";
import { CommandPalette } from "./components/CommandPalette";
import { DetailPanel } from "./components/DetailPanel";
import { EmptyState } from "./components/EmptyState";
import { ImportStageRail } from "./components/QuietAcknowledgment";
import { Sidebar } from "./components/Sidebar";
import { LibraryHealth } from "./components/LibraryHealth";
import { ProjectWorkspaceBar } from "./components/ProjectWorkspaceBar";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Popover,
  PopoverContent,
  PopoverHeading,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn, collapseHomePath } from "@/lib/utils";
import { toggleAudioPlayback } from "./audioPlayback";

const EMPTY_PATHS: string[] = [];
import { godotExportCompletionCopy } from "./godotExportCompletion";
import { readProjectModelFormats, writeProjectModelFormats } from "./godotExportPreferences";
import { readSavedViews, resolveSavedSelection, writeSavedViews } from "./savedViews";
import type { SavedAssetView, SavedViewFilters } from "./savedViews";
import { assetListRowHeight, isAssetKeyboardTarget } from "./workspaceShortcuts";
import type {
  Asset,
  AssetQuery,
  AssetSort,
  AssetSortDirection,
  AssetType,
  ImportProgress,
  FilterOptions,
  GodotExportPreview,
  GodotExportResult,
  GodotProjectRemovalPreview,
  LibrarySelection,
  LibrarySnapshot,
  PackSummary,
  ProjectSummary,
} from "./types";

const emptySnapshot: LibrarySnapshot = {
  totalAssets: 0,
  duplicateAssets: 0,
  removedAssets: 0,
  missingAssets: 0,
  hashingAssets: false,
  packs: [],
  collections: [],
  projects: [],
  typeCounts: [],
};
const emptyFilterOptions: FilterOptions = { extensions: [], mapRoles: [], tags: [] };

const assetPageSize = 160;
const searchDebounceMs = 300;
const guardedBulkEditThreshold = 10;
const clearedFilters: SavedViewFilters = { type: "", extension: "", mapRole: "", tag: "", minWidth: "", minConfidence: "", status: "", projectUsage: "" };
type AssetFilters = SavedViewFilters;

const typeLabels: Record<AssetType, string> = {
  image: "Images",
  texture: "Textures",
  audio: "Audio",
  model: "Models",
  video: "Video",
  font: "Fonts",
  shader: "Shaders",
  material: "Materials",
  archive: "Archives",
  other: "Other",
};

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

const sortLabels: Record<AssetSort, string> = {
  name: "Name",
  newest: "Date modified",
  largest: "File size",
  type: "Type",
};

function sortDirectionLabel(sort: AssetSort, direction: AssetSortDirection) {
  if (sort === "name") return direction === "asc" ? "A–Z" : "Z–A";
  if (sort === "newest") return direction === "desc" ? "Newest first" : "Oldest first";
  if (sort === "largest") return direction === "desc" ? "Largest first" : "Smallest first";
  return direction === "asc" ? "Type A–Z" : "Type Z–A";
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${unit}`;
}

function savedPanelWidth(key: string, fallback: number, minimum: number, maximum: number) {
  const value = Number(window.localStorage.getItem(key));
  return Number.isFinite(value) && value >= minimum && value <= maximum ? value : fallback;
}

function isTypingTarget(target: EventTarget | null) {
  return target instanceof HTMLElement &&
    (target.matches("input, textarea, select") ||
      target.isContentEditable ||
      Boolean(target.closest("[data-slot=dialog-content], [data-slot=alert-dialog-content], [data-slot=select-content], [data-slot=dropdown-menu-content], [data-slot=context-menu-content]")));
}

function AssetSearch({
  inputRef,
  value,
  onValueChange,
  onQueryChange,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  value: string;
  onValueChange: (value: string) => void;
  onQueryChange: (query: string) => void;
}) {
  useEffect(() => {
    const timer = window.setTimeout(
      () => onQueryChange(value.trim()),
      searchDebounceMs,
    );
    return () => window.clearTimeout(timer);
  }, [onQueryChange, value]);

  function clear() {
    onValueChange("");
    onQueryChange("");
    inputRef.current?.focus();
  }

  return (
    <div className="relative w-full max-w-xl">
      <Search className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <Input
        ref={inputRef}
        value={value}
        onChange={(event) => onValueChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            if (value) {
              clear();
            } else {
              inputRef.current?.blur();
            }
          }
        }}
        placeholder="Search names, paths, packs, and tags"
        aria-label="Search assets"
        className="h-9 rounded-md bg-muted/20 pr-8 pl-9 text-xs"
      />
      {value && (
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="absolute top-1/2 right-1 size-6 -translate-y-1/2 rounded-sm text-muted-foreground"
          onClick={clear}
          aria-label="Clear search"
        >
          <X />
        </Button>
      )}
    </div>
  );
}

function FilterSelect({
  label,
  value,
  placeholder,
  options,
  onValueChange,
  className,
}: {
  label: string;
  value: string;
  placeholder: string;
  options: Array<{ value: string; label: string }>;
  onValueChange: (value: string) => void;
  className?: string;
}) {
  const items = [{ value: "__all", label: placeholder }, ...options];
  return (
    <label className={cn("block min-w-0", className)}>
      <span className="mb-1.5 block text-[11px] font-medium text-muted-foreground">{label}</span>
      <Select
        items={items}
        value={value || "__all"}
        onValueChange={(next) => onValueChange(next === "__all" || next === null ? "" : next)}
      >
        <SelectTrigger aria-label={label} className="w-full text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false} align="start">
          <SelectGroup>
            {items.map((item) => (
              <SelectItem key={item.value} value={item.value} className="text-xs">
                {item.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    </label>
  );
}

function MultiFilterSelect({
  label,
  value,
  placeholder,
  options,
  onValueChange,
  className,
}: {
  label: string;
  value: string;
  placeholder: string;
  options: Array<{ value: string; label: string }>;
  onValueChange: (value: string) => void;
  className?: string;
}) {
  const [filterQuery, setFilterQuery] = useState("");
  const selectedValues = useMemo(
    () => (value ? value.split(",").map((s) => s.trim()).filter(Boolean) : []),
    [value],
  );

  const filteredOptions = useMemo(() => {
    if (!filterQuery.trim()) return options;
    const q = filterQuery.toLowerCase();
    return options.filter((opt) => opt.label.toLowerCase().includes(q) || opt.value.toLowerCase().includes(q));
  }, [options, filterQuery]);

  const toggleValue = (val: string) => {
    let next: string[];
    if (selectedValues.includes(val)) {
      next = selectedValues.filter((v) => v !== val);
    } else {
      next = [...selectedValues, val];
    }
    onValueChange(next.join(","));
  };

  if (options.length === 0) return null;

  return (
    <div className={cn("block min-w-0", className)}>
      <div className="mb-1 flex items-center justify-between">
        <span className="text-[11px] font-medium text-muted-foreground">{label}</span>
        {selectedValues.length > 0 && (
          <button
            type="button"
            onClick={() => onValueChange("")}
            className="text-[11px] text-muted-foreground hover:text-foreground cursor-pointer"
          >
            Clear ({selectedValues.length})
          </button>
        )}
      </div>
      {options.length > 6 && (
        <input
          type="text"
          value={filterQuery}
          onChange={(e) => setFilterQuery(e.target.value)}
          placeholder={`Search ${label.toLowerCase()}...`}
          className="mb-1 h-5 w-full rounded-xs border border-border/60 bg-background/80 px-1.5 text-[11px] text-foreground placeholder:text-muted-foreground/60 focus:border-primary/60 focus:outline-none"
        />
      )}
      <div className="quiet-scrollbar max-h-24 overflow-y-auto rounded-sm border border-border/60 bg-muted/10 p-0.5 space-y-0.5">
        {filteredOptions.length === 0 ? (
          <div className="px-1.5 py-1 text-[11px] text-muted-foreground">No matches</div>
        ) : (
          filteredOptions.map((option) => {
            const isChecked = selectedValues.includes(option.value);
            return (
              <button
                key={option.value}
                type="button"
                className={cn(
                  "flex h-5 w-full items-center justify-between rounded-xs px-1.5 text-[11px] text-left transition-colors cursor-pointer",
                  isChecked
                    ? "bg-primary/20 text-foreground font-medium"
                    : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                )}
                onClick={() => toggleValue(option.value)}
              >
                <span className="truncate">{option.label}</span>
                {isChecked && <Check className="size-3 text-primary shrink-0 ml-1" />}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}

function App() {
  const queryClient = useQueryClient();
  const [selection, setSelection] = useState<LibrarySelection>({ kind: "all" });
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(() => new Set());
  const [searchValue, setSearchValue] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [activeProjectId, setActiveProjectId] = useState<number | null>(() => {
    const saved = Number(window.localStorage.getItem("lootbox:active-project"));
    return Number.isInteger(saved) && saved > 0 ? saved : null;
  });
  const [savedViews, setSavedViews] = useState<SavedAssetView[]>(readSavedViews);
  const [activeSavedViewId, setActiveSavedViewId] = useState<string | null>(null);
  const [savingView, setSavingView] = useState(false);
  const [savedViewName, setSavedViewName] = useState("");
  const [view, setView] = useState<"grid" | "list">(() => window.localStorage.getItem("lootbox:asset-view") === "list" ? "list" : "grid");
  const [sort, setSort] = useState<AssetSort>(() => {
    const saved = window.localStorage.getItem("lootbox:asset-sort");
    return saved === "newest" || saved === "largest" || saved === "type"
      ? saved
      : "name";
  });
  const [sortDirection, setSortDirection] = useState<AssetSortDirection>(() => {
    const saved = window.localStorage.getItem("lootbox:asset-sort-direction");
    if (saved === "asc" || saved === "desc") return saved;
    const savedSort = window.localStorage.getItem("lootbox:asset-sort");
    return savedSort === "newest" || savedSort === "largest" ? "desc" : "asc";
  });
  const [importing, setImporting] = useState(false);
  const [pendingImportCount, setPendingImportCount] = useState(0);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [errorContext, setErrorContext] = useState("ui");
  const [notice, setNotice] = useState<string | null>(null);
  const [godotExportNotice, setGodotExportNotice] = useState<{
    project: ProjectSummary;
    result: GodotExportResult;
  } | null>(null);
  const [undoRemoval, setUndoRemoval] = useState<{ ids: number[]; label: string } | null>(null);
  const [metadataUndo, setMetadataUndo] = useState<{ label: string; run: () => Promise<void> } | null>(null);
  const [creatingCollection, setCreatingCollection] = useState(false);
  const [addSelectionToNewCollection, setAddSelectionToNewCollection] = useState(false);
  const [collectionName, setCollectionName] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [confirmAssetRemoval, setConfirmAssetRemoval] = useState<Asset[]>([]);
  const [confirmProjectRemoval, setConfirmProjectRemoval] = useState<ProjectSummary | null>(null);
  const [confirmPurge, setConfirmPurge] = useState<PackSummary | null>(null);
  const [pendingRestorePath, setPendingRestorePath] = useState<string | null>(null);
  const [confirmClearCache, setConfirmClearCache] = useState(false);
  const [pendingBulkMutation, setPendingBulkMutation] = useState<{
    title: string;
    description: string;
    run: () => Promise<void>;
  } | null>(null);
  const [editingSelection, setEditingSelection] = useState(false);
  const [godotExport, setGodotExport] = useState<{
    project: ProjectSummary;
    ids: number[];
    preview: GodotExportPreview | null;
    selectedModelFormats: string[];
    loading: boolean;
    exporting: boolean;
  } | null>(null);
  const [godotProjectRemoval, setGodotProjectRemoval] = useState<{
    project: ProjectSummary;
    ids: number[];
    preview: GodotProjectRemovalPreview | null;
    loading: boolean;
    removing: boolean;
  } | null>(null);
  const [renamingPack, setRenamingPack] = useState<PackSummary | null>(null);
  const [packName, setPackName] = useState("");
  const [filters, setFilters] = useState<AssetFilters>({ ...clearedFilters });
  const [filterDraft, setFilterDraft] = useState<AssetFilters>({ ...clearedFilters });
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [reviewSelectionOpen, setReviewSelectionOpen] = useState(false);
  const [reviewSelectionLimit, setReviewSelectionLimit] = useState(250);
  const [settingsMessage, setSettingsMessage] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const tagInputRef = useRef<HTMLInputElement>(null);
  const assetScrollRef = useRef<HTMLDivElement>(null);
  const selectedIdsRef = useRef<Set<number>>(new Set());
  const selectedIdRef = useRef<number | null>(null);
  const selectedAssetCacheRef = useRef<Map<number, Asset>>(new Map());
  const selectedPathCacheRef = useRef<Map<number, string>>(new Map());
  const assetsRef = useRef<Asset[]>([]);
  const selectionAnchorRef = useRef<number | null>(null);
  const pendingImportCountRef = useRef(0);
  const importJobsRef = useRef(new Set<string>());
  const godotPreviewRequestRef = useRef(0);
  const [assetViewportWidth, setAssetViewportWidth] = useState(0);
  const [windowWidth, setWindowWidth] = useState(window.innerWidth);
  const [leftPanelWidth, setLeftPanelWidth] = useState(() =>
    savedPanelWidth(
      "lootbox:left-panel-width",
      window.innerWidth >= 1280 ? 220 : 208,
      168,
      320,
    ),
  );
  const [leftPanelCollapsed, setLeftPanelCollapsed] = useState(() =>
    window.localStorage.getItem("lootbox:left-panel-collapsed") === "true",
  );
  const [rightPanelWidth, setRightPanelWidth] = useState(() =>
    savedPanelWidth(
      "lootbox:right-panel-width",
      window.innerWidth >= 1280 ? 340 : 320,
      260,
      480,
    ),
  );
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(() =>
    window.localStorage.getItem("lootbox:right-panel-collapsed") === "true",
  );

  const query = useMemo<AssetQuery>(() => {
    const next: AssetQuery = { query: debouncedSearch, sort, sortDirection };
    if (selection.kind === "type") next.assetType = selection.assetType;
    if (filters.type) next.assetType = filters.type as AssetType;
    if (selection.kind === "pack") next.packId = selection.packId;
    if (selection.kind === "removed") {
      if (selection.packId !== undefined) next.packId = selection.packId;
      next.excluded = true;
    }
    if (selection.kind === "missing") {
      if (selection.packId !== undefined) next.packId = selection.packId;
      next.missing = true;
    }
    if (selection.kind === "collection") next.collectionId = selection.collectionId;
    if (selection.kind === "duplicates") next.duplicatesOnly = true;
    if (selection.kind === "project") next.projectId = selection.projectId;
    if (filters.extension) next.extension = filters.extension;
    if (filters.mapRole) next.mapRole = filters.mapRole;
    if (filters.tag) next.tag = filters.tag;
    if (filters.minWidth) {
      next.minWidth = Number(filters.minWidth);
      next.minHeight = Number(filters.minWidth);
    }
    if (filters.minConfidence) next.minConfidence = Number(filters.minConfidence);
    if (filters.status === "missing") next.missing = true;
    if (filters.projectUsage === "active" && activeProjectId !== null) next.projectId = activeProjectId;
    if (filters.projectUsage === "unused") next.unusedByProjects = true;
    return next;
  }, [activeProjectId, debouncedSearch, filters, selection, sort, sortDirection]);
  const selectionScopeKey = useMemo(
    () => JSON.stringify({ debouncedSearch, filters, selection }),
    [debouncedSearch, filters, selection],
  );
  const previousSelectionScopeRef = useRef(selectionScopeKey);
  const selectionScopeKeyRef = useRef(selectionScopeKey);
  selectionScopeKeyRef.current = selectionScopeKey;

  const snapshotQuery = useQuery({
    queryKey: ["library-snapshot"],
    queryFn: api.snapshot,
    initialData: emptySnapshot,
    refetchInterval: (current) => current.state.data?.hashingAssets ? 1000 : false,
  });
  const filterOptionsQuery = useQuery({
    queryKey: ["filter-options"],
    queryFn: api.filterOptions,
    initialData: emptyFilterOptions,
  });
  const cacheStatusQuery = useQuery({
    queryKey: ["cache-status"],
    queryFn: api.cacheStatus,
    enabled: settingsOpen,
  });
  const diagnosticsQuery = useQuery({
    queryKey: ["diagnostics"],
    queryFn: api.diagnostics,
    enabled: settingsOpen,
  });
  const assetPagesQuery = useInfiniteQuery({
    queryKey: ["assets", query],
    queryFn: ({ pageParam }) => api.assets({
      ...query,
      limit: assetPageSize,
      offset: pageParam,
    }),
    initialPageParam: 0,
    getNextPageParam: (lastPage, pages) => lastPage.hasMore
      ? pages.reduce((total, page) => total + page.items.length, 0)
      : undefined,
    placeholderData: (previous) => previous,
    enabled: selection.kind !== "health",
  });
  const snapshot = snapshotQuery.data;
  const activeProject = useMemo(
    () => activeProjectId === null ? null : snapshot.projects.find((project) => project.id === activeProjectId) ?? null,
    [activeProjectId, snapshot.projects],
  );
  const projectStatusQuery = useQuery({
    queryKey: ["project-status", activeProjectId],
    queryFn: () => api.projectStatus(activeProjectId!),
    enabled: activeProjectId !== null && Boolean(activeProject?.available),
    staleTime: 30_000,
  });
  const activeProjectAttention = projectStatusQuery.data
    ? projectStatusQuery.data.sourceChangedFiles + projectStatusQuery.data.sourceMissingFiles + projectStatusQuery.data.projectModifiedFiles + projectStatusQuery.data.projectMissingFiles
    : 0;
  const filterOptions = filterOptionsQuery.data;
  const cacheStatus = cacheStatusQuery.data ?? null;
  const diagnostics = diagnosticsQuery.data ?? [];
  const assets = useMemo(
    () => assetPagesQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [assetPagesQuery.data],
  );
  const assetTotal = assetPagesQuery.data?.pages[0]?.total ?? 0;
  const hasMoreAssets = Boolean(assetPagesQuery.hasNextPage);
  const loadingMore = assetPagesQuery.isFetchingNextPage;
  const loading = assetPagesQuery.isPending ||
    (assetPagesQuery.isFetching && !assetPagesQuery.isFetchingNextPage);

  useEffect(() => {
    if (activeProjectId === null || snapshotQuery.isPending) return;
    if (!snapshot.projects.some((project) => project.id === activeProjectId)) {
      setActiveProjectId(null);
      window.localStorage.removeItem("lootbox:active-project");
      if (selection.kind === "project") setSelection({ kind: "all" });
    }
  }, [activeProjectId, selection.kind, snapshot.projects, snapshotQuery.isPending]);

  assetsRef.current = assets;
  const assetsById = useMemo(() => {
    const map = new Map<number, Asset>();
    for (const asset of assets) {
      map.set(asset.id, asset);
    }
    return map;
  }, [assets]);

  const selectedAsset = useMemo(() => {
    if (selectedId === null) return null;
    return assetsById.get(selectedId) ??
      selectedAssetCacheRef.current.get(selectedId) ?? null;
  }, [assetsById, selectedId]);

  const selectedAssets = useMemo(() => {
    const result: Asset[] = [];
    for (const id of selectedIds) {
      const asset = assetsById.get(id) ?? selectedAssetCacheRef.current.get(id);
      if (asset) result.push(asset);
    }
    return result;
  }, [assetsById, selectedIds]);

  const selectedDragPaths = useMemo(
    () => [...selectedIds].flatMap((id) => {
      const path = selectedPathCacheRef.current.get(id);
      return path ? [path] : [];
    }),
    [selectedIds],
  );
  const layoutLeftPanelWidth = leftPanelCollapsed ? 0 : Math.min(leftPanelWidth, windowWidth < 1100 ? 200 : 320);
  const layoutRightPanelWidth = rightPanelCollapsed ? 0 : Math.min(
    rightPanelWidth,
    Math.max(260, windowWidth - layoutLeftPanelWidth - 480),
  );

  const applyAssetSelection = useCallback((ids: Set<number>, activeId: number | null) => {
    if (ids.size === 0) {
      selectedAssetCacheRef.current.clear();
      selectedPathCacheRef.current.clear();
      selectedIdsRef.current = ids;
      selectedIdRef.current = null;
      setSelectedIds(ids);
      setSelectedId(null);
      return;
    }
    const nextCache = new Map(
      [...selectedAssetCacheRef.current].filter(([id]) => ids.has(id)),
    );
    const nextPathCache = new Map(
      [...selectedPathCacheRef.current].filter(([id]) => ids.has(id)),
    );
    for (const asset of assetsRef.current) {
      if (ids.has(asset.id)) {
        nextCache.set(asset.id, asset);
        nextPathCache.set(asset.id, asset.absolutePath);
      }
    }
    selectedAssetCacheRef.current = nextCache;
    selectedPathCacheRef.current = nextPathCache;
    selectedIdsRef.current = ids;
    selectedIdRef.current = activeId;
    setSelectedIds(ids);
    setSelectedId(activeId);
  }, []);

  const clearAssetSelection = useCallback(() => {
    selectionAnchorRef.current = null;
    selectedAssetCacheRef.current.clear();
    selectedPathCacheRef.current.clear();
    applyAssetSelection(new Set(), null);
  }, [applyAssetSelection]);
  const activateProject = useCallback((project: ProjectSummary | null) => {
    setActiveProjectId(project?.id ?? null);
    setActiveSavedViewId(null);
    if (project) {
      window.localStorage.setItem("lootbox:active-project", String(project.id));
      setSelection({ kind: "project", projectId: project.id });
    } else {
      window.localStorage.removeItem("lootbox:active-project");
      setSelection((current) => current.kind === "project" ? { kind: "all" } : current);
    }
    clearAssetSelection();
  }, [clearAssetSelection]);
  const selectedPack = useMemo(
    () =>
      selection.kind === "pack" || selection.kind === "removed" || selection.kind === "missing"
        ? snapshot.packs.find((pack) => pack.id === selection.packId) ?? null
        : null,
    [selection, snapshot.packs],
  );
  const selectedProject = useMemo(
    () => selection.kind === "project"
      ? snapshot.projects.find((project) => project.id === selection.projectId) ?? null
      : null,
    [selection, snapshot.projects],
  );
  const activeFilters = useMemo(() => {
    const labels: Record<keyof typeof filters, (value: string) => string> = {
      type: (value) => `Type: ${typeLabels[value as AssetType] ?? value}`,
      extension: (value) => {
        const parts = value.split(",").map((s) => `.${s.trim()}`).filter((s) => s.length > 1);
        return parts.length > 2 ? `Format: ${parts.length} formats` : `Format ${parts.join(", ")}`;
      },
      mapRole: (value) => {
        const parts = value.split(",").map((s) => s.trim().replaceAll("_", " ")).filter(Boolean);
        return parts.length > 2 ? `Maps: ${parts.length} roles` : `Map ${parts.join(", ")}`;
      },
      tag: (value) => {
        const parts = value.split(",").map((s) => s.trim()).filter(Boolean);
        return parts.length > 2 ? `Tags: ${parts.length} tags` : `Tag ${parts.join(", ")}`;
      },
      minWidth: (value) => `${value} × ${value}+`,
      minConfidence: (value) => `Confidence ≤ ${value}%`,
      status: () => "Missing files",
      projectUsage: (value) => value === "active" ? `In ${activeProject?.name ?? "active project"}` : "Not used by projects",
    };
    return (Object.entries(filters) as Array<[keyof typeof filters, string]>)
      .filter(([key, value]) => Boolean(value) && key !== "type")
      .map(([key, value]) => ({ key, label: labels[key](value) }));
  }, [activeProject?.name, filters]);

  const dynamicTypeCounts = useMemo(() => {
    const isScoped = selection.kind !== "all" || debouncedSearch.trim().length > 0 || Boolean(filters.extension || filters.mapRole || filters.tag);
    if (!isScoped) {
      return {
        total: snapshot.totalAssets,
        counts: new Map(snapshot.typeCounts.map((tc) => [tc.assetType, tc.count])),
        isScoped: false,
      };
    }
    const counts = new Map<string, number>();
    for (const a of assets) {
      counts.set(a.assetType, (counts.get(a.assetType) ?? 0) + 1);
    }
    const totalCount = selection.kind === "pack" && selectedPack
      ? selectedPack.assetCount
      : selection.kind === "project" && selectedProject
      ? selectedProject.assetCount
      : assetTotal;
    return {
      total: totalCount,
      counts,
      isScoped: true,
    };
  }, [selection.kind, debouncedSearch, filters.extension, filters.mapRole, filters.tag, snapshot.totalAssets, snapshot.typeCounts, assets, selectedPack, selectedProject, assetTotal]);

  const selectionSummary = useMemo(() => {
    if (selectedIds.size === 0) return "";
    if (selectedAssets.length !== selectedIds.size) {
      return `${selectedIds.size.toLocaleString()} selected · includes unloaded results`;
    }
    const typeCount = new Set(selectedAssets.map((asset) => asset.assetType)).size;
    const packCount = new Set(selectedAssets.map((asset) => asset.packId)).size;
    return `${selectedIds.size.toLocaleString()} selected · ${typeCount} ${typeCount === 1 ? "type" : "types"} · ${packCount} ${packCount === 1 ? "pack" : "packs"}`;
  }, [selectedAssets, selectedIds.size]);

  const reportError = useCallback((caught: unknown, context = "ui") => {
    const message = errorMessage(caught);
    setError(message);
    setErrorContext(context);
    void api.logDiagnostic("error", context, message);
  }, []);
  const loadSnapshot = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: ["library-snapshot"], exact: true });
  }, [queryClient]);

  useEffect(() => {
    void api.hashLibrary().then(loadSnapshot).catch((caught) => reportError(caught, "content-hashing"));
  }, [loadSnapshot, reportError]);

  const serverQueryError = snapshotQuery.error ?? filterOptionsQuery.error ??
    assetPagesQuery.error ?? projectStatusQuery.error ?? cacheStatusQuery.error ?? diagnosticsQuery.error;
  useEffect(() => {
    if (serverQueryError) reportError(serverQueryError, "server-query");
  }, [reportError, serverQueryError]);

  useEffect(() => {
    if ((!notice && !godotExportNotice) || error) return;
    const timer = window.setTimeout(() => {
      setNotice(null);
      setGodotExportNotice(null);
      setUndoRemoval(null);
      setMetadataUndo(null);
    }, undoRemoval || metadataUndo ? 10_000 : godotExportNotice ? 8_000 : 4_000);
    return () => window.clearTimeout(timer);
  }, [error, godotExportNotice, metadataUndo, notice, undoRemoval]);

  const refresh = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["library-snapshot"] }),
      queryClient.invalidateQueries({ queryKey: ["assets"] }),
      queryClient.invalidateQueries({ queryKey: ["filter-options"] }),
      queryClient.invalidateQueries({ queryKey: ["project-status"] }),
    ]);
  }, [queryClient]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:left-panel-width", String(leftPanelWidth));
  }, [leftPanelWidth]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:left-panel-collapsed", String(leftPanelCollapsed));
  }, [leftPanelCollapsed]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:right-panel-width", String(rightPanelWidth));
  }, [rightPanelWidth]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:right-panel-collapsed", String(rightPanelCollapsed));
  }, [rightPanelCollapsed]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:asset-sort", sort);
  }, [sort]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:asset-sort-direction", sortDirection);
  }, [sortDirection]);

  useEffect(() => {
    window.localStorage.setItem("lootbox:asset-view", view);
  }, [view]);

  useEffect(() => {
    writeSavedViews(savedViews);
  }, [savedViews]);

  function clampPanelWidth(side: "left" | "right", width: number) {
    const otherWidth = side === "left" ? rightPanelWidth : leftPanelWidth;
    const minimum = side === "left" ? 168 : 260;
    const preferredMaximum = side === "left" ? 320 : 480;
    const availableMaximum = window.innerWidth - otherWidth - 368;
    return Math.max(minimum, Math.min(width, preferredMaximum, availableMaximum));
  }

  function startPanelResize(side: "left" | "right", event: React.PointerEvent) {
    event.preventDefault();
    const previousCursor = document.body.style.cursor;
    const previousSelection = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    const move = (nextEvent: PointerEvent) => {
      const width = side === "left" ? nextEvent.clientX : window.innerWidth - nextEvent.clientX;
      if (side === "left") {
        setLeftPanelCollapsed(false);
        setLeftPanelWidth(clampPanelWidth(side, width));
      } else {
        setRightPanelCollapsed(false);
        setRightPanelWidth(clampPanelWidth(side, width));
      }
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousSelection;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }

  function resizePanelWithKeyboard(side: "left" | "right", event: React.KeyboardEvent) {
    const minimum = side === "left" ? 168 : 260;
    const maximum = side === "left" ? 320 : 480;
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    if (side === "left" && leftPanelCollapsed) setLeftPanelCollapsed(false);
    if (side === "right" && rightPanelCollapsed) setRightPanelCollapsed(false);
    if (event.key === "Home" || event.key === "End") {
      const width = event.key === "Home" ? minimum : clampPanelWidth(side, maximum);
      if (side === "left") setLeftPanelWidth(width);
      else setRightPanelWidth(width);
      return;
    }
    const direction = event.key === "ArrowRight" ? 1 : -1;
    if (side === "left") {
      setLeftPanelWidth((width) => clampPanelWidth(side, width + direction * 12));
    } else {
      setRightPanelWidth((width) => clampPanelWidth(side, width - direction * 12));
    }
  }

  useEffect(() => {
    const scopeChanged = previousSelectionScopeRef.current !== selectionScopeKey;
    previousSelectionScopeRef.current = selectionScopeKey;
    if (scopeChanged && selectedIdsRef.current.size > 1) clearAssetSelection();
  }, [clearAssetSelection, selectionScopeKey]);

  useEffect(() => {
    setError(null);
    assetScrollRef.current?.scrollTo({ top: 0 });
  }, [query]);

  useEffect(() => {
    if (assetPagesQuery.isFetching || assetPagesQuery.isPlaceholderData || selectedId === null) {
      return;
    }
    const visibleMatch = assets.find((asset) => asset.id === selectedId);
    if (visibleMatch) {
      selectedAssetCacheRef.current.set(visibleMatch.id, visibleMatch);
      return;
    }

    let active = true;
    void queryClient.fetchQuery({
      queryKey: ["asset-match", query, selectedId],
      queryFn: () => api.assets({ ...query, assetId: selectedId, limit: 1, offset: 0 }),
      staleTime: 0,
      gcTime: 0,
    }).then((page) => {
      if (!active) return;
      const match = page.items[0];
      if (!match) clearAssetSelection();
      else selectedAssetCacheRef.current.set(match.id, match);
    }).catch((caught) => {
      if (active) reportError(caught, "selected-asset-query");
    });
    return () => {
      active = false;
    };
  }, [assetPagesQuery.isFetching, assetPagesQuery.isPlaceholderData, assets, clearAssetSelection, query, queryClient, reportError, selectedId]);

  const viewportObserverRef = useRef<ResizeObserver | null>(null);
  const setAssetScrollRef = useCallback((element: HTMLDivElement | null) => {
    assetScrollRef.current = element;
    viewportObserverRef.current?.disconnect();
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      setAssetViewportWidth(entry.contentRect.width);
    });
    observer.observe(element);
    viewportObserverRef.current = observer;
    setAssetViewportWidth(element.clientWidth);
  }, []);

  useEffect(() => {
    return () => viewportObserverRef.current?.disconnect();
  }, []);

  useEffect(() => {
    const updateWindowWidth = () => setWindowWidth(window.innerWidth);
    window.addEventListener("resize", updateWindowWidth);
    return () => window.removeEventListener("resize", updateWindowWidth);
  }, []);

  const loadMoreAssets = useCallback(async () => {
    if (!assetPagesQuery.hasNextPage || assetPagesQuery.isFetchingNextPage || assetPagesQuery.isPlaceholderData) return;
    await assetPagesQuery.fetchNextPage();
  }, [assetPagesQuery.fetchNextPage, assetPagesQuery.hasNextPage, assetPagesQuery.isFetchingNextPage, assetPagesQuery.isPlaceholderData]);

  useHotkeys([
    {
      hotkey: "Mod+K",
      callback: () => setCommandPaletteOpen((open) => !open),
    },
    {
      hotkey: "Mod+B",
      callback: () => setLeftPanelCollapsed((current) => !current),
    },
    {
      hotkey: "Mod+I",
      callback: () => setRightPanelCollapsed((current) => !current),
    },
    {
      hotkey: "Mod+Alt+B",
      callback: () => setRightPanelCollapsed((current) => !current),
    },
    {
      hotkey: "Mod+F",
      callback: () => searchRef.current?.focus(),
    },
    {
      hotkey: "Mod+Shift+F",
      callback: () => setFiltersOpen((open) => !open),
    },
    {
      hotkey: "Mod+E",
      callback: () => {
        if (selectedIdsRef.current.size > 0) {
          if (activeProject?.available) void addSelectionToGodot(activeProject.id);
          else setNotice("Select an available workspace project before exporting");
        }
      },
    },
    {
      hotkey: "Mod+Shift+C",
      callback: () => {
        if (selectedIdsRef.current.size > 0) {
          setAddSelectionToNewCollection(true);
          setCreatingCollection(true);
        }
      },
    },
    {
      hotkey: "Mod+Shift+A",
      callback: () => clearAssetSelection(),
    },
    {
      hotkey: "Shift+/" as any,
      callback: () => setShortcutsOpen(true),
    },
    {
      hotkey: "/",
      callback: () => setShortcutsOpen(true),
    },
    {
      hotkey: "G",
      callback: (event) => {
        if (isAssetKeyboardTarget(event.target)) setView("grid");
      },
    },
    {
      hotkey: "L",
      callback: (event) => {
        if (isAssetKeyboardTarget(event.target)) setView("list");
      },
    },
    {
      hotkey: "T",
      callback: (event) => {
        if (isAssetKeyboardTarget(event.target) && selectedIdsRef.current.size > 0) {
          tagInputRef.current?.focus();
        }
      },
    },
  ]);

  async function runQueuedImport(path: string) {
    const jobId = crypto.randomUUID();
    importJobsRef.current.add(jobId);
    pendingImportCountRef.current += 1;
    setPendingImportCount(pendingImportCountRef.current);
    setImporting(true);
    try {
      return await api.importPack(path, jobId, (progress) => {
        setImportProgress(progress);
      });
    } finally {
      importJobsRef.current.delete(jobId);
      pendingImportCountRef.current -= 1;
      setPendingImportCount(pendingImportCountRef.current);
      if (pendingImportCountRef.current === 0) {
        setImporting(false);
        setImportProgress(null);
      }
    }
  }

  async function cancelImports() {
    await Promise.all([...importJobsRef.current].map((jobId) => api.cancelImport(jobId)));
  }


  async function importPack() {
    try {
      const selected = await open({
        directory: true,
        multiple: true,
        title: "Import folders",
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      setError(null);
      const results = await Promise.allSettled(paths.map(runQueuedImport));
      const imported = results.flatMap((result) =>
        result.status === "fulfilled" ? [result.value] : [],
      );
      const failures = results.flatMap((result) =>
        result.status === "rejected" && !errorMessage(result.reason).includes("Import cancelled")
          ? [errorMessage(result.reason)]
          : [],
      );
      const lastPack = imported.at(-1);
      if (lastPack) setSelection({ kind: "pack", packId: lastPack.id });
      await loadSnapshot();
      if (failures.length > 0) {
        reportError(
          failures.length === 1
            ? failures[0]
            : `${failures.length} folders could not be imported: ${failures.join("; ")}`,
          "import-pack",
        );
      } else if (imported.length > 0) {
        setNotice(imported.length === 1 ? `${imported[0].name} imported` : `${imported.length} packs imported`);
      }
    } catch (caught) {
      reportError(caught, "import-picker");
    }
  }

  async function createCollection(event: React.FormEvent) {
    event.preventDefault();
    const name = collectionName.trim();
    if (!name) return;
    try {
      const collection = await api.createCollection(name);
      if (addSelectionToNewCollection && selectedIdsRef.current.size > 0) {
        await api.setCollectionMemberships([...selectedIdsRef.current], collection.id, true);
      }
      setCollectionName("");
      setCreatingCollection(false);
      setAddSelectionToNewCollection(false);
      await loadSnapshot();
      if (addSelectionToNewCollection) {
        setNotice(`Added ${selectedIdsRef.current.size} assets to ${collection.name}`);
        await refresh();
      } else {
        setSelection({ kind: "collection", collectionId: collection.id });
      }
    } catch (caught) {
      reportError(caught, "collection-create");
    }
  }

  async function addGodotProject(preserveAssetSelection = false) {
    const path = await open({
      directory: true,
      multiple: false,
      title: "Select the folder containing project.godot",
    });
    if (!path) return;
    try {
      setError(null);
      const project = await api.addGodotProject(path);
      await loadSnapshot();
      if (!preserveAssetSelection) {
        setActiveProjectId(project.id);
        window.localStorage.setItem("lootbox:active-project", String(project.id));
        setSelection({ kind: "project", projectId: project.id });
        clearAssetSelection();
      }
      return project;
    } catch (caught) {
      reportError(caught, "add-godot-project");
    }
  }

  async function forgetGodotProject(project: ProjectSummary) {
    try {
      await api.removeProject(project.id);
      if (selection.kind === "project" && selection.projectId === project.id) {
        setSelection({ kind: "all" });
        clearAssetSelection();
      }
      if (activeProjectId === project.id) {
        setActiveProjectId(null);
        window.localStorage.removeItem("lootbox:active-project");
      }
      setConfirmProjectRemoval(null);
      await loadSnapshot();
    } catch (caught) {
      reportError(caught, "forget-godot-project");
    }
  }

  async function purgeMissingRecords(pack: PackSummary) {
    try {
      await api.purgeMissingAssets(pack.id);
      setConfirmPurge(null);
      await refresh();
    } catch (caught) {
      reportError(caught, "purge-missing");
    }
  }

  async function restoreSelectedBackup() {
    if (!pendingRestorePath) return;
    try {
      await api.restoreBackup(pendingRestorePath);
      setPendingRestorePath(null);
      setSettingsMessage("Backup restored successfully");
      await refresh();
    } catch (caught) {
      reportError(caught, "backup-restore");
    }
  }

  async function clearAllPreviews() {
    try {
      const status = await api.clearCache();
      queryClient.setQueryData(["cache-status"], status);
      const module = await import("./components/ModelCardPreview");
      module.resetModelPreviewCache();
      setSettingsMessage("Generated previews cleared. They will rebuild as assets appear.");
      setConfirmClearCache(false);
      await refresh();
    } catch (caught) {
      reportError(caught, "cache-clear");
    }
  }

  async function addSelectionToGodot(projectId: number, projectName?: string, projectRootPath?: string) {
    const ids = selectedIds.size > 0
      ? [...selectedIds]
      : selectedAsset ? [selectedAsset.id] : [];
    if (ids.length === 0) return;
    const project = snapshot.projects.find((item) => item.id === projectId) ?? {
      id: projectId,
      name: projectName ?? "Godot project",
      rootPath: projectRootPath ?? "",
      assetCount: 0,
      available: true,
      lastExportedAt: null,
    };
    setError(null);
    setNotice(null);
    setGodotExportNotice(null);
    setUndoRemoval(null);
    const savedModelFormats = readProjectModelFormats(projectId);
    const requestId = ++godotPreviewRequestRef.current;
    setGodotExport({ project, ids, preview: null, selectedModelFormats: savedModelFormats ?? [], loading: true, exporting: false });
    try {
      const preview = await api.previewAssetsToGodot(projectId, ids, savedModelFormats);
      if (requestId !== godotPreviewRequestRef.current) return;
      if (savedModelFormats) writeProjectModelFormats(projectId, preview.selectedModelFormats);
      setGodotExport((current) => current && current.project.id === projectId
        ? { ...current, preview, selectedModelFormats: preview.selectedModelFormats, loading: false }
        : current);
    } catch (caught) {
      if (requestId !== godotPreviewRequestRef.current) return;
      setGodotExport((current) => current && current.project.id === projectId
        ? { ...current, loading: false }
        : current);
      reportError(caught, "godot-export-preview");
    }
  }

  async function updateGodotModelFormat(extension: string, included: boolean) {
    if (!godotExport?.preview || godotExport.loading || godotExport.exporting) return;
    const previousFormats = godotExport.selectedModelFormats;
    const nextFormats = included
      ? [...new Set([...previousFormats, extension])].sort()
      : previousFormats.filter((format) => format !== extension);
    if (nextFormats.length === 0) return;
    const { project, ids } = godotExport;
    const requestId = ++godotPreviewRequestRef.current;
    setGodotExport((current) => current
      ? { ...current, selectedModelFormats: nextFormats, loading: true }
      : current);
    try {
      const preview = await api.previewAssetsToGodot(project.id, ids, nextFormats);
      if (requestId !== godotPreviewRequestRef.current) return;
      writeProjectModelFormats(project.id, preview.selectedModelFormats);
      setGodotExport((current) => current && current.project.id === project.id
        ? { ...current, preview, selectedModelFormats: preview.selectedModelFormats, loading: false }
        : current);
    } catch (caught) {
      if (requestId !== godotPreviewRequestRef.current) return;
      setGodotExport((current) => current && current.project.id === project.id
        ? { ...current, selectedModelFormats: previousFormats, loading: false }
        : current);
      reportError(caught, "godot-export-preview");
    }
  }

  async function confirmGodotExport() {
    if (!godotExport?.preview || godotExport.exporting) return;
    setGodotExport((current) => current ? { ...current, exporting: true } : current);
    try {
      const result = await api.exportAssetsToGodot(
        godotExport.project.id,
        godotExport.ids,
        godotExport.selectedModelFormats,
      );
      setGodotExportNotice({ project: godotExport.project, result });
      setGodotExport(null);
      void refresh().catch((caught) => {
        void api.logDiagnostic("error", "library-refresh", errorMessage(caught));
      });
    } catch (caught) {
      setGodotExport((current) => current ? { ...current, exporting: false } : current);
      reportError(caught, "godot-export");
    }
  }

  const mutateSelected = useCallback(async (mutation: () => Promise<void>) => {
    if (editingSelection) return false;
    setEditingSelection(true);
    try {
      await mutation();
      await refresh();
      return true;
    } catch (caught) {
      reportError(caught, "asset-edit");
      return false;
    } finally {
      setEditingSelection(false);
    }
  }, [editingSelection, refresh, reportError]);

  const guardedBulkMutation = useCallback((
    title: string,
    description: string,
    mutation: () => Promise<void>,
  ) => {
    if (selectedIdsRef.current.size >= guardedBulkEditThreshold) {
      setPendingBulkMutation({ title, description, run: async () => { await mutateSelected(mutation); } });
      return Promise.resolve();
    }
    return mutateSelected(mutation).then(() => undefined);
  }, [mutateSelected]);

  async function deleteCurrentSource() {
    try {
      if (selection.kind === "pack" || selection.kind === "removed" || selection.kind === "missing") {
        if (selection.packId === undefined) return;
        await api.removePack(selection.packId);
      }
      else if (selection.kind === "collection") {
        await api.deleteCollection(selection.collectionId);
      } else return;
      setConfirmDelete(false);
      setSelection({ kind: "all" });
      clearAssetSelection();
      await loadSnapshot();
    } catch (caught) {
      reportError(caught, "source-delete");
    }
  }

  async function rescanPack(pack: PackSummary) {
    try {
      setError(null);
      await runQueuedImport(pack.rootPath);
      await refresh();
    } catch (caught) {
      reportError(caught, "rescan-pack");
    }
  }

  function startRenamePack(pack: PackSummary) {
    setRenamingPack(pack);
    setPackName(pack.name);
  }

  async function renamePack(event: React.FormEvent) {
    event.preventDefault();
    if (!renamingPack || !packName.trim()) return;
    try {
      await api.renamePack(renamingPack.id, packName.trim());
      setRenamingPack(null);
      await loadSnapshot();
    } catch (caught) {
      reportError(caught, "rename-pack");
    }
  }

  async function relocatePack(pack: PackSummary) {
    const path = await open({
      directory: true,
      multiple: false,
      title: `Locate ${pack.name}`,
    });
    if (!path) return;
    try {
      setError(null);
      await api.relocatePack(pack.id, path);
      await refresh();
    } catch (caught) {
      reportError(caught, "relocate-pack");
    }
  }

  async function relocateGodotProject(project: ProjectSummary) {
    const path = await open({
      directory: true,
      multiple: false,
      title: `Locate ${project.name}`,
    });
    if (!path) return;
    try {
      setError(null);
      const relocated = await api.relocateGodotProject(project.id, path);
      await refresh();
      setNotice(`${relocated.name} reconnected`);
    } catch (caught) {
      reportError(caught, "relocate-project");
    }
  }

  function requestForgetPack(pack: PackSummary) {
    setSelection({ kind: "pack", packId: pack.id });
    clearAssetSelection();
    setConfirmDelete(true);
  }

  async function removeAssetFromLootbox() {
    if (confirmAssetRemoval.length === 0) return;
    const removed = [...confirmAssetRemoval];
    try {
      await api.setAssetsExcluded(removed.map((asset) => asset.id), true);
      clearAssetSelection();
      setConfirmAssetRemoval([]);
      setUndoRemoval({
        ids: removed.map((asset) => asset.id),
        label: removed.length === 1 ? removed[0].name : `${removed.length} assets`,
      });
      setNotice(removed.length === 1 ? `${removed[0].name} removed` : `${removed.length} assets removed`);
      await refresh();
    } catch (caught) {
      reportError(caught, "remove-asset");
    }
  }

  const sectionTitle = useMemo(() => {
    if (selection.kind === "type") return typeLabels[selection.assetType];
    if (selection.kind === "pack") {
      return snapshot.packs.find((pack) => pack.id === selection.packId)?.name ?? "Pack";
    }
    if (selection.kind === "removed") {
      if (selection.packId === undefined) return "Removed assets";
      const name = snapshot.packs.find((pack) => pack.id === selection.packId)?.name ?? "Pack";
      return `${name} · Removed`;
    }
    if (selection.kind === "missing") {
      if (selection.packId === undefined) return "Missing files";
      const name = snapshot.packs.find((pack) => pack.id === selection.packId)?.name ?? "Pack";
      return `${name} · Missing`;
    }
    if (selection.kind === "collection") {
      return (
        snapshot.collections.find((item) => item.id === selection.collectionId)?.name ??
        "Collection"
      );
    }
    if (selection.kind === "duplicates") return "Duplicates";
    if (selection.kind === "health") return "Library health";
    if (selection.kind === "project") {
      return snapshot.projects.find((item) => item.id === selection.projectId)?.name ?? "Godot project";
    }
    return "All assets";
  }, [selection, snapshot.collections, snapshot.packs, snapshot.projects]);

  function openSavedView(view: SavedAssetView) {
    const resolved = resolveSavedSelection(view.selection, new Set(snapshot.projects.map((project) => project.id)));
    setSelection(resolved.selection);
    if (resolved.selection.kind === "project") {
      const projectId = resolved.selection.projectId;
      const project = snapshot.projects.find((candidate) => candidate.id === projectId);
      if (project) {
        setActiveProjectId(project.id);
        window.localStorage.setItem("lootbox:active-project", String(project.id));
      }
    }
    setSearchValue(view.query);
    setDebouncedSearch(view.query.trim());
    setFilters({ ...clearedFilters, ...view.filters });
    setFilterDraft({ ...clearedFilters, ...view.filters });
    setSort(view.sort);
    setSortDirection(view.sortDirection);
    setActiveSavedViewId(view.id);
    clearAssetSelection();
    if (resolved.staleProject) {
      setNotice(`“${view.name}” referred to a project that is no longer registered. Opened all assets instead.`);
    }
  }

  function saveCurrentView(event: React.FormEvent) {
    event.preventDefault();
    const name = savedViewName.trim();
    if (!name) return;
    const savedView: SavedAssetView = {
      id: crypto.randomUUID(),
      name,
      query: searchValue.trim(),
      filters: { ...filters },
      sort,
      sortDirection,
      selection,
    };
    setSavedViews((current) => [...current, savedView]);
    setActiveSavedViewId(savedView.id);
    setSavingView(false);
    setSavedViewName("");
    setNotice(`${name} saved`);
  }

  const gridColumns = Math.max(
    1,
    Math.floor((Math.max(assetViewportWidth - 32, 128) + 12) / 140),
  );
  const gridItemWidth =
    (Math.max(assetViewportWidth - 32, 128) - 12 * (gridColumns - 1)) / gridColumns;
  const gridRowHeight = Math.max(gridItemWidth * 0.75 + 39, 135);
  const assetRowCount =
    view === "grid" ? Math.ceil(assets.length / gridColumns) : assets.length;
  const getAssetScrollElement = useCallback(() => assetScrollRef.current, []);
  const estimateAssetRowSize = useCallback(
    () => (view === "grid" ? gridRowHeight : assetListRowHeight),
    [gridRowHeight, view],
  );
  const getAssetRowKey = useCallback(
    (index: number) => {
      if (index >= assetRowCount) return "load-more";
      const assetIndex = view === "grid" ? index * gridColumns : index;
      return assets[assetIndex]?.id ?? index;
    },
    [assetRowCount, assets, gridColumns, view],
  );
  const virtualizer = useVirtualizer({
    count: assetRowCount + (hasMoreAssets ? 1 : 0),
    getScrollElement: getAssetScrollElement,
    estimateSize: estimateAssetRowSize,
    gap: view === "grid" ? 16 : 0,
    overscan: view === "grid" ? 4 : 8,
    getItemKey: getAssetRowKey,
  });
  const virtualRows = virtualizer.getVirtualItems();
  const lastVirtualRow = virtualRows.at(-1)?.index ?? -1;

  const focusAssetAtIndex = useCallback((index: number) => {
    const asset = assets[index];
    if (!asset) return;
    virtualizer.scrollToIndex(
      view === "grid" ? Math.floor(index / gridColumns) : index,
      { align: "auto" },
    );
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        document.getElementById(`asset-option-${asset.id}`)?.focus();
      });
    });
  }, [assets, gridColumns, view, virtualizer]);

  useEffect(() => {
    virtualizer.measure();
  }, [gridColumns, gridRowHeight, view, virtualizer]);

  useEffect(() => {
    if (lastVirtualRow >= assetRowCount - 3) void loadMoreAssets();
  }, [assetRowCount, lastVirtualRow, loadMoreAssets]);

  const deletingPack = selection.kind === "pack" || selection.kind === "removed" || selection.kind === "missing";
  const errorPresentation = useMemo(() => {
    if (!error) return null;
    if (errorContext.startsWith("godot-export")) return {
      title: "Godot export didn’t finish",
      message: "Check that the project is available, then review the export and try again. Project-owned files are never overwritten.",
    };
    if (errorContext.includes("import") || errorContext.includes("rescan")) return {
      title: "The folder couldn’t be imported",
      message: "Check that the folder is readable and still connected, then choose it again.",
    };
    if (errorContext === "server-query" || errorContext.includes("query")) return {
      title: "The library couldn’t be refreshed",
      message: "Your files are untouched. Retry the library refresh; technical details are available if it keeps failing.",
    };
    if (errorContext.includes("open") || errorContext.includes("reveal")) return {
      title: "The file couldn’t be opened",
      message: "It may have moved or the default application may be unavailable. Locate the source folder and try again.",
    };
    if (errorContext === "relocate-pack") return {
      title: "The pack couldn’t be reconnected",
      message: "Choose the folder that contains the original pack files and try again.",
    };
    if (errorContext === "relocate-project") return {
      title: "The project couldn’t be reconnected",
      message: "Choose the folder containing project.godot and try again.",
    };
    if (errorContext.includes("backup")) return {
      title: "The backup operation didn’t finish",
      message: "The current library metadata is still available. Check the destination and try again.",
    };
    return {
      title: "Lootbox couldn’t complete that action",
      message: "Your source files are untouched. Try the action again or open Maintenance to inspect diagnostics.",
    };
  }, [error, errorContext]);

  const errorRecovery = error ? (
    errorContext.startsWith("godot-export") && godotExport
      ? { label: "Review export", run: () => undefined }
      : errorContext === "server-query" || errorContext.includes("query")
      ? { label: "Retry", run: () => void refresh() }
      : errorContext.includes("import")
        ? { label: "Choose folder", run: () => void importPack() }
        : errorContext === "relocate-pack" && selectedPack
          ? { label: "Locate folder", run: () => void relocatePack(selectedPack) }
          : errorContext === "relocate-project" && activeProject
            ? { label: "Locate project", run: () => void relocateGodotProject(activeProject) }
          : errorContext === "open-asset" && selectedAsset
            ? { label: "Reveal in folder", run: () => void api.revealAsset(selectedAsset.absolutePath).catch((caught) => reportError(caught, "reveal-asset")) }
            : errorContext === "reveal-asset" && selectedAsset
              ? { label: "Open file", run: () => void api.openAsset(selectedAsset.absolutePath).catch((caught) => reportError(caught, "open-asset")) }
        : { label: "Open diagnostics", run: () => { setSettingsMessage(""); setSettingsOpen(true); } }
  ) : null;

  const selectAsset = useCallback(
    (asset: Asset, event: React.MouseEvent<HTMLButtonElement>) => {
      const additive = event.metaKey || event.ctrlKey;
      if (event.shiftKey && selectionAnchorRef.current !== null) {
        const anchorIndex = assets.findIndex(
          (candidate) => candidate.id === selectionAnchorRef.current,
        );
        const targetIndex = assets.findIndex((candidate) => candidate.id === asset.id);
        if (anchorIndex >= 0 && targetIndex >= 0) {
          const range = assets
            .slice(Math.min(anchorIndex, targetIndex), Math.max(anchorIndex, targetIndex) + 1)
            .map((candidate) => candidate.id);
          applyAssetSelection(
            new Set(additive ? [...selectedIdsRef.current, ...range] : range),
            asset.id,
          );
          return;
        }
      }

      selectionAnchorRef.current = asset.id;
      if (additive) {
        const next = new Set(selectedIdsRef.current);
        if (next.has(asset.id)) next.delete(asset.id);
        else next.add(asset.id);
        const nextActive = next.has(asset.id) ? asset.id : (next.values().next().value ?? null);
        applyAssetSelection(next, nextActive);
      } else {
        applyAssetSelection(new Set([asset.id]), asset.id);
      }
    },
    [applyAssetSelection, assets],
  );

  const openAsset = useCallback((asset: Asset) => {
    void api
      .openAsset(asset.absolutePath)
      .catch((caught) => reportError(caught, "open-asset"));
  }, [reportError]);

  const selectAssetForContextMenu = useCallback((asset: Asset) => {
    if (selectedIdsRef.current.has(asset.id)) return;
    selectionAnchorRef.current = asset.id;
    selectedAssetCacheRef.current.set(asset.id, asset);
    applyAssetSelection(new Set([asset.id]), asset.id);
  }, [applyAssetSelection]);

  const deselectReviewedAsset = useCallback((id: number) => {
    const next = new Set(selectedIdsRef.current);
    next.delete(id);
    const nextActive = selectedIdRef.current === id
      ? (next.values().next().value ?? null)
      : selectedIdRef.current;
    if (selectionAnchorRef.current === id) selectionAnchorRef.current = nextActive;
    applyAssetSelection(next, nextActive);
    if (next.size === 0) setReviewSelectionOpen(false);
  }, [applyAssetSelection]);

  const copyAssetPath = useCallback((path: string) => {
    void navigator.clipboard.writeText(path)
      .then(() => {
        setUndoRemoval(null);
        setNotice("Path copied");
      })
      .catch((caught) => reportError(caught, "copy-path"));
  }, [reportError]);

  const revealAsset = useCallback((asset: Asset) => {
    void api
      .revealAsset(asset.absolutePath)
      .catch((caught) => reportError(caught, "reveal-asset"));
  }, [reportError]);

  const selectedRemovalTargets = useCallback((asset: Asset) => (
    selectedIdsRef.current.has(asset.id)
      ? [...selectedIdsRef.current].flatMap((id) => {
          const candidate = selectedAssetCacheRef.current.get(id) ??
            assets.find((item) => item.id === id);
          return candidate ? [candidate] : [];
        })
      : [asset]
  ), [assets]);

  const requestAssetRemoval = useCallback(
    (asset: Asset) => {
      const targets = selectedRemovalTargets(asset);
      if (targets.length >= 10) {
        setConfirmAssetRemoval(targets);
        return;
      }
      if (targets.length === 0) return;
      const ids = targets.map((target) => target.id);
      const label = targets.length === 1 ? targets[0].name : `${targets.length} assets`;
      void api.setAssetsExcluded(ids, true)
        .then(async () => {
          clearAssetSelection();
          setUndoRemoval({ ids, label });
          setNotice(`${label} removed`);
          await refresh();
        })
        .catch((caught) => reportError(caught, "remove-asset"));
    },
    [clearAssetSelection, refresh, reportError, selectedRemovalTargets],
  );

  const requestProjectAssetRemoval = useCallback((asset: Asset) => {
    if (selection.kind !== "project") return;
    const project = snapshot.projects.find((candidate) => candidate.id === selection.projectId);
    if (!project) return;
    const ids = selectedIdsRef.current.has(asset.id)
      ? [...selectedIdsRef.current]
      : [asset.id];
    if (ids.length === 0) return;
    setError(null);
    setNotice(null);
    setGodotProjectRemoval({ project, ids, preview: null, loading: true, removing: false });
    void api.previewRemoveAssetsFromGodotProject(project.id, ids)
      .then((preview) => {
        setGodotProjectRemoval((current) => current && current.project.id === project.id
          ? { ...current, preview, loading: false }
          : current);
      })
      .catch((caught) => {
        setGodotProjectRemoval(null);
        reportError(caught, "godot-project-removal-preview");
      });
  }, [reportError, selection, snapshot.projects]);

  const requestDisplayedAssetRemoval = useCallback((asset: Asset) => {
    if (selection.kind === "project") requestProjectAssetRemoval(asset);
    else requestAssetRemoval(asset);
  }, [requestAssetRemoval, requestProjectAssetRemoval, selection.kind]);

  async function confirmProjectAssetRemoval() {
    if (!godotProjectRemoval?.preview || godotProjectRemoval.removing) return;
    const removal = godotProjectRemoval;
    const preview = godotProjectRemoval.preview;
    setGodotProjectRemoval((current) => current ? { ...current, removing: true } : current);
    try {
      const result = await api.removeAssetsFromGodotProject(removal.project.id, removal.ids);
      const assetLabel = `${preview.selected.toLocaleString()} ${preview.selected === 1 ? "asset" : "assets"}`;
      const details = [
        result.deleted > 0 ? `${result.deleted.toLocaleString()} ${result.deleted === 1 ? "file" : "files"} deleted` : null,
        result.keptModified > 0 ? `${result.keptModified.toLocaleString()} modified kept` : null,
        result.keptShared > 0 ? `${result.keptShared.toLocaleString()} shared kept` : null,
        result.cleanedMissing > 0 ? `${result.cleanedMissing.toLocaleString()} missing cleaned` : null,
      ].filter(Boolean).join(" · ");
      setGodotProjectRemoval(null);
      clearAssetSelection();
      setNotice(`${assetLabel} removed from ${removal.project.name}${details ? ` · ${details}` : ""}`);
      void refresh().catch((caught) => {
        void api.logDiagnostic("error", "library-refresh", errorMessage(caught));
      });
    } catch (caught) {
      setGodotProjectRemoval((current) => current ? { ...current, removing: false } : current);
      reportError(caught, "godot-project-removal");
    }
  }

  const reportCardError = useCallback((caught: unknown) => {
    reportError(caught, "asset-action");
  }, [reportError]);

  const reportCardPreviewError = useCallback((asset: Asset, caught: unknown) => {
    void api.logDiagnostic("warning", `asset-preview:${asset.relativePath}`, errorMessage(caught));
  }, []);

  const restoreAsset = useCallback(
    (asset: Asset) => {
      const targets = selectedIdsRef.current.has(asset.id)
        ? [...selectedIdsRef.current].flatMap((id) => {
            const candidate = selectedAssetCacheRef.current.get(id) ??
              assets.find((item) => item.id === id);
            return candidate ? [candidate] : [];
          })
        : [asset];
      void api
        .setAssetsExcluded(targets.map((candidate) => candidate.id), false)
        .then(() => {
          clearAssetSelection();
          return refresh();
        })
        .catch((caught) => reportError(caught, "restore-asset"));
    },
    [assets, clearAssetSelection, refresh, reportError],
  );

  const handleSidebarSelect = useCallback((next: LibrarySelection) => {
    setSelection(next);
    setActiveSavedViewId(null);
    clearAssetSelection();
  }, [clearAssetSelection]);

  const handleRelocateProject = useCallback((project: ProjectSummary) => {
    void relocateGodotProject(project);
  }, [relocateGodotProject]);

  const handleDeleteSavedView = useCallback((view: SavedAssetView) => {
    setSavedViews((current) => current.filter((candidate) => candidate.id !== view.id));
    setActiveSavedViewId((current) => (current === view.id ? null : current));
    setMetadataUndo({
      label: `Undo deleting “${view.name}”`,
      run: async () => setSavedViews((current) => current.some((candidate) => candidate.id === view.id) ? current : [...current, view]),
    });
    setNotice(`${view.name} deleted`);
  }, []);

  const handleSidebarImport = useCallback(() => {
    void importPack();
  }, [importPack]);

  const handleStartCollection = useCallback(() => {
    setAddSelectionToNewCollection(false);
    setCreatingCollection(true);
  }, []);

  const handleRescanPack = useCallback((pack: PackSummary) => {
    void rescanPack(pack);
  }, [rescanPack]);

  const handleOpenPack = useCallback((pack: PackSummary) => {
    void api.openAsset(pack.rootPath).catch((caught) => reportError(caught, "open-pack"));
  }, [reportError]);

  const handleRelocatePack = useCallback((pack: PackSummary) => {
    void relocatePack(pack);
  }, [relocatePack]);

  const handleViewRemoved = useCallback((pack: PackSummary) => {
    setSelection({ kind: "removed", packId: pack.id });
    clearAssetSelection();
  }, [clearAssetSelection]);

  const handleViewMissing = useCallback((pack: PackSummary) => {
    setSelection({ kind: "missing", packId: pack.id });
    clearAssetSelection();
  }, [clearAssetSelection]);

  const handleAddProject = useCallback(() => {
    void addGodotProject();
  }, [addGodotProject]);

  const handleOpenProject = useCallback((project: ProjectSummary) => {
    void api.openAsset(project.rootPath).catch((caught) => reportError(caught, "open-project"));
  }, [reportError]);

  const handleOpenSettings = useCallback(() => {
    setSettingsMessage("");
    setSettingsOpen(true);
  }, []);

  const handleOpenShortcuts = useCallback(() => {
    setShortcutsOpen(true);
  }, []);

  const handleOpenActiveProject = useCallback(() => {
    if (!activeProject) return;
    void api.openAsset(activeProject.rootPath).catch((caught) => reportError(caught, "open-project"));
  }, [activeProject, reportError]);

  const handleRefreshActiveProject = useCallback(() => {
    void projectStatusQuery.refetch();
  }, [projectStatusQuery]);

  const handleViewActiveProjectAssets = useCallback(() => {
    if (!activeProject) return;
    setSelection({ kind: "project", projectId: activeProject.id });
    setActiveSavedViewId(null);
    clearAssetSelection();
  }, [activeProject, clearAssetSelection]);

  const handleClearActiveProjectTarget = useCallback(() => {
    activateProject(null);
  }, [activateProject]);

  const handleDetailAddTag = useCallback(async (name: string) => {
    let changed: number[] = [];
    if (await mutateSelected(async () => { changed = await api.addTags([...selectedIdsRef.current], name); }) && changed.length > 0) {
      setMetadataUndo({ label: `Undo adding “${name}”`, run: async () => { await api.removeTags(changed, name); await refresh(); } });
      setNotice(`Added “${name}” to ${changed.length.toLocaleString()} assets`);
    }
  }, [mutateSelected, refresh]);

  const handleDetailRemoveTag = useCallback(async (name: string) => {
    let changed: number[] = [];
    if (await mutateSelected(async () => { changed = await api.removeTags([...selectedIdsRef.current], name); }) && changed.length > 0) {
      setMetadataUndo({ label: `Undo removing “${name}”`, run: async () => { await api.addTags(changed, name); await refresh(); } });
      setNotice(`Removed “${name}” from ${changed.length.toLocaleString()} assets`);
    }
  }, [mutateSelected, refresh]);

  const handleDetailMembership = useCallback(async (collectionId: number, included: boolean) => {
    const collection = snapshot.collections.find((item) => item.id === collectionId);
    let changed: number[] = [];
    if (await mutateSelected(async () => { changed = await api.setCollectionMemberships([...selectedIdsRef.current], collectionId, included); }) && changed.length > 0) {
      setMetadataUndo({ label: `Undo collection change`, run: async () => { await api.setCollectionMemberships(changed, collectionId, !included); await refresh(); } });
      setNotice(`${included ? "Added to" : "Removed from"} ${collection?.name ?? "collection"} · ${changed.length.toLocaleString()} assets`);
    }
  }, [mutateSelected, refresh, snapshot.collections]);

  const handleDetailClassification = useCallback((assetType?: string, mapRole?: string) => {
    return guardedBulkMutation(
      `Change classification for ${selectedIdsRef.current.size.toLocaleString()} assets?`,
      `This applies the new classification to every selected asset. Source files stay untouched.`,
      async () => {
        const snapshots = await api.setClassificationOverride([...selectedIdsRef.current], assetType, mapRole);
        if (snapshots.length > 0) {
          setMetadataUndo({ label: "Undo classification change", run: async () => {
            await api.restoreClassificationOverrides(snapshots);
            await refresh();
          } });
          setNotice(`Classification updated · ${snapshots.length.toLocaleString()} assets`);
        }
      },
    );
  }, [guardedBulkMutation, refresh]);

  const handleDetailGroup = useCallback((action: "merge" | "split") => guardedBulkMutation(
    `${action === "merge" ? "Group" : "Separate"} ${selectedIdsRef.current.size.toLocaleString()} assets?`,
    `${action === "merge" ? "Grouping" : "Separating"} changes how all selected files are presented and exported. Source files stay untouched.`,
    async () => {
      const snapshots = await api.setClassificationOverride([...selectedIdsRef.current], undefined, undefined, action);
      if (snapshots.length > 0) {
        setMetadataUndo({ label: `Undo ${action === "merge" ? "grouping" : "separation"}`, run: async () => {
          await api.restoreClassificationOverrides(snapshots);
          await refresh();
        } });
        setNotice(`${action === "merge" ? "Grouped" : "Separated"} ${snapshots.length.toLocaleString()} assets`);
      }
    },
  ), [guardedBulkMutation, refresh]);

  const handleDetailResetClassification = useCallback(() => mutateSelected(async () => {
    const snapshots = await api.resetClassificationOverride([...selectedIdsRef.current]);
    if (snapshots.some((snapshot) => snapshot.existed)) {
      setMetadataUndo({ label: "Undo automatic classification", run: async () => {
        await api.restoreClassificationOverrides(snapshots);
        await refresh();
      } });
      setNotice(`Automatic classification restored · ${snapshots.length.toLocaleString()} assets`);
    }
  }).then(() => undefined), [mutateSelected, refresh]);

  const handleDetailAddCollection = useCallback(() => {
    setAddSelectionToNewCollection(true);
    setCreatingCollection(true);
  }, []);

  const handleDetailOpen = useCallback(() => {
    if (!selectedAsset) return;
    void api.openAsset(selectedAsset.absolutePath).catch((caught) => reportError(caught, "open-asset"));
  }, [reportError, selectedAsset]);

  const handleDetailOpenVariant = useCallback((path: string) => {
    void api.openAsset(path).catch((caught) => reportError(caught, "open-asset"));
  }, [reportError]);

  const handleDetailRevealPath = useCallback((path: string) => {
    void api.revealAsset(path).catch((caught) => reportError(caught, "reveal-asset"));
  }, [reportError]);

  const selectAllAssets = useCallback(async () => {
    try {
      const scopeKey = selectionScopeKeyRef.current;
      const matches = await api.assetSelections(query);
      if (scopeKey !== selectionScopeKeyRef.current) return;
      const idSet = new Set(matches.map((match) => match.id));
      selectedPathCacheRef.current = new Map(
        matches.map((match) => [match.id, match.absolutePath]),
      );
      selectedAssetCacheRef.current = new Map(
        assets
          .filter((asset) => idSet.has(asset.id))
          .map((asset) => [asset.id, asset]),
      );
      const activeId = assets.find((asset) => idSet.has(asset.id))?.id ?? null;
      selectionAnchorRef.current = activeId;
      applyAssetSelection(idSet, activeId);
    } catch (caught) {
      reportError(caught, "select-all");
    }
  }, [applyAssetSelection, assets, query, reportError]);

  useEffect(() => {
    const handleLibraryKeys = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        isTypingTarget(event.target) ||
        creatingCollection ||
        renamingPack !== null ||
        confirmDelete ||
        confirmAssetRemoval.length > 0
      ) {
        return;
      }

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        void selectAllAssets();
        return;
      }

      const currentIndex = selectedId === null
        ? -1
        : assets.findIndex((asset) => asset.id === selectedId);
      const browserHasFocus = isAssetKeyboardTarget(event.target);
      const visibleRows = Math.max(1, Math.floor((assetScrollRef.current?.clientHeight ?? 440) / (view === "grid" ? gridRowHeight : assetListRowHeight)));
      const pageStep = visibleRows * (view === "grid" ? gridColumns : 1);
      const directions: Record<string, number> = view === "grid"
        ? {
            ArrowLeft: -1,
            ArrowRight: 1,
            ArrowUp: -gridColumns,
            ArrowDown: gridColumns,
            PageUp: -pageStep,
            PageDown: pageStep,
          }
        : { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -1, ArrowDown: 1, PageUp: -pageStep, PageDown: pageStep };
      const direction = directions[event.key];
      if (browserHasFocus && (direction !== undefined || event.key === "Home" || event.key === "End")) {
        event.preventDefault();
        const nextIndex = event.key === "Home"
          ? 0
          : event.key === "End"
            ? assets.length - 1
            : Math.max(0, Math.min(assets.length - 1, currentIndex < 0 ? 0 : currentIndex + (direction ?? 0)));
        const nextAsset = assets[nextIndex];
        if (!nextAsset) return;
        if (event.shiftKey) {
          const anchorId = selectionAnchorRef.current ?? selectedId ?? nextAsset.id;
          selectionAnchorRef.current = anchorId;
          const anchorIndex = assets.findIndex((asset) => asset.id === anchorId);
          const range = assets
            .slice(Math.min(anchorIndex, nextIndex), Math.max(anchorIndex, nextIndex) + 1)
            .map((asset) => asset.id);
          applyAssetSelection(
            new Set(event.metaKey || event.ctrlKey ? [...selectedIdsRef.current, ...range] : range),
            nextAsset.id,
          );
        } else {
          selectionAnchorRef.current = nextAsset.id;
          applyAssetSelection(new Set([nextAsset.id]), nextAsset.id);
        }
        focusAssetAtIndex(nextIndex);
        if (nextIndex >= assets.length - gridColumns * 2) void loadMoreAssets();
        return;
      }

      if (browserHasFocus && event.key === "Enter" && selectedAsset) {
        event.preventDefault();
        openAsset(selectedAsset);
      } else if (browserHasFocus && event.key === " " && selectedAsset?.assetType === "audio" && !event.repeat) {
        event.preventDefault();
        void toggleAudioPlayback(selectedAsset.absolutePath).catch((caught) =>
          reportError(caught, "audio-playback"),
        );
      } else if (
        event.key === "Delete" &&
        browserHasFocus &&
        selectedAsset &&
        selection.kind !== "removed" &&
        (selection.kind === "project" || activeProjectId === null)
      ) {
        event.preventDefault();
        requestDisplayedAssetRemoval(selectedAsset);
      }
    };
    window.addEventListener("keydown", handleLibraryKeys);
    return () => window.removeEventListener("keydown", handleLibraryKeys);
  }, [
    applyAssetSelection,
    activeProjectId,
    assets,
    confirmAssetRemoval.length,
    confirmDelete,
    creatingCollection,
    focusAssetAtIndex,
    gridColumns,
    gridRowHeight,
    loadMoreAssets,
    openAsset,
    query,
    renamingPack,
    requestDisplayedAssetRemoval,
    reportError,
    selectedAsset,
    selectedId,
    selection.kind,
    view,
    virtualizer,
  ]);

  const godotCompletion = godotExportNotice
    ? godotExportCompletionCopy(godotExportNotice.project.name, godotExportNotice.result)
    : null;

  return (
    <div className="dark flex h-full w-full overflow-hidden bg-background text-foreground">
      {!leftPanelCollapsed && (
        <div
          className="h-full shrink-0 overflow-hidden"
          style={{ width: `${layoutLeftPanelWidth}px` }}
        >
          <Sidebar
            snapshot={snapshot}
            selection={selection}
            creatingCollection={creatingCollection}
            activeProjectId={activeProjectId}
            activeProjectAttention={activeProjectAttention}
            savedViews={savedViews}
            activeSavedViewId={activeSavedViewId}
            onSelect={handleSidebarSelect}
            onActivateProject={activateProject}
            onRelocateProject={handleRelocateProject}
            onOpenSavedView={openSavedView}
            onDeleteSavedView={handleDeleteSavedView}
            onImport={handleSidebarImport}
            onStartCollection={handleStartCollection}
            onRenamePack={startRenamePack}
            onRescanPack={handleRescanPack}
            onOpenPack={handleOpenPack}
            onRelocatePack={handleRelocatePack}
            onForgetPack={requestForgetPack}
            onViewRemoved={handleViewRemoved}
            onViewMissing={handleViewMissing}
            onPurgeMissing={setConfirmPurge}
            onAddProject={handleAddProject}
            onOpenProject={handleOpenProject}
            onForgetProject={setConfirmProjectRemoval}
            onSettings={handleOpenSettings}
            onShortcuts={handleOpenShortcuts}
          />
        </div>
      )}

      <div
        role="separator"
        aria-label="Resize library sidebar"
        aria-orientation="vertical"
        aria-valuemin={168}
        aria-valuemax={320}
        aria-valuenow={Math.round(layoutLeftPanelWidth || leftPanelWidth)}
        aria-valuetext={`${Math.round(layoutLeftPanelWidth || leftPanelWidth)} pixels`}
        tabIndex={0}
        className="group relative z-20 flex w-1.5 shrink-0 cursor-col-resize items-center justify-center outline-none bg-border/40 hover:bg-primary/50 transition-colors select-none"
        onPointerDown={(event) => startPanelResize("left", event)}
        onKeyDown={(event) => resizePanelWithKeyboard("left", event)}
      >
        <button
          type="button"
          aria-label={leftPanelCollapsed ? "Expand sidebar (Ctrl+B)" : "Collapse sidebar (Ctrl+B)"}
          title={leftPanelCollapsed ? "Expand sidebar (Ctrl+B)" : "Collapse sidebar (Ctrl+B)"}
          onClick={(e) => {
            e.stopPropagation();
            setLeftPanelCollapsed((c) => !c);
          }}
          className="absolute z-30 flex size-5.5 items-center justify-center rounded-full border bg-background text-muted-foreground shadow-xs transition-transform hover:scale-115 hover:text-foreground cursor-pointer"
        >
          {leftPanelCollapsed ? <ChevronRight className="size-3.5" /> : <ChevronLeft className="size-3.5" />}
        </button>
      </div>

      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center justify-between gap-2 border-b bg-background/95 px-4">
          <div className="flex min-w-0 flex-1 items-center gap-2">
            {leftPanelCollapsed && (
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      className="h-9 gap-1.5 rounded-md px-2.5 text-xs font-medium shrink-0 max-w-48 truncate"
                      aria-label="Switch workspace"
                      title={activeProject ? `Target: ${activeProject.name}` : "Target: Global Library"}
                    >
                      {activeProject ? <Gamepad2 className="size-3.5 text-primary shrink-0" /> : <HardDrive className="size-3.5 text-muted-foreground shrink-0" />}
                      <span className="truncate">{activeProject ? activeProject.name : "Global Library"}</span>
                    </Button>
                  }
                />
                <DropdownMenuContent align="start" className="w-56 rounded-md">
                  <DropdownMenuItem
                    className="gap-2 text-xs font-medium cursor-pointer"
                    onClick={() => {
                      activateProject(null);
                      setSelection({ kind: "all" });
                      setActiveSavedViewId(null);
                      clearAssetSelection();
                    }}
                  >
                    <HardDrive className="size-3.5" />
                    <span className="flex-1">Global Library</span>
                    {!activeProject && <Check className="size-3.5 text-primary" />}
                  </DropdownMenuItem>
                  {snapshot.projects.length > 0 && <DropdownMenuSeparator />}
                  {snapshot.projects.map((project) => (
                    <DropdownMenuItem
                      key={project.id}
                      className="gap-2 text-xs cursor-pointer"
                      onClick={() => activateProject(project)}
                    >
                      <Gamepad2 className="size-3.5" />
                      <span className="flex-1 truncate">{project.name}</span>
                      {activeProject?.id === project.id && <Check className="size-3.5 text-primary" />}
                    </DropdownMenuItem>
                  ))}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    className="gap-2 text-xs cursor-pointer"
                    onClick={() => void addGodotProject()}
                  >
                    <Plus className="size-3.5" />
                    <span>Add Godot Project...</span>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}

            <AssetSearch
              inputRef={searchRef}
              value={searchValue}
              onValueChange={(value) => {
                setSearchValue(value);
                setActiveSavedViewId(null);
              }}
              onQueryChange={setDebouncedSearch}
            />
          </div>

          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="rounded-md gap-1.5 text-xs text-muted-foreground hover:text-foreground"
              onClick={() => setCommandPaletteOpen(true)}
              aria-label="Open command palette"
              title="Command Palette (Ctrl+K)"
            >
              <Command className="size-3.5" />
              <span className="max-[1150px]:hidden">Commands</span>
              <Kbd className="ml-0.5">⌘K</Kbd>
            </Button>

            <Popover open={filtersOpen} onOpenChange={setFiltersOpen}>
              <PopoverTrigger
                render={
                  <Button
                    type="button"
                    variant={activeFilters.length > 0 || Boolean(filters.type) ? "secondary" : "outline"}
                    size="sm"
                    className="rounded-md"
                    aria-label={activeFilters.length > 0 || Boolean(filters.type) ? `Filter assets, ${activeFilters.length + (filters.type ? 1 : 0)} active` : "Filter assets"}
                    title={activeFilters.length > 0 || Boolean(filters.type) ? [...(filters.type ? [`Type: ${typeLabels[filters.type as AssetType] ?? filters.type}`] : []), ...activeFilters.map((f) => f.label)].join(" · ") : "Filters (Ctrl+Shift+F)"}
                  >
                    <SlidersHorizontal />
                    <span className="max-[1150px]:hidden">Filters</span>
                    {(activeFilters.length > 0 || Boolean(filters.type)) && (
                      <span className="font-mono text-[11px] text-primary max-[1150px]:hidden">{activeFilters.length + (filters.type ? 1 : 0)}</span>
                    )}
                  </Button>
                }
              />
              <PopoverContent align="end" side="bottom" sideOffset={8} className="w-96 gap-3 p-3 text-xs">
                <div className="flex items-center justify-between border-b pb-2">
                  <PopoverHeading className="text-xs font-semibold text-foreground">Filter assets</PopoverHeading>
                  {(activeFilters.length > 0 || Boolean(filters.type)) && (
                    <button
                      type="button"
                      onClick={() => {
                        setFilters({ ...clearedFilters });
                        setActiveSavedViewId(null);
                      }}
                      className="text-[11px] text-muted-foreground hover:text-foreground underline cursor-pointer"
                    >
                      Reset all
                    </button>
                  )}
                </div>
                <div className="grid grid-cols-2 gap-2.5 pt-1">
                  <MultiFilterSelect label="Format" value={filters.extension} placeholder="All formats" options={filterOptions.extensions.map((value) => ({ value, label: `.${value}` }))} onValueChange={(value) => { setFilters((current) => ({ ...current, extension: value })); setActiveSavedViewId(null); }} />
                  <MultiFilterSelect label="Map role" value={filters.mapRole} placeholder="All map roles" options={filterOptions.mapRoles.map((value) => ({ value, label: value.replaceAll("_", " ") }))} onValueChange={(value) => { setFilters((current) => ({ ...current, mapRole: value })); setActiveSavedViewId(null); }} />
                  <MultiFilterSelect className="col-span-2" label="Tag" value={filters.tag} placeholder="All tags" options={filterOptions.tags.map((value) => ({ value, label: value }))} onValueChange={(value) => { setFilters((current) => ({ ...current, tag: value })); setActiveSavedViewId(null); }} />
                  <FilterSelect label="Minimum resolution" value={filters.minWidth} placeholder="Any resolution" options={[256, 512, 1024, 2048, 4096, 8192].map((value) => ({ value: String(value), label: `${value} × ${value}+` }))} onValueChange={(value) => { setFilters((current) => ({ ...current, minWidth: value })); setActiveSavedViewId(null); }} />
                  <FilterSelect label="Classification confidence" value={filters.minConfidence} placeholder="Any confidence" options={[{ value: "80", label: "Needs review · ≤80%" }, { value: "60", label: "Uncertain · ≤60%" }]} onValueChange={(value) => { setFilters((current) => ({ ...current, minConfidence: value })); setActiveSavedViewId(null); }} />
                  <FilterSelect className="col-span-2" label="File status" value={filters.status} placeholder="Available files" options={[{ value: "missing", label: "Missing files" }]} onValueChange={(value) => { setFilters((current) => ({ ...current, status: value })); setActiveSavedViewId(null); }} />
                  <FilterSelect
                    className="col-span-2"
                    label="Project usage"
                    value={filters.projectUsage}
                    placeholder="Any project usage"
                    options={[
                      ...(activeProject ? [{ value: "active", label: `Used in ${activeProject.name}` }] : []),
                      { value: "unused", label: "Not used by any project" },
                    ]}
                    onValueChange={(value) => { setFilters((current) => ({ ...current, projectUsage: value })); setActiveSavedViewId(null); }}
                  />
                </div>
              </PopoverContent>
            </Popover>
            {selection.kind !== "health" && (
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                className="rounded-md"
                onClick={() => {
                  setSavedViewName(activeSavedViewId ? `${sectionTitle} copy` : sectionTitle);
                  setSavingView(true);
                }}
                aria-label="Save current view"
                title="Save current search, filters, scope, and sorting"
              >
                <BookmarkPlus />
              </Button>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="min-w-44 justify-start rounded-md max-[1150px]:min-w-0"
                    aria-label={`Sort assets by ${sortLabels[sort]}, ${sortDirectionLabel(sort, sortDirection)}`}
                    title="Sort"
                  />
                }
              >
                <ArrowUpDown />
                <span className="max-[1150px]:hidden">{sortLabels[sort]}</span>
                <span className="text-[11px] text-muted-foreground max-[1150px]:hidden">{sortDirectionLabel(sort, sortDirection)}</span>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40 rounded-md">
                {([
                  ["name", "Name"],
                  ["newest", "Date modified"],
                  ["largest", "File size"],
                  ["type", "Type"],
                ] as const).map(([value, label]) => (
                  <DropdownMenuItem
                    key={value}
                    className="rounded-sm text-xs"
                    onClick={() => {
                      setSort(value);
                      setSortDirection(value === "newest" || value === "largest" ? "desc" : "asc");
                      setActiveSavedViewId(null);
                    }}
                  >
                    <Check className={cn(sort === value ? "opacity-100" : "opacity-0")} />
                    {label}
                  </DropdownMenuItem>
                ))}
                <DropdownMenuSeparator />
                {(["asc", "desc"] as const).map((value) => (
                  <DropdownMenuItem
                    key={value}
                    className="rounded-sm text-xs"
                    onClick={() => {
                      setSortDirection(value);
                      setActiveSavedViewId(null);
                    }}
                  >
                    <Check className={cn(sortDirection === value ? "opacity-100" : "opacity-0")} />
                    {sortDirectionLabel(sort, value)}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
            <ButtonGroup aria-label="View">
              <Button
                type="button"
                variant={view === "grid" ? "secondary" : "outline"}
                size="icon-sm"
                className="rounded-l-md rounded-r-none"
                onClick={() => setView("grid")}
                aria-label="Grid view"
                title="Grid view"
              >
                <Grid2X2 />
              </Button>
              <Button
                type="button"
                variant={view === "list" ? "secondary" : "outline"}
                size="icon-sm"
                className="rounded-r-md rounded-l-none"
                onClick={() => setView("list")}
                aria-label="List view"
                title="List view"
              >
                <List />
              </Button>
            </ButtonGroup>
          </div>
        </header>

        {activeProject && (
          <ProjectWorkspaceBar
            project={activeProject}
            status={projectStatusQuery.data}
            loading={projectStatusQuery.isFetching}
            isProjectView={selection.kind === "project" && selection.projectId === activeProject.id}
            onOpen={handleOpenActiveProject}
            onRefresh={handleRefreshActiveProject}
            onViewAssets={handleViewActiveProjectAssets}
            onClearTarget={handleClearActiveProjectTarget}
          />
        )}

        {selection.kind !== "health" && (
          <div className="quiet-scrollbar flex h-9 shrink-0 items-center gap-1.5 overflow-x-auto border-b bg-muted/10 px-4 text-xs select-none" aria-label="Asset filters">
            <button
              type="button"
              onClick={() => {
                setFilters((current) => ({ ...current, type: "" }));
                setActiveSavedViewId(null);
              }}
              className={cn(
                "flex h-6 shrink-0 items-center gap-1.5 rounded-sm px-2 text-[11px] font-medium transition-colors cursor-pointer",
                !filters.type
                  ? "bg-primary text-primary-foreground shadow-xs font-semibold"
                  : "bg-background/80 text-muted-foreground border border-border/60 hover:bg-accent hover:text-foreground",
              )}
            >
              <span>All types</span>
              <span className={cn("font-mono text-[11px] tabular-nums", !filters.type ? "text-primary-foreground/85" : "text-muted-foreground")}>
                {dynamicTypeCounts.total > 9999 ? "9k+" : dynamicTypeCounts.total.toLocaleString()}
              </span>
            </button>

            {snapshot.typeCounts.map(({ assetType, count }) => {
              const item = typeMetadata[assetType];
              const Icon = item.icon;
              const isActive = filters.type === assetType;
              const displayCount = dynamicTypeCounts.isScoped ? (dynamicTypeCounts.counts.get(assetType) ?? 0) : count;
              if (displayCount === 0 && !isActive) return null;
              return (
                <button
                  key={assetType}
                  type="button"
                  onClick={() => {
                    setFilters((current) => ({ ...current, type: isActive ? "" : assetType }));
                    setActiveSavedViewId(null);
                  }}
                  className={cn(
                    "flex h-6 shrink-0 items-center gap-1.5 rounded-sm px-2 text-[11px] font-medium transition-colors cursor-pointer",
                    isActive
                      ? "bg-primary text-primary-foreground shadow-xs font-semibold"
                      : "bg-background/80 text-muted-foreground border border-border/60 hover:bg-accent hover:text-foreground",
                  )}
                >
                  <Icon className="size-3" />
                  <span>{item.label}</span>
                  <span className={cn("font-mono text-[11px] tabular-nums", isActive ? "text-primary-foreground/85" : "text-muted-foreground")}>
                    {displayCount > 9999 ? "9k+" : displayCount.toLocaleString()}
                  </span>
                </button>
              );
            })}

            {activeFilters.length > 0 && (
              <>
                <div className="h-3.5 w-px shrink-0 bg-border/80 mx-0.5" role="separator" />
                {activeFilters.map((filter) => (
                  <Button
                    key={filter.key}
                    type="button"
                    variant="secondary"
                    size="xs"
                    className="h-6 shrink-0 rounded-sm px-2 text-[11px] gap-1"
                    onClick={() => {
                      setFilters((current) => ({ ...current, [filter.key]: "" }));
                      setActiveSavedViewId(null);
                    }}
                    aria-label={`Remove ${filter.label} filter`}
                  >
                    <span>{filter.label}</span>
                    <X className="size-3" />
                  </Button>
                ))}
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  className="h-6 shrink-0 text-[11px] text-muted-foreground hover:text-foreground"
                  onClick={() => {
                    setFilters({ ...clearedFilters });
                    setActiveSavedViewId(null);
                  }}
                >
                  Clear all
                </Button>
              </>
            )}
          </div>
        )}

        {selection.kind === "health" ? (
          <LibraryHealth
            snapshot={snapshot}
            activeProjectName={activeProject?.name}
            projectStatus={projectStatusQuery.data}
            projectStatusLoading={projectStatusQuery.isFetching}
            onViewMissing={() => {
              setSelection({ kind: "missing" });
              clearAssetSelection();
            }}
            onViewRemoved={() => {
              setSelection({ kind: "removed" });
              clearAssetSelection();
            }}
            onRelocatePack={(packId) => {
              const pack = snapshot.packs.find((candidate) => candidate.id === packId);
              if (pack) void relocatePack(pack);
            }}
            onRelocateProject={(projectId) => {
              const project = snapshot.projects.find((candidate) => candidate.id === projectId);
              if (project) void relocateGodotProject(project);
            }}
            onViewProject={() => {
              if (activeProject) setSelection({ kind: "project", projectId: activeProject.id });
            }}
            onRefreshProject={() => void projectStatusQuery.refetch()}
          />
        ) : (
        <>
        <div className={cn("flex h-12 shrink-0 items-center justify-between border-b px-4 transition-colors", selectedIds.size > 0 && "bg-primary/[0.04] border-b-primary/25")}>
          {selectedIds.size > 0 ? (
            <div className="flex min-w-0 items-center gap-2.5">
              <span className="inline-flex items-center gap-1.5 rounded-sm border border-primary/40 bg-primary/15 px-2 py-0.5 font-mono text-[11px] font-semibold text-primary">
                <span>{selectedIds.size.toLocaleString()}</span>
                <span>selected</span>
              </span>
              <span className="truncate text-xs text-muted-foreground hidden sm:inline">
                {selectionSummary.includes("·") ? selectionSummary.slice(selectionSummary.indexOf("·") + 1).trim() : selectionSummary}
              </span>
              <Button type="button" variant="secondary" size="xs" className="h-6 shrink-0 rounded-sm px-2 text-[11px] font-medium" onClick={() => setReviewSelectionOpen(true)}>
                Review selection
              </Button>
            </div>
          ) : (
            <div className="flex min-w-0 items-baseline gap-2">
              <h1 className="truncate text-sm font-semibold tracking-[-0.01em]">{sectionTitle}</h1>
              <span className="font-mono text-[11px] text-muted-foreground">
                {loading ? "…" : assetTotal.toLocaleString()}
              </span>
              {selectedPack && !selectedPack.available && (
                <span className="flex items-center gap-1 text-xs text-destructive" title={selectedPack.rootPath}>
                  <FolderCog className="size-3.5" /> Folder missing
                </span>
              )}
              {selectedProject && !selectedProject.available && (
                <span className="flex items-center gap-1 text-xs text-destructive" title={selectedProject.rootPath}>
                  <FolderCog className="size-3.5" /> Project missing
                </span>
              )}
            </div>
          )}

          {selectedIds.size > 0 && selectedAsset ? (
            <div className="flex items-center gap-1.5">
              {selectedIds.size === 1 && (
                <Button type="button" variant="outline" size="sm" className="h-7 rounded-sm px-2.5 text-xs font-normal" onClick={() => openAsset(selectedAsset)} aria-label="Open asset" title="Open asset">
                  <ExternalLink className="size-3.5" /> <span className="max-[1150px]:hidden">Open</span>
                </Button>
              )}
              {selection.kind !== "removed" && activeProject?.available && (
                <Button
                  type="button"
                  size="sm"
                  className="h-7 gap-1.5 rounded-sm bg-primary px-2.5 text-xs font-medium text-primary-foreground shadow-xs hover:bg-primary/90"
                  aria-label={`Review export to ${activeProject.name}`}
                  title={`Export to ${activeProject.name}`}
                  onClick={() => void addSelectionToGodot(activeProject.id)}
                >
                  <Gamepad2 className="size-3.5" /> <span>Export to {activeProject.name}</span>
                </Button>
              )}
              {(selection.kind === "removed" || selection.kind === "project" || activeProjectId === null) && (
                <Button
                  type="button"
                  variant={selection.kind === "removed" ? "outline" : "ghost"}
                  size="sm"
                  className={cn("h-7 rounded-sm px-2 text-xs", selection.kind !== "removed" && "text-destructive hover:bg-destructive/10 hover:text-destructive")}
                  aria-label={selection.kind === "removed" ? "Restore assets" : selection.kind === "project" ? "Remove assets from project" : "Remove assets from Lootbox"}
                  title={selection.kind === "removed" ? "Restore" : selection.kind === "project" ? "Remove from project" : "Remove from Lootbox"}
                  onClick={() => selection.kind === "removed" ? restoreAsset(selectedAsset) : requestDisplayedAssetRemoval(selectedAsset)}
                >
                  {selection.kind === "removed"
                    ? <><ArchiveRestore className="size-3.5" /> <span className="max-[1150px]:hidden">Restore</span></>
                    : selection.kind === "project"
                      ? <><FolderMinus className="size-3.5" /> <span className="max-[1150px]:hidden">Remove</span></>
                      : <><Trash2 className="size-3.5" /> <span className="max-[1150px]:hidden">Remove</span></>}
                </Button>
              )}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 gap-1 rounded-sm px-2 text-xs text-muted-foreground hover:text-foreground"
                onClick={clearAssetSelection}
                aria-label="Clear selection"
                title="Clear selection (Esc)"
              >
                <X className="size-3.5" />
                <span className="max-[1150px]:hidden">Deselect</span>
              </Button>
            </div>
          ) : (selection.kind === "pack" || selection.kind === "removed" || selection.kind === "missing" || selection.kind === "collection" || selection.kind === "project") && (
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    className="rounded-sm text-muted-foreground"
                  />
                }
              >
                <MoreHorizontal />
                <span className="sr-only">Actions</span>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-40 rounded-md">
                {selection.kind === "removed" && selectedPack && (
                  <DropdownMenuItem
                    className="rounded-sm text-xs"
                    onClick={() => setSelection({ kind: "pack", packId: selectedPack.id })}
                  >
                    <FolderOpen /> Back to pack
                  </DropdownMenuItem>
                )}
                {selection.kind === "missing" && selectedPack && (
                  <DropdownMenuItem
                    className="rounded-sm text-xs"
                    onClick={() => setSelection({ kind: "pack", packId: selectedPack.id })}
                  >
                    <FolderOpen /> Back to pack
                  </DropdownMenuItem>
                )}
                {selectedPack?.available && (
                  <DropdownMenuItem className="rounded-sm text-xs" onClick={() => void api.openAsset(selectedPack.rootPath)}>
                    <FolderOpen /> Open folder
                  </DropdownMenuItem>
                )}
                {selectedPack?.available && (
                  <DropdownMenuItem className="rounded-sm text-xs" onClick={() => void rescanPack(selectedPack)}>
                    <RefreshCw /> Rescan
                  </DropdownMenuItem>
                )}
                {selectedPack && !selectedPack.available && (
                  <DropdownMenuItem className="rounded-sm text-xs" onClick={() => void relocatePack(selectedPack)}>
                    <FolderCog /> Fix location
                  </DropdownMenuItem>
                )}
                {selectedPack && (
                  <DropdownMenuItem className="rounded-sm text-xs" onClick={() => startRenamePack(selectedPack)}>
                    <Pencil /> Rename
                  </DropdownMenuItem>
                )}
                {selection.kind === "pack" && selectedPack && selectedPack.removedAssetCount > 0 && (
                  <DropdownMenuItem
                    className="rounded-sm text-xs"
                    onClick={() => {
                      setSelection({ kind: "removed", packId: selectedPack.id });
                      clearAssetSelection();
                    }}
                  >
                    <ArchiveRestore /> Removed items
                  </DropdownMenuItem>
                )}
                {selectedPack && <DropdownMenuSeparator />}
                {selection.kind === "project" && selectedProject && (
                  <DropdownMenuItem className="rounded-sm text-xs" disabled={!selectedProject.available} onClick={() => void api.openAsset(selectedProject.rootPath).catch((caught) => reportError(caught, "open-project"))}>
                    <FolderOpen /> Open project folder
                  </DropdownMenuItem>
                )}
                {activeProjectId === null && (selectedPack || selection.kind === "collection") && (
                  <DropdownMenuItem
                    variant="destructive"
                    className="rounded-sm text-xs"
                    onClick={() => setConfirmDelete(true)}
                  >
                    <Trash2 /> {deletingPack ? "Forget pack" : "Delete collection"}
                  </DropdownMenuItem>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>

        <div
          ref={setAssetScrollRef}
          data-asset-browser
          className="quiet-scrollbar min-h-0 flex-1 overflow-y-auto"
          role={assets.length > 0 ? "listbox" : undefined}
          aria-label={assets.length > 0 ? `${sectionTitle} assets` : undefined}
          aria-multiselectable={assets.length > 0 ? true : undefined}
          aria-busy={loading || loadingMore}
          onClick={(event) => {
            const target = event.target as HTMLElement;
            if (!target.closest("[data-asset-card]")) clearAssetSelection();
          }}
        >
          {loading && assets.length === 0 ? (
            <div
              className="grid gap-3 p-4"
              style={{ gridTemplateColumns: view === "grid" ? `repeat(${gridColumns}, minmax(0, 1fr))` : "1fr" }}
              aria-label="Loading assets"
            >
              {Array.from({ length: view === "grid" ? Math.max(gridColumns * 3, 6) : 10 }, (_, index) => (
                <div key={index} className={cn("min-w-0", view === "list" && "flex h-12 items-center gap-3 px-2")}>
                  <div className={cn("skeleton-shimmer rounded-md", view === "grid" ? "aspect-[4/3]" : "size-8 shrink-0")} />
                  <div className={cn("mt-2 space-y-1.5", view === "list" && "mt-0 flex-1")}>
                    <div className="skeleton-shimmer h-2.5 w-2/3 rounded" />
                    <div className="skeleton-shimmer h-2 w-1/3 rounded opacity-70" />
                  </div>
                </div>
              ))}
            </div>
          ) : assets.length > 0 ? (
            <>
            {view === "list" && (
              <div className="sticky top-0 z-10 grid h-8 grid-cols-[34px_minmax(130px,0.8fr)_minmax(120px,0.7fr)_minmax(0,1.2fr)_84px] items-center gap-3 border-b bg-background/95 px-4 text-[11px] font-medium text-muted-foreground backdrop-blur-sm">
                <span />
                <span>Name</span>
                <span>Specs</span>
                <span>Location</span>
                <span className="text-right">Format · size</span>
              </div>
            )}
            <div
              className="relative w-full"
              style={{
                height: virtualizer.getTotalSize() + 24,
                contain: "layout paint",
              }}
            >
              {virtualRows.map((virtualRow) => {
                if (virtualRow.index >= assetRowCount) {
                  return (
                    <div
                      key={virtualRow.key}
                      className="absolute top-0 left-0 grid w-full place-items-center text-muted-foreground"
                      style={{
                        height: virtualRow.size,
                        transform: `translateY(${virtualRow.start}px)`,
                        contain: "layout paint",
                      }}
                    >
                      {loadingMore && <LoaderCircle className="size-3.5 animate-spin" />}
                    </div>
                  );
                }

                const rowAssets =
                  view === "grid"
                    ? assets.slice(
                        virtualRow.index * gridColumns,
                        (virtualRow.index + 1) * gridColumns,
                      )
                    : [assets[virtualRow.index]];
                return (
                  <div
                    key={virtualRow.key}
                    className={cn(
                      "absolute top-0 left-0",
                      view === "grid"
                        ? "grid w-full gap-x-3 px-4"
                        : "right-2 left-2",
                    )}
                    style={{
                      width: view === "grid" ? "100%" : undefined,
                      height: `${virtualRow.size}px`,
                      gridTemplateColumns:
                        view === "grid"
                          ? `repeat(${gridColumns}, minmax(0, 1fr))`
                          : undefined,
                      transform: `translateY(${virtualRow.start}px)`,
                      contain: "layout paint",
                    }}
                  >
                    {rowAssets.filter(Boolean).map((asset, columnIndex) => {
                      const optionIndex = view === "grid"
                        ? virtualRow.index * gridColumns + columnIndex
                        : virtualRow.index;
                      return (
                      <AssetCard
                        key={asset.id}
                        asset={asset}
                        selected={selectedIds.has(asset.id)}
                        view={view}
                        onSelect={selectAsset}
                        onContextSelect={selectAssetForContextMenu}
                        onOpen={openAsset}
                        onReveal={revealAsset}
                        onRemove={requestDisplayedAssetRemoval}
                        onRestore={restoreAsset}
                        removed={selection.kind === "removed"}
                        projectAsset={selection.kind === "project"}
                        allowRemove={selection.kind === "removed" || selection.kind === "project" || activeProjectId === null}
                        selectionCount={selectedIds.has(asset.id) ? selectedIds.size : 1}
                        dragPaths={selectedIds.has(asset.id) ? selectedDragPaths : EMPTY_PATHS}
                        onCopyPath={copyAssetPath}
                        onError={reportCardError}
                        onPreviewError={reportCardPreviewError}
                        optionId={`asset-option-${asset.id}`}
                        optionIndex={optionIndex}
                        optionCount={assetTotal}
                        tabIndex={selectedId === asset.id || (selectedId === null && optionIndex === 0) ? 0 : -1}
                      />
                      );
                    })}
                  </div>
                );
              })}
            </div>
            </>
          ) : (
            <div className="grid h-full min-h-64 place-items-center">
              {selection.kind === "removed" ? (
                <EmptyState icon={ArchiveRestore} title="No removed assets" description={selection.packId === undefined ? "Assets removed from Lootbox appear here." : "Removed assets from this pack appear here."} />
              ) : selection.kind === "missing" ? (
                <EmptyState icon={FolderCog} title="No missing files" description={selection.packId === undefined ? "Missing source files across the library appear here." : "Missing source files from this pack appear here."} />
              ) : selection.kind === "duplicates" ? (
                <EmptyState icon={Copy} title={snapshot.hashingAssets ? "Checking file contents" : "No duplicate files"} description={snapshot.hashingAssets ? "This view updates when the check finishes." : "No indexed files have matching contents."} />
              ) : selection.kind === "project" ? (
                <EmptyState icon={Gamepad2} title="No project assets" description="Select library assets and export them to the active project." />
              ) : snapshot.totalAssets === 0 ? (
                <EmptyState icon={FolderPlus} title="No asset packs" description="Import folders to build a local catalog. Lootbox indexes them in place and never modifies source files." action={{ label: "Import packs", onClick: () => void importPack() }} acknowledgment="archive" />
              ) : (
                <EmptyState icon={SearchX} title="No matching assets" description="Try another search or clear the filters." action={activeFilters.length > 0 ? { label: "Clear filters", onClick: () => setFilters({ ...clearedFilters }) } : undefined} />
              )}
            </div>
          )}
        </div>
        </>
        )}
      </main>

      <div
        role="separator"
        aria-label="Resize details panel"
        aria-orientation="vertical"
        aria-valuemin={260}
        aria-valuemax={480}
        aria-valuenow={Math.round(layoutRightPanelWidth || rightPanelWidth)}
        aria-valuetext={`${Math.round(layoutRightPanelWidth || rightPanelWidth)} pixels`}
        tabIndex={0}
        className="group relative z-20 flex w-1.5 shrink-0 cursor-col-resize items-center justify-center outline-none bg-border/40 hover:bg-primary/50 transition-colors select-none"
        onPointerDown={(event) => startPanelResize("right", event)}
        onKeyDown={(event) => resizePanelWithKeyboard("right", event)}
      >
        <button
          type="button"
          aria-label={rightPanelCollapsed ? "Expand inspector (Ctrl+I)" : "Collapse inspector (Ctrl+I)"}
          title={rightPanelCollapsed ? "Expand inspector (Ctrl+I)" : "Collapse inspector (Ctrl+I)"}
          onClick={(e) => {
            e.stopPropagation();
            setRightPanelCollapsed((c) => !c);
          }}
          className="absolute z-30 flex size-5.5 items-center justify-center rounded-full border bg-background text-muted-foreground shadow-xs transition-transform hover:scale-115 hover:text-foreground cursor-pointer"
        >
          {rightPanelCollapsed ? <ChevronLeft className="size-3.5" /> : <ChevronRight className="size-3.5" />}
        </button>
      </div>

      {!rightPanelCollapsed && (
        <div
          className="h-full shrink-0 overflow-hidden"
          style={{ width: `${layoutRightPanelWidth}px` }}
        >
          {selectedAsset ? (
            <DetailPanel
              asset={selectedAsset}
              selectedCount={Math.max(selectedIds.size, 1)}
              selectedAssets={selectedAssets}
              tagInputRef={tagInputRef}
              busy={editingSelection}
              collections={snapshot.collections}
              onAddTag={handleDetailAddTag}
              onRemoveTag={handleDetailRemoveTag}
              onMembership={handleDetailMembership}
              onClassification={handleDetailClassification}
              onGroup={handleDetailGroup}
              onResetClassification={handleDetailResetClassification}
              onAddCollection={handleDetailAddCollection}
              onCopyPath={copyAssetPath}
              onOpen={handleDetailOpen}
              onOpenVariant={handleDetailOpenVariant}
              onRevealPath={handleDetailRevealPath}
            />
          ) : (
            <aside className="h-full min-w-0 border-l bg-background">
              <header className="flex h-[58px] items-center border-b px-4">
                <h2 className="text-xs font-semibold">Details</h2>
              </header>
              <div className="flex h-[calc(100%-58px)] items-center justify-center px-6 text-center">
                <div>
                  <p className="text-xs font-medium">No asset selected</p>
                  <p className="mt-1 text-[11px] text-muted-foreground">Select an asset, or use the arrow keys to browse.</p>
                </div>
              </div>
            </aside>
          )}
        </div>
      )}

      {(error || notice || godotExportNotice) && (
        <div className={cn(
          "fixed top-3 left-1/2 z-[80] flex max-w-[min(680px,calc(100vw-32px))] -translate-x-1/2 items-start gap-2 rounded-md border bg-popover px-3 py-2 text-xs shadow-lg",
          error ? "border-destructive/40" : "border-primary/35",
        )} role={error ? "alert" : "status"} aria-live={error ? "assertive" : "polite"} aria-atomic="true">
          {error ? <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" /> : <Check className="mt-0.5 size-4 shrink-0 text-primary" />}
          <div className="min-w-0 flex-1">
            {errorPresentation ? (
              <><p className="font-medium">{errorPresentation.title}</p><p className="mt-0.5 text-[11px] text-muted-foreground">{errorPresentation.message}</p></>
            ) : godotCompletion ? (
              <><p className="font-medium">{godotCompletion.title}</p><p className="mt-0.5 text-[11px] text-muted-foreground">{godotCompletion.message}</p></>
            ) : <span className="break-words">{notice}</span>}
          </div>
          {error && errorRecovery && (
            <Button type="button" variant="outline" size="sm" className="shrink-0" onClick={() => { setError(null); errorRecovery.run(); }}>
              {errorRecovery.label}
            </Button>
          )}
          {error && (
            <Button type="button" variant="ghost" size="sm" onClick={() => void navigator.clipboard.writeText(error).then(() => {
              setNotice("Technical details copied");
            }).catch((caught) => reportError(caught, "copy-error"))}>
              Copy technical details
            </Button>
          )}
          {!error && undoRemoval && (
            <Button type="button" variant="ghost" size="sm" onClick={() => void api.setAssetsExcluded(undoRemoval.ids, false).then(async () => {
              setUndoRemoval(null);
              setNotice(`${undoRemoval.label} restored`);
              await refresh();
            }).catch((caught) => reportError(caught, "undo-remove"))}>
              Undo
            </Button>
          )}
          {!error && !undoRemoval && metadataUndo && (
            <Button type="button" variant="ghost" size="sm" onClick={() => void metadataUndo.run().then(() => { setNotice("Metadata change undone"); setMetadataUndo(null); }).catch((caught) => reportError(caught, "undo-metadata"))}>
              {metadataUndo.label}
            </Button>
          )}
          {!error && godotExportNotice?.project.rootPath && (
            <Button type="button" variant="ghost" size="sm" className="shrink-0" onClick={() => void api.openAsset(godotExportNotice.project.rootPath).catch((caught) => reportError(caught, "open-project"))}>
              <FolderOpen /> Open project
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={error ? "Dismiss error" : "Dismiss notification"}
            onClick={() => {
              if (error) setError(null);
              else {
                setNotice(null);
                setGodotExportNotice(null);
                setUndoRemoval(null);
              }
            }}
          >
            <X />
          </Button>
        </div>
      )}

      <Dialog open={godotExport !== null} onOpenChange={(open) => {
        if (!open && !godotExport?.exporting) {
          godotPreviewRequestRef.current += 1;
          setGodotExport(null);
        }
      }}>
        <DialogContent className="gap-4 sm:max-w-lg">
          <>
              <DialogHeader className="gap-1">
                <DialogTitle className="text-sm">Review Godot export</DialogTitle>
                <DialogDescription className="sr-only">Choose model formats and confirm the export contents.</DialogDescription>
              </DialogHeader>
              {!godotExport?.preview ? godotExport?.loading ? (
                <div className="space-y-3 py-6 text-center" role="status" aria-live="polite">
                  <LoaderCircle className="mx-auto size-5 animate-spin text-primary" />
                  <p className="text-xs text-muted-foreground">Checking related files and destination conflicts…</p>
                </div>
              ) : (
                <div className="space-y-3 rounded-md border border-destructive/35 bg-destructive/5 p-4 text-xs">
                  <div className="flex items-start gap-2">
                    <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
                    <div><p className="font-medium">The export review could not be prepared.</p><p className="mt-1 text-muted-foreground">Check that the active project and source files are available, then retry.</p></div>
                  </div>
                  <Button type="button" variant="outline" size="sm" onClick={() => godotExport && void addSelectionToGodot(godotExport.project.id)}>Retry review</Button>
                </div>
              ) : (
                <>
                  {godotExport.preview.modelFormats.length > 1 && (
                    <fieldset className="rounded-md border bg-muted/10 px-3 py-2.5">
                      <legend className="px-1 text-[11px] font-medium">Model formats</legend>
                      <div className="flex flex-wrap gap-x-4 gap-y-2">
                        {godotExport.preview.modelFormats.map((format) => {
                          const checked = godotExport.selectedModelFormats.includes(format.extension);
                          const lastSelected = checked && godotExport.selectedModelFormats.length === 1;
                          return (
                            <label key={format.extension} className="flex min-h-7 items-center gap-2 text-[11px]">
                              <Checkbox
                                checked={checked}
                                disabled={godotExport.loading || godotExport.exporting || lastSelected}
                                onCheckedChange={(nextChecked) => void updateGodotModelFormat(format.extension, Boolean(nextChecked))}
                                aria-label={`${checked ? "Exclude" : "Include"} ${format.extension.toUpperCase()} models`}
                              />
                              <span className="font-medium uppercase">{format.extension}</span>
                              <span className="text-muted-foreground">{format.count.toLocaleString()}</span>
                            </label>
                          );
                        })}
                      </div>
                      <div className="mt-1.5 flex min-h-4 items-center gap-1.5 text-[11px] text-muted-foreground" aria-live="polite">
                        {godotExport.loading && <LoaderCircle className="size-3 animate-spin" />}
                        <span>{godotExport.loading ? "Updating included files…" : "Required companion files stay included automatically."}</span>
                      </div>
                    </fieldset>
                  )}
                  <dl className="grid grid-cols-[112px_minmax(0,1fr)] gap-y-2 rounded-md border bg-muted/10 p-3 text-[11px]">
                    <dt className="text-muted-foreground">Project</dt><dd className="min-w-0" title={collapseHomePath(godotExport.project.rootPath)}><span className="block truncate">{godotExport.project.name}</span><span className="block truncate font-mono text-[11px] text-muted-foreground">{collapseHomePath(godotExport.project.rootPath)}</span></dd>
                    <dt className="text-muted-foreground">Selected</dt><dd>{godotExport.preview.selected.toLocaleString()} assets</dd>
                    <dt className="text-muted-foreground">Grouped files</dt><dd>{godotExport.preview.grouped.toLocaleString()} related maps or formats</dd>
                    <dt className="text-muted-foreground">Dependencies</dt><dd>{godotExport.preview.dependencies.toLocaleString()} referenced files</dd>
                    <dt className="text-muted-foreground">Files to check</dt><dd>{godotExport.preview.totalFiles.toLocaleString()}</dd>
                    <dt className="text-muted-foreground">Conflicts</dt><dd>{godotExport.preview.conflicts === 0 ? "None" : `${godotExport.preview.conflicts.toLocaleString()} will receive safe Lootbox names`}</dd>
                    <dt className="text-muted-foreground">Destination</dt><dd className="font-mono">{godotExport.preview.destination}</dd>
                    <dt className="text-muted-foreground">Manifest</dt><dd className="font-mono break-all">{godotExport.preview.manifest}</dd>
                  </dl>
                  {godotExport.preview.conflictFiles.length > 0 && (
                    <div>
                      <p className="mb-1.5 text-[11px] font-medium">Safe conflict names</p>
                      <div className="quiet-scrollbar max-h-20 overflow-y-auto rounded-md border bg-background p-2 font-mono text-[11px] text-muted-foreground">
                        {godotExport.preview.conflictFiles.map((file) => <p key={file} className="truncate" title={file}>{file}</p>)}
                      </div>
                    </div>
                  )}
                  <div>
                    <p className="mb-1.5 text-[11px] font-medium">Included files</p>
                    <div className="quiet-scrollbar max-h-28 overflow-y-auto rounded-md border bg-background p-2 font-mono text-[11px] text-muted-foreground">
                      {godotExport.preview.files.slice(0, 50).map((file) => <p key={file} className="truncate" title={file}>{file}</p>)}
                      {godotExport.preview.files.length > 50 && <p className="mt-1 text-foreground">+ {godotExport.preview.files.length - 50} more</p>}
                    </div>
                  </div>
                  {godotExport.exporting && (
                    <div role="status" aria-live="polite" className="space-y-2">
                      <p className="text-[11px] text-muted-foreground">Copying files and updating the manifest…</p>
                      <Progress value={null} aria-label="Exporting assets to Godot" />
                    </div>
                  )}
                  {errorContext === "godot-export" && error && (
                    <p className="text-[11px] text-destructive" role="status">The export can be retried safely; files already copied will be detected as current.</p>
                  )}
                </>
              )}
              <DialogFooter>
                <Button type="button" variant="outline" size="sm" onClick={() => {
                  godotPreviewRequestRef.current += 1;
                  setGodotExport(null);
                }} disabled={godotExport?.exporting}>Cancel</Button>
                <Button type="button" size="sm" onClick={() => void confirmGodotExport()} disabled={!godotExport?.preview || godotExport.loading || godotExport.exporting}>
                  {godotExport?.exporting ? <><LoaderCircle className="animate-spin" /> Exporting…</> : `Export ${godotExport?.preview?.totalFiles.toLocaleString() ?? ""} files`}
                </Button>
              </DialogFooter>
          </>
        </DialogContent>
      </Dialog>

      <Dialog open={godotProjectRemoval !== null} onOpenChange={(open) => {
        if (!open && !godotProjectRemoval?.removing) setGodotProjectRemoval(null);
      }}>
        <DialogContent className="gap-4 sm:max-w-lg">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">Remove from {godotProjectRemoval?.project.name ?? "project"}?</DialogTitle>
            <DialogDescription className="text-xs">
              Lootbox will remove its exported copies from this project. Original files in your asset packs stay untouched.
            </DialogDescription>
          </DialogHeader>
          {!godotProjectRemoval?.preview ? (
            <div className="space-y-3 py-6 text-center" role="status" aria-live="polite">
              <LoaderCircle className="mx-auto size-5 animate-spin text-primary" />
              <p className="text-xs text-muted-foreground">Checking tracked files and project changes…</p>
            </div>
          ) : (
            <>
              <dl className="grid grid-cols-[128px_minmax(0,1fr)] gap-y-2 rounded-md border bg-muted/10 p-3 text-[11px]">
                <dt className="text-muted-foreground">Selected</dt><dd>{godotProjectRemoval.preview.selected.toLocaleString()} {godotProjectRemoval.preview.selected === 1 ? "asset" : "assets"}</dd>
                <dt className="text-muted-foreground">Files removed</dt><dd>{godotProjectRemoval.preview.removeFiles.length.toLocaleString()}</dd>
                <dt className="text-muted-foreground">Modified files kept</dt><dd>{godotProjectRemoval.preview.modifiedFiles.length.toLocaleString()}</dd>
                <dt className="text-muted-foreground">Shared files kept</dt><dd>{godotProjectRemoval.preview.sharedFiles.length.toLocaleString()}</dd>
                <dt className="text-muted-foreground">Missing records cleaned</dt><dd>{godotProjectRemoval.preview.missingFiles.length.toLocaleString()}</dd>
                <dt className="text-muted-foreground">Location</dt><dd className="font-mono">{godotProjectRemoval.preview.destination}</dd>
              </dl>
              {godotProjectRemoval.preview.modifiedFiles.length > 0 && (
                <div className="flex gap-2 rounded-md border bg-muted/10 p-2.5 text-[11px]">
                  <AlertCircle className="mt-0.5 size-3.5 shrink-0 text-primary" />
                  <p><span className="font-medium">Project edits are protected.</span> Modified files stay in place, and Lootbox stops tracking them.</p>
                </div>
              )}
              {godotProjectRemoval.preview.sharedFiles.length > 0 && (
                <p className="text-[11px] text-muted-foreground">Shared files remain tracked because other exported assets still need them.</p>
              )}
              <div>
                <p className="mb-1.5 text-[11px] font-medium">Unchanged files removed</p>
                <div className="quiet-scrollbar max-h-28 overflow-y-auto rounded-md border bg-background p-2 font-mono text-[11px] text-muted-foreground">
                  {godotProjectRemoval.preview.removeFiles.length > 0
                    ? godotProjectRemoval.preview.removeFiles.slice(0, 50).map((file) => <p key={file} className="truncate" title={file}>{file}</p>)
                    : <p className="font-sans">No unchanged files need deletion.</p>}
                  {godotProjectRemoval.preview.removeFiles.length > 50 && <p className="mt-1 text-foreground">+ {godotProjectRemoval.preview.removeFiles.length - 50} more</p>}
                </div>
              </div>
              {godotProjectRemoval.removing && (
                <div role="status" aria-live="polite" className="space-y-2">
                  <p className="text-[11px] text-muted-foreground">Removing tracked copies and updating the manifest…</p>
                  <Progress value={null} aria-label="Removing assets from Godot project" />
                </div>
              )}
            </>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" size="sm" onClick={() => setGodotProjectRemoval(null)} disabled={godotProjectRemoval?.removing}>Cancel</Button>
            <Button type="button" variant="destructive" size="sm" onClick={() => void confirmProjectAssetRemoval()} disabled={!godotProjectRemoval?.preview || godotProjectRemoval.loading || godotProjectRemoval.removing || godotProjectRemoval.preview.selected === 0}>
              {godotProjectRemoval?.removing
                ? <><LoaderCircle className="animate-spin" /> Removing…</>
                : `Remove ${godotProjectRemoval?.preview?.selected.toLocaleString() ?? ""} ${godotProjectRemoval?.preview?.selected === 1 ? "asset" : "assets"}`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={reviewSelectionOpen} onOpenChange={(open) => {
        setReviewSelectionOpen(open);
        if (open) setReviewSelectionLimit(250);
      }}>
        <DialogContent className="gap-4 sm:max-w-lg">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">Review selection</DialogTitle>
            <DialogDescription className="text-xs">{selectionSummary}. Inspector changes apply to every item below.</DialogDescription>
          </DialogHeader>
          <div className="quiet-scrollbar max-h-80 overflow-y-auto rounded-md border" role="list" aria-label="Selected assets">
            {[...selectedIds].slice(0, reviewSelectionLimit).map((id) => {
              const item = selectedAssetCacheRef.current.get(id) ?? assets.find((asset) => asset.id === id);
              const path = item?.relativePath ?? selectedPathCacheRef.current.get(id) ?? `Asset ${id}`;
              const name = item?.name ?? path.split(/[\\/]/).at(-1) ?? `Asset ${id}`;
              return <div key={id} role="listitem" className="grid grid-cols-[minmax(120px,0.45fr)_minmax(0,1fr)_28px] items-center gap-3 border-b px-3 py-2 last:border-b-0">
                <div className="min-w-0"><p className="truncate text-xs font-medium">{name}</p><p className="truncate text-[11px] text-muted-foreground">{item ? `${item.packName} · ${typeLabels[item.assetType]}` : "Result not currently loaded"}</p></div>
                <p className="self-center truncate font-mono text-[11px] text-muted-foreground" title={path}>{path}</p>
                <Button type="button" variant="ghost" size="icon-xs" className="rounded-sm text-muted-foreground hover:text-destructive" onClick={() => deselectReviewedAsset(id)} aria-label={`Remove ${name} from selection`} title="Remove from selection"><X /></Button>
              </div>;
            })}
            {selectedIds.size > reviewSelectionLimit && <div className="px-3 py-2"><Button type="button" variant="ghost" size="xs" className="w-full rounded-sm text-[11px] text-muted-foreground" onClick={() => setReviewSelectionLimit((current) => current + 250)}>Show {Math.min(250, selectedIds.size - reviewSelectionLimit).toLocaleString()} more selected assets</Button></div>}
          </div>
          <DialogFooter><Button type="button" size="sm" onClick={() => setReviewSelectionOpen(false)}>Done</Button></DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog open={pendingBulkMutation !== null} onOpenChange={(open) => { if (!open) setPendingBulkMutation(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{pendingBulkMutation?.title}</AlertDialogTitle>
            <AlertDialogDescription>{pendingBulkMutation?.description}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => {
              const pending = pendingBulkMutation;
              setPendingBulkMutation(null);
              if (pending) void pending.run();
            }}>Apply to {selectedIds.size.toLocaleString()} assets</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog open={creatingCollection} onOpenChange={(open) => {
        setCreatingCollection(open);
        if (!open) setAddSelectionToNewCollection(false);
      }}>
        <DialogContent className="gap-4 sm:max-w-xs">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">New collection</DialogTitle>
          </DialogHeader>
          <form onSubmit={createCollection} className="space-y-3">
            <label className="block text-[11px] font-medium text-muted-foreground">Collection name</label>
            <Input
              value={collectionName}
              onChange={(event) => setCollectionName(event.target.value)}
              placeholder="Name"
              aria-label="Collection name"
              autoFocus
              className="text-xs"
            />
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  setCreatingCollection(false);
                  setAddSelectionToNewCollection(false);
                }}
              >
                Cancel
              </Button>
              <Button type="submit" size="sm">{addSelectionToNewCollection ? "Create and add" : "Create collection"}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog open={savingView} onOpenChange={setSavingView}>
        <DialogContent className="gap-4 sm:max-w-xs">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">Save current view</DialogTitle>
            <DialogDescription className="text-xs">Keeps this scope, search, filters, and sorting together.</DialogDescription>
          </DialogHeader>
          <form onSubmit={saveCurrentView} className="space-y-3">
            <label className="block text-xs font-medium text-muted-foreground">View name</label>
            <Input value={savedViewName} onChange={(event) => setSavedViewName(event.target.value)} autoFocus aria-label="Saved view name" className="text-xs" />
            <DialogFooter>
              <Button type="button" variant="outline" size="sm" onClick={() => setSavingView(false)}>Cancel</Button>
              <Button type="submit" size="sm" disabled={!savedViewName.trim()}>Save view</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={renamingPack !== null}
        onOpenChange={(open) => {
          if (!open) setRenamingPack(null);
        }}
      >
        <DialogContent className="gap-4 sm:max-w-xs">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">Rename pack</DialogTitle>
            <DialogDescription className="text-xs">Only the library label changes; the source folder stays untouched.</DialogDescription>
          </DialogHeader>
          <form onSubmit={renamePack} className="space-y-3">
            <label className="block text-[11px] font-medium text-muted-foreground">Display name</label>
            <Input
              value={packName}
              onChange={(event) => setPackName(event.target.value)}
              aria-label="Pack name"
              autoFocus
              className="text-xs"
            />
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setRenamingPack(null)}
              >
                Cancel
              </Button>
              <Button type="submit" size="sm">Save name</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={confirmAssetRemoval.length > 0}
        onOpenChange={(open) => {
          if (!open) setConfirmAssetRemoval([]);
        }}
      >
        <AlertDialogContent className="rounded-md">
          <AlertDialogHeader>
            <AlertDialogTitle>
              Remove {confirmAssetRemoval.length} assets?
            </AlertDialogTitle>
            <AlertDialogDescription>
              They will disappear from Lootbox, including grouped formats. The source files stay untouched.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="rounded-b-md">
            <AlertDialogCancel className="rounded-sm">Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              className="rounded-sm"
              onClick={() => void removeAssetFromLootbox()}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialogContent className="rounded-md">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {deletingPack ? "Forget this pack?" : "Delete this collection?"}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {deletingPack ? "Files stay where they are." : "Assets stay in the library."}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="rounded-b-md">
            <AlertDialogCancel className="rounded-sm">Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              className="rounded-sm"
              onClick={() => void deleteCurrentSource()}
            >
              {deletingPack ? "Forget" : "Delete"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>



      <Dialog open={shortcutsOpen} onOpenChange={setShortcutsOpen}>
        <DialogContent className="gap-4 sm:max-w-md">
          <DialogHeader className="gap-1">
            <DialogTitle>Keyboard shortcuts</DialogTitle>
          </DialogHeader>
          <div className="grid grid-cols-2 gap-x-6 gap-y-2.5 text-xs">
            {[
              ["Ctrl K", "Command palette"],
              ["Ctrl F", "Search"],
              ["Ctrl Shift F", "Filters"],
              ["Ctrl B", "Toggle sidebar"],
              ["Ctrl I", "Toggle inspector"],
              ["Ctrl A", "Select all results"],
              ["G / L", "Grid / list view"],
              ["Ctrl E", "Export to active project"],
              ["T", "Add a tag to selection"],
              ["Ctrl Shift C", "New collection from selection"],
              ["↑ ↓ ← →", "Navigate assets"],
              ["Shift + arrows", "Extend selection"],
              ["Ctrl + click", "Toggle selection"],
              ["Enter", "Open selected asset"],
              ["Space", "Play or pause audio"],
              ["Delete", activeProject ? "Remove from active project view" : "Remove from Lootbox"],
              ["Ctrl Shift A", "Clear selection"],
              ["?", "Show shortcuts"],
            ].map(([keys, action]) => (
              <div key={keys} className="contents">
                <Kbd className="w-fit text-foreground">{keys}</Kbd>
                <span className="self-center text-muted-foreground">{action}</span>
              </div>
            ))}
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={settingsOpen} onOpenChange={(open) => {
        setSettingsOpen(open);
        if (open) setSettingsMessage("");
      }}>
        <DialogContent className="gap-4 sm:max-w-xl">
          <DialogHeader className="gap-1">
            <DialogTitle className="text-sm">Maintenance</DialogTitle>
          </DialogHeader>
          <section className="space-y-3 pb-4">
            <div className="mb-2 flex items-start gap-2.5">
              <DatabaseBackup className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <div><h3 className="text-xs font-medium">Metadata backups</h3>
              <p className="mt-0.5 text-[11px] text-muted-foreground">Five rotating safety backups are maintained automatically.</p></div>
            </div>
            <div className="flex flex-wrap gap-2 pl-6.5">
              <Button type="button" variant="outline" size="sm" className="rounded-sm text-xs" onClick={() => void (async () => {
                const destination = await save({ title: "Export Lootbox metadata", defaultPath: "lootbox-metadata.db", filters: [{ name: "SQLite database", extensions: ["db"] }] });
                if (!destination) return;
                const path = await api.createBackup(destination);
                setSettingsMessage(`Backup saved to ${path}`);
              })().catch((caught) => reportError(caught, "backup-export"))}>Export backup</Button>
              <Button type="button" variant="outline" size="sm" className="rounded-sm text-xs" onClick={() => void (async () => {
                const path = await open({ multiple: false, directory: false, title: "Restore Lootbox metadata", filters: [{ name: "SQLite database", extensions: ["db"] }] });
                if (!path || Array.isArray(path)) return;
                setPendingRestorePath(path);
              })().catch((caught) => reportError(caught, "backup-restore-picker"))}>Restore backup</Button>
            </div>
          </section>
          <section className="space-y-3 border-t py-4">
            <div className="mb-2 flex items-start gap-2.5">
              <HardDrive className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <div><h3 className="text-xs font-medium">Preview cache</h3>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {cacheStatus ? `${cacheStatus.thumbnailFiles.toLocaleString()} files · ${formatBytes(cacheStatus.thumbnailBytes)} · ${cacheStatus.orphanFiles.toLocaleString()} orphaned` : "Loading…"}
            </p></div></div>
            <div className="flex flex-wrap gap-2 pl-6.5">
              <Button type="button" variant="outline" size="sm" className="rounded-sm text-xs" onClick={() => void api.cleanCache().then((status) => queryClient.setQueryData(["cache-status"], status)).catch((caught) => reportError(caught, "cache-clean"))}>Clean unused</Button>
              <Button type="button" variant="outline" size="sm" className="rounded-sm text-xs" onClick={() => {
                setSettingsMessage("Regenerating image previews…");
                void api.regenerateImageThumbnails().then((status) => {
                  queryClient.setQueryData(["cache-status"], status);
                  setSettingsMessage("Image previews regenerated; model previews regenerate as they appear.");
                  return refresh();
                }).catch((caught) => reportError(caught, "cache-regenerate"));
              }}>Regenerate</Button>
              <Button type="button" variant="outline" size="sm" className="rounded-sm text-xs text-destructive" onClick={() => setConfirmClearCache(true)}>Clear all previews</Button>
            </div>
          </section>
          <section className="space-y-2 border-t pt-4">
            <div className="flex items-center justify-between">
              <h3 className="flex items-center gap-2 text-xs font-medium"><Activity className="size-3.5 text-muted-foreground" /> Diagnostics</h3>
              <Button type="button" variant="ghost" size="sm" className="rounded-sm text-xs" onClick={() => void navigator.clipboard.writeText(diagnostics.map((entry) => `${new Date(entry.timestamp * 1000).toISOString()} [${entry.context}] ${entry.message}`).join("\n")).then(() => setNotice("Diagnostics copied")).catch((caught) => reportError(caught, "copy-diagnostics"))}>Copy details</Button>
            </div>
            <div className="quiet-scrollbar max-h-28 overflow-y-auto rounded-sm border bg-muted/10 p-2 font-mono text-[11px] text-muted-foreground">
              {diagnostics.length === 0 ? "No errors recorded this session." : diagnostics.slice(-20).reverse().map((entry, index) => <div key={`${entry.timestamp}-${index}`} className="mb-1 break-words">[{entry.context}] {entry.message}</div>)}
            </div>
          </section>
          {settingsMessage && <p className="text-[11px] text-primary" role="status" aria-live="polite">{settingsMessage}</p>}
        </DialogContent>
      </Dialog>

      <AlertDialog open={confirmProjectRemoval !== null} onOpenChange={(open) => { if (!open) setConfirmProjectRemoval(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Disconnect {confirmProjectRemoval?.name}?</AlertDialogTitle>
            <AlertDialogDescription>Lootbox will remove this project connection and its export history. Files already copied into the Godot project stay in place.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => confirmProjectRemoval && void forgetGodotProject(confirmProjectRemoval)}>Disconnect project</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmPurge !== null} onOpenChange={(open) => { if (!open) setConfirmPurge(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Purge missing records?</AlertDialogTitle>
            <AlertDialogDescription>{confirmPurge ? `${confirmPurge.missingAssetCount.toLocaleString()} missing records will be permanently removed from ${confirmPurge.name}. Source folders and files are never deleted.` : ""}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => confirmPurge && void purgeMissingRecords(confirmPurge)}>Purge records</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={pendingRestorePath !== null} onOpenChange={(open) => { if (!open) setPendingRestorePath(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Restore this metadata backup?</AlertDialogTitle>
            <AlertDialogDescription>Your current library metadata will be replaced after Lootbox creates a new safety backup. Asset source files are not affected.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void restoreSelectedBackup()}>Restore backup</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmClearCache} onOpenChange={setConfirmClearCache}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Clear every generated preview?</AlertDialogTitle>
            <AlertDialogDescription>Thumbnails and generated model previews will be removed. They regenerate as assets appear, and source files remain untouched.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => void clearAllPreviews()}>Clear previews</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <CommandPalette
        open={commandPaletteOpen}
        onOpenChange={setCommandPaletteOpen}
        snapshot={snapshot}
        activeProject={activeProject}
        savedViews={savedViews}
        selectedCount={selectedIds.size}
        view={view}
        leftPanelCollapsed={leftPanelCollapsed}
        rightPanelCollapsed={rightPanelCollapsed}
        onSelectScope={(scope) => {
          setSelection(scope);
          setActiveSavedViewId(null);
          clearAssetSelection();
        }}
        onOpenSavedView={openSavedView}
        onActivateProject={activateProject}
        onImportPack={() => void importPack()}
        onStartCollection={() => {
          setAddSelectionToNewCollection(selectedIds.size > 0);
          setCreatingCollection(true);
        }}
        onSaveCurrentView={() => {
          setSavedViewName("");
          setSavingView(true);
        }}
        onAddProject={() => void addGodotProject()}
        onExportToActiveProject={() => {
          if (activeProject?.available) void addSelectionToGodot(activeProject.id);
          else setNotice("Select an available workspace project before exporting");
        }}
        onOpenSettings={() => {
          setSettingsMessage("");
          setSettingsOpen(true);
        }}
        onOpenShortcuts={() => setShortcutsOpen(true)}
        onSetView={setView}
        onToggleSidebar={() => setLeftPanelCollapsed((current) => !current)}
        onToggleDetailPanel={() => setRightPanelCollapsed((current) => !current)}
        onSelectAll={() => void selectAllAssets()}
        onClearSelection={clearAssetSelection}
        onSetFilterType={(type) => {
          setFilters((current) => ({ ...current, type: type ?? "" }));
          setActiveSavedViewId(null);
        }}
        onSetSort={(sort) => {
          setSort(sort as "name" | "newest" | "largest" | "type");
          setActiveSavedViewId(null);
        }}
        onCleanCache={() => void api.cleanCache().then((status) => queryClient.setQueryData(["cache-status"], status)).catch((caught) => reportError(caught, "cache-clean"))}
        onClearCache={() => setConfirmClearCache(true)}
      />

      {importing && (
        <div className="quiet-import-arrival fixed right-4 bottom-4 z-50 w-80 rounded-lg border bg-popover/95 p-4 text-xs shadow-xl backdrop-blur-md" role="status" aria-live="polite" aria-atomic="true" aria-label="Import progress">
          <div className="mb-3 flex items-start gap-2.5">
            <LoaderCircle className="mt-0.5 size-4 shrink-0 animate-spin text-primary" />
            <div className="min-w-0 flex-1">
            <span className="block font-medium">
              {!importProgress
                ? "Waiting to import"
                : importProgress.phase === "scanning"
                ? "Scanning files"
                : importProgress.phase === "hashing"
                  ? "Checking file contents"
                : importProgress.phase === "indexing"
                  ? "Indexing library"
                : importProgress.phase === "finalizing"
                  ? "Shelving assets"
                  : "Import complete"}
            </span>
            <span className="mt-0.5 block text-[11px] text-muted-foreground">
              {pendingImportCount > 1 ? `${pendingImportCount} packs remaining` : "1 pack remaining"}
            </span>
            </div>
            {importProgress && importProgress.total > 0 && (
              <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
                {importProgress.current.toLocaleString()} / {importProgress.total.toLocaleString()}
              </span>
            )}
          </div>
          <ImportStageRail phase={importProgress?.phase ?? null} />
          <Progress
            value={
              importProgress && importProgress.total > 0
                ? (importProgress.current / importProgress.total) * 100
                : null
            }
            className="gap-0"
            aria-label="Folder import progress"
          />
          {importProgress?.path && (
            <p
              className="mt-2 truncate font-mono text-[11px] text-muted-foreground"
              title={importProgress.path}
            >
              {importProgress.path}
            </p>
          )}
          <div className="mt-3 flex items-center justify-between border-t pt-2.5">
            <span className="text-[11px] text-muted-foreground">
              {pendingImportCount > 1 ? `${pendingImportCount - 1} waiting` : ""}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="h-7 text-[11px] text-muted-foreground"
              onClick={() => void cancelImports().catch((caught) => reportError(caught, "cancel-import"))}
            >
              Cancel imports
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
