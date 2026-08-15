import type { AssetSort, AssetSortDirection, AssetType, LibrarySelection } from "./types";

export interface SavedViewFilters {
  extension: string;
  mapRole: string;
  tag: string;
  minWidth: string;
  minConfidence: string;
  status: string;
  projectUsage: string;
}

export interface SavedAssetView {
  id: string;
  name: string;
  query: string;
  filters: SavedViewFilters;
  sort: AssetSort;
  sortDirection: AssetSortDirection;
  selection: LibrarySelection;
}

const storageKey = "lootbox:saved-views";

const assetTypes = new Set<AssetType>([
  "image", "texture", "audio", "model", "video", "font", "shader", "material", "archive", "other",
]);
const assetSorts = new Set<AssetSort>(["name", "newest", "largest", "type"]);
const sortDirections = new Set<AssetSortDirection>(["asc", "desc"]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isPositiveId(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}

function readSelection(value: unknown): LibrarySelection | null {
  if (!isRecord(value) || typeof value.kind !== "string") return null;
  if (value.kind === "all" || value.kind === "health" || value.kind === "duplicates") return { kind: value.kind };
  if (value.kind === "type" && typeof value.assetType === "string" && assetTypes.has(value.assetType as AssetType)) {
    return { kind: "type", assetType: value.assetType as AssetType };
  }
  if (value.kind === "pack" && isPositiveId(value.packId)) return { kind: "pack", packId: value.packId };
  if (value.kind === "collection" && isPositiveId(value.collectionId)) return { kind: "collection", collectionId: value.collectionId };
  if (value.kind === "project" && isPositiveId(value.projectId)) return { kind: "project", projectId: value.projectId };
  if (value.kind === "removed" || value.kind === "missing") {
    if (value.packId === undefined) return { kind: value.kind };
    if (isPositiveId(value.packId)) return { kind: value.kind, packId: value.packId };
  }
  return null;
}

function readFilters(value: unknown): SavedViewFilters | null {
  if (!isRecord(value)) return null;
  const stringValue = (key: keyof SavedViewFilters) => typeof value[key] === "string" ? value[key] : "";
  const status = stringValue("status");
  const projectUsage = stringValue("projectUsage");
  if (status !== "" && status !== "missing") return null;
  if (projectUsage !== "" && projectUsage !== "active" && projectUsage !== "unused") return null;
  return {
    extension: stringValue("extension"),
    mapRole: stringValue("mapRole"),
    tag: stringValue("tag"),
    minWidth: stringValue("minWidth"),
    minConfidence: stringValue("minConfidence"),
    status,
    projectUsage,
  };
}

export function resolveSavedSelection(
  selection: LibrarySelection,
  projectIds: ReadonlySet<number>,
): { selection: LibrarySelection; staleProject: boolean } {
  if (selection.kind === "project" && !projectIds.has(selection.projectId)) {
    return { selection: { kind: "all" }, staleProject: true };
  }
  return { selection, staleProject: false };
}

export function readSavedViews(): SavedAssetView[] {
  try {
    const value = JSON.parse(window.localStorage.getItem(storageKey) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.flatMap((item): SavedAssetView[] => {
      if (!isRecord(item) || typeof item.id !== "string" || !item.id || typeof item.name !== "string" || !item.name || typeof item.query !== "string") return [];
      if (typeof item.sort !== "string" || !assetSorts.has(item.sort as AssetSort)) return [];
      if (typeof item.sortDirection !== "string" || !sortDirections.has(item.sortDirection as AssetSortDirection)) return [];
      const filters = readFilters(item.filters);
      const selection = readSelection(item.selection);
      if (!filters || !selection) return [];
      return [{
        id: item.id,
        name: item.name,
        query: item.query,
        filters,
        sort: item.sort as AssetSort,
        sortDirection: item.sortDirection as AssetSortDirection,
        selection,
      }];
    });
  } catch {
    return [];
  }
}

export function writeSavedViews(views: SavedAssetView[]) {
  window.localStorage.setItem(storageKey, JSON.stringify(views));
}
