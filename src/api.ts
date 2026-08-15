import { Channel, invoke } from "@tauri-apps/api/core";
import {
  mockSnapshot,
  mockFilterOptions,
  mockAssets,
  getMockAssetPage,
} from "./mockData";
import type {
  AssetPage,
  AssetQuery,
  AssetSelection,
  AudioAnalysis,
  AudioStatus,
  CollectionSummary,
  CacheStatus,
  ClassificationOverrideSnapshot,
  DiagnosticEntry,
  FilterOptions,
  ImportProgress,
  LibrarySnapshot,
  PackSummary,
  ProjectSummary,
  ProjectStatus,
  GodotExportResult,
  GodotExportPreview,
  GodotProjectRemovalPreview,
  GodotProjectRemovalResult,
} from "./types";

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    return invoke<T>(cmd, args);
  }
  // Browser dev mock fallback
  if (cmd === "get_library_snapshot") return mockSnapshot as unknown as T;
  if (cmd === "query_assets") return getMockAssetPage((args?.request ?? {}) as AssetQuery) as unknown as T;
  if (cmd === "query_asset_selections") {
    return mockAssets.map((a) => ({ id: a.id, assetType: a.assetType })) as unknown as T;
  }
  if (cmd === "get_filter_options") return mockFilterOptions as unknown as T;
  if (cmd === "get_project_status") {
    return {
      projectId: (args?.projectId as number) ?? 1,
      destination: "res://assets/lootbox",
      trackedFiles: 18,
      upToDateFiles: 18,
      sourceChangedFiles: 0,
      sourceMissingFiles: 0,
      projectModifiedFiles: 0,
      projectMissingFiles: 0,
      lastExportedAt: "2026-08-15T16:00:00Z",
      runs: [],
    } as unknown as T;
  }
  if (cmd === "get_diagnostics") return [] as unknown as T;
  if (cmd === "get_cache_status") return { thumbnailCount: 42, totalSizeBytes: 1048576 } as unknown as T;
  return undefined as unknown as T;
}

export const api = {
  snapshot: () => safeInvoke<LibrarySnapshot>("get_library_snapshot"),
  assets: (request: AssetQuery) => safeInvoke<AssetPage>("query_assets", { request }),
  assetSelections: (request: AssetQuery) =>
    safeInvoke<AssetSelection[]>("query_asset_selections", { request }),
  importPack: (path: string, jobId: string, onProgress: (progress: ImportProgress) => void) => {
    const onEvent = new Channel<ImportProgress>();
    onEvent.onmessage = onProgress;
    return safeInvoke<PackSummary>("import_pack", { path, jobId, onEvent });
  },
  cancelImport: (jobId: string) => safeInvoke<void>("cancel_import", { jobId }),
  saveModelThumbnail: (assetId: number, pngData: string) =>
    safeInvoke<string>("save_model_thumbnail", { assetId, pngData }),
  removePack: (packId: number) => safeInvoke<void>("remove_pack", { packId }),
  renamePack: (packId: number, name: string) =>
    safeInvoke<void>("rename_pack", { packId, name }),
  setAssetsExcluded: (assetIds: number[], excluded: boolean) =>
    safeInvoke<void>("set_assets_excluded", { assetIds, excluded }),
  relocatePack: (packId: number, path: string) =>
    safeInvoke<PackSummary>("relocate_pack", { packId, path }),
  addTag: (assetId: number, name: string) =>
    safeInvoke<void>("add_tag", { assetId, name }),
  addTags: (assetIds: number[], name: string) =>
    safeInvoke<number[]>("add_tags", { assetIds, name }),
  removeTag: (assetId: number, name: string) =>
    safeInvoke<void>("remove_tag", { assetId, name }),
  removeTags: (assetIds: number[], name: string) =>
    safeInvoke<number[]>("remove_tags", { assetIds, name }),
  createCollection: (name: string) =>
    safeInvoke<CollectionSummary>("create_collection", { name }),
  setCollectionMembership: (
    assetId: number,
    collectionId: number,
    included: boolean,
  ) =>
    safeInvoke<void>("set_collection_membership", {
      assetId,
      collectionId,
      included,
    }),
  setCollectionMemberships: (
    assetIds: number[],
    collectionId: number,
    included: boolean,
  ) => safeInvoke<number[]>("set_collection_memberships", { assetIds, collectionId, included }),
  setClassificationOverride: (
    assetIds: number[],
    assetType?: string,
    mapRole?: string,
    groupAction?: "merge" | "split",
  ) => safeInvoke<ClassificationOverrideSnapshot[]>("set_classification_override", {
    assetIds,
    assetType: assetType || null,
    mapRole: mapRole || null,
    groupAction: groupAction || null,
  }),
  resetClassificationOverride: (assetIds: number[]) =>
    safeInvoke<ClassificationOverrideSnapshot[]>("reset_classification_override", { assetIds }),
  restoreClassificationOverrides: (snapshots: ClassificationOverrideSnapshot[]) =>
    safeInvoke<void>("restore_classification_overrides", { snapshots }),
  purgeMissingAssets: (packId: number) =>
    safeInvoke<void>("purge_missing_assets", { packId }),
  filterOptions: () => safeInvoke<FilterOptions>("get_filter_options"),
  cacheStatus: () => safeInvoke<CacheStatus>("get_cache_status"),
  cleanCache: () => safeInvoke<CacheStatus>("clean_thumbnail_cache"),
  clearCache: () => safeInvoke<CacheStatus>("clear_thumbnail_cache"),
  regenerateImageThumbnails: () => safeInvoke<CacheStatus>("regenerate_image_thumbnails"),
  createBackup: (destination?: string) =>
    safeInvoke<string>("create_metadata_backup", { destination: destination ?? null }),
  restoreBackup: (path: string) => safeInvoke<void>("restore_metadata_backup", { path }),
  diagnostics: () => safeInvoke<DiagnosticEntry[]>("get_diagnostics"),
  logDiagnostic: (level: string, context: string, message: string) =>
    safeInvoke<void>("log_diagnostic", { level, context, message }),
  deleteCollection: (collectionId: number) =>
    safeInvoke<void>("delete_collection", { collectionId }),
  addGodotProject: (path: string) =>
    safeInvoke<ProjectSummary>("add_godot_project", { path }),
  relocateGodotProject: (projectId: number, path: string) =>
    safeInvoke<ProjectSummary>("relocate_godot_project", { projectId, path }),
  removeProject: (projectId: number) =>
    safeInvoke<void>("remove_project", { projectId }),
  projectStatus: (projectId: number) =>
    safeInvoke<ProjectStatus>("get_project_status", { projectId }),
  previewAssetsToGodot: (projectId: number, assetIds: number[], modelFormats?: string[] | null) =>
    safeInvoke<GodotExportPreview>("preview_assets_to_godot", {
      projectId,
      assetIds,
      modelFormats: modelFormats ?? null,
    }),
  exportAssetsToGodot: (projectId: number, assetIds: number[], modelFormats?: string[] | null) =>
    safeInvoke<GodotExportResult>("export_assets_to_godot", {
      projectId,
      assetIds,
      modelFormats: modelFormats ?? null,
    }),
  previewRemoveAssetsFromGodotProject: (projectId: number, assetIds: number[]) =>
    safeInvoke<GodotProjectRemovalPreview>("preview_remove_assets_from_godot_project", {
      projectId,
      assetIds,
    }),
  removeAssetsFromGodotProject: (projectId: number, assetIds: number[]) =>
    safeInvoke<GodotProjectRemovalResult>("remove_assets_from_godot_project", {
      projectId,
      assetIds,
    }),
  hashLibrary: () => safeInvoke<number>("hash_library"),
  openAsset: (path: string) => safeInvoke<void>("open_asset", { path }),
  revealAsset: (path: string) => safeInvoke<void>("reveal_asset", { path }),
  audioDuration: (path: string) => safeInvoke<number>("get_audio_duration", { path }),
  audioAnalysis: (path: string) =>
    safeInvoke<AudioAnalysis>("get_audio_analysis", { path }),
  toggleAudio: (path: string) => safeInvoke<AudioStatus>("toggle_audio", { path }),
  audioStatus: () => safeInvoke<AudioStatus>("get_audio_status"),
  seekAudio: (path: string, positionSeconds: number) =>
    safeInvoke<AudioStatus>("seek_audio", { path, positionSeconds }),
  stopAudio: (path: string) => safeInvoke<void>("stop_audio", { path }),
};
