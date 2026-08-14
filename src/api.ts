import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AssetPage,
  AssetQuery,
  AssetSelection,
  AudioAnalysis,
  AudioStatus,
  CollectionSummary,
  CacheStatus,
  DiagnosticEntry,
  FilterOptions,
  ImportProgress,
  LibrarySnapshot,
  PackSummary,
  ProjectSummary,
  GodotExportResult,
} from "./types";

export const api = {
  snapshot: () => invoke<LibrarySnapshot>("get_library_snapshot"),
  assets: (request: AssetQuery) => invoke<AssetPage>("query_assets", { request }),
  assetSelections: (request: AssetQuery) =>
    invoke<AssetSelection[]>("query_asset_selections", { request }),
  importPack: (path: string, jobId: string, onProgress: (progress: ImportProgress) => void) => {
    const onEvent = new Channel<ImportProgress>();
    onEvent.onmessage = onProgress;
    return invoke<PackSummary>("import_pack", { path, jobId, onEvent });
  },
  cancelImport: (jobId: string) => invoke<void>("cancel_import", { jobId }),
  saveModelThumbnail: (assetId: number, pngData: string) =>
    invoke<string>("save_model_thumbnail", { assetId, pngData }),
  removePack: (packId: number) => invoke<void>("remove_pack", { packId }),
  renamePack: (packId: number, name: string) =>
    invoke<void>("rename_pack", { packId, name }),
  setAssetsExcluded: (assetIds: number[], excluded: boolean) =>
    invoke<void>("set_assets_excluded", { assetIds, excluded }),
  relocatePack: (packId: number, path: string) =>
    invoke<PackSummary>("relocate_pack", { packId, path }),
  addTag: (assetId: number, name: string) =>
    invoke<void>("add_tag", { assetId, name }),
  addTags: (assetIds: number[], name: string) =>
    invoke<void>("add_tags", { assetIds, name }),
  removeTag: (assetId: number, name: string) =>
    invoke<void>("remove_tag", { assetId, name }),
  removeTags: (assetIds: number[], name: string) =>
    invoke<void>("remove_tags", { assetIds, name }),
  createCollection: (name: string) =>
    invoke<CollectionSummary>("create_collection", { name }),
  setCollectionMembership: (
    assetId: number,
    collectionId: number,
    included: boolean,
  ) =>
    invoke<void>("set_collection_membership", {
      assetId,
      collectionId,
      included,
    }),
  setCollectionMemberships: (
    assetIds: number[],
    collectionId: number,
    included: boolean,
  ) => invoke<void>("set_collection_memberships", { assetIds, collectionId, included }),
  setClassificationOverride: (
    assetIds: number[],
    assetType?: string,
    mapRole?: string,
    groupAction?: "merge" | "split",
  ) => invoke<void>("set_classification_override", {
    assetIds,
    assetType: assetType || null,
    mapRole: mapRole || null,
    groupAction: groupAction || null,
  }),
  resetClassificationOverride: (assetIds: number[]) =>
    invoke<void>("reset_classification_override", { assetIds }),
  purgeMissingAssets: (packId: number) =>
    invoke<void>("purge_missing_assets", { packId }),
  filterOptions: () => invoke<FilterOptions>("get_filter_options"),
  cacheStatus: () => invoke<CacheStatus>("get_cache_status"),
  cleanCache: () => invoke<CacheStatus>("clean_thumbnail_cache"),
  clearCache: () => invoke<CacheStatus>("clear_thumbnail_cache"),
  regenerateImageThumbnails: () => invoke<CacheStatus>("regenerate_image_thumbnails"),
  createBackup: (destination?: string) =>
    invoke<string>("create_metadata_backup", { destination: destination ?? null }),
  restoreBackup: (path: string) => invoke<void>("restore_metadata_backup", { path }),
  diagnostics: () => invoke<DiagnosticEntry[]>("get_diagnostics"),
  logDiagnostic: (level: string, context: string, message: string) =>
    invoke<void>("log_diagnostic", { level, context, message }),
  deleteCollection: (collectionId: number) =>
    invoke<void>("delete_collection", { collectionId }),
  addGodotProject: (path: string) =>
    invoke<ProjectSummary>("add_godot_project", { path }),
  removeProject: (projectId: number) =>
    invoke<void>("remove_project", { projectId }),
  exportAssetsToGodot: (projectId: number, assetIds: number[]) =>
    invoke<GodotExportResult>("export_assets_to_godot", { projectId, assetIds }),
  hashLibrary: () => invoke<number>("hash_library"),
  openAsset: (path: string) => invoke<void>("open_asset", { path }),
  revealAsset: (path: string) => invoke<void>("reveal_asset", { path }),
  audioDuration: (path: string) => invoke<number>("get_audio_duration", { path }),
  audioAnalysis: (path: string) =>
    invoke<AudioAnalysis>("get_audio_analysis", { path }),
  toggleAudio: (path: string) => invoke<AudioStatus>("toggle_audio", { path }),
  audioStatus: () => invoke<AudioStatus>("get_audio_status"),
  seekAudio: (path: string, positionSeconds: number) =>
    invoke<AudioStatus>("seek_audio", { path, positionSeconds }),
  stopAudio: (path: string) => invoke<void>("stop_audio", { path }),
};
