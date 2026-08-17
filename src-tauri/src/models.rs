use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub timestamp: i64,
    pub level: String,
    pub context: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    pub thumbnail_files: usize,
    pub thumbnail_bytes: u64,
    pub orphan_files: usize,
    pub orphan_bytes: u64,
    pub limit_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub extensions: Vec<String>,
    pub map_roles: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSummary {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub asset_count: i64,
    pub last_scanned_at: Option<String>,
    pub available: bool,
    pub removed_asset_count: i64,
    pub missing_asset_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: &'static str,
    pub current: usize,
    pub total: usize,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: i64,
    pub name: String,
    pub asset_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeCount {
    pub asset_type: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub total_assets: i64,
    pub duplicate_assets: i64,
    pub removed_assets: i64,
    pub missing_assets: i64,
    pub hashing_assets: bool,
    pub packs: Vec<PackSummary>,
    pub collections: Vec<CollectionSummary>,
    pub projects: Vec<ProjectSummary>,
    pub type_counts: Vec<TypeCount>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub asset_count: i64,
    pub available: bool,
    pub last_exported_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectExportRun {
    pub id: i64,
    pub exported_at: String,
    pub selected_count: i64,
    pub copied_count: i64,
    pub unchanged_count: i64,
    pub model_formats: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub project_id: i64,
    pub destination: String,
    pub tracked_files: i64,
    pub up_to_date_files: i64,
    pub source_changed_files: i64,
    pub source_missing_files: i64,
    pub project_modified_files: i64,
    pub project_missing_files: i64,
    pub last_exported_at: Option<String>,
    pub runs: Vec<ProjectExportRun>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotExportResult {
    pub copied: usize,
    pub unchanged: usize,
    pub destination: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotExportPreview {
    pub selected: usize,
    pub related: usize,
    pub grouped: usize,
    pub dependencies: usize,
    pub total_files: usize,
    pub conflicts: usize,
    pub conflict_files: Vec<String>,
    pub destination: String,
    pub manifest: String,
    pub files: Vec<String>,
    pub model_formats: Vec<GodotModelFormat>,
    pub selected_model_formats: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotModelFormat {
    pub extension: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotProjectRemovalPreview {
    pub selected: usize,
    pub destination: String,
    pub remove_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub missing_files: Vec<String>,
    pub shared_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotProjectRemovalResult {
    pub deleted: usize,
    pub kept_modified: usize,
    pub cleaned_missing: usize,
    pub kept_shared: usize,
    pub destination: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateLocation {
    pub id: i64,
    pub pack_name: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: i64,
    pub pack_id: i64,
    pub pack_name: String,
    pub name: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub extension: String,
    pub asset_type: String,
    pub file_type: String,
    pub usage: Option<String>,
    pub map_role: Option<String>,
    pub resolution: Option<String>,
    pub classification_confidence: i64,
    pub classification_basis: String,
    pub size_bytes: i64,
    pub modified_at: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub triangles: Option<i64>,
    pub vertices: Option<i64>,
    pub thumbnail_path: Option<String>,
    pub variants: Vec<AssetVariant>,
    pub resources: Vec<AssetResource>,
    pub tags: Vec<String>,
    pub collection_ids: Vec<i64>,
    pub missing: bool,
    pub manual_classification: bool,
    pub content_hash: Option<String>,
    pub duplicate_count: i64,
    pub duplicate_locations: Vec<DuplicateLocation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVariant {
    pub id: i64,
    pub extension: String,
    pub asset_type: String,
    pub file_type: String,
    pub usage: Option<String>,
    pub map_role: Option<String>,
    pub resolution: Option<String>,
    pub triangles: Option<i64>,
    pub vertices: Option<i64>,
    pub absolute_path: String,
    pub relative_path: String,
    pub size_bytes: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetResource {
    pub id: i64,
    pub name: String,
    pub extension: String,
    pub asset_type: String,
    pub file_type: String,
    pub usage: Option<String>,
    pub map_role: Option<String>,
    pub resolution: Option<String>,
    pub triangles: Option<i64>,
    pub vertices: Option<i64>,
    pub absolute_path: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub thumbnail_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetQuery {
    pub query: Option<String>,
    pub asset_id: Option<i64>,
    pub asset_type: Option<String>,
    pub pack_id: Option<i64>,
    pub collection_id: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub excluded: Option<bool>,
    pub sort: Option<String>,
    pub sort_direction: Option<String>,
    pub extension: Option<String>,
    pub map_role: Option<String>,
    pub tag: Option<String>,
    pub min_width: Option<i64>,
    pub min_height: Option<i64>,
    pub min_confidence: Option<i64>,
    pub missing: Option<bool>,
    pub project_id: Option<i64>,
    pub unused_by_projects: Option<bool>,
    pub duplicates_only: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetPage {
    pub items: Vec<Asset>,
    pub total: i64,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSelection {
    pub id: i64,
    pub absolute_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationOverrideSnapshot {
    pub asset_id: i64,
    pub asset_type: Option<String>,
    pub map_role: Option<String>,
    pub group_key: Option<String>,
    pub existed: bool,
}

