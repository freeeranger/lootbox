export type AssetType =
  | "image"
  | "texture"
  | "audio"
  | "model"
  | "video"
  | "font"
  | "shader"
  | "material"
  | "archive"
  | "other";

export interface PackSummary {
  id: number;
  name: string;
  rootPath: string;
  assetCount: number;
  lastScannedAt: string | null;
  available: boolean;
  removedAssetCount: number;
  missingAssetCount: number;
}

export interface ImportProgress {
  phase: "scanning" | "hashing" | "indexing" | "finalizing" | "complete";
  current: number;
  total: number;
  path: string | null;
}

export interface CollectionSummary {
  id: number;
  name: string;
  assetCount: number;
}

export interface ProjectSummary {
  id: number;
  name: string;
  rootPath: string;
  assetCount: number;
  available: boolean;
}

export interface GodotExportResult {
  copied: number;
  unchanged: number;
  destination: string;
}

export interface TypeCount {
  assetType: AssetType;
  count: number;
}

export interface LibrarySnapshot {
  totalAssets: number;
  duplicateAssets: number;
  hashingAssets: boolean;
  packs: PackSummary[];
  collections: CollectionSummary[];
  projects: ProjectSummary[];
  typeCounts: TypeCount[];
}

export interface Asset {
  id: number;
  packId: number;
  packName: string;
  name: string;
  relativePath: string;
  absolutePath: string;
  extension: string;
  assetType: AssetType;
  fileType: AssetType;
  usage: "texture" | null;
  mapRole: string | null;
  resolution: string | null;
  classificationConfidence: number;
  classificationBasis: string;
  sizeBytes: number;
  modifiedAt: number;
  width: number | null;
  height: number | null;
  thumbnailPath: string | null;
  variants: AssetVariant[];
  resources: AssetResource[];
  tags: string[];
  collectionIds: number[];
  missing: boolean;
  manualClassification: boolean;
  contentHash: string | null;
  duplicateCount: number;
  duplicateLocations: DuplicateLocation[];
}

export interface DuplicateLocation {
  id: number;
  packName: string;
  relativePath: string;
  absolutePath: string;
  sizeBytes: number;
}

export interface AssetVariant {
  id: number;
  extension: string;
  assetType: AssetType;
  fileType: AssetType;
  usage: "texture" | null;
  mapRole: string | null;
  resolution: string | null;
  absolutePath: string;
  relativePath: string;
  sizeBytes: number;
}

export interface AssetResource {
  id: number;
  name: string;
  extension: string;
  assetType: AssetType;
  fileType: AssetType;
  usage: "texture" | null;
  mapRole: string | null;
  resolution: string | null;
  absolutePath: string;
  relativePath: string;
  sizeBytes: number;
  thumbnailPath: string | null;
}

export interface AssetQuery {
  query?: string;
  assetId?: number;
  assetType?: AssetType;
  packId?: number;
  collectionId?: number;
  limit?: number;
  offset?: number;
  excluded?: boolean;
  sort?: AssetSort;
  sortDirection?: AssetSortDirection;
  extension?: string;
  mapRole?: string;
  tag?: string;
  minWidth?: number;
  minHeight?: number;
  minConfidence?: number;
  missing?: boolean;
  projectId?: number;
  duplicatesOnly?: boolean;
}

export type AssetSort = "name" | "newest" | "largest" | "type";
export type AssetSortDirection = "asc" | "desc";

export interface AssetPage {
  items: Asset[];
  total: number;
  hasMore: boolean;
}

export interface AssetSelection {
  id: number;
  absolutePath: string;
}

export interface AudioStatus {
  path: string | null;
  playing: boolean;
  positionSeconds: number;
  durationSeconds: number;
}

export interface AudioAnalysis {
  durationSeconds: number;
  peaks: number[];
}

export interface ModelStats {
  triangles: number;
  vertices: number;
}

export interface FilterOptions {
  extensions: string[];
  mapRoles: string[];
  tags: string[];
}

export interface CacheStatus {
  thumbnailFiles: number;
  thumbnailBytes: number;
  orphanFiles: number;
  orphanBytes: number;
  limitBytes: number;
}

export interface DiagnosticEntry {
  timestamp: number;
  level: string;
  context: string;
  message: string;
}

export type LibrarySelection =
  | { kind: "all" }
  | { kind: "duplicates" }
  | { kind: "type"; assetType: AssetType }
  | { kind: "pack"; packId: number }
  | { kind: "removed"; packId: number }
  | { kind: "missing"; packId: number }
  | { kind: "collection"; collectionId: number }
  | { kind: "project"; projectId: number };
