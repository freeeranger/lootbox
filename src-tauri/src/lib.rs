use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rayon::prelude::*;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering as CmpOrdering,
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{ipc::Channel, Manager, State};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, thiserror::Error)]
enum LootboxError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("The selected folder does not exist or is not a directory")]
    InvalidDirectory,
    #[error("Could not open this item with the operating system")]
    OpenFailed,
    #[error("The import worker stopped unexpectedly")]
    ImportWorker,
    #[error("Import cancelled")]
    ImportCancelled,
    #[error("Invalid thumbnail data")]
    InvalidThumbnail,
    #[error("Audio error: {0}")]
    Audio(String),
    #[error("Pack name cannot be empty")]
    InvalidPackName,
    #[error("That folder does not match this pack: {0}")]
    InvalidPackLocation(String),
    #[error("Invalid backup: {0}")]
    InvalidBackup(String),
    #[error("Invalid Godot project: {0}")]
    InvalidGodotProject(String),
    #[error("Project export failed: {0}")]
    ProjectExport(String),
}

impl Serialize for LootboxError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

type Result<T> = std::result::Result<T, LootboxError>;

#[derive(Clone)]
struct AppState {
    database_path: PathBuf,
    thumbnail_directory: PathBuf,
    backup_directory: PathBuf,
    diagnostic_log_path: PathBuf,
    write_queue: Arc<Mutex<()>>,
    import_cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    diagnostics: Arc<Mutex<VecDeque<DiagnosticEntry>>>,
    hashing_library: Arc<AtomicBool>,
}

#[derive(Default)]
struct AudioPlayback {
    device: Option<MixerDeviceSink>,
    player: Option<Player>,
    path: Option<String>,
    duration: Duration,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioStatus {
    path: Option<String>,
    playing: bool,
    position_seconds: f64,
    duration_seconds: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioAnalysis {
    duration_seconds: f64,
    peaks: Vec<f32>,
}

fn audio_status(playback: &AudioPlayback) -> AudioStatus {
    let player = playback.player.as_ref();
    AudioStatus {
        path: playback.path.clone(),
        playing: player.is_some_and(|player| !player.is_paused() && !player.empty()),
        position_seconds: player
            .map(Player::get_pos)
            .unwrap_or_default()
            .min(playback.duration)
            .as_secs_f64(),
        duration_seconds: playback.duration.as_secs_f64(),
    }
}

impl AppState {
    fn connect(&self) -> Result<Connection> {
        let connection = Connection::open(&self.database_path)?;
        register_collations(&connection)?;
        connection.busy_timeout(Duration::from_secs(15))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(connection)
    }

    fn record(&self, level: &str, context: &str, message: impl Into<String>) {
        let entry = DiagnosticEntry {
            timestamp: unix_timestamp(),
            level: level.to_string(),
            context: context.to_string(),
            message: message.into(),
        };
        if let Ok(mut entries) = self.diagnostics.lock() {
            entries.push_back(entry.clone());
            while entries.len() > 500 {
                entries.pop_front();
            }
        }
        if let Some(parent) = self.diagnostic_log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if rotate_log_if_needed(&self.diagnostic_log_path).is_ok() {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.diagnostic_log_path)
            {
                let _ = writeln!(
                    file,
                    "{}\t{}\t{}\t{}",
                    entry.timestamp,
                    entry.level,
                    entry.context,
                    entry.message.replace('\n', " ")
                );
            }
        }
    }
}

fn natural_name_cmp(left: &str, right: &str) -> CmpOrdering {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    let left = left.as_bytes();
    let right = right.as_bytes();
    let (mut left_index, mut right_index) = (0, 0);

    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let left_end = (left_index..left.len())
                .find(|index| !left[*index].is_ascii_digit())
                .unwrap_or(left.len());
            let right_end = (right_index..right.len())
                .find(|index| !right[*index].is_ascii_digit())
                .unwrap_or(right.len());
            let left_number = &left[left_index..left_end];
            let right_number = &right[right_index..right_end];
            let left_significant = left_number
                .iter()
                .position(|byte| *byte != b'0')
                .map(|index| &left_number[index..])
                .unwrap_or(&left_number[left_number.len().saturating_sub(1)..]);
            let right_significant = right_number
                .iter()
                .position(|byte| *byte != b'0')
                .map(|index| &right_number[index..])
                .unwrap_or(&right_number[right_number.len().saturating_sub(1)..]);
            let ordering = left_significant
                .len()
                .cmp(&right_significant.len())
                .then_with(|| left_significant.cmp(right_significant))
                .then_with(|| left_number.len().cmp(&right_number.len()));
            if ordering != CmpOrdering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let ordering = left[left_index].cmp(&right[right_index]);
        if ordering != CmpOrdering::Equal {
            return ordering;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

fn register_collations(connection: &Connection) -> Result<()> {
    connection.create_collation("LOOTBOX_NATURAL", natural_name_cmp)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticEntry {
    timestamp: i64,
    level: String,
    context: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheStatus {
    thumbnail_files: usize,
    thumbnail_bytes: u64,
    orphan_files: usize,
    orphan_bytes: u64,
    limit_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FilterOptions {
    extensions: Vec<String>,
    map_roles: Vec<String>,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackSummary {
    id: i64,
    name: String,
    root_path: String,
    asset_count: i64,
    last_scanned_at: Option<String>,
    available: bool,
    removed_asset_count: i64,
    missing_asset_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    phase: &'static str,
    current: usize,
    total: usize,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionSummary {
    id: i64,
    name: String,
    asset_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TypeCount {
    asset_type: String,
    count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySnapshot {
    total_assets: i64,
    duplicate_assets: i64,
    removed_assets: i64,
    missing_assets: i64,
    hashing_assets: bool,
    packs: Vec<PackSummary>,
    collections: Vec<CollectionSummary>,
    projects: Vec<ProjectSummary>,
    type_counts: Vec<TypeCount>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    id: i64,
    name: String,
    root_path: String,
    asset_count: i64,
    available: bool,
    last_exported_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectExportRun {
    id: i64,
    exported_at: String,
    selected_count: i64,
    copied_count: i64,
    unchanged_count: i64,
    model_formats: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStatus {
    project_id: i64,
    destination: String,
    tracked_files: i64,
    up_to_date_files: i64,
    source_changed_files: i64,
    source_missing_files: i64,
    project_modified_files: i64,
    project_missing_files: i64,
    last_exported_at: Option<String>,
    runs: Vec<ProjectExportRun>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GodotExportResult {
    copied: usize,
    unchanged: usize,
    destination: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GodotExportPreview {
    selected: usize,
    related: usize,
    grouped: usize,
    dependencies: usize,
    total_files: usize,
    conflicts: usize,
    conflict_files: Vec<String>,
    destination: String,
    manifest: String,
    files: Vec<String>,
    model_formats: Vec<GodotModelFormat>,
    selected_model_formats: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GodotModelFormat {
    extension: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GodotProjectRemovalPreview {
    selected: usize,
    destination: String,
    remove_files: Vec<String>,
    modified_files: Vec<String>,
    missing_files: Vec<String>,
    shared_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GodotProjectRemovalResult {
    deleted: usize,
    kept_modified: usize,
    cleaned_missing: usize,
    kept_shared: usize,
    destination: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateLocation {
    id: i64,
    pack_name: String,
    relative_path: String,
    absolute_path: String,
    size_bytes: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Asset {
    id: i64,
    pack_id: i64,
    pack_name: String,
    name: String,
    relative_path: String,
    absolute_path: String,
    extension: String,
    asset_type: String,
    file_type: String,
    usage: Option<String>,
    map_role: Option<String>,
    resolution: Option<String>,
    classification_confidence: i64,
    classification_basis: String,
    size_bytes: i64,
    modified_at: i64,
    width: Option<i64>,
    height: Option<i64>,
    triangles: Option<i64>,
    vertices: Option<i64>,
    thumbnail_path: Option<String>,
    variants: Vec<AssetVariant>,
    resources: Vec<AssetResource>,
    tags: Vec<String>,
    collection_ids: Vec<i64>,
    missing: bool,
    manual_classification: bool,
    content_hash: Option<String>,
    duplicate_count: i64,
    duplicate_locations: Vec<DuplicateLocation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetVariant {
    id: i64,
    extension: String,
    asset_type: String,
    file_type: String,
    usage: Option<String>,
    map_role: Option<String>,
    resolution: Option<String>,
    triangles: Option<i64>,
    vertices: Option<i64>,
    absolute_path: String,
    relative_path: String,
    size_bytes: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetResource {
    id: i64,
    name: String,
    extension: String,
    asset_type: String,
    file_type: String,
    usage: Option<String>,
    map_role: Option<String>,
    resolution: Option<String>,
    triangles: Option<i64>,
    vertices: Option<i64>,
    absolute_path: String,
    relative_path: String,
    size_bytes: i64,
    thumbnail_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetQuery {
    query: Option<String>,
    asset_id: Option<i64>,
    asset_type: Option<String>,
    pack_id: Option<i64>,
    collection_id: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
    excluded: Option<bool>,
    sort: Option<String>,
    sort_direction: Option<String>,
    extension: Option<String>,
    map_role: Option<String>,
    tag: Option<String>,
    min_width: Option<i64>,
    min_height: Option<i64>,
    min_confidence: Option<i64>,
    missing: Option<bool>,
    project_id: Option<i64>,
    unused_by_projects: Option<bool>,
    duplicates_only: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetPage {
    items: Vec<Asset>,
    total: i64,
    has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetSelection {
    id: i64,
    absolute_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClassificationOverrideSnapshot {
    asset_id: i64,
    asset_type: Option<String>,
    map_role: Option<String>,
    group_key: Option<String>,
    existed: bool,
}

const SCHEMA_VERSION: i64 = 6;
const IMAGE_THUMBNAIL_VERSION: i64 = 2;
const MODEL_THUMBNAIL_VERSION: i64 = 4;
const DEFAULT_CACHE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|name| name == column))
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    declaration: &str,
) -> Result<()> {
    if !column_exists(connection, table, column)? {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {declaration}"
        ))?;
    }
    Ok(())
}

fn initialize_database(connection: &Connection) -> Result<()> {
    register_collations(connection)?;
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS packs (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL UNIQUE,
            imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            last_scanned_at TEXT,
            generation INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS app_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS assets (
            id INTEGER PRIMARY KEY,
            pack_id INTEGER NOT NULL REFERENCES packs(id) ON DELETE CASCADE,
            relative_path TEXT NOT NULL,
            absolute_path TEXT NOT NULL,
            name TEXT NOT NULL,
            extension TEXT NOT NULL,
            asset_type TEXT NOT NULL,
            file_type TEXT NOT NULL DEFAULT 'other',
            usage TEXT,
            map_role TEXT,
            resolution TEXT,
            classification_confidence INTEGER NOT NULL DEFAULT 100,
            classification_basis TEXT NOT NULL DEFAULT 'extension',
            size_bytes INTEGER NOT NULL,
            modified_at INTEGER NOT NULL,
            width INTEGER,
            height INTEGER,
            triangles INTEGER,
            vertices INTEGER,
            thumbnail_path TEXT,
            variant_group TEXT,
            group_key TEXT,
            is_primary INTEGER NOT NULL DEFAULT 1,
            excluded INTEGER NOT NULL DEFAULT 0,
            missing INTEGER NOT NULL DEFAULT 0,
            missing_since TEXT,
            thumbnail_version INTEGER NOT NULL DEFAULT 0,
            content_hash TEXT,
            generation INTEGER NOT NULL DEFAULT 0,
            UNIQUE(pack_id, relative_path)
        );

        CREATE INDEX IF NOT EXISTS assets_pack_idx ON assets(pack_id);
        CREATE INDEX IF NOT EXISTS assets_type_idx ON assets(asset_type);
        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL COLLATE NOCASE UNIQUE
        );

        CREATE TABLE IF NOT EXISTS asset_tags (
            asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY(asset_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS collections (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL COLLATE NOCASE UNIQUE,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS collection_assets (
            collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            PRIMARY KEY(collection_id, asset_id)
        );
        CREATE INDEX IF NOT EXISTS collection_assets_asset_idx
            ON collection_assets(asset_id, collection_id);

        CREATE TABLE IF NOT EXISTS asset_dependencies (
            owner_asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            dependency_asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            PRIMARY KEY(owner_asset_id, dependency_asset_id)
        );

        CREATE INDEX IF NOT EXISTS asset_dependencies_owner_idx
            ON asset_dependencies(owner_asset_id);

        CREATE TABLE IF NOT EXISTS classification_overrides (
            asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
            asset_type TEXT,
            map_role TEXT,
            group_key TEXT,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            root_path TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS project_exports (
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            exported_path TEXT NOT NULL,
            content_hash TEXT,
            exported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(project_id, asset_id)
        );
        CREATE INDEX IF NOT EXISTS project_exports_asset_idx ON project_exports(asset_id);

        CREATE TABLE IF NOT EXISTS project_export_runs (
            id INTEGER PRIMARY KEY,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            exported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            selected_count INTEGER NOT NULL,
            copied_count INTEGER NOT NULL,
            unchanged_count INTEGER NOT NULL,
            model_formats TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS project_export_runs_project_idx
            ON project_export_runs(project_id, exported_at DESC);

        CREATE VIRTUAL TABLE IF NOT EXISTS assets_fts USING fts5(
            asset_id UNINDEXED,
            name,
            relative_path,
            pack_name,
            tags,
            tokenize = 'unicode61 remove_diacritics 2',
            prefix = '2 3'
        );
        "#,
    )?;
    // Explicit, idempotent migrations for databases created by older builds.
    for (column, declaration) in [
        ("thumbnail_path", "TEXT"),
        ("variant_group", "TEXT"),
        ("file_type", "TEXT NOT NULL DEFAULT 'other'"),
        ("usage", "TEXT"),
        ("map_role", "TEXT"),
        ("resolution", "TEXT"),
        ("classification_confidence", "INTEGER NOT NULL DEFAULT 100"),
        ("classification_basis", "TEXT NOT NULL DEFAULT 'extension'"),
        ("group_key", "TEXT"),
        ("is_primary", "INTEGER NOT NULL DEFAULT 1"),
        ("excluded", "INTEGER NOT NULL DEFAULT 0"),
        ("missing", "INTEGER NOT NULL DEFAULT 0"),
        ("missing_since", "TEXT"),
        ("thumbnail_version", "INTEGER NOT NULL DEFAULT 0"),
        ("content_hash", "TEXT"),
        ("triangles", "INTEGER"),
        ("vertices", "INTEGER"),
    ] {
        add_column_if_missing(&transaction, "assets", column, declaration)?;
    }
    transaction.execute(
        "CREATE INDEX IF NOT EXISTS assets_variant_idx ON assets(pack_id, variant_group)",
        [],
    )?;
    transaction.execute(
        "CREATE INDEX IF NOT EXISTS assets_group_idx ON assets(pack_id, group_key)",
        [],
    )?;
    transaction.execute(
        "CREATE INDEX IF NOT EXISTS assets_missing_idx ON assets(pack_id, missing)",
        [],
    )?;
    transaction.execute(
        "CREATE INDEX IF NOT EXISTS assets_content_hash_idx ON assets(content_hash) WHERE content_hash IS NOT NULL",
        [],
    )?;
    transaction.execute(
        "CREATE INDEX IF NOT EXISTS collection_assets_asset_idx ON collection_assets(asset_id, collection_id)",
        [],
    )?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    rebuild_search_index(connection)?;
    Ok(())
}

fn rebuild_search_index(connection: &Connection) -> Result<()> {
    connection.execute("DELETE FROM assets_fts", [])?;
    connection.execute(
        r#"
        INSERT INTO assets_fts(asset_id, name, relative_path, pack_name, tags)
        SELECT
            a.id,
            a.name,
            a.relative_path,
            p.name,
            COALESCE(GROUP_CONCAT(DISTINCT t.name), '') || ' ' ||
            COALESCE(GROUP_CONCAT(DISTINCT resource.name), '')
        FROM assets a
        JOIN packs p ON p.id = a.pack_id
        LEFT JOIN asset_tags at ON at.asset_id = a.id
        LEFT JOIN tags t ON t.id = at.tag_id
        LEFT JOIN asset_dependencies dependency ON dependency.owner_asset_id = a.id
        LEFT JOIN assets resource ON resource.id = dependency.dependency_asset_id
        WHERE a.is_primary = 1 AND a.excluded = 0 AND a.missing = 0
        GROUP BY a.id
        "#,
        [],
    )?;
    Ok(())
}

fn should_visit(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if entry.file_type().is_dir() {
        !name.starts_with('.') && name != "node_modules" && name != "target"
    } else {
        !name.starts_with('.')
    }
}

fn classify_extension(extension: &str) -> &'static str {
    match extension {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" | "tga" | "tif" | "tiff"
        | "exr" | "hdr" | "dds" | "ktx" | "ktx2" => "image",
        "wav" | "mp3" | "ogg" | "flac" | "aac" | "m4a" | "opus" => "audio",
        "glb" | "gltf" | "fbx" | "obj" | "dae" | "blend" | "3ds" | "stl" | "ply" | "usd"
        | "usda" | "usdc" | "usdz" => "model",
        "mp4" | "mov" | "webm" | "avi" | "mkv" => "video",
        "ttf" | "otf" | "woff" | "woff2" => "font",
        "glsl" | "hlsl" | "shader" | "vert" | "frag" | "wgsl" => "shader",
        "mat" | "material" | "mtl" => "material",
        "zip" | "rar" | "7z" | "tar" | "gz" => "archive",
        _ => "other",
    }
}

fn normalized_texture_token(value: &str) -> String {
    let mut normalized = String::new();
    for character in value.to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
        } else if !normalized.ends_with('_') {
            normalized.push('_');
        }
    }
    normalized.trim_matches('_').to_string()
}

fn texture_map_role(value: &str) -> Option<&'static str> {
    let token = normalized_texture_token(value);
    let token = token
        .strip_suffix("_maps")
        .or_else(|| token.strip_suffix("_map"))
        .unwrap_or(&token);
    match token {
        "color" | "colour" | "base_color" | "base_colour" | "albedo" | "diffuse" => Some("color"),
        "normal" | "normals" | "nrm" => Some("normal"),
        "normalgl" | "normal_opengl" => Some("normal_gl"),
        "normaldx" | "normal_directx" => Some("normal_dx"),
        "rough" | "roughness" => Some("roughness"),
        "metal" | "metallic" | "metalness" => Some("metalness"),
        "ambient_occlusion" | "occlusion" | "ao" => Some("occlusion"),
        "height" | "displacement" | "disp" | "bump" => Some("height"),
        "opacity" | "alpha" | "transparency" => Some("opacity"),
        "emission" | "emissive" => Some("emissive"),
        "spec" | "specular" => Some("specular"),
        "gloss" | "glossiness" => Some("glossiness"),
        "orm" | "arm" => Some("occlusion_roughness_metalness"),
        "rma" => Some("roughness_metalness_occlusion"),
        _ => None,
    }
}

fn is_texture_directory(value: &str) -> bool {
    let token = normalized_texture_token(value);
    matches!(
        token.as_str(),
        "texture" | "textures" | "map" | "maps" | "texture_maps" | "material_maps"
    ) || texture_map_role(&token).is_some()
}

fn is_resolution_directory(value: &str) -> bool {
    let token = normalized_texture_token(value);
    if token.parse::<u32>().is_ok_and(|size| size >= 64) {
        return true;
    }
    if token
        .strip_suffix('k')
        .and_then(|size| size.parse::<u32>().ok())
        .is_some_and(|size| (1..=32).contains(&size))
    {
        return true;
    }
    token.split_once('x').is_some_and(|(width, height)| {
        width.parse::<u32>().is_ok_and(|size| size >= 64)
            && height.parse::<u32>().is_ok_and(|size| size >= 64)
    })
}

fn texture_stem_and_role(stem: &str) -> (String, Option<&'static str>) {
    let stem = normalized_texture_token(stem);
    if let Some(role) = texture_map_role(&stem) {
        return (String::new(), Some(role));
    }
    const SUFFIXES: &[(&str, &str)] = &[
        ("normal_directx", "normal_dx"),
        ("normal_opengl", "normal_gl"),
        ("normaldx", "normal_dx"),
        ("normalgl", "normal_gl"),
        ("base_colour", "color"),
        ("base_color", "color"),
        ("basecolour", "color"),
        ("basecolor", "color"),
        ("ambient_occlusion", "occlusion"),
        ("displacement", "height"),
        ("glossiness", "glossiness"),
        ("transparency", "opacity"),
        ("roughness", "roughness"),
        ("metalness", "metalness"),
        ("metallic", "metalness"),
        ("specular", "specular"),
        ("emissive", "emissive"),
        ("emission", "emissive"),
        ("occlusion", "occlusion"),
        ("diffuse", "color"),
        ("normal", "normal"),
        ("albedo", "color"),
        ("opacity", "opacity"),
        ("height", "height"),
        ("colour", "color"),
        ("color", "color"),
        ("rough", "roughness"),
        ("gloss", "glossiness"),
        ("alpha", "opacity"),
        ("bump", "height"),
        ("nrm", "normal"),
        ("disp", "height"),
        ("spec", "specular"),
        ("ao", "occlusion"),
        ("orm", "occlusion_roughness_metalness"),
        ("arm", "occlusion_roughness_metalness"),
        ("rma", "roughness_metalness_occlusion"),
    ];
    for (suffix, role) in SUFFIXES {
        if let Some(base) = stem.strip_suffix(&format!("_{suffix}")) {
            if !base.is_empty() {
                return (base.to_string(), Some(*role));
            }
        }
    }
    // Common engine conventions such as T_Brick_D / T_Brick_N. Single-letter
    // roles are only accepted when there is a non-empty base to reduce false positives.
    const SHORT_SUFFIXES: &[(&str, &str)] = &[
        ("d", "color"),
        ("n", "normal"),
        ("r", "roughness"),
        ("m", "metalness"),
        ("h", "height"),
        ("e", "emissive"),
    ];
    for (suffix, role) in SHORT_SUFFIXES {
        if let Some(base) = stem.strip_suffix(&format!("_{suffix}")) {
            if !base.is_empty() {
                return (base.to_string(), Some(*role));
            }
        }
    }
    (stem, None)
}

fn texture_resolution(relative_path: &Path) -> Option<String> {
    relative_path.parent()?.components().find_map(|component| {
        let value = component.as_os_str().to_string_lossy();
        is_resolution_directory(&value).then(|| normalized_texture_token(&value))
    })
}

fn texture_directory_evidence(relative_path: &Path) -> (Option<&'static str>, bool) {
    let mut role = None;
    let mut texture_directory = false;
    if let Some(parent) = relative_path.parent() {
        for component in parent.components() {
            let value = component.as_os_str().to_string_lossy();
            if let Some(candidate) = texture_map_role(&value) {
                role = Some(candidate);
                texture_directory = true;
            } else if is_texture_directory(&value) {
                texture_directory = true;
            }
        }
    }
    (role, texture_directory)
}

fn texture_group_key(relative_path: &Path) -> String {
    let mut parts = Vec::new();
    for component in relative_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
    {
        let component = component.as_os_str().to_string_lossy();
        if !is_texture_directory(&component) && !is_resolution_directory(&component) {
            parts.push(normalized_texture_token(&component));
        }
    }
    let stem = relative_path
        .file_stem()
        .or_else(|| relative_path.file_name())
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let base = texture_stem_and_role(&stem).0;
    if !base.is_empty() {
        parts.push(base);
    }
    if parts.is_empty() {
        parts.push("surface".into());
    }
    format!("texture:{}", parts.join("/"))
}

fn classify_asset_type(relative_path: &Path, extension: &str) -> &'static str {
    let asset_type = classify_extension(extension);
    let (directory_role, texture_directory) = texture_directory_evidence(relative_path);
    let stem_role = relative_path
        .file_stem()
        .or_else(|| relative_path.file_name())
        .and_then(|stem| texture_stem_and_role(&stem.to_string_lossy()).1);
    if asset_type == "image"
        && (texture_directory || directory_role.is_some() || stem_role.is_some())
    {
        "texture"
    } else {
        asset_type
    }
}

fn is_model_format_directory(name: &str) -> bool {
    let token = name
        .split(|character: char| character.is_whitespace() || matches!(character, '(' | '['))
        .next()
        .unwrap_or(name);
    matches!(
        token,
        "other-formats"
            | "other_formats"
            | "glb"
            | "gltf"
            | "fbx"
            | "obj"
            | "dae"
            | "blend"
            | "3ds"
            | "stl"
            | "ply"
            | "usd"
            | "usda"
            | "usdc"
            | "usdz"
    )
}

fn model_variant_group(relative_path: &Path, asset_type: &str, extension: &str) -> Option<String> {
    if asset_type != "model" && extension != "mtl" {
        return None;
    }

    let mut parts = Vec::new();
    let parent = relative_path.parent().unwrap_or_else(|| Path::new(""));
    for component in parent.components() {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if !is_model_format_directory(&name) {
            parts.push(name);
        }
    }
    let stem = relative_path
        .file_stem()
        .or_else(|| relative_path.file_name())
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    parts.push(stem);
    Some(format!("model:{}", parts.join("/")))
}

fn recompute_primary_assets(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
    let pack_filter = pack_id
        .map(|id| format!(" WHERE pack_id = {id}"))
        .unwrap_or_default();
    connection.execute_batch(&format!(
        r#"
        UPDATE assets AS candidate
        SET is_primary = CASE
            WHEN group_key IS NULL THEN 1
            WHEN id = (
                SELECT variant.id
                FROM assets variant
                WHERE variant.pack_id = candidate.pack_id
                  AND variant.group_key = candidate.group_key
                ORDER BY
                    variant.missing ASC,
                    CASE
                        WHEN variant.extension = 'glb' THEN 0
                        WHEN variant.extension = 'gltf' THEN 1
                        WHEN variant.extension = 'fbx' THEN 2
                        WHEN variant.extension = 'obj' THEN 3
                        WHEN variant.extension = 'dae' THEN 4
                        WHEN variant.extension = 'blend' THEN 5
                        WHEN variant.extension = 'usd' THEN 6
                        WHEN variant.extension = 'usdc' THEN 7
                        WHEN variant.extension = 'usda' THEN 8
                        WHEN variant.extension = 'usdz' THEN 9
                        WHEN variant.extension = '3ds' THEN 10
                        WHEN variant.extension = 'stl' THEN 11
                        WHEN variant.extension = 'ply' THEN 12
                        WHEN variant.usage = 'texture' AND variant.map_role = 'color' THEN 20
                        WHEN variant.usage = 'texture' AND variant.map_role IN ('normal', 'normal_gl', 'normal_dx') THEN 22
                        WHEN variant.asset_type = 'texture' THEN 25
                        WHEN variant.asset_type = 'image' THEN 30
                        WHEN variant.extension = 'mtl' THEN 100
                        ELSE 50
                    END,
                    COALESCE(variant.width * variant.height, 0) DESC,
                    variant.relative_path COLLATE NOCASE,
                    variant.id
                LIMIT 1
            ) THEN 1
            ELSE 0
        END{pack_filter};

        INSERT OR IGNORE INTO asset_tags(asset_id, tag_id)
        SELECT primary_asset.id, asset_tags.tag_id
        FROM assets variant
        JOIN assets primary_asset
          ON primary_asset.pack_id = variant.pack_id
         AND primary_asset.group_key = variant.group_key
         AND primary_asset.is_primary = 1
        JOIN asset_tags ON asset_tags.asset_id = variant.id
        WHERE variant.is_primary = 0;

        INSERT OR IGNORE INTO collection_assets(collection_id, asset_id)
        SELECT collection_assets.collection_id, primary_asset.id
        FROM assets variant
        JOIN assets primary_asset
          ON primary_asset.pack_id = variant.pack_id
         AND primary_asset.group_key = variant.group_key
         AND primary_asset.is_primary = 1
        JOIN collection_assets ON collection_assets.asset_id = variant.id
        WHERE variant.is_primary = 0;
        "#
    ))?;
    Ok(())
}

fn recompute_texture_groups(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
    let pack_filter = pack_id
        .map(|id| format!(" WHERE a.pack_id = {id}"))
        .unwrap_or_default();
    let missing_filter = if pack_id.is_some() {
        "AND a.missing = 0"
    } else {
        "WHERE a.missing = 0"
    };
    let entries = {
        let mut statement = connection.prepare(&format!(
            r#"
            SELECT a.id, a.relative_path, a.extension
            FROM assets a
            {pack_filter}
            {missing_filter}
            "#
        ))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    let mut group_evidence: HashMap<String, (HashSet<String>, usize)> = HashMap::new();
    for (_, relative_path, extension) in &entries {
        if classify_extension(extension) != "image" {
            continue;
        }
        let relative_path = Path::new(relative_path);
        let (directory_role, _) = texture_directory_evidence(relative_path);
        let stem_role = relative_path
            .file_stem()
            .or_else(|| relative_path.file_name())
            .and_then(|stem| texture_stem_and_role(&stem.to_string_lossy()).1);
        let evidence = group_evidence
            .entry(texture_group_key(relative_path))
            .or_insert_with(|| (HashSet::new(), 0));
        evidence.1 += 1;
        if let Some(role) = stem_role.or(directory_role) {
            evidence.0.insert(role.to_string());
        }
    }

    for (id, relative_path, extension) in entries {
        let relative_path = Path::new(&relative_path);
        let file_type = classify_extension(&extension);
        if file_type != "image" {
            let group_key = model_variant_group(relative_path, file_type, &extension);
            connection.execute(
                r#"
                UPDATE assets
                SET file_type = ?1, usage = NULL, map_role = NULL, resolution = NULL,
                    classification_confidence = 100, classification_basis = 'extension',
                    asset_type = ?1, group_key = ?2, variant_group = ?2
                WHERE id = ?3
                "#,
                params![file_type, group_key, id],
            )?;
            continue;
        }

        let group_key = texture_group_key(relative_path);
        let (directory_role, texture_directory) = texture_directory_evidence(relative_path);
        let stem_role = relative_path
            .file_stem()
            .or_else(|| relative_path.file_name())
            .and_then(|stem| texture_stem_and_role(&stem.to_string_lossy()).1);
        let group = group_evidence.get(&group_key);
        let sibling_set = group
            .is_some_and(|(roles, count)| roles.len() >= 2 || (*count >= 2 && !roles.is_empty()));
        let is_texture = texture_directory || directory_role.is_some() || sibling_set;
        let inferred_base_role =
            (sibling_set && stem_role.is_none() && directory_role.is_none()).then_some("color");
        let map_role = stem_role.or(directory_role).or(inferred_base_role);
        let mut basis = Vec::new();
        if texture_directory {
            basis.push("texture-directory");
        }
        if directory_role.is_some() {
            basis.push("map-role-directory");
        }
        if stem_role.is_some() {
            basis.push("map-role-filename");
        }
        if sibling_set {
            basis.push("sibling-map-set");
        }
        if inferred_base_role.is_some() {
            basis.push("unmarked-base-map");
        }
        if basis.is_empty() {
            basis.push("extension-only");
        }
        let confidence = if directory_role.is_some() && stem_role.is_some() {
            100
        } else if texture_directory && sibling_set {
            95
        } else if directory_role.is_some() || sibling_set {
            90
        } else if texture_directory {
            85
        } else if stem_role.is_some() {
            55
        } else {
            100
        };
        connection.execute(
            r#"
            UPDATE assets
            SET file_type = 'image', usage = ?1, map_role = ?2, resolution = ?3,
                classification_confidence = ?4, classification_basis = ?5,
                asset_type = ?6, group_key = ?7, variant_group = ?7
            WHERE id = ?8
            "#,
            params![
                is_texture.then_some("texture"),
                map_role,
                texture_resolution(relative_path),
                confidence,
                basis.join(","),
                if is_texture { "texture" } else { "image" },
                is_texture.then_some(group_key),
                id,
            ],
        )?;
    }

    let candidate_filter = pack_id
        .map(|id| format!(" AND candidate.pack_id = {id}"))
        .unwrap_or_default();
    let asset_filter = pack_id
        .map(|id| format!(" AND pack_id = {id}"))
        .unwrap_or_default();
    connection.execute_batch(&format!(
        r#"
        UPDATE assets AS candidate
        SET group_key = (
            SELECT canonical.group_key
            FROM assets canonical
            WHERE canonical.pack_id = candidate.pack_id
              AND lower(canonical.name) = lower(candidate.name)
              AND canonical.asset_type = 'texture'
              AND canonical.id != candidate.id
              AND (
                  instr('/' || lower(canonical.relative_path) || '/', '/texture/') > 0 OR
                  instr('/' || lower(canonical.relative_path) || '/', '/textures/') > 0
              )
            ORDER BY canonical.relative_path COLLATE NOCASE
            LIMIT 1
        )
        WHERE candidate.file_type = 'image'
          {candidate_filter}
          AND (
              instr('/' || lower(candidate.relative_path) || '/', '/glb/') > 0 OR
              instr('/' || lower(candidate.relative_path) || '/', '/gltf/') > 0 OR
              instr('/' || lower(candidate.relative_path) || '/', '/fbx/') > 0 OR
              instr('/' || lower(candidate.relative_path) || '/', '/obj/') > 0 OR
              instr('/' || lower(candidate.relative_path) || '/', '/dae/') > 0 OR
              instr('/' || lower(candidate.relative_path) || '/', '/blend/') > 0
          )
          AND EXISTS (
              SELECT 1
              FROM assets canonical
              WHERE canonical.pack_id = candidate.pack_id
                AND lower(canonical.name) = lower(candidate.name)
                AND canonical.asset_type = 'texture'
                AND canonical.id != candidate.id
                AND (
                    instr('/' || lower(canonical.relative_path) || '/', '/texture/') > 0 OR
                    instr('/' || lower(canonical.relative_path) || '/', '/textures/') > 0
                )
          );

        UPDATE assets
        SET usage = 'texture', asset_type = 'texture',
            classification_confidence = MAX(classification_confidence, 95),
            classification_basis = CASE
                WHEN instr(classification_basis, 'matching-canonical-texture') > 0
                THEN classification_basis
                ELSE classification_basis || ',matching-canonical-texture'
            END,
            variant_group = group_key
        WHERE group_key LIKE 'texture:%'
          AND file_type = 'image'
          {asset_filter};
        "#
    ))?;
    Ok(())
}

fn apply_classification_overrides(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
    let pack_filter = pack_id
        .map(|id| format!(" AND assets.pack_id = {id}"))
        .unwrap_or_default();
    connection.execute_batch(&format!(
        r#"
        UPDATE assets
        SET
            asset_type = COALESCE((SELECT asset_type FROM classification_overrides WHERE asset_id = assets.id), asset_type),
            usage = CASE
                WHEN (SELECT asset_type FROM classification_overrides WHERE asset_id = assets.id) = 'texture' THEN 'texture'
                WHEN (SELECT asset_type FROM classification_overrides WHERE asset_id = assets.id) IS NOT NULL THEN NULL
                ELSE usage
            END,
            map_role = CASE
                WHEN (SELECT map_role FROM classification_overrides WHERE asset_id = assets.id) = '__none' THEN NULL
                ELSE COALESCE((SELECT map_role FROM classification_overrides WHERE asset_id = assets.id), map_role)
            END,
            group_key = COALESCE((SELECT group_key FROM classification_overrides WHERE asset_id = assets.id), group_key),
            variant_group = COALESCE((SELECT group_key FROM classification_overrides WHERE asset_id = assets.id), variant_group),
            classification_confidence = 100,
            classification_basis = 'manual-override'
        WHERE EXISTS (SELECT 1 FROM classification_overrides WHERE asset_id = assets.id)
          {pack_filter};
        "#,
    ))?;
    Ok(())
}

fn resource_name(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\''))
        .replace('\\', "/");
    let path = Path::new(&normalized);
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if classify_extension(&extension) != "image" {
        return None;
    }
    path.file_stem()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
}

fn extract_resource_names(path: &Path, extension: &str) -> HashSet<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return HashSet::new();
    };
    let mut names = HashSet::new();
    if extension == "mtl" {
        for line in contents.lines() {
            let line = line.trim();
            let Some((keyword, value)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let keyword = keyword.to_ascii_lowercase();
            if keyword.starts_with("map_")
                || matches!(keyword.as_str(), "bump" | "disp" | "decal" | "norm")
            {
                if let Some(name) = resource_name(value.split_whitespace().last().unwrap_or(value))
                {
                    names.insert(name);
                }
            }
        }
    } else if extension == "dae" {
        let mut rest = contents.as_str();
        while let Some(start) = rest.find("<init_from>") {
            rest = &rest[start + "<init_from>".len()..];
            let Some(end) = rest.find('<') else {
                break;
            };
            if let Some(name) = resource_name(&rest[..end]) {
                names.insert(name);
            }
            rest = &rest[end..];
        }
    }
    names
}

fn recompute_asset_dependencies(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
    let pack_filter = pack_id
        .map(|id| format!(" AND source.pack_id = {id}"))
        .unwrap_or_default();
    let delete_filter = pack_id
        .map(|id| format!(" WHERE owner_asset_id IN (SELECT id FROM assets WHERE pack_id = {id})"))
        .unwrap_or_default();
    connection.execute(
        &format!("DELETE FROM asset_dependencies{delete_filter}"),
        [],
    )?;

    let sources = {
        let mut statement = connection.prepare(&format!(
            r#"
            SELECT source.pack_id, source.group_key, source.absolute_path, source.extension
            FROM assets source
            WHERE source.extension IN ('mtl', 'dae')
              AND source.group_key IS NOT NULL
              AND source.missing = 0
              {pack_filter}
            ORDER BY CASE source.extension WHEN 'mtl' THEN 0 ELSE 1 END
            "#
        ))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    let mut resolved_groups = HashSet::new();
    for (source_pack_id, variant_group, absolute_path, extension) in sources {
        let group_key = (source_pack_id, variant_group.clone());
        if extension == "dae" && resolved_groups.contains(&group_key) {
            continue;
        }
        let Some(owner_id) = connection
            .query_row(
                "SELECT id FROM assets WHERE pack_id = ?1 AND group_key = ?2 AND is_primary = 1 LIMIT 1",
                params![source_pack_id, variant_group],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        else {
            continue;
        };
        let mut linked_resource = false;
        for name in extract_resource_names(Path::new(&absolute_path), &extension) {
            let dependency_id = connection
                .query_row(
                    r#"
                    SELECT id
                    FROM assets
                    WHERE pack_id = ?1
                      AND lower(name) = ?2
                      AND asset_type IN ('texture', 'image')
                      AND missing = 0
                    ORDER BY
                        CASE asset_type WHEN 'texture' THEN 0 ELSE 1 END,
                        is_primary DESC,
                        relative_path COLLATE NOCASE,
                        id
                    LIMIT 1
                    "#,
                    params![source_pack_id, name],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if let Some(dependency_id) = dependency_id {
                connection.execute(
                    "INSERT OR IGNORE INTO asset_dependencies(owner_asset_id, dependency_asset_id) VALUES (?1, ?2)",
                    params![owner_id, dependency_id],
                )?;
                linked_resource = true;
            }
        }
        if linked_resource {
            resolved_groups.insert(group_key);
        }
    }

    let texture_pack_filter = pack_id
        .map(|id| format!(" AND primary_asset.pack_id = {id}"))
        .unwrap_or_default();
    connection.execute(
        &format!(
            r#"
            INSERT OR IGNORE INTO asset_dependencies(owner_asset_id, dependency_asset_id)
            SELECT primary_asset.id, map.id
            FROM assets primary_asset
            JOIN assets map
              ON map.pack_id = primary_asset.pack_id
             AND map.group_key = primary_asset.group_key
             AND map.id != primary_asset.id
            WHERE primary_asset.is_primary = 1
              AND primary_asset.missing = 0
              AND map.missing = 0
              AND primary_asset.asset_type = 'texture'
              AND primary_asset.group_key LIKE 'texture:%'
              AND NOT EXISTS (
                  SELECT 1
                  FROM asset_dependencies model_dependency
                  WHERE model_dependency.dependency_asset_id = primary_asset.id
              )
              {texture_pack_filter}
            "#
        ),
        [],
    )?;

    connection.execute(
        "UPDATE assets SET is_primary = 0 WHERE id IN (SELECT dependency_asset_id FROM asset_dependencies)",
        [],
    )?;
    connection.execute_batch(
        r#"
        UPDATE assets
        SET is_primary = 0
        WHERE asset_type = 'image'
          AND (
              instr('/' || lower(relative_path) || '/', '/glb/') > 0 OR
              instr('/' || lower(relative_path) || '/', '/gltf/') > 0 OR
              instr('/' || lower(relative_path) || '/', '/fbx/') > 0 OR
              instr('/' || lower(relative_path) || '/', '/obj/') > 0 OR
              instr('/' || lower(relative_path) || '/', '/dae/') > 0 OR
              instr('/' || lower(relative_path) || '/', '/blend/') > 0
          );
        "#,
    )?;
    Ok(())
}

fn migrate_classification(connection: &mut Connection) -> Result<()> {
    const CLASSIFICATION_VERSION: &str = "2";
    let current_version = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = 'classification_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current_version.as_deref() == Some(CLASSIFICATION_VERSION) {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    recompute_texture_groups(&transaction, None)?;
    apply_classification_overrides(&transaction, None)?;
    recompute_primary_assets(&transaction, None)?;
    recompute_asset_dependencies(&transaction, None)?;
    transaction.execute(
        r#"
        INSERT INTO app_metadata(key, value) VALUES ('classification_version', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        params![CLASSIFICATION_VERSION],
    )?;
    transaction.commit()?;
    rebuild_search_index(connection)
}

fn modified_timestamp(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_unhashed_assets(state: &AppState) -> Result<usize> {
    if state.hashing_library.swap(true, Ordering::AcqRel) {
        return Ok(0);
    }
    let result = (|| {
        let connection = state.connect()?;
        let jobs = {
            let mut statement = connection.prepare(
                "SELECT id, absolute_path, size_bytes, modified_at FROM assets WHERE content_hash IS NULL AND missing = 0",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let hashes = jobs
            .into_iter()
            .filter_map(|(id, path, size, modified)| {
                hash_file(Path::new(&path))
                    .ok()
                    .map(|hash| (id, size, modified, hash))
            })
            .collect::<Vec<_>>();
        let _guard = state
            .write_queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = state.connect()?;
        let transaction = connection.transaction()?;
        for (id, size, modified, hash) in &hashes {
            transaction.execute(
                "UPDATE assets SET content_hash = ?1 WHERE id = ?2 AND size_bytes = ?3 AND modified_at = ?4 AND missing = 0",
                params![hash, id, size, modified],
            )?;
        }
        transaction.commit()?;
        Ok(hashes.len())
    })();
    state.hashing_library.store(false, Ordering::Release);
    result
}

fn image_dimensions(path: &Path, asset_type: &str) -> (Option<i64>, Option<i64>) {
    if asset_type != "image" && asset_type != "texture" {
        return (None, None);
    }
    imagesize::size(path)
        .map(|size| (Some(size.width as i64), Some(size.height as i64)))
        .unwrap_or((None, None))
}

fn parse_gltf_json(json_bytes: &[u8]) -> Option<(i64, i64)> {
    let val: serde_json::Value = serde_json::from_slice(json_bytes).ok()?;
    let accessors = val.get("accessors")?.as_array()?;
    let meshes = val.get("meshes")?.as_array()?;
    let mut total_vertices = 0i64;
    let mut total_triangles = 0i64;

    for mesh in meshes {
        if let Some(primitives) = mesh.get("primitives").and_then(|p| p.as_array()) {
            for prim in primitives {
                if let Some(pos_idx) = prim
                    .get("attributes")
                    .and_then(|a| a.get("POSITION"))
                    .and_then(|idx| idx.as_u64())
                {
                    if let Some(acc) = accessors.get(pos_idx as usize) {
                        let vert_count = acc.get("count").and_then(|c| c.as_i64()).unwrap_or(0);
                        total_vertices += vert_count;

                        if let Some(ind_idx) = prim.get("indices").and_then(|idx| idx.as_u64()) {
                            if let Some(ind_acc) = accessors.get(ind_idx as usize) {
                                let ind_count =
                                    ind_acc.get("count").and_then(|c| c.as_i64()).unwrap_or(0);
                                let mode = prim.get("mode").and_then(|m| m.as_i64()).unwrap_or(4);
                                match mode {
                                    4 => total_triangles += ind_count / 3,
                                    5 | 6 => total_triangles += (ind_count - 2).max(0),
                                    _ => {}
                                }
                            }
                        } else {
                            let mode = prim.get("mode").and_then(|m| m.as_i64()).unwrap_or(4);
                            match mode {
                                4 => total_triangles += vert_count / 3,
                                5 | 6 => total_triangles += (vert_count - 2).max(0),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    if total_vertices > 0 || total_triangles > 0 {
        Some((total_triangles, total_vertices))
    } else {
        None
    }
}

fn glb_poly_count(path: &Path) -> Option<(i64, i64)> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    let magic = u32::from_le_bytes(header[0..4].try_into().ok()?);
    if magic != 0x4654_6C67 {
        return None;
    }
    let mut chunk_header = [0u8; 8];
    file.read_exact(&mut chunk_header).ok()?;
    let chunk_len = u32::from_le_bytes(chunk_header[0..4].try_into().ok()?) as usize;
    let chunk_type = u32::from_le_bytes(chunk_header[4..8].try_into().ok()?);
    if chunk_type != 0x4E4F_534A || chunk_len > 32 * 1024 * 1024 {
        return None;
    }
    let mut json_bytes = vec![0u8; chunk_len];
    file.read_exact(&mut json_bytes).ok()?;
    parse_gltf_json(&json_bytes)
}

fn gltf_poly_count(path: &Path) -> Option<(i64, i64)> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    let reader = BufReader::new(file);
    let val: serde_json::Value = serde_json::from_reader(reader).ok()?;
    let json_bytes = serde_json::to_vec(&val).ok()?;
    parse_gltf_json(&json_bytes)
}

fn obj_poly_count(path: &Path) -> Option<(i64, i64)> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut vertices = 0i64;
    let mut triangles = 0i64;
    for line in reader.lines().flatten() {
        let trimmed = line.trim();
        if trimmed.starts_with("v ") {
            vertices += 1;
        } else if trimmed.starts_with("f ") {
            let count = trimmed.split_whitespace().count();
            if count >= 4 {
                triangles += (count as i64 - 1 - 2).max(1);
            } else if count == 3 {
                triangles += 1;
            }
        }
    }
    if vertices > 0 || triangles > 0 {
        Some((triangles, vertices))
    } else {
        None
    }
}

fn model_poly_count(path: &Path, extension: &str) -> (Option<i64>, Option<i64>) {
    let result = match extension.to_ascii_lowercase().as_str() {
        "glb" => glb_poly_count(path),
        "gltf" => gltf_poly_count(path),
        "obj" => obj_poly_count(path),
        _ => None,
    };
    match result {
        Some((triangles, vertices)) => (Some(triangles), Some(vertices)),
        None => (None, None),
    }
}

fn generate_thumbnail(source: &Path, destination: &Path) -> Option<()> {
    let file = File::open(source).ok()?;
    let reader = BufReader::new(file);
    let image = image::ImageReader::new(reader)
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    fs::create_dir_all(destination.parent()?).ok()?;
    let out_file = File::create(destination).ok()?;
    let mut writer = BufWriter::new(out_file);
    image
        .thumbnail(384, 288)
        .write_to(&mut writer, image::ImageFormat::Png)
        .ok()?;
    Some(())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn rotate_log_if_needed(path: &Path) -> std::io::Result<()> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() >= 2 * 1024 * 1024) {
        let rotated = path.with_extension("log.1");
        if rotated.is_file() {
            fs::remove_file(&rotated)?;
        }
        fs::rename(path, rotated)?;
    }
    Ok(())
}

fn create_backup(connection: &Connection, destination: &Path) -> Result<String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    connection.backup(
        rusqlite::MAIN_DB,
        destination,
        None::<fn(rusqlite::backup::Progress)>,
    )?;
    Ok(path_string(destination))
}

fn create_rotating_backup(
    state: &AppState,
    connection: &Connection,
    reason: &str,
) -> Result<String> {
    fs::create_dir_all(&state.backup_directory)?;
    let destination = state
        .backup_directory
        .join(format!("lootbox-{}-{reason}.db", unix_timestamp()));
    let result = create_backup(connection, &destination)?;
    let mut backups = fs::read_dir(&state.backup_directory)?
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .collect::<Vec<_>>();
    backups.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    let remove_count = backups.len().saturating_sub(5);
    for entry in backups.into_iter().take(remove_count) {
        fs::remove_file(entry.path())?;
    }
    Ok(result)
}

fn cache_status_from_connection(state: &AppState, connection: &Connection) -> Result<CacheStatus> {
    let referenced = {
        let mut statement = connection
            .prepare("SELECT thumbnail_path FROM assets WHERE thumbnail_path IS NOT NULL")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<HashSet<_>, _>>()?;
        rows
    };
    let mut status = CacheStatus {
        thumbnail_files: 0,
        thumbnail_bytes: 0,
        orphan_files: 0,
        orphan_bytes: 0,
        limit_bytes: DEFAULT_CACHE_LIMIT_BYTES,
    };
    for entry in fs::read_dir(&state.thumbnail_directory)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        status.thumbnail_files += 1;
        status.thumbnail_bytes += metadata.len();
        if !referenced.contains(&path_string(&entry.path())) {
            status.orphan_files += 1;
            status.orphan_bytes += metadata.len();
        }
    }
    Ok(status)
}

fn clean_thumbnail_cache_from_connection(
    state: &AppState,
    connection: &Connection,
) -> Result<CacheStatus> {
    let referenced = {
        let mut statement = connection.prepare(
            "SELECT thumbnail_path FROM assets WHERE thumbnail_path IS NOT NULL AND ((asset_type = 'model' AND thumbnail_version = ?1) OR (asset_type != 'model' AND thumbnail_version = ?2))",
        )?;
        let rows = statement
            .query_map(
                params![MODEL_THUMBNAIL_VERSION, IMAGE_THUMBNAIL_VERSION],
                |row| row.get::<_, String>(0),
            )?
            .collect::<std::result::Result<HashSet<_>, _>>()?;
        rows
    };
    for entry in fs::read_dir(&state.thumbnail_directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && !referenced.contains(&path_string(&entry.path())) {
            fs::remove_file(entry.path())?;
        }
    }
    let stale_rows = {
        let mut statement = connection.prepare("SELECT id, thumbnail_path, thumbnail_version, asset_type FROM assets WHERE thumbnail_path IS NOT NULL")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    for (asset_id, path, version, asset_type) in stale_rows {
        let current_version = if asset_type == "model" {
            MODEL_THUMBNAIL_VERSION
        } else {
            IMAGE_THUMBNAIL_VERSION
        };
        let file_path = Path::new(&path);
        let is_corrupted = match fs::metadata(file_path) {
            Ok(meta) => meta.len() < 1000,
            Err(_) => true,
        };
        if version != current_version || is_corrupted {
            let _ = fs::remove_file(file_path);
            connection.execute(
                "UPDATE assets SET thumbnail_path = NULL, thumbnail_version = 0 WHERE id = ?1",
                params![asset_id],
            )?;
        }
    }
    // Enforce the cap by evicting oldest referenced previews; they regenerate lazily.
    let mut files = fs::read_dir(&state.thumbnail_directory)?
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then_some((entry.path(), metadata.len(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| entry.2);
    let mut total = files.iter().map(|entry| entry.1).sum::<u64>();
    for (path, size, _) in files {
        if total <= DEFAULT_CACHE_LIMIT_BYTES {
            break;
        }
        fs::remove_file(&path)?;
        connection.execute(
            "UPDATE assets SET thumbnail_path = NULL, thumbnail_version = 0 WHERE thumbnail_path = ?1",
            params![path_string(&path)],
        )?;
        total = total.saturating_sub(size);
    }
    cache_status_from_connection(state, connection)
}

#[tauri::command]
fn cancel_import(job_id: String, state: State<'_, AppState>) -> Result<()> {
    if let Some(flag) = state
        .import_cancellations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&job_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
fn get_cache_status(state: State<'_, AppState>) -> Result<CacheStatus> {
    cache_status_from_connection(&state, &state.connect()?)
}

#[tauri::command]
fn clean_thumbnail_cache(state: State<'_, AppState>) -> Result<CacheStatus> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clean_thumbnail_cache_from_connection(&state, &state.connect()?)
}

#[tauri::command]
fn clear_thumbnail_cache(state: State<'_, AppState>) -> Result<CacheStatus> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for entry in fs::read_dir(&state.thumbnail_directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
        }
    }
    let connection = state.connect()?;
    connection.execute(
        "UPDATE assets SET thumbnail_path = NULL, thumbnail_version = 0",
        [],
    )?;
    cache_status_from_connection(&state, &connection)
}

#[tauri::command]
async fn regenerate_image_thumbnails(state: State<'_, AppState>) -> Result<CacheStatus> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state
            .write_queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = state.connect()?;
        let jobs = {
            let mut statement = connection.prepare(
                "SELECT id, absolute_path FROM assets WHERE missing = 0 AND file_type = 'image' AND thumbnail_path IS NULL",
            )?;
            let rows = statement
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let results: Vec<(i64, PathBuf)> = jobs
            .into_par_iter()
            .filter_map(|(asset_id, source)| {
                let destination = state.thumbnail_directory.join(format!("{asset_id}.png"));
                if generate_thumbnail(Path::new(&source), &destination).is_some() {
                    Some((asset_id, destination))
                } else {
                    None
                }
            })
            .collect();
        if !results.is_empty() {
            let tx = connection.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "UPDATE assets SET thumbnail_path = ?1, thumbnail_version = ?2 WHERE id = ?3",
                )?;
                for (asset_id, destination) in results {
                    stmt.execute(params![path_string(&destination), IMAGE_THUMBNAIL_VERSION, asset_id])?;
                }
            }
            tx.commit()?;
        }
        cache_status_from_connection(&state, &connection)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
fn create_metadata_backup(
    destination: Option<String>,
    state: State<'_, AppState>,
) -> Result<String> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = state.connect()?;
    match destination {
        Some(path) => create_backup(&connection, Path::new(&path)),
        None => create_rotating_backup(&state, &connection, "manual"),
    }
}

#[tauri::command]
fn restore_metadata_backup(path: String, state: State<'_, AppState>) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let source =
        Connection::open(&path).map_err(|error| LootboxError::InvalidBackup(error.to_string()))?;
    let integrity: String = source
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| LootboxError::InvalidBackup(error.to_string()))?;
    let has_assets: bool = source
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'assets')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| LootboxError::InvalidBackup(error.to_string()))?;
    if integrity != "ok" || !has_assets {
        return Err(LootboxError::InvalidBackup(
            "integrity check failed or schema is not Lootbox".into(),
        ));
    }
    drop(source);
    let mut connection = state.connect()?;
    create_rotating_backup(&state, &connection, "before-restore")?;
    connection.restore(
        rusqlite::MAIN_DB,
        &path,
        None::<fn(rusqlite::backup::Progress)>,
    )?;
    initialize_database(&connection)?;
    migrate_classification(&mut connection)?;
    state.record("info", "backup", format!("Restored metadata from {path}"));
    Ok(())
}

#[tauri::command]
fn get_diagnostics(state: State<'_, AppState>) -> Result<Vec<DiagnosticEntry>> {
    Ok(state
        .diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect())
}

#[tauri::command]
fn log_diagnostic(
    level: String,
    context: String,
    message: String,
    state: State<'_, AppState>,
) -> Result<()> {
    state.record(&level, &context, message);
    Ok(())
}

#[tauri::command]
async fn save_model_thumbnail(
    asset_id: i64,
    png_data: String,
    state: State<'_, AppState>,
) -> Result<String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_model_thumbnail_from_state(asset_id, png_data, &state)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

fn save_model_thumbnail_from_state(
    asset_id: i64,
    png_data: String,
    state: &AppState,
) -> Result<String> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    let png_data = BASE64
        .decode(png_data)
        .map_err(|_| LootboxError::InvalidThumbnail)?;
    if png_data.len() < 1000 || png_data.len() > 2 * 1024 * 1024 || !png_data.starts_with(PNG_SIGNATURE) {
        return Err(LootboxError::InvalidThumbnail);
    }

    let connection = state.connect()?;
    let is_model: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets WHERE id = ?1 AND asset_type = 'model')",
        params![asset_id],
        |row| row.get(0),
    )?;
    if !is_model {
        return Err(LootboxError::InvalidThumbnail);
    }

    fs::create_dir_all(&state.thumbnail_directory)?;
    let thumbnail_path = state.thumbnail_directory.join(format!("{asset_id}.png"));
    let temporary_path = state.thumbnail_directory.join(format!("{asset_id}.tmp"));
    fs::write(&temporary_path, png_data)?;
    fs::rename(&temporary_path, &thumbnail_path)?;
    let thumbnail_path = path_string(&thumbnail_path);
    connection.execute(
        "UPDATE assets SET thumbnail_path = ?1, thumbnail_version = ?2 WHERE id = ?3",
        params![thumbnail_path, MODEL_THUMBNAIL_VERSION, asset_id],
    )?;
    Ok(thumbnail_path)
}

#[tauri::command]
async fn import_pack(
    path: String,
    job_id: String,
    on_event: Channel<ImportProgress>,
    state: State<'_, AppState>,
) -> Result<PackSummary> {
    let state = state.inner().clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    state
        .import_cancellations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(job_id.clone(), cancelled.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| {
            // Every writer uses this gate. Imports, tag edits and thumbnail updates
            // therefore queue consistently instead of racing SQLite's single writer.
            let _queue_guard = state
                .write_queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if cancelled.load(Ordering::Relaxed) {
                return Err(LootboxError::ImportCancelled);
            }
            let root = fs::canonicalize(PathBuf::from(path))?;
            if !root.is_dir() {
                return Err(LootboxError::InvalidDirectory);
            }
            let mut connection = state.connect()?;
            let mut report = |progress| {
                let _ = on_event.send(progress);
            };
            import_pack_from_path(
                &mut connection,
                &root,
                Some(&state.thumbnail_directory),
                Some(cancelled.as_ref()),
                &mut report,
            )
        })();
        state
            .import_cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&job_id);
        if let Err(error) = &result {
            state.record("error", "import", error.to_string());
        }
        result
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

fn import_pack_from_path(
    connection: &mut Connection,
    root: &Path,
    thumbnail_directory: Option<&Path>,
    cancelled: Option<&AtomicBool>,
    on_progress: &mut impl FnMut(ImportProgress),
) -> Result<PackSummary> {
    on_progress(ImportProgress {
        phase: "scanning",
        current: 0,
        total: 0,
        path: None,
    });
    let mut files = Vec::new();
    let mut last_progress = Instant::now();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(LootboxError::ImportCancelled);
        }
        files.push(entry.into_path());
        if files.len() % 250 == 0 || last_progress.elapsed() >= Duration::from_millis(100) {
            on_progress(ImportProgress {
                phase: "scanning",
                current: files.len(),
                total: 0,
                path: None,
            });
            last_progress = Instant::now();
        }
    }

    let total = files.len();
    on_progress(ImportProgress {
        phase: "hashing",
        current: 0,
        total,
        path: None,
    });
    let existing_hashes = {
        let mut statement = connection.prepare(
            "SELECT relative_path, size_bytes, modified_at, content_hash FROM assets WHERE pack_id = (SELECT id FROM packs WHERE root_path = ?1)",
        )?;
        let rows = statement
            .query_map(params![path_string(root)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(path, size, modified, hash)| (path, (size, modified, hash)))
            .collect::<HashMap<_, _>>()
    };
    let mut content_hashes = HashMap::new();
    last_progress = Instant::now();
    for (index, absolute_path) in files.iter().enumerate() {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(LootboxError::ImportCancelled);
        }
        let relative_path = absolute_path.strip_prefix(root).unwrap_or(absolute_path);
        let relative_string = path_string(relative_path);
        let hash = fs::metadata(absolute_path).ok().and_then(|metadata| {
            let modified = modified_timestamp(&metadata);
            existing_hashes
                .get(&relative_string)
                .filter(|(size, previous_modified, hash)| {
                    *size == metadata.len() as i64
                        && *previous_modified == modified
                        && hash.is_some()
                })
                .and_then(|entry| entry.2.clone())
                .or_else(|| hash_file(absolute_path).ok())
        });
        if let Some(hash) = hash {
            content_hashes.insert(absolute_path.clone(), hash);
        }
        if index + 1 == total || last_progress.elapsed() >= Duration::from_millis(100) {
            on_progress(ImportProgress {
                phase: "hashing",
                current: index + 1,
                total,
                path: Some(relative_string),
            });
            last_progress = Instant::now();
        }
    }
    on_progress(ImportProgress {
        phase: "indexing",
        current: 0,
        total,
        path: None,
    });

    let pack_name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Asset pack".to_string());
    let root_path = path_string(&root);
    let transaction = connection.transaction()?;
    let mut thumbnail_jobs = Vec::new();

    transaction.execute(
        r#"
        INSERT INTO packs(name, root_path, last_scanned_at, generation)
        VALUES (?1, ?2, CURRENT_TIMESTAMP, 1)
        ON CONFLICT(root_path) DO UPDATE SET
            last_scanned_at = CURRENT_TIMESTAMP,
            generation = packs.generation + 1
        "#,
        params![pack_name, root_path],
    )?;

    let (pack_id, generation): (i64, i64) = transaction.query_row(
        "SELECT id, generation FROM packs WHERE root_path = ?1",
        params![root_path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    last_progress = Instant::now();
    for (index, absolute_path) in files.iter().enumerate() {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(LootboxError::ImportCancelled);
        }
        let relative_path = absolute_path.strip_prefix(&root).unwrap_or(absolute_path);
        let relative_path_string = path_string(relative_path);
        if index == 0 || last_progress.elapsed() >= Duration::from_millis(100) {
            on_progress(ImportProgress {
                phase: "indexing",
                current: index,
                total,
                path: Some(relative_path_string.clone()),
            });
            last_progress = Instant::now();
        }
        let metadata = match fs::metadata(absolute_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                transaction.execute(
                    "UPDATE assets SET generation = ?1, missing = 1, missing_since = COALESCE(missing_since, CURRENT_TIMESTAMP) WHERE pack_id = ?2 AND relative_path = ?3",
                    params![generation, pack_id, relative_path_string],
                )?;
                continue;
            }
        };
        let extension = absolute_path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let asset_type = classify_asset_type(relative_path, &extension);
        let name = absolute_path
            .file_stem()
            .or_else(|| absolute_path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        let variant_group = model_variant_group(relative_path, asset_type, &extension);
        let (width, height) = image_dimensions(absolute_path, asset_type);
        let (triangles, vertices) = if asset_type == "model" {
            model_poly_count(absolute_path, &extension)
        } else {
            (None, None)
        };
        let modified_at = modified_timestamp(&metadata);
        let content_hash = content_hashes.get(absolute_path).cloned();
        let mut existing: Option<(i64, i64, i64, Option<String>, i64)> = transaction
            .query_row(
                "SELECT id, modified_at, size_bytes, thumbnail_path, thumbnail_version FROM assets WHERE pack_id = ?1 AND relative_path = ?2",
                params![pack_id, relative_path_string],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;

        // A unique stale size/mtime/format match is a rename or move. Reuse its
        // row so tags, collections, exclusions and manual overrides survive.
        if existing.is_none() {
            let candidates = if let Some(hash) = &content_hash {
                let mut statement = transaction.prepare(
                    "SELECT id, modified_at, size_bytes, thumbnail_path, thumbnail_version FROM assets WHERE pack_id = ?1 AND generation != ?2 AND content_hash = ?3 AND extension = ?4 LIMIT 2",
                )?;
                let rows = statement
                    .query_map(params![pack_id, generation, hash, extension], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            } else {
                let mut statement = transaction.prepare(
                    "SELECT id, modified_at, size_bytes, thumbnail_path, thumbnail_version FROM assets WHERE pack_id = ?1 AND generation != ?2 AND size_bytes = ?3 AND modified_at = ?4 AND extension = ?5 LIMIT 2",
                )?;
                let rows = statement
                    .query_map(
                        params![
                            pack_id,
                            generation,
                            metadata.len() as i64,
                            modified_at,
                            extension
                        ],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        },
                    )?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            };
            if candidates.len() == 1 {
                existing = candidates.into_iter().next();
                transaction.execute(
                    "UPDATE assets SET relative_path = ?1 WHERE id = ?2",
                    params![relative_path_string, existing.as_ref().map(|entry| entry.0)],
                )?;
            }
        }

        transaction.execute(
            r#"
            INSERT INTO assets(
                pack_id, relative_path, absolute_path, name, extension, asset_type,
                size_bytes, modified_at, width, height, triangles, vertices, variant_group, content_hash, generation
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(pack_id, relative_path) DO UPDATE SET
                absolute_path = excluded.absolute_path,
                name = excluded.name,
                extension = excluded.extension,
                asset_type = excluded.asset_type,
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                width = excluded.width,
                height = excluded.height,
                triangles = excluded.triangles,
                vertices = excluded.vertices,
                variant_group = excluded.variant_group,
                content_hash = excluded.content_hash,
                generation = excluded.generation,
                missing = 0,
                missing_since = NULL
            "#,
            params![
                pack_id,
                relative_path_string,
                path_string(absolute_path),
                name,
                extension,
                asset_type,
                metadata.len() as i64,
                modified_at,
                width,
                height,
                triangles,
                vertices,
                variant_group,
                content_hash,
                generation,
            ],
        )?;

        if asset_type == "image" || asset_type == "texture" {
            if let Some(thumbnail_directory) = thumbnail_directory {
                let asset_id = existing
                    .as_ref()
                    .map(|entry| entry.0)
                    .unwrap_or_else(|| transaction.last_insert_rowid());
                let thumbnail_path = thumbnail_directory.join(format!("{asset_id}.png"));
                let thumbnail_is_current = existing.as_ref().is_some_and(|entry| {
                    entry.1 == modified_at
                        && entry.2 == metadata.len() as i64
                        && entry.4 == IMAGE_THUMBNAIL_VERSION
                        && entry
                            .3
                            .as_ref()
                            .is_some_and(|path| Path::new(path).is_file())
                });
                if thumbnail_is_current {
                    transaction.execute(
                        "UPDATE assets SET thumbnail_path = ?1, thumbnail_version = ?2 WHERE id = ?3",
                        params![path_string(&thumbnail_path), IMAGE_THUMBNAIL_VERSION, asset_id],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE assets SET thumbnail_path = NULL, thumbnail_version = 0 WHERE id = ?1",
                        params![asset_id],
                    )?;
                    thumbnail_jobs.push((asset_id, absolute_path.clone(), thumbnail_path));
                }
            }
        }

        if index + 1 == total {
            on_progress(ImportProgress {
                phase: "indexing",
                current: total,
                total,
                path: Some(path_string(relative_path)),
            });
        }
    }

    on_progress(ImportProgress {
        phase: "finalizing",
        current: total,
        total,
        path: None,
    });
    transaction.execute(
        "UPDATE assets SET missing = 1, missing_since = COALESCE(missing_since, CURRENT_TIMESTAMP) WHERE pack_id = ?1 AND generation != ?2",
        params![pack_id, generation],
    )?;
    recompute_texture_groups(&transaction, Some(pack_id))?;
    apply_classification_overrides(&transaction, Some(pack_id))?;
    recompute_primary_assets(&transaction, Some(pack_id))?;
    recompute_asset_dependencies(&transaction, Some(pack_id))?;
    transaction.commit()?;
    // Image decoding and resizing deliberately runs after the indexing
    // transaction, parallelized with Rayon across all CPU cores.
    let mut cancelled_after_commit = false;
    let results: Vec<(i64, PathBuf)> = thumbnail_jobs
        .into_par_iter()
        .filter_map(|(asset_id, source, destination)| {
            if cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return None;
            }
            if generate_thumbnail(&source, &destination).is_some() {
                Some((asset_id, destination))
            } else {
                None
            }
        })
        .collect();

    if cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        cancelled_after_commit = true;
    }

    if !results.is_empty() {
        let tx = connection.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE assets SET thumbnail_path = ?1, thumbnail_version = ?2 WHERE id = ?3",
            )?;
            for (asset_id, destination) in results {
                stmt.execute(params![path_string(&destination), IMAGE_THUMBNAIL_VERSION, asset_id])?;
            }
        }
        tx.commit()?;
    }
    rebuild_search_index(connection)?;
    let pack = get_pack(connection, pack_id)?;
    if cancelled_after_commit {
        return Err(LootboxError::ImportCancelled);
    }
    on_progress(ImportProgress {
        phase: "complete",
        current: total,
        total,
        path: None,
    });
    Ok(pack)
}

fn get_pack(connection: &Connection, pack_id: i64) -> Result<PackSummary> {
    Ok(connection.query_row(
        r#"
        SELECT p.id, p.name, p.root_path, COUNT(a.id), p.last_scanned_at,
            (SELECT COUNT(*) FROM assets removed
             WHERE removed.pack_id = p.id AND removed.is_primary = 1 AND removed.excluded = 1),
            (SELECT COUNT(*) FROM assets missing
             WHERE missing.pack_id = p.id AND missing.missing = 1)
        FROM packs p
        LEFT JOIN assets a ON a.pack_id = p.id AND a.is_primary = 1 AND a.excluded = 0 AND a.missing = 0
        WHERE p.id = ?1
        GROUP BY p.id
        "#,
        params![pack_id],
        |row| {
            let root_path: String = row.get(2)?;
            Ok(PackSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                available: Path::new(&root_path).is_dir(),
                root_path,
                asset_count: row.get(3)?,
                last_scanned_at: row.get(4)?,
                removed_asset_count: row.get(5)?,
                missing_asset_count: row.get(6)?,
            })
        },
    )?)
}

#[tauri::command]
fn get_library_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot> {
    let connection = state.connect()?;
    let total_assets = connection.query_row(
        "SELECT COUNT(*) FROM assets WHERE is_primary = 1 AND excluded = 0 AND missing = 0",
        [],
        |row| row.get(0),
    )?;
    let duplicate_assets = connection.query_row(
        r#"
        SELECT COUNT(*) FROM assets a
        WHERE a.is_primary = 1 AND a.excluded = 0 AND a.missing = 0
          AND a.content_hash IS NOT NULL
          AND (SELECT COUNT(*) FROM assets copy WHERE copy.content_hash = a.content_hash AND copy.missing = 0) > 1
        "#,
        [],
        |row| row.get(0),
    )?;
    let removed_assets = connection.query_row(
        "SELECT COUNT(*) FROM assets WHERE is_primary = 1 AND excluded = 1",
        [],
        |row| row.get(0),
    )?;
    let missing_assets =
        connection.query_row("SELECT COUNT(*) FROM assets WHERE missing = 1", [], |row| {
            row.get(0)
        })?;

    let packs = {
        let mut statement = connection.prepare(
            r#"
            SELECT p.id, p.name, p.root_path, COUNT(a.id), p.last_scanned_at,
                (SELECT COUNT(*) FROM assets removed
                 WHERE removed.pack_id = p.id AND removed.is_primary = 1 AND removed.excluded = 1),
                (SELECT COUNT(*) FROM assets missing
                 WHERE missing.pack_id = p.id AND missing.missing = 1)
            FROM packs p
            LEFT JOIN assets a ON a.pack_id = p.id AND a.is_primary = 1 AND a.excluded = 0 AND a.missing = 0
            GROUP BY p.id
            ORDER BY p.name COLLATE NOCASE
            "#,
        )?;
        let rows = statement
            .query_map([], |row| {
                let root_path: String = row.get(2)?;
                Ok(PackSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    available: Path::new(&root_path).is_dir(),
                    root_path,
                    asset_count: row.get(3)?,
                    last_scanned_at: row.get(4)?,
                    removed_asset_count: row.get(5)?,
                    missing_asset_count: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    let collections = {
        let mut statement = connection.prepare(
            r#"
            SELECT c.id, c.name, COUNT(a.id)
            FROM collections c
            LEFT JOIN collection_assets ca ON ca.collection_id = c.id
            LEFT JOIN assets a ON a.id = ca.asset_id AND a.is_primary = 1 AND a.excluded = 0
            GROUP BY c.id
            ORDER BY c.name COLLATE NOCASE
            "#,
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(CollectionSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    asset_count: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    let projects = {
        let mut statement = connection.prepare(
            r#"
            SELECT project.id, project.name, project.root_path,
                   COUNT(DISTINCT CASE WHEN asset.is_primary = 1 AND asset.missing = 0 THEN asset.id END),
                   MAX(exported.exported_at)
            FROM projects project
            LEFT JOIN project_exports exported ON exported.project_id = project.id
            LEFT JOIN assets asset ON asset.id = exported.asset_id
            GROUP BY project.id
            ORDER BY project.name COLLATE NOCASE
            "#,
        )?;
        let rows = statement
            .query_map([], |row| {
                let root_path: String = row.get(2)?;
                Ok(ProjectSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    available: Path::new(&root_path).join("project.godot").is_file(),
                    root_path,
                    asset_count: row.get(3)?,
                    last_exported_at: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    let type_counts = {
        let mut statement = connection.prepare(
            "SELECT asset_type, COUNT(*) FROM assets WHERE is_primary = 1 AND excluded = 0 AND missing = 0 GROUP BY asset_type ORDER BY asset_type",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(TypeCount {
                    asset_type: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    Ok(LibrarySnapshot {
        total_assets,
        duplicate_assets,
        removed_assets,
        missing_assets,
        hashing_assets: state.hashing_library.load(Ordering::Acquire),
        packs,
        collections,
        projects,
        type_counts,
    })
}

fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|term| {
            term.chars()
                .filter(|character| {
                    character.is_alphanumeric() || *character == '_' || *character == '-'
                })
                .collect::<String>()
        })
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

fn asset_query_filter(request: &AssetQuery) -> (String, Vec<rusqlite::types::Value>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if !request.missing.unwrap_or(false) {
        conditions.push("a.is_primary = 1".to_string());
    }
    conditions.push(if request.excluded.unwrap_or(false) {
        "a.excluded = 1".to_string()
    } else {
        "a.excluded = 0".to_string()
    });
    conditions.push(if request.missing.unwrap_or(false) {
        "a.missing = 1".to_string()
    } else {
        "a.missing = 0".to_string()
    });

    if let Some(search_query) = request.query.as_deref().and_then(fts_query) {
        conditions.push(
            "a.id IN (SELECT CAST(asset_id AS INTEGER) FROM assets_fts WHERE assets_fts MATCH ?)".to_string(),
        );
        values.push(search_query.into());
    }
    if let Some(asset_id) = request.asset_id {
        conditions.push("a.id = ?".to_string());
        values.push(asset_id.into());
    }
    if let Some(asset_type) = request
        .asset_type
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        conditions.push("a.asset_type = ?".to_string());
        values.push(asset_type.to_owned().into());
    }
    if let Some(pack_id) = request.pack_id {
        conditions.push("a.pack_id = ?".to_string());
        values.push(pack_id.into());
    }
    if let Some(collection_id) = request.collection_id {
        conditions.push(
            "EXISTS (SELECT 1 FROM collection_assets selected_ca WHERE selected_ca.asset_id = a.id AND selected_ca.collection_id = ?)".to_string(),
        );
        values.push(collection_id.into());
    }
    if let Some(project_id) = request.project_id {
        conditions.push(
            r#"EXISTS (
                SELECT 1
                FROM project_exports selected_export
                JOIN assets exported_asset ON exported_asset.id = selected_export.asset_id
                WHERE selected_export.project_id = ?
                  AND (
                    exported_asset.id = a.id OR
                    (a.group_key IS NOT NULL
                      AND exported_asset.pack_id = a.pack_id
                      AND exported_asset.group_key = a.group_key)
                  )
            )"#.to_string(),
        );
        values.push(project_id.into());
    }
    if request.unused_by_projects.unwrap_or(false) {
        conditions.push(
            r#"NOT EXISTS (
                SELECT 1
                FROM project_exports any_export
                JOIN assets exported_asset ON exported_asset.id = any_export.asset_id
                WHERE exported_asset.id = a.id OR
                  (a.group_key IS NOT NULL
                    AND exported_asset.pack_id = a.pack_id
                    AND exported_asset.group_key = a.group_key)
            )"#.to_string(),
        );
    }
    if request.duplicates_only.unwrap_or(false) {
        conditions.push("a.content_hash IS NOT NULL AND (SELECT COUNT(*) FROM assets duplicate WHERE duplicate.content_hash = a.content_hash AND duplicate.missing = 0) > 1".to_string());
    }
    if let Some(extension) = request
        .extension
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let exts: Vec<&str> = extension.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if exts.len() == 1 {
            conditions.push("a.extension = ?".to_string());
            values.push(exts[0].to_owned().into());
        } else if !exts.is_empty() {
            let placeholders = vec!["?"; exts.len()].join(", ");
            conditions.push(format!("a.extension IN ({})", placeholders));
            for ext in exts {
                values.push(ext.to_owned().into());
            }
        }
    }
    if let Some(map_role) = request
        .map_role
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let roles: Vec<&str> = map_role.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if roles.len() == 1 {
            conditions.push("a.map_role = ?".to_string());
            values.push(roles[0].to_owned().into());
        } else if !roles.is_empty() {
            let placeholders = vec!["?"; roles.len()].join(", ");
            conditions.push(format!("a.map_role IN ({})", placeholders));
            for role in roles {
                values.push(role.to_owned().into());
            }
        }
    }
    if let Some(tag) = request.tag.as_deref().filter(|value| !value.is_empty()) {
        let tags: Vec<&str> = tag.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if tags.len() == 1 {
            conditions.push("EXISTS (SELECT 1 FROM asset_tags filter_at JOIN tags filter_tag ON filter_tag.id = filter_at.tag_id WHERE filter_at.asset_id = a.id AND filter_tag.name = ? COLLATE NOCASE)".to_string());
            values.push(tags[0].to_owned().into());
        } else if !tags.is_empty() {
            let placeholders = vec!["?"; tags.len()].join(", ");
            conditions.push(format!("EXISTS (SELECT 1 FROM asset_tags filter_at JOIN tags filter_tag ON filter_tag.id = filter_at.tag_id WHERE filter_at.asset_id = a.id AND filter_tag.name IN ({}) COLLATE NOCASE)", placeholders));
            for t in tags {
                values.push(t.to_owned().into());
            }
        }
    }
    if let Some(min_width) = request.min_width.filter(|value| *value > 0) {
        conditions.push("a.width >= ?".to_string());
        values.push(min_width.into());
    }
    if let Some(min_height) = request.min_height.filter(|value| *value > 0) {
        conditions.push("a.height >= ?".to_string());
        values.push(min_height.into());
    }
    if let Some(min_confidence) = request.min_confidence {
        conditions.push("a.classification_confidence <= ?".to_string());
        values.push(min_confidence.clamp(0, 100).into());
    }
    (conditions.join(" AND "), values)
}

fn asset_query_order(request: &AssetQuery) -> String {
    let descending = match request.sort_direction.as_deref() {
        Some("desc") => true,
        Some("asc") => false,
        _ => matches!(request.sort.as_deref(), Some("newest" | "largest")),
    };
    let direction = if descending { "DESC" } else { "ASC" };
    match request.sort.as_deref() {
        Some("newest") => format!(
            "a.modified_at {direction}, a.name COLLATE LOOTBOX_NATURAL {direction}, a.id {direction}"
        ),
        Some("largest") => format!(
            "a.size_bytes {direction}, a.name COLLATE LOOTBOX_NATURAL {direction}, a.id {direction}"
        ),
        Some("type") => format!(
            "a.asset_type COLLATE NOCASE {direction}, a.name COLLATE LOOTBOX_NATURAL {direction}, a.id {direction}"
        ),
        _ => format!("a.name COLLATE LOOTBOX_NATURAL {direction}, a.id {direction}"),
    }
}

#[tauri::command]
async fn query_assets(request: AssetQuery, state: State<'_, AppState>) -> Result<AssetPage> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        query_assets_from_connection(request, &connection)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
async fn query_asset_selections(
    request: AssetQuery,
    state: State<'_, AppState>,
) -> Result<Vec<AssetSelection>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        query_asset_selections_from_connection(&request, &connection)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

fn query_asset_selections_from_connection(
    request: &AssetQuery,
    connection: &Connection,
) -> Result<Vec<AssetSelection>> {
    let (where_clause, values) = asset_query_filter(request);
    let sql = format!(
        "SELECT a.id, a.absolute_path FROM assets a WHERE {where_clause} ORDER BY {}",
        asset_query_order(request)
    );
    let mut statement = connection.prepare(&sql)?;
    let selections = statement
        .query_map(params_from_iter(values), |row| {
            Ok(AssetSelection {
                id: row.get(0)?,
                absolute_path: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(selections)
}

fn query_assets_from_connection(request: AssetQuery, connection: &Connection) -> Result<AssetPage> {
    let mut sql = String::from(
        r#"
        SELECT
            a.id, a.pack_id, p.name, a.name, a.relative_path, a.absolute_path,
            a.extension, a.asset_type, a.size_bytes, a.modified_at, a.width, a.height,
            a.triangles, a.vertices,
            a.thumbnail_path,
            COALESCE((
                SELECT json_group_array(json_object(
                    'id', variant.id,
                    'extension', variant.extension,
                    'assetType', variant.asset_type,
                    'fileType', variant.file_type,
                    'usage', variant.usage,
                    'mapRole', variant.map_role,
                    'resolution', variant.resolution,
                    'triangles', variant.triangles,
                    'vertices', variant.vertices,
                    'absolutePath', variant.absolute_path,
                    'relativePath', variant.relative_path,
                    'sizeBytes', variant.size_bytes
                ))
                FROM assets variant
                WHERE variant.pack_id = a.pack_id
                  AND variant.group_key = a.group_key
            ), '[]'),
            COALESCE((
                SELECT json_group_array(json_object(
                    'id', resource.id,
                    'name', resource.name,
                    'extension', resource.extension,
                    'assetType', resource.asset_type,
                    'fileType', resource.file_type,
                    'usage', resource.usage,
                    'mapRole', resource.map_role,
                    'resolution', resource.resolution,
                    'triangles', resource.triangles,
                    'vertices', resource.vertices,
                    'absolutePath', resource.absolute_path,
                    'relativePath', resource.relative_path,
                    'sizeBytes', resource.size_bytes,
                    'thumbnailPath', resource.thumbnail_path
                ))
                FROM asset_dependencies dependency
                JOIN assets resource ON resource.id = dependency.dependency_asset_id
                WHERE dependency.owner_asset_id = a.id
            ), '[]'),
            COALESCE((
                SELECT GROUP_CONCAT(DISTINCT tag.name)
                FROM asset_tags tagged
                JOIN tags tag ON tag.id = tagged.tag_id
                WHERE tagged.asset_id = a.id
            ), ''),
            COALESCE((
                SELECT GROUP_CONCAT(DISTINCT membership.collection_id)
                FROM collection_assets membership
                WHERE membership.asset_id = a.id
            ), ''),
            a.file_type, a.usage, a.map_role, a.resolution,
            a.classification_confidence, a.classification_basis, a.missing,
            EXISTS(SELECT 1 FROM classification_overrides override WHERE override.asset_id = a.id),
            a.content_hash,
            CASE WHEN a.content_hash IS NULL THEN 0 ELSE
                (SELECT COUNT(*) FROM assets copy WHERE copy.content_hash = a.content_hash AND copy.missing = 0)
            END,
            COALESCE((
                SELECT json_group_array(json_object(
                    'id', copy.id,
                    'packName', copy_pack.name,
                    'relativePath', copy.relative_path,
                    'absolutePath', copy.absolute_path,
                    'sizeBytes', copy.size_bytes
                ))
                FROM assets copy
                JOIN packs copy_pack ON copy_pack.id = copy.pack_id
                WHERE copy.content_hash = a.content_hash
                  AND copy.content_hash IS NOT NULL
                  AND copy.missing = 0
                  AND copy.id != a.id
            ), '[]')
        FROM assets a
        JOIN packs p ON p.id = a.pack_id
        "#,
    );
    let (where_clause, mut values) = asset_query_filter(&request);
    let count_sql = format!("SELECT COUNT(*) FROM assets a WHERE {where_clause}");
    let total = connection.query_row(&count_sql, params_from_iter(values.iter()), |row| {
        row.get::<_, i64>(0)
    })?;

    sql.push_str(" WHERE ");
    sql.push_str(&where_clause);
    sql.push_str(" ORDER BY ");
    sql.push_str(&asset_query_order(&request));
    sql.push_str(" LIMIT ? OFFSET ?");
    let limit = request.limit.unwrap_or(160).clamp(1, 10_000);
    let offset = request.offset.unwrap_or(0).max(0);
    values.push(limit.into());
    values.push(offset.into());

    let mut statement = connection.prepare(&sql)?;
    let assets = statement
        .query_map(params_from_iter(values), |row| {
            let variants: String = row.get(15)?;
            let resources: String = row.get(16)?;
            let tags: String = row.get(17)?;
            let collection_ids: String = row.get(18)?;
            Ok(Asset {
                id: row.get(0)?,
                pack_id: row.get(1)?,
                pack_name: row.get(2)?,
                name: row.get(3)?,
                relative_path: row.get(4)?,
                absolute_path: row.get(5)?,
                extension: row.get(6)?,
                asset_type: row.get(7)?,
                file_type: row.get(19)?,
                usage: row.get(20)?,
                map_role: row.get(21)?,
                resolution: row.get(22)?,
                classification_confidence: row.get(23)?,
                classification_basis: row.get(24)?,
                missing: row.get(25)?,
                manual_classification: row.get(26)?,
                content_hash: row.get(27)?,
                duplicate_count: row.get(28)?,
                duplicate_locations: serde_json::from_str(&row.get::<_, String>(29)?)
                    .unwrap_or_default(),
                size_bytes: row.get(8)?,
                modified_at: row.get(9)?,
                width: row.get(10)?,
                height: row.get(11)?,
                triangles: row.get(12)?,
                vertices: row.get(13)?,
                thumbnail_path: row.get(14)?,
                variants: serde_json::from_str(&variants).unwrap_or_default(),
                resources: serde_json::from_str(&resources).unwrap_or_default(),
                tags: tags
                    .split(',')
                    .filter(|tag| !tag.is_empty())
                    .map(str::to_string)
                    .collect(),
                collection_ids: collection_ids
                    .split(',')
                    .filter_map(|id| id.parse::<i64>().ok())
                    .collect(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let has_more = offset + (assets.len() as i64) < total;
    Ok(AssetPage {
        items: assets,
        total,
        has_more,
    })
}

#[tauri::command]
fn add_tag(asset_id: i64, name: String, state: State<'_, AppState>) -> Result<()> {
    add_tags(vec![asset_id], name, state).map(|_| ())
}

#[tauri::command]
fn add_tags(asset_ids: Vec<i64>, name: String, state: State<'_, AppState>) -> Result<Vec<i64>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT OR IGNORE INTO tags(name) VALUES (?1)",
        params![name],
    )?;
    let tag_id: i64 = transaction.query_row(
        "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
        params![name],
        |row| row.get(0),
    )?;
    let mut changed = Vec::new();
    for asset_id in asset_ids {
        if transaction.execute(
            "INSERT OR IGNORE INTO asset_tags(asset_id, tag_id) VALUES (?1, ?2)",
            params![asset_id, tag_id],
        )? > 0
        {
            changed.push(asset_id);
        }
    }
    transaction.commit()?;
    rebuild_search_index(&connection)?;
    Ok(changed)
}

#[tauri::command]
fn remove_tag(asset_id: i64, name: String, state: State<'_, AppState>) -> Result<()> {
    remove_tags(vec![asset_id], name, state).map(|_| ())
}

#[tauri::command]
fn remove_tags(asset_ids: Vec<i64>, name: String, state: State<'_, AppState>) -> Result<Vec<i64>> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    let transaction = connection.transaction()?;
    let mut changed = Vec::new();
    for asset_id in asset_ids {
        if transaction.execute(
            r#"
            DELETE FROM asset_tags
            WHERE asset_id = ?1
              AND tag_id = (SELECT id FROM tags WHERE name = ?2 COLLATE NOCASE)
            "#,
            params![asset_id, name],
        )? > 0
        {
            changed.push(asset_id);
        }
    }
    transaction.commit()?;
    rebuild_search_index(&connection)?;
    Ok(changed)
}

#[tauri::command]
fn create_collection(name: String, state: State<'_, AppState>) -> Result<CollectionSummary> {
    let name = name.trim();
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = state.connect()?;
    if !name.is_empty() {
        connection.execute(
            "INSERT OR IGNORE INTO collections(name) VALUES (?1)",
            params![name],
        )?;
    }
    let collection = connection
        .query_row(
            "SELECT id, name FROM collections WHERE name = ?1 COLLATE NOCASE",
            params![name],
            |row| {
                Ok(CollectionSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    asset_count: 0,
                })
            },
        )
        .optional()?;
    collection.ok_or(LootboxError::Database(rusqlite::Error::QueryReturnedNoRows))
}

#[tauri::command]
fn set_collection_membership(
    asset_id: i64,
    collection_id: i64,
    included: bool,
    state: State<'_, AppState>,
) -> Result<()> {
    set_collection_memberships(vec![asset_id], collection_id, included, state).map(|_| ())
}

#[tauri::command]
fn set_collection_memberships(
    asset_ids: Vec<i64>,
    collection_id: i64,
    included: bool,
    state: State<'_, AppState>,
) -> Result<Vec<i64>> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    let transaction = connection.transaction()?;
    let mut changed = Vec::new();
    for asset_id in asset_ids {
        let affected = if included {
            transaction.execute(
                "INSERT OR IGNORE INTO collection_assets(collection_id, asset_id) VALUES (?1, ?2)",
                params![collection_id, asset_id],
            )?
        } else {
            transaction.execute(
                "DELETE FROM collection_assets WHERE collection_id = ?1 AND asset_id = ?2",
                params![collection_id, asset_id],
            )?
        };
        if affected > 0 {
            changed.push(asset_id);
        }
    }
    transaction.commit()?;
    Ok(changed)
}

#[tauri::command]
fn delete_collection(collection_id: i64, state: State<'_, AppState>) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.connect()?.execute(
        "DELETE FROM collections WHERE id = ?1",
        params![collection_id],
    )?;
    Ok(())
}

fn project_summary(connection: &Connection, project_id: i64) -> Result<ProjectSummary> {
    Ok(connection.query_row(
        r#"
        SELECT project.id, project.name, project.root_path, (
            SELECT COUNT(*)
            FROM assets primary_asset
            WHERE primary_asset.is_primary = 1
              AND primary_asset.missing = 0
              AND EXISTS (
                SELECT 1
                FROM project_exports exported
                JOIN assets exported_asset ON exported_asset.id = exported.asset_id
                WHERE exported.project_id = project.id
                  AND (
                    exported_asset.id = primary_asset.id OR
                    (primary_asset.group_key IS NOT NULL
                      AND exported_asset.pack_id = primary_asset.pack_id
                      AND exported_asset.group_key = primary_asset.group_key)
                  )
              )
        ), (SELECT MAX(exported_at) FROM project_exports WHERE project_id = project.id)
        FROM projects project
        WHERE project.id = ?1
        "#,
        params![project_id],
        |row| {
            let root_path: String = row.get(2)?;
            Ok(ProjectSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                available: Path::new(&root_path).join("project.godot").is_file(),
                root_path,
                asset_count: row.get(3)?,
                last_exported_at: row.get(4)?,
            })
        },
    )?)
}

fn godot_project_name(root: &Path) -> String {
    fs::read_to_string(root.join("project.godot"))
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("config/name=")
                    .map(|value| value.trim().trim_matches('"').to_string())
                    .filter(|value| !value.is_empty())
            })
        })
        .or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "Godot project".into())
}

#[tauri::command]
fn add_godot_project(path: String, state: State<'_, AppState>) -> Result<ProjectSummary> {
    let root = fs::canonicalize(path)?;
    if !root.is_dir() || !root.join("project.godot").is_file() {
        return Err(LootboxError::InvalidGodotProject(
            "select the folder containing project.godot".into(),
        ));
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = state.connect()?;
    connection.execute(
        r#"
        INSERT INTO projects(name, root_path) VALUES (?1, ?2)
        ON CONFLICT(root_path) DO UPDATE SET name = excluded.name
        "#,
        params![godot_project_name(&root), path_string(&root)],
    )?;
    let id = connection.query_row(
        "SELECT id FROM projects WHERE root_path = ?1",
        params![path_string(&root)],
        |row| row.get(0),
    )?;
    project_summary(&connection, id)
}

#[tauri::command]
fn relocate_godot_project(
    project_id: i64,
    path: String,
    state: State<'_, AppState>,
) -> Result<ProjectSummary> {
    let root = fs::canonicalize(path)?;
    if !root.is_dir() || !root.join("project.godot").is_file() {
        return Err(LootboxError::InvalidGodotProject(
            "select the folder containing project.godot".into(),
        ));
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    relocate_godot_project_from_connection(&mut connection, project_id, &root)
}

fn relocate_godot_project_from_connection(
    connection: &mut Connection,
    project_id: i64,
    root: &Path,
) -> Result<ProjectSummary> {
    let previous_root: String = connection.query_row(
        "SELECT root_path FROM projects WHERE id = ?1",
        params![project_id],
        |row| row.get(0),
    )?;
    let previous_root = PathBuf::from(previous_root);
    let tracked_paths = {
        let mut statement = connection
            .prepare("SELECT asset_id, exported_path FROM project_exports WHERE project_id = ?1")?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    let rebased_paths = tracked_paths
        .into_iter()
        .map(|(asset_id, tracked_path)| {
            let relative = Path::new(&tracked_path)
                .strip_prefix(&previous_root)
                .map_err(|_| {
                    LootboxError::ProjectExport(
                        "a tracked export path is outside the registered project".into(),
                    )
                })?;
            if relative.as_os_str().is_empty() {
                return Err(LootboxError::ProjectExport(
                    "a tracked export path points at the project root".into(),
                ));
            }
            Ok((asset_id, path_string(&root.join(relative))))
        })
        .collect::<Result<Vec<_>>>()?;

    let transaction = connection.transaction()?;
    for (asset_id, rebased_path) in rebased_paths {
        transaction.execute(
            "UPDATE project_exports SET exported_path = ?1 WHERE project_id = ?2 AND asset_id = ?3",
            params![rebased_path, project_id, asset_id],
        )?;
    }
    transaction.execute(
        "UPDATE projects SET name = ?1, root_path = ?2 WHERE id = ?3",
        params![godot_project_name(root), path_string(root), project_id],
    )?;
    let summary = project_summary(&transaction, project_id)?;
    transaction.commit()?;
    Ok(summary)
}

#[tauri::command]
fn remove_project(project_id: i64, state: State<'_, AppState>) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .connect()?
        .execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
    Ok(())
}

fn project_status_from_connection(
    connection: &Connection,
    project_id: i64,
) -> Result<ProjectStatus> {
    let project = project_summary(connection, project_id)?;
    let mut tracked_files = 0;
    let mut up_to_date_files = 0;
    let mut source_changed_files = 0;
    let mut source_missing_files = 0;
    let mut project_modified_files = 0;
    let mut project_missing_files = 0;

    let mut statement = connection.prepare(
        r#"
        SELECT exported.exported_path, exported.content_hash,
               asset.absolute_path, asset.missing
        FROM project_exports exported
        JOIN assets asset ON asset.id = exported.asset_id
        WHERE exported.project_id = ?1
        "#,
    )?;
    let rows = statement.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
        ))
    })?;
    for row in rows {
        let (exported_path, expected_hash, source_path, source_missing) = row?;
        tracked_files += 1;
        let expected_hash = expected_hash.as_deref();
        let source_path = Path::new(&source_path);
        let source_current = if source_missing || !source_path.is_file() {
            source_missing_files += 1;
            false
        } else if expected_hash.is_some() && hash_file(source_path).ok().as_deref() != expected_hash
        {
            source_changed_files += 1;
            false
        } else {
            true
        };

        let exported_path = Path::new(&exported_path);
        let project_current = if !exported_path.is_file() {
            project_missing_files += 1;
            false
        } else if expected_hash.is_some()
            && hash_file(exported_path).ok().as_deref() != expected_hash
        {
            project_modified_files += 1;
            false
        } else {
            true
        };
        if source_current && project_current {
            up_to_date_files += 1;
        }
    }
    drop(statement);

    let runs = {
        let mut statement = connection.prepare(
            r#"
            SELECT id, exported_at, selected_count, copied_count, unchanged_count, model_formats
            FROM project_export_runs
            WHERE project_id = ?1
            ORDER BY id DESC
            LIMIT 30
            "#,
        )?;
        let rows = statement
            .query_map(params![project_id], |row| {
                let formats: String = row.get(5)?;
                Ok(ProjectExportRun {
                    id: row.get(0)?,
                    exported_at: row.get(1)?,
                    selected_count: row.get(2)?,
                    copied_count: row.get(3)?,
                    unchanged_count: row.get(4)?,
                    model_formats: serde_json::from_str(&formats).unwrap_or_default(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    Ok(ProjectStatus {
        project_id,
        destination: "res://assets/lootbox".into(),
        tracked_files,
        up_to_date_files,
        source_changed_files,
        source_missing_files,
        project_modified_files,
        project_missing_files,
        last_exported_at: project.last_exported_at,
        runs,
    })
}

#[tauri::command]
async fn get_project_status(project_id: i64, state: State<'_, AppState>) -> Result<ProjectStatus> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        project_status_from_connection(&connection, project_id)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

fn safe_export_relative_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(LootboxError::ProjectExport(
            "an indexed path escapes its pack".into(),
        ));
    }
    Ok(path.to_path_buf())
}

fn safe_project_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    cleaned.trim_matches('-').to_string()
}

fn collision_export_path(path: &Path, asset_id: i64, attempt: usize) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".into());
    let extension = path.extension().map(|value| value.to_string_lossy());
    let suffix = if attempt == 0 {
        format!("lootbox-{asset_id}")
    } else {
        format!("lootbox-{asset_id}-{}", attempt + 1)
    };
    let name = match extension {
        Some(extension) => format!("{stem}-{suffix}.{extension}"),
        None => format!("{stem}-{suffix}"),
    };
    path.with_file_name(name)
}

fn export_destination_conflicts(
    destination: &Path,
    source_hash: &str,
    tracked: Option<&(String, Option<String>)>,
) -> bool {
    if !destination.is_file() {
        return false;
    }
    let destination_hash = hash_file(destination).ok();
    let destination_path = path_string(destination);
    match tracked {
        Some((tracked_path, tracked_hash)) if tracked_path == &destination_path => {
            tracked_hash.is_some() && destination_hash.as_deref() != tracked_hash.as_deref()
        }
        _ => destination_hash.as_deref() != Some(source_hash),
    }
}

fn safe_collision_destination(
    destination: &Path,
    asset_id: i64,
    source_hash: &str,
) -> Option<PathBuf> {
    (0..10_000)
        .map(|attempt| collision_export_path(destination, asset_id, attempt))
        .find(|candidate| {
            !candidate.exists() || hash_file(candidate).ok().as_deref() == Some(source_hash)
        })
}

struct GodotExportSelection {
    physical_ids: HashSet<i64>,
    grouped_ids: HashSet<i64>,
    dependency_ids: HashSet<i64>,
    selected: usize,
    model_formats: Vec<GodotModelFormat>,
    selected_model_formats: Vec<String>,
}

fn collect_godot_export_selection(
    connection: &Connection,
    asset_ids: &[i64],
    requested_model_formats: Option<&[String]>,
) -> Result<GodotExportSelection> {
    let requested_ids = asset_ids.iter().copied().collect::<HashSet<_>>();
    let mut available_formats = std::collections::BTreeMap::<String, usize>::new();
    let mut selected_assets = Vec::new();

    for asset_id in &requested_ids {
        let asset: Option<(i64, Option<String>, String, String)> = connection
            .query_row(
                "SELECT pack_id, group_key, asset_type, extension FROM assets WHERE id = ?1 AND missing = 0",
                params![asset_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((pack_id, group_key, asset_type, extension)) = asset else {
            continue;
        };
        if let Some(group_key) = &group_key {
            let mut statement = connection.prepare(
                "SELECT extension FROM assets WHERE pack_id = ?1 AND group_key = ?2 AND asset_type = 'model' AND missing = 0",
            )?;
            for format in statement
                .query_map(params![pack_id, group_key], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
            {
                *available_formats
                    .entry(format.to_ascii_lowercase())
                    .or_default() += 1;
            }
        } else if asset_type == "model" {
            *available_formats
                .entry(extension.to_ascii_lowercase())
                .or_default() += 1;
        }
        selected_assets.push((*asset_id, pack_id, group_key, asset_type, extension));
    }

    let available_set = available_formats.keys().cloned().collect::<HashSet<_>>();
    let requested_formats = requested_model_formats.map(|formats| {
        formats
            .iter()
            .map(|format| format.trim_start_matches('.').to_ascii_lowercase())
            .filter(|format| available_set.contains(format))
            .collect::<HashSet<_>>()
    });
    let selected_format_set = match requested_formats {
        Some(formats) if !formats.is_empty() => formats,
        _ => available_set.clone(),
    };

    let mut physical_ids = HashSet::new();
    let mut grouped_ids = HashSet::new();
    let mut dependency_ids = HashSet::new();
    let mut selected = 0;
    for (asset_id, pack_id, group_key, asset_type, extension) in selected_assets {
        let mut owner_ids = vec![asset_id];
        let mut included_requested_asset = false;
        if let Some(group_key) = group_key {
            let mut statement = connection.prepare(
                "SELECT id, asset_type, extension FROM assets WHERE pack_id = ?1 AND group_key = ?2 AND missing = 0",
            )?;
            let group_assets = statement
                .query_map(params![pack_id, &group_key], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let model_group = group_assets
                .iter()
                .any(|(_, grouped_type, _)| grouped_type == "model");
            owner_ids = group_assets.iter().map(|(id, _, _)| *id).collect();
            for (grouped_id, grouped_type, grouped_extension) in group_assets {
                let include = if model_group {
                    (grouped_type == "model"
                        && selected_format_set.contains(&grouped_extension.to_ascii_lowercase()))
                        || (grouped_extension.eq_ignore_ascii_case("mtl")
                            && selected_format_set.contains("obj"))
                } else {
                    true
                };
                if include {
                    physical_ids.insert(grouped_id);
                    if grouped_id == asset_id || (asset_type == "model" && grouped_type == "model")
                    {
                        included_requested_asset = true;
                    }
                    if !requested_ids.contains(&grouped_id) {
                        grouped_ids.insert(grouped_id);
                    }
                }
            }
        } else {
            let include = asset_type != "model"
                || selected_format_set.contains(&extension.to_ascii_lowercase());
            if include {
                physical_ids.insert(asset_id);
                included_requested_asset = true;
            }
        }
        if included_requested_asset {
            selected += 1;
        }

        for owner_id in owner_ids {
            let mut statement = connection.prepare(
                "SELECT dependency_asset_id FROM asset_dependencies WHERE owner_asset_id = ?1",
            )?;
            for dependency_id in statement
                .query_map(params![owner_id], |row| row.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
            {
                physical_ids.insert(dependency_id);
                dependency_ids.insert(dependency_id);
            }
        }
    }
    grouped_ids.retain(|id| !requested_ids.contains(id));
    dependency_ids.retain(|id| !requested_ids.contains(id) && !grouped_ids.contains(id));

    let mut model_formats = available_formats
        .into_iter()
        .map(|(extension, count)| GodotModelFormat { extension, count })
        .collect::<Vec<_>>();
    model_formats.sort_by_key(|format| match format.extension.as_str() {
        "glb" => 0,
        "gltf" => 1,
        "fbx" => 2,
        "obj" => 3,
        "dae" => 4,
        "blend" => 5,
        "usd" => 6,
        "usdc" => 7,
        "usda" => 8,
        "usdz" => 9,
        "3ds" => 10,
        "stl" => 11,
        "ply" => 12,
        _ => 13,
    });
    Ok(GodotExportSelection {
        physical_ids,
        grouped_ids,
        dependency_ids,
        selected,
        model_formats,
        selected_model_formats: selected_format_set.into_iter().collect(),
    })
}

fn preview_assets_to_godot_from_connection(
    connection: &Connection,
    project_id: i64,
    asset_ids: &[i64],
    model_formats: Option<&[String]>,
) -> Result<GodotExportPreview> {
    let project = project_summary(connection, project_id)?;
    let root = PathBuf::from(&project.root_path);
    if !root.join("project.godot").is_file() {
        return Err(LootboxError::InvalidGodotProject(
            "project.godot is missing".into(),
        ));
    }
    let mut selection = collect_godot_export_selection(connection, asset_ids, model_formats)?;
    selection.selected_model_formats.sort();
    let physical_ids = &selection.physical_ids;
    let export_root = root.join("assets").join("lootbox");
    let mut files = Vec::new();
    let mut conflicts = 0;
    let mut conflict_files = Vec::new();
    let mut ids = physical_ids.iter().copied().collect::<Vec<_>>();
    ids.sort_unstable();
    for asset_id in ids {
        let (pack_id, pack_name, relative_path, source): (i64, String, String, String) = connection
            .query_row(
                r#"
                SELECT asset.pack_id, pack.name, asset.relative_path, asset.absolute_path
                FROM assets asset JOIN packs pack ON pack.id = asset.pack_id
                WHERE asset.id = ?1 AND asset.missing = 0
                "#,
                params![asset_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let source = PathBuf::from(source);
        if !source.is_file() {
            continue;
        }
        let pack_component = format!(
            "{}-{pack_id}",
            safe_project_component(&pack_name).to_ascii_lowercase()
        );
        let relative = safe_export_relative_path(&relative_path)?;
        let destination = export_root.join(&pack_component).join(&relative);
        let tracked = tracked_project_export(connection, project_id, asset_id)?;
        let source_hash = hash_file(&source)?;
        let mut planned_destination = destination.clone();
        if export_destination_conflicts(&destination, &source_hash, tracked.as_ref()) {
            conflicts += 1;
            let renamed = safe_collision_destination(&destination, asset_id, &source_hash);
            if let Some(renamed) = renamed {
                let original_name = destination
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default();
                let renamed_name = renamed
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default();
                conflict_files.push(format!("{original_name} → {renamed_name}"));
                planned_destination = renamed;
            }
        }
        files.push(
            planned_destination
                .strip_prefix(&export_root)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| format!("{pack_component}/{}", relative.to_string_lossy())),
        );
    }
    files.sort();
    let selected = selection.selected;
    Ok(GodotExportPreview {
        selected,
        related: physical_ids.len().saturating_sub(selected),
        grouped: selection.grouped_ids.len(),
        dependencies: selection.dependency_ids.len(),
        total_files: files.len(),
        conflicts,
        conflict_files,
        destination: "res://assets/lootbox".into(),
        manifest: "res://assets/lootbox/lootbox-manifest.json".into(),
        files,
        model_formats: selection.model_formats,
        selected_model_formats: selection.selected_model_formats,
    })
}

fn ensure_godot_export_root(root: &Path) -> Result<PathBuf> {
    let root_metadata = fs::symlink_metadata(root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(LootboxError::ProjectExport(
            "the project folder is not a regular directory".into(),
        ));
    }
    let assets_root = root.join("assets");
    for directory in [&assets_root, &assets_root.join("lootbox")] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(LootboxError::ProjectExport(
                    "the project export folder contains an unexpected symbolic link".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(directory)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(assets_root.join("lootbox"))
}

fn project_path_contains_symlink(root: &Path, path: &Path) -> Result<bool> {
    let relative = path.strip_prefix(root).map_err(|_| {
        LootboxError::ProjectExport("a tracked export path is outside the project".into())
    })?;
    let relative = safe_export_relative_path(&relative.to_string_lossy())?;
    let mut current = root.to_path_buf();
    if fs::symlink_metadata(&current)?.file_type().is_symlink() {
        return Ok(true);
    }
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}

fn write_godot_manifest(
    connection: &Connection,
    project_id: i64,
    project_name: &str,
    export_root: &Path,
) -> Result<()> {
    let manifest_entries = {
        let mut statement = connection.prepare(
            r#"
            SELECT asset.id, pack.name, asset.relative_path, exported.exported_path,
                   exported.content_hash, exported.exported_at
            FROM project_exports exported
            JOIN assets asset ON asset.id = exported.asset_id
            JOIN packs pack ON pack.id = asset.pack_id
            WHERE exported.project_id = ?1
            ORDER BY pack.name COLLATE NOCASE, asset.relative_path COLLATE NOCASE
            "#,
        )?;
        let entries = statement
            .query_map(params![project_id], |row| {
                Ok(serde_json::json!({
                    "assetId": row.get::<_, i64>(0)?,
                    "pack": row.get::<_, String>(1)?,
                    "sourcePath": row.get::<_, String>(2)?,
                    "exportedPath": row.get::<_, String>(3)?,
                    "sha256": row.get::<_, Option<String>>(4)?,
                    "exportedAt": row.get::<_, String>(5)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        entries
    };
    let manifest = serde_json::json!({
        "format": 1,
        "generator": "Lootbox",
        "project": project_name,
        "assets": manifest_entries,
    });
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| LootboxError::ProjectExport(error.to_string()))?;
    let temporary = export_root.join(format!(
        ".lootbox-manifest-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, export_root.join("lootbox-manifest.json"))?;
    Ok(())
}

fn export_assets_to_godot_from_connection(
    connection: &mut Connection,
    project_id: i64,
    asset_ids: &[i64],
    model_formats: Option<&[String]>,
) -> Result<GodotExportResult> {
    let project = project_summary(connection, project_id)?;
    let root = PathBuf::from(&project.root_path);
    if !root.join("project.godot").is_file() {
        return Err(LootboxError::InvalidGodotProject(
            "project.godot is missing".into(),
        ));
    }
    let physical_ids =
        collect_godot_export_selection(connection, asset_ids, model_formats)?.physical_ids;

    let export_root = ensure_godot_export_root(&root)?;
    let mut copied = 0;
    let mut unchanged = 0;
    for asset_id in physical_ids {
        let (pack_id, pack_name, relative_path, source): (i64, String, String, String) = connection
            .query_row(
                r#"
            SELECT asset.pack_id, pack.name, asset.relative_path, asset.absolute_path
            FROM assets asset JOIN packs pack ON pack.id = asset.pack_id
            WHERE asset.id = ?1 AND asset.missing = 0
            "#,
                params![asset_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let source = PathBuf::from(source);
        if !source.is_file() {
            continue;
        }
        let hash = hash_file(&source)?;
        connection.execute(
            "UPDATE assets SET content_hash = ?1 WHERE id = ?2",
            params![&hash, asset_id],
        )?;
        let pack_component = format!(
            "{}-{pack_id}",
            safe_project_component(&pack_name).to_ascii_lowercase()
        );
        let relative = safe_export_relative_path(&relative_path)?;
        let mut destination = export_root.join(pack_component).join(relative);
        let tracked = tracked_project_export(connection, project_id, asset_id)?;
        if export_destination_conflicts(&destination, &hash, tracked.as_ref()) {
            let original_destination = destination;
            destination = safe_collision_destination(&original_destination, asset_id, &hash)
                .ok_or_else(|| {
                    LootboxError::ProjectExport(format!(
                        "could not find a safe destination for {}",
                        relative_path
                    ))
                })?;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let destination_matches =
            destination.is_file() && hash_file(&destination).ok().as_ref() == Some(&hash);
        if destination_matches {
            unchanged += 1;
        } else {
            fs::copy(&source, &destination)?;
            copied += 1;
        }
        connection.execute(
            r#"
            INSERT INTO project_exports(project_id, asset_id, exported_path, content_hash)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(project_id, asset_id) DO UPDATE SET
                exported_path = excluded.exported_path,
                content_hash = excluded.content_hash,
                exported_at = CURRENT_TIMESTAMP
            "#,
            params![project_id, asset_id, path_string(&destination), &hash],
        )?;
    }

    write_godot_manifest(connection, project_id, &project.name, &export_root)?;
    let formats = serde_json::to_string(model_formats.unwrap_or(&[]))
        .map_err(|error| LootboxError::ProjectExport(error.to_string()))?;
    connection.execute(
        r#"
        INSERT INTO project_export_runs(
            project_id, selected_count, copied_count, unchanged_count, model_formats
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            project_id,
            asset_ids.len() as i64,
            copied as i64,
            unchanged as i64,
            formats
        ],
    )?;
    connection.execute(
        r#"
        DELETE FROM project_export_runs
        WHERE project_id = ?1 AND id NOT IN (
            SELECT id FROM project_export_runs
            WHERE project_id = ?1 ORDER BY id DESC LIMIT 100
        )
        "#,
        params![project_id],
    )?;
    Ok(GodotExportResult {
        copied,
        unchanged,
        destination: "res://assets/lootbox".into(),
    })
}

struct GodotProjectRemovalFile {
    path: PathBuf,
    expected_hash: String,
}

struct GodotProjectRemovalPlan {
    preview: GodotProjectRemovalPreview,
    delete_files: Vec<GodotProjectRemovalFile>,
    untrack_ids: Vec<i64>,
}

fn tracked_project_export(
    connection: &Connection,
    project_id: i64,
    asset_id: i64,
) -> Result<Option<(String, Option<String>)>> {
    Ok(connection
        .query_row(
            "SELECT exported_path, content_hash FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
            params![project_id, asset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn safe_tracked_project_path(export_root: &Path, tracked_path: &str) -> Result<(PathBuf, String)> {
    let path = PathBuf::from(tracked_path);
    let relative = path.strip_prefix(export_root).map_err(|_| {
        LootboxError::ProjectExport("a tracked export path is outside assets/lootbox".into())
    })?;
    let relative = safe_export_relative_path(&relative.to_string_lossy())?;
    if relative.as_os_str().is_empty() {
        return Err(LootboxError::ProjectExport(
            "a tracked export path points at the export folder".into(),
        ));
    }
    Ok((
        export_root.join(&relative),
        relative.to_string_lossy().to_string(),
    ))
}

fn project_tracks_dependency_owner(
    connection: &Connection,
    project_id: i64,
    owner_id: i64,
    pack_id: i64,
    group_key: Option<&str>,
) -> Result<bool> {
    if let Some(group_key) = group_key {
        Ok(connection.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM project_exports exported
                JOIN assets exported_asset ON exported_asset.id = exported.asset_id
                WHERE exported.project_id = ?1
                  AND exported_asset.pack_id = ?2
                  AND exported_asset.group_key = ?3
            )
            "#,
            params![project_id, pack_id, group_key],
            |row| row.get(0),
        )?)
    } else {
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM project_exports WHERE project_id = ?1 AND asset_id = ?2)",
            params![project_id, owner_id],
            |row| row.get(0),
        )?)
    }
}

fn plan_assets_from_godot_project_removal(
    connection: &Connection,
    project_id: i64,
    asset_ids: &[i64],
) -> Result<GodotProjectRemovalPlan> {
    let project = project_summary(connection, project_id)?;
    let root = PathBuf::from(&project.root_path);
    if !root.join("project.godot").is_file() {
        return Err(LootboxError::InvalidGodotProject(
            "project.godot is missing".into(),
        ));
    }
    let export_root = root.join("assets").join("lootbox");
    let requested_ids = asset_ids.iter().copied().collect::<HashSet<_>>();
    let mut selected_groups = HashSet::<(i64, String)>::new();
    let mut selected_singles = HashSet::<i64>::new();
    let mut candidate_ids = HashSet::<i64>::new();
    let mut selected = 0;

    for asset_id in requested_ids {
        let asset: Option<(i64, Option<String>)> = connection
            .query_row(
                "SELECT pack_id, group_key FROM assets WHERE id = ?1",
                params![asset_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((pack_id, group_key)) = asset else {
            continue;
        };
        let owner_ids = if let Some(group_key) = &group_key {
            selected_groups.insert((pack_id, group_key.clone()));
            let mut statement = connection
                .prepare("SELECT id FROM assets WHERE pack_id = ?1 AND group_key = ?2")?;
            let owner_ids = statement
                .query_map(params![pack_id, group_key], |row| row.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            owner_ids
        } else {
            selected_singles.insert(asset_id);
            vec![asset_id]
        };
        let before = candidate_ids.len();
        for owner_id in &owner_ids {
            if tracked_project_export(connection, project_id, *owner_id)?.is_some() {
                candidate_ids.insert(*owner_id);
            }
        }
        if candidate_ids.len() > before {
            selected += 1;
        }
        for owner_id in owner_ids {
            let mut statement = connection.prepare(
                "SELECT dependency_asset_id FROM asset_dependencies WHERE owner_asset_id = ?1",
            )?;
            for dependency_id in statement
                .query_map(params![owner_id], |row| row.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
            {
                if tracked_project_export(connection, project_id, dependency_id)?.is_some() {
                    candidate_ids.insert(dependency_id);
                }
            }
        }
    }

    let mut remove_files = Vec::new();
    let mut modified_files = Vec::new();
    let mut missing_files = Vec::new();
    let mut shared_files = Vec::new();
    let mut delete_files = Vec::new();
    let mut untrack_ids = Vec::new();
    let mut sorted_candidates = candidate_ids.into_iter().collect::<Vec<_>>();
    sorted_candidates.sort_unstable();
    for candidate_id in sorted_candidates {
        let Some((tracked_path, stored_hash)) =
            tracked_project_export(connection, project_id, candidate_id)?
        else {
            continue;
        };
        let (path, display_path) = safe_tracked_project_path(&export_root, &tracked_path)?;
        let mut shared = false;
        let mut statement = connection.prepare(
            r#"
            SELECT owner.id, owner.pack_id, owner.group_key
            FROM asset_dependencies dependency
            JOIN assets owner ON owner.id = dependency.owner_asset_id
            WHERE dependency.dependency_asset_id = ?1
            "#,
        )?;
        let owners = statement
            .query_map(params![candidate_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (owner_id, pack_id, group_key) in owners {
            let selected_owner = group_key
                .as_ref()
                .is_some_and(|group_key| selected_groups.contains(&(pack_id, group_key.clone())))
                || (group_key.is_none() && selected_singles.contains(&owner_id));
            if !selected_owner
                && project_tracks_dependency_owner(
                    connection,
                    project_id,
                    owner_id,
                    pack_id,
                    group_key.as_deref(),
                )?
            {
                shared = true;
                break;
            }
        }
        if shared {
            shared_files.push(display_path);
            continue;
        }

        if project_path_contains_symlink(&root, &path)? {
            modified_files.push(display_path);
            untrack_ids.push(candidate_id);
            continue;
        }

        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                modified_files.push(display_path);
                untrack_ids.push(candidate_id);
            }
            Ok(_) => {
                let current_hash = hash_file(&path)?;
                if stored_hash.as_deref() == Some(current_hash.as_str()) {
                    remove_files.push(display_path);
                    delete_files.push(GodotProjectRemovalFile {
                        path,
                        expected_hash: current_hash,
                    });
                    untrack_ids.push(candidate_id);
                } else {
                    modified_files.push(display_path);
                    untrack_ids.push(candidate_id);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing_files.push(display_path);
                untrack_ids.push(candidate_id);
            }
            Err(error) => return Err(error.into()),
        }
    }
    remove_files.sort();
    modified_files.sort();
    missing_files.sort();
    shared_files.sort();
    Ok(GodotProjectRemovalPlan {
        preview: GodotProjectRemovalPreview {
            selected,
            destination: "res://assets/lootbox".into(),
            remove_files,
            modified_files,
            missing_files,
            shared_files,
        },
        delete_files,
        untrack_ids,
    })
}

fn remove_assets_from_godot_project_from_connection(
    connection: &mut Connection,
    project_id: i64,
    asset_ids: &[i64],
) -> Result<GodotProjectRemovalResult> {
    let project = project_summary(connection, project_id)?;
    let root = PathBuf::from(&project.root_path);
    let plan = plan_assets_from_godot_project_removal(connection, project_id, asset_ids)?;
    for file in &plan.delete_files {
        if project_path_contains_symlink(&root, &file.path)? {
            return Err(LootboxError::ProjectExport(
                "an exported path changed while removal was being prepared; review the removal again"
                    .into(),
            ));
        }
        let metadata = fs::symlink_metadata(&file.path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || hash_file(&file.path)? != file.expected_hash
        {
            return Err(LootboxError::ProjectExport(
                "an exported file changed while removal was being prepared; review the removal again"
                    .into(),
            ));
        }
        fs::remove_file(&file.path)?;
    }
    let export_root = ensure_godot_export_root(&root)?;
    let transaction = connection.transaction()?;
    for asset_id in &plan.untrack_ids {
        transaction.execute(
            "DELETE FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
            params![project_id, asset_id],
        )?;
    }
    write_godot_manifest(&transaction, project_id, &project.name, &export_root)?;
    transaction.commit()?;
    Ok(GodotProjectRemovalResult {
        deleted: plan.preview.remove_files.len(),
        kept_modified: plan.preview.modified_files.len(),
        cleaned_missing: plan.preview.missing_files.len(),
        kept_shared: plan.preview.shared_files.len(),
        destination: plan.preview.destination,
    })
}

#[tauri::command]
async fn preview_assets_to_godot(
    project_id: i64,
    asset_ids: Vec<i64>,
    model_formats: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<GodotExportPreview> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        preview_assets_to_godot_from_connection(
            &connection,
            project_id,
            &asset_ids,
            model_formats.as_deref(),
        )
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
async fn export_assets_to_godot(
    project_id: i64,
    asset_ids: Vec<i64>,
    model_formats: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<GodotExportResult> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state
            .write_queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = state.connect()?;
        export_assets_to_godot_from_connection(
            &mut connection,
            project_id,
            &asset_ids,
            model_formats.as_deref(),
        )
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
async fn preview_remove_assets_from_godot_project(
    project_id: i64,
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<GodotProjectRemovalPreview> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        Ok(plan_assets_from_godot_project_removal(&connection, project_id, &asset_ids)?.preview)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
async fn remove_assets_from_godot_project(
    project_id: i64,
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<GodotProjectRemovalResult> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state
            .write_queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = state.connect()?;
        remove_assets_from_godot_project_from_connection(&mut connection, project_id, &asset_ids)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
async fn hash_library(state: State<'_, AppState>) -> Result<usize> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || hash_unhashed_assets(&state))
        .await
        .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
fn remove_pack(pack_id: i64, state: State<'_, AppState>) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = state.connect()?;
    let thumbnail_paths = {
        let mut statement = connection.prepare(
            "SELECT thumbnail_path FROM assets WHERE pack_id = ?1 AND thumbnail_path IS NOT NULL",
        )?;
        let rows = statement
            .query_map(params![pack_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    connection.execute("DELETE FROM packs WHERE id = ?1", params![pack_id])?;
    rebuild_search_index(&connection)?;
    for path in thumbnail_paths {
        if Path::new(&path).is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[tauri::command]
fn rename_pack(pack_id: i64, name: String, state: State<'_, AppState>) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LootboxError::InvalidPackName);
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = state.connect()?;
    connection.execute(
        "UPDATE packs SET name = ?1 WHERE id = ?2",
        params![name, pack_id],
    )?;
    rebuild_search_index(&connection)
}

#[tauri::command]
fn set_assets_excluded(
    asset_ids: Vec<i64>,
    excluded: bool,
    state: State<'_, AppState>,
) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    set_assets_excluded_from_connection(&asset_ids, excluded, &mut connection)
}

fn set_assets_excluded_from_connection(
    asset_ids: &[i64],
    excluded: bool,
    connection: &mut Connection,
) -> Result<()> {
    let transaction = connection.transaction()?;
    for asset_id in asset_ids {
        let asset: Option<(i64, Option<String>)> = transaction
            .query_row(
                "SELECT pack_id, group_key FROM assets WHERE id = ?1",
                params![asset_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((pack_id, Some(variant_group))) = asset {
            transaction.execute(
                "UPDATE assets SET excluded = ?1 WHERE pack_id = ?2 AND group_key = ?3",
                params![excluded, pack_id, variant_group],
            )?;
        } else if asset.is_some() {
            transaction.execute(
                "UPDATE assets SET excluded = ?1 WHERE id = ?2",
                params![excluded, asset_id],
            )?;
        }
    }
    transaction.commit()?;
    rebuild_search_index(&connection)
}

#[tauri::command]
fn set_classification_override(
    asset_ids: Vec<i64>,
    asset_type: Option<String>,
    map_role: Option<String>,
    group_action: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ClassificationOverrideSnapshot>> {
    const VALID_TYPES: &[&str] = &[
        "image", "texture", "audio", "model", "video", "font", "shader", "material", "archive",
        "other",
    ];
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    if asset_type
        .as_deref()
        .is_some_and(|value| !VALID_TYPES.contains(&value))
    {
        return Err(LootboxError::InvalidPackLocation(
            "unsupported asset type".into(),
        ));
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    let transaction = connection.transaction()?;
    let snapshots = asset_ids
        .iter()
        .map(|asset_id| {
            let previous = transaction
                .query_row(
                    "SELECT asset_type, map_role, group_key FROM classification_overrides WHERE asset_id = ?1",
                    params![asset_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let (asset_type, map_role, group_key, existed) = match previous {
                Some((asset_type, map_role, group_key)) => (asset_type, map_role, group_key, true),
                None => (None, None, None, false),
            };
            Ok(ClassificationOverrideSnapshot {
                asset_id: *asset_id,
                asset_type,
                map_role,
                group_key,
                existed,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let pack_ids = asset_ids
        .iter()
        .map(|asset_id| {
            transaction.query_row(
                "SELECT pack_id FROM assets WHERE id = ?1",
                params![asset_id],
                |row| row.get::<_, i64>(0),
            )
        })
        .collect::<std::result::Result<HashSet<_>, _>>()?;
    if group_action.as_deref() == Some("merge") && pack_ids.len() != 1 {
        return Err(LootboxError::InvalidPackLocation(
            "assets from different packs cannot be grouped".into(),
        ));
    }
    let merged_group = group_action
        .as_deref()
        .filter(|action| *action == "merge")
        .map(|_| {
            format!(
                "manual:{}:{}",
                pack_ids.iter().next().copied().unwrap_or_default(),
                unix_timestamp()
            )
        });
    for asset_id in &asset_ids {
        let group_key = match group_action.as_deref() {
            Some("merge") => merged_group.clone(),
            Some("split") => Some(format!("manual:split:{asset_id}:{}", unix_timestamp())),
            _ => None,
        };
        transaction.execute(
            r#"
            INSERT INTO classification_overrides(asset_id, asset_type, map_role, group_key)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(asset_id) DO UPDATE SET
                asset_type = COALESCE(excluded.asset_type, classification_overrides.asset_type),
                map_role = COALESCE(excluded.map_role, classification_overrides.map_role),
                group_key = COALESCE(excluded.group_key, classification_overrides.group_key),
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![asset_id, asset_type, map_role, group_key],
        )?;
    }
    for pack_id in pack_ids {
        recompute_texture_groups(&transaction, Some(pack_id))?;
        apply_classification_overrides(&transaction, Some(pack_id))?;
        recompute_primary_assets(&transaction, Some(pack_id))?;
        recompute_asset_dependencies(&transaction, Some(pack_id))?;
    }
    transaction.commit()?;
    rebuild_search_index(&connection)?;
    Ok(snapshots)
}

#[tauri::command]
fn reset_classification_override(
    asset_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ClassificationOverrideSnapshot>> {
    if asset_ids.is_empty() {
        return Ok(Vec::new());
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    let transaction = connection.transaction()?;
    let snapshots = asset_ids
        .iter()
        .map(|asset_id| {
            let previous = transaction
                .query_row(
                    "SELECT asset_type, map_role, group_key FROM classification_overrides WHERE asset_id = ?1",
                    params![asset_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let (asset_type, map_role, group_key, existed) = match previous {
                Some((asset_type, map_role, group_key)) => (asset_type, map_role, group_key, true),
                None => (None, None, None, false),
            };
            Ok(ClassificationOverrideSnapshot {
                asset_id: *asset_id,
                asset_type,
                map_role,
                group_key,
                existed,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let pack_ids = asset_ids
        .iter()
        .filter_map(|asset_id| {
            transaction
                .query_row(
                    "SELECT pack_id FROM assets WHERE id = ?1",
                    params![asset_id],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
        })
        .collect::<HashSet<_>>();
    for asset_id in asset_ids {
        transaction.execute(
            "DELETE FROM classification_overrides WHERE asset_id = ?1",
            params![asset_id],
        )?;
    }
    for pack_id in pack_ids {
        recompute_texture_groups(&transaction, Some(pack_id))?;
        apply_classification_overrides(&transaction, Some(pack_id))?;
        recompute_primary_assets(&transaction, Some(pack_id))?;
        recompute_asset_dependencies(&transaction, Some(pack_id))?;
    }
    transaction.commit()?;
    rebuild_search_index(&connection)?;
    Ok(snapshots)
}

#[tauri::command]
fn restore_classification_overrides(
    snapshots: Vec<ClassificationOverrideSnapshot>,
    state: State<'_, AppState>,
) -> Result<()> {
    if snapshots.is_empty() {
        return Ok(());
    }
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut connection = state.connect()?;
    let transaction = connection.transaction()?;
    let pack_ids = snapshots
        .iter()
        .filter_map(|snapshot| {
            transaction
                .query_row(
                    "SELECT pack_id FROM assets WHERE id = ?1",
                    params![snapshot.asset_id],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
        })
        .collect::<HashSet<_>>();
    for snapshot in snapshots {
        if snapshot.existed {
            transaction.execute(
                r#"
                INSERT INTO classification_overrides(asset_id, asset_type, map_role, group_key)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(asset_id) DO UPDATE SET
                    asset_type = excluded.asset_type,
                    map_role = excluded.map_role,
                    group_key = excluded.group_key,
                    updated_at = CURRENT_TIMESTAMP
                "#,
                params![
                    snapshot.asset_id,
                    snapshot.asset_type,
                    snapshot.map_role,
                    snapshot.group_key
                ],
            )?;
        } else {
            transaction.execute(
                "DELETE FROM classification_overrides WHERE asset_id = ?1",
                params![snapshot.asset_id],
            )?;
        }
    }
    for pack_id in pack_ids {
        recompute_texture_groups(&transaction, Some(pack_id))?;
        apply_classification_overrides(&transaction, Some(pack_id))?;
        recompute_primary_assets(&transaction, Some(pack_id))?;
        recompute_asset_dependencies(&transaction, Some(pack_id))?;
    }
    transaction.commit()?;
    rebuild_search_index(&connection)
}

#[tauri::command]
fn purge_missing_assets(pack_id: i64, state: State<'_, AppState>) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let connection = state.connect()?;
    let paths = {
        let mut statement = connection.prepare("SELECT thumbnail_path FROM assets WHERE pack_id = ?1 AND missing = 1 AND thumbnail_path IS NOT NULL")?;
        let rows = statement
            .query_map(params![pack_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    connection.execute(
        "DELETE FROM assets WHERE pack_id = ?1 AND missing = 1",
        params![pack_id],
    )?;
    rebuild_search_index(&connection)?;
    for path in paths {
        if Path::new(&path).is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[tauri::command]
fn get_filter_options(state: State<'_, AppState>) -> Result<FilterOptions> {
    let connection = state.connect()?;
    fn string_column(connection: &Connection, sql: &str) -> Result<Vec<String>> {
        let mut statement = connection.prepare(sql)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    Ok(FilterOptions {
        extensions: string_column(&connection, "SELECT DISTINCT extension FROM assets WHERE extension != '' AND missing = 0 ORDER BY extension COLLATE NOCASE")?,
        map_roles: string_column(&connection, "SELECT DISTINCT map_role FROM assets WHERE map_role IS NOT NULL AND missing = 0 ORDER BY map_role COLLATE NOCASE")?,
        tags: string_column(&connection, "SELECT name FROM tags ORDER BY name COLLATE NOCASE")?,
    })
}

fn validate_pack_location(
    connection: &Connection,
    pack_id: i64,
    root: &Path,
) -> Result<Vec<(String, i64)>> {
    let indexed_files = {
        let mut statement = connection
            .prepare("SELECT relative_path, size_bytes FROM assets WHERE pack_id = ?1")?;
        let rows = statement
            .query_map(params![pack_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    if indexed_files.is_empty() {
        return Err(LootboxError::InvalidPackLocation(
            "there are no indexed files to validate against".into(),
        ));
    }
    let exact_matches = indexed_files
        .iter()
        .filter(|(relative_path, size_bytes)| {
            fs::metadata(root.join(relative_path))
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() == *size_bytes as u64)
        })
        .count();
    let required_matches = if indexed_files.len() <= 4 {
        indexed_files.len()
    } else {
        (indexed_files.len() * 3).div_ceil(5)
    };
    if exact_matches < required_matches {
        return Err(LootboxError::InvalidPackLocation(format!(
            "only {exact_matches} of {} indexed files match",
            indexed_files.len()
        )));
    }
    Ok(indexed_files)
}

#[tauri::command]
async fn relocate_pack(
    pack_id: i64,
    path: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PackSummary> {
    let state = state.inner().clone();
    let (pack, root) =
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = state
                .write_queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let root = fs::canonicalize(path)?;
            if !root.is_dir() {
                return Err(LootboxError::InvalidDirectory);
            }
            let mut connection = state.connect()?;
            let indexed_files = validate_pack_location(&connection, pack_id, &root)?;

            let transaction = connection.transaction()?;
            transaction.execute(
                "UPDATE packs SET root_path = ?1 WHERE id = ?2",
                params![path_string(&root), pack_id],
            )?;
            for (relative_path, _) in &indexed_files {
                transaction.execute(
                "UPDATE assets SET absolute_path = ?1 WHERE pack_id = ?2 AND relative_path = ?3",
                params![path_string(&root.join(relative_path)), pack_id, relative_path],
            )?;
            }
            transaction.commit()?;
            rebuild_search_index(&connection)?;
            Ok((get_pack(&connection, pack_id)?, root))
        })
        .await
        .map_err(|error| LootboxError::InvalidPackLocation(error.to_string()))??;
    app.asset_protocol_scope()
        .allow_directory(&root, true)
        .map_err(|error| LootboxError::InvalidPackLocation(error.to_string()))?;
    Ok(pack)
}

fn run_open_command(path: &str, reveal: bool) -> Result<()> {
    let path = PathBuf::from(path);
    let mut command;

    #[cfg(target_os = "macos")]
    {
        command = Command::new("open");
        if reveal {
            command.arg("-R");
        }
        command.arg(&path);
    }

    #[cfg(target_os = "windows")]
    {
        command = Command::new("explorer");
        if reveal {
            command.arg(format!("/select,{}", path.to_string_lossy()));
        } else {
            command.arg(&path);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command = Command::new("xdg-open");
        if reveal {
            command.arg(path.parent().unwrap_or(&path));
        } else {
            command.arg(&path);
        }
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|_| LootboxError::OpenFailed)
}

#[tauri::command]
fn open_asset(path: String) -> Result<()> {
    run_open_command(&path, false)
}

#[tauri::command]
fn reveal_asset(path: String) -> Result<()> {
    run_open_command(&path, true)
}

fn open_audio_decoder(path: &str) -> Result<Decoder<std::io::BufReader<File>>> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "mp3" | "ogg" | "wav") {
        return Err(LootboxError::Audio(
            "Only MP3, OGG, and WAV playback is supported for now".into(),
        ));
    }
    let file = File::open(path)?;
    Decoder::try_from(file).map_err(|error| LootboxError::Audio(error.to_string()))
}

fn start_audio(playback: &mut AudioPlayback, path: String) -> Result<()> {
    let decoder = open_audio_decoder(&path)?;
    let duration = decoder.total_duration().unwrap_or_default();
    if playback.device.is_none() {
        playback.device = Some(
            DeviceSinkBuilder::open_default_sink()
                .map_err(|error| LootboxError::Audio(error.to_string()))?,
        );
    }
    if let Some(player) = playback.player.take() {
        player.stop();
    }
    let player = Player::connect_new(
        playback
            .device
            .as_ref()
            .expect("audio device was initialized")
            .mixer(),
    );
    player.append(decoder);
    player.play();
    playback.player = Some(player);
    playback.path = Some(path);
    playback.duration = duration;
    Ok(())
}

#[tauri::command]
async fn get_audio_duration(path: String) -> Result<f64> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(open_audio_decoder(&path)?
            .total_duration()
            .unwrap_or_default()
            .as_secs_f64())
    })
    .await
    .map_err(|error| LootboxError::Audio(error.to_string()))?
}

#[tauri::command]
async fn get_audio_analysis(path: String) -> Result<AudioAnalysis> {
    tauri::async_runtime::spawn_blocking(move || {
        const BUCKETS: usize = 240;
        let decoder = open_audio_decoder(&path)?;
        let duration = decoder.total_duration().unwrap_or_default();
        let channels = decoder.channels().get() as usize;
        let total_frames =
            (duration.as_secs_f64() * f64::from(decoder.sample_rate().get())) as usize;
        let mut peaks = vec![0.0_f32; BUCKETS];
        if total_frames > 0 && channels > 0 {
            for (index, sample) in decoder.enumerate() {
                let frame = index / channels;
                let bucket = (frame.saturating_mul(BUCKETS) / total_frames).min(BUCKETS - 1);
                peaks[bucket] = peaks[bucket].max(sample.abs());
            }
        }
        Ok(AudioAnalysis {
            duration_seconds: duration.as_secs_f64(),
            peaks,
        })
    })
    .await
    .map_err(|error| LootboxError::Audio(error.to_string()))?
}

#[tauri::command]
fn toggle_audio(path: String, state: State<'_, Mutex<AudioPlayback>>) -> Result<AudioStatus> {
    let mut playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    let is_current = playback.path.as_deref() == Some(path.as_str());
    if is_current {
        if let Some(player) = playback.player.as_ref() {
            if player.empty() {
                start_audio(&mut playback, path)?;
            } else if player.is_paused() {
                player.play();
            } else {
                player.pause();
            }
        } else {
            start_audio(&mut playback, path)?;
        }
    } else {
        start_audio(&mut playback, path)?;
    }
    Ok(audio_status(&playback))
}

#[tauri::command]
fn get_audio_status(state: State<'_, Mutex<AudioPlayback>>) -> Result<AudioStatus> {
    let playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    Ok(audio_status(&playback))
}

#[tauri::command]
fn seek_audio(
    path: String,
    position_seconds: f64,
    state: State<'_, Mutex<AudioPlayback>>,
) -> Result<AudioStatus> {
    let mut playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    if playback.path.as_deref() != Some(path.as_str()) || playback.player.is_none() {
        start_audio(&mut playback, path)?;
    }
    if let Some(player) = playback.player.as_ref() {
        player
            .try_seek(Duration::from_secs_f64(position_seconds.max(0.0)))
            .map_err(|error| LootboxError::Audio(error.to_string()))?;
    }
    Ok(audio_status(&playback))
}

#[tauri::command]
fn stop_audio(path: String, state: State<'_, Mutex<AudioPlayback>>) -> Result<()> {
    let mut playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    if playback.path.as_deref() == Some(path.as_str()) {
        if let Some(player) = playback.player.take() {
            player.stop();
        }
        playback.path = None;
        playback.duration = Duration::ZERO;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_linux_webview();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data_directory)?;
            let database_path = app_data_directory.join("lootbox.db");
            let database_existed = database_path.is_file();
            let state = AppState {
                database_path,
                thumbnail_directory: app_data_directory.join("thumbnails"),
                backup_directory: app_data_directory.join("backups"),
                diagnostic_log_path: app_data_directory.join("logs/lootbox.log"),
                write_queue: Arc::new(Mutex::new(())),
                import_cancellations: Arc::new(Mutex::new(HashMap::new())),
                diagnostics: Arc::new(Mutex::new(VecDeque::new())),
                hashing_library: Arc::new(AtomicBool::new(false)),
            };
            fs::create_dir_all(&state.thumbnail_directory)?;
            let mut connection = state.connect().map_err(|error| error.to_string())?;
            let old_schema_version: i64 = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap_or_default();
            if database_existed && old_schema_version < SCHEMA_VERSION {
                create_rotating_backup(&state, &connection, "before-migration")
                    .map_err(|error| error.to_string())?;
            }
            initialize_database(&connection).map_err(|error| error.to_string())?;
            migrate_classification(&mut connection).map_err(|error| error.to_string())?;
            create_rotating_backup(&state, &connection, "startup")
                .map_err(|error| error.to_string())?;
            clean_thumbnail_cache_from_connection(&state, &connection)
                .map_err(|error| error.to_string())?;
            state.record("info", "startup", "Database initialized and cache checked");

            // Asset protocol access is scoped at runtime. Restore access for cached
            // thumbnails and previously imported packs on every launch.
            let asset_scope = app.asset_protocol_scope();
            asset_scope.allow_directory(&state.thumbnail_directory, true)?;
            let mut statement = connection.prepare("SELECT root_path FROM packs")?;
            let roots = statement.query_map([], |row| row.get::<_, String>(0))?;
            for root in roots.flatten() {
                let _ = asset_scope.allow_directory(root, true);
            }
            drop(statement);
            let hashing_state = state.clone();
            app.manage(state);
            app.manage(Mutex::new(AudioPlayback::default()));
            tauri::async_runtime::spawn_blocking(move || {
                match hash_unhashed_assets(&hashing_state) {
                    Ok(count) if count > 0 => hashing_state.record(
                        "info",
                        "content-hashing",
                        format!("Hashed {count} existing assets"),
                    ),
                    Ok(_) => {}
                    Err(error) => {
                        hashing_state.record("error", "content-hashing", error.to_string())
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_pack,
            cancel_import,
            save_model_thumbnail,
            get_library_snapshot,
            query_assets,
            query_asset_selections,
            get_filter_options,
            add_tag,
            add_tags,
            remove_tag,
            remove_tags,
            create_collection,
            set_collection_membership,
            set_collection_memberships,
            delete_collection,
            add_godot_project,
            relocate_godot_project,
            remove_project,
            get_project_status,
            preview_assets_to_godot,
            export_assets_to_godot,
            preview_remove_assets_from_godot_project,
            remove_assets_from_godot_project,
            hash_library,
            remove_pack,
            rename_pack,
            set_assets_excluded,
            set_classification_override,
            reset_classification_override,
            restore_classification_overrides,
            purge_missing_assets,
            relocate_pack,
            get_cache_status,
            clean_thumbnail_cache,
            clear_thumbnail_cache,
            regenerate_image_thumbnails,
            create_metadata_backup,
            restore_metadata_backup,
            get_diagnostics,
            log_diagnostic,
            open_asset,
            reveal_asset,
            get_audio_duration,
            get_audio_analysis,
            toggle_audio,
            get_audio_status,
            seek_audio,
            stop_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lootbox");
}

#[cfg(target_os = "linux")]
fn configure_linux_webview() {
    // WebKitGTK's DMABUF renderer currently crashes at startup on some hybrid/NVIDIA
    // Wayland systems. Respect an explicit value so users can re-enable it with `=0`
    // after their driver or WebKit version fixes the upstream issue.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_webview() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naturally_sorts_numeric_runs_in_asset_names() {
        let connection = Connection::open_in_memory().unwrap();
        register_collations(&connection).unwrap();
        connection
            .execute("CREATE TABLE names (name TEXT NOT NULL)", [])
            .unwrap();
        for name in [
            "ambience_d19_loop",
            "ambience_d2_loop",
            "ambience_d10_loop",
            "ambience_d1_loop",
        ] {
            connection
                .execute("INSERT INTO names(name) VALUES (?1)", [name])
                .unwrap();
        }
        let mut statement = connection
            .prepare("SELECT name FROM names ORDER BY name COLLATE LOOTBOX_NATURAL ASC")
            .unwrap();
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            names,
            [
                "ambience_d1_loop",
                "ambience_d2_loop",
                "ambience_d10_loop",
                "ambience_d19_loop"
            ]
        );
    }

    #[test]
    fn classifies_common_game_asset_formats() {
        assert_eq!(classify_extension("png"), "image");
        assert_eq!(
            classify_asset_type(Path::new("Textures/stone.png"), "png"),
            "texture"
        );
        assert_eq!(
            classify_asset_type(Path::new("References/stone.png"), "png"),
            "image"
        );
        assert_eq!(classify_extension("wav"), "audio");
        assert_eq!(classify_extension("glb"), "model");
        assert_eq!(classify_extension("wgsl"), "shader");
        assert_eq!(classify_extension("prefab"), "other");
        assert_eq!(
            classify_asset_type(Path::new("512/Color Maps/brick.png"), "png"),
            "texture"
        );
        assert_eq!(
            classify_asset_type(Path::new("brick_normal.png"), "png"),
            "texture"
        );
        assert_eq!(
            texture_group_key(Path::new("256/Color Maps/brick.png")),
            texture_group_key(Path::new("512/Normal Maps/brick_normal.png"))
        );
    }

    #[test]
    fn groups_model_formats_across_export_directories() {
        let glb = model_variant_group(
            Path::new("Models/GLB (recommended)/Props/crate.glb"),
            "model",
            "glb",
        );
        let fbx = model_variant_group(
            Path::new("Models/other-formats/FBX/Props/crate.fbx"),
            "model",
            "fbx",
        );
        let mtl = model_variant_group(
            Path::new("Models/other-formats/OBJ/Props/crate.mtl"),
            "material",
            "mtl",
        );
        assert_eq!(glb, fbx);
        assert_eq!(glb, mtl);
        assert_ne!(
            glb,
            model_variant_group(
                Path::new("Models/GLB (recommended)/Buildings/crate.glb"),
                "model",
                "glb"
            )
        );
    }

    #[test]
    fn creates_safe_prefix_searches() {
        assert_eq!(
            fts_query("wooden sword"),
            Some("\"wooden\"* AND \"sword\"*".to_string())
        );
        assert_eq!(fts_query("!!!"), None);
    }

    #[test]
    fn initializes_an_empty_database() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let reverse_collection_index: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'collection_assets_asset_idx')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(reverse_collection_index);
    }

    #[test]
    fn groups_texture_maps_and_resolution_variants() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Surface Pack");
        for resolution in ["256", "512"] {
            fs::create_dir_all(pack.join(resolution).join("Color Maps")).unwrap();
            fs::create_dir_all(pack.join(resolution).join("Normal Maps")).unwrap();
            let size = if resolution == "512" { 16 } else { 8 };
            image::RgbaImage::from_pixel(size, size, image::Rgba([120, 90, 70, 255]))
                .save(pack.join(resolution).join("Color Maps/wall.png"))
                .unwrap();
            image::RgbaImage::from_pixel(size, size, image::Rgba([128, 128, 255, 255]))
                .save(pack.join(resolution).join("Normal Maps/wall_normal.png"))
                .unwrap();
        }

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        assert_eq!(imported.asset_count, 1);

        let page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: Some("texture".into()),
                pack_id: Some(imported.id),
                collection_id: None,
                limit: None,
                offset: None,
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].relative_path, "512/Color Maps/wall.png");
        assert_eq!(page.items[0].variants.len(), 4);
        assert_eq!(page.items[0].resources.len(), 3);
    }

    #[test]
    fn classifies_generic_texture_conventions_from_correlated_evidence() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Mixed Asset Pack");
        fs::create_dir_all(pack.join("Materials/Brick")).unwrap();
        fs::create_dir_all(pack.join("Materials/Stone")).unwrap();
        fs::create_dir_all(pack.join("Unreal")).unwrap();
        fs::create_dir_all(pack.join("References")).unwrap();
        for path in [
            "Materials/Brick/Albedo.png",
            "Materials/Brick/NormalGL.png",
            "Materials/Stone/stone.png",
            "Materials/Stone/stone_normal.png",
            "Unreal/T_Metal_D.png",
            "Unreal/T_Metal_N.png",
            "Unreal/T_Metal_ORM.png",
            "References/hero_color.png",
        ] {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([120, 90, 70, 255]))
                .save(pack.join(path))
                .unwrap();
        }

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        assert_eq!(imported.asset_count, 4);

        let texture_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE pack_id = ?1 AND usage = 'texture' AND is_primary = 1",
                params![imported.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(texture_count, 3);

        let inferred_role: String = connection
            .query_row(
                "SELECT map_role FROM assets WHERE pack_id = ?1 AND relative_path = 'Materials/Stone/stone.png'",
                params![imported.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(inferred_role, "color");

        let (asset_type, usage, confidence, basis): (String, Option<String>, i64, String) =
            connection
                .query_row(
                    "SELECT asset_type, usage, classification_confidence, classification_basis FROM assets WHERE pack_id = ?1 AND relative_path = 'References/hero_color.png'",
                    params![imported.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();
        assert_eq!(asset_type, "image");
        assert_eq!(usage, None);
        assert_eq!(confidence, 55);
        assert!(basis.contains("map-role-filename"));

        let roles: HashSet<String> = {
            let mut statement = connection
                .prepare("SELECT map_role FROM assets WHERE pack_id = ?1 AND usage = 'texture'")
                .unwrap();
            statement
                .query_map(params![imported.id], |row| row.get(0))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap()
        };
        assert!(roles.contains("color"));
        assert!(roles.contains("normal_gl"));
        assert!(roles.contains("occlusion_roughness_metalness"));
    }

    #[test]
    fn migrates_existing_rows_to_the_versioned_classifier() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Migrated Pack");
        fs::create_dir_all(pack.join("Brick")).unwrap();
        for path in ["Brick/Albedo.png", "Brick/NormalDX.png"] {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 128, 255]))
                .save(pack.join(path))
                .unwrap();
        }

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection.execute("DELETE FROM app_metadata", []).unwrap();
        connection
            .execute(
                "UPDATE assets SET file_type = 'other', usage = NULL, map_role = NULL, group_key = NULL, variant_group = NULL, asset_type = 'image'",
                [],
            )
            .unwrap();

        migrate_classification(&mut connection).unwrap();
        let classified: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE file_type = 'image' AND usage = 'texture' AND group_key IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(classified, 2);
        let version: String = connection
            .query_row(
                "SELECT value FROM app_metadata WHERE key = 'classification_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
        migrate_classification(&mut connection).unwrap();
    }

    #[test]
    fn imports_and_rescans_a_real_folder() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Starter Pack");
        fs::create_dir_all(pack.join("models")).unwrap();
        fs::create_dir_all(pack.join("models/other-formats/FBX")).unwrap();
        fs::create_dir_all(pack.join("models/other-formats/OBJ")).unwrap();
        fs::create_dir_all(pack.join("models/other-formats/DAE")).unwrap();
        fs::create_dir_all(pack.join("textures")).unwrap();
        fs::create_dir_all(pack.join(".ignored")).unwrap();
        fs::write(pack.join("models").join("wooden_sword.glb"), b"glb").unwrap();
        fs::write(
            pack.join("models/other-formats/FBX/wooden_sword.fbx"),
            b"fbx",
        )
        .unwrap();
        fs::write(
            pack.join("models/other-formats/OBJ/wooden_sword.obj"),
            b"obj",
        )
        .unwrap();
        fs::write(
            pack.join("models/other-formats/OBJ/wooden_sword.mtl"),
            b"newmtl wooden_sword\nmap_Kd C:/sword_diffuse.png\n",
        )
        .unwrap();
        fs::write(pack.join("impact.wav"), b"wave").unwrap();
        image::RgbaImage::from_pixel(8, 4, image::Rgba([150, 175, 140, 255]))
            .save(pack.join("grass.png"))
            .unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([90, 110, 80, 255]))
            .save(pack.join("textures/sword_diffuse.png"))
            .unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([90, 110, 80, 255]))
            .save(pack.join("models/other-formats/DAE/sword_diffuse.png"))
            .unwrap();
        fs::write(pack.join(".ignored").join("secret.png"), b"ignored").unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let thumbnails = temporary.path().join("thumbnails");
        let mut progress = Vec::new();
        let imported = import_pack_from_path(
            &mut connection,
            &pack,
            Some(&thumbnails),
            None,
            &mut |event| progress.push(event),
        )
        .unwrap();
        assert_eq!(imported.name, "Starter Pack");
        assert_eq!(imported.asset_count, 3);
        assert!(imported.available);
        assert_eq!(progress.first().unwrap().phase, "scanning");
        assert_eq!(progress.last().unwrap().phase, "complete");
        assert_eq!(progress.last().unwrap().current, 8);
        assert!(validate_pack_location(&connection, imported.id, &pack).is_ok());
        let wrong_location = temporary.path().join("Wrong Pack");
        fs::create_dir(&wrong_location).unwrap();
        assert!(validate_pack_location(&connection, imported.id, &wrong_location).is_err());

        let model_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE asset_type = 'model'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let search_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets_fts WHERE assets_fts MATCH '\"wooden\"*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(model_count, 3);
        let primary_model_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE asset_type = 'model' AND is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let grouped_variant_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE variant_group IS NOT NULL AND is_primary = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(primary_model_count, 1);
        assert_eq!(grouped_variant_count, 5);
        let texture_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE asset_type = 'texture' AND is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(texture_count, 0);
        let dependency_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM asset_dependencies", [], |row| {
                row.get(0)
            })
            .unwrap();
        let dependency_search_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets_fts WHERE assets_fts MATCH '\"sword_diffuse\"*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dependency_count, 1);
        assert_eq!(dependency_search_count, 1);
        assert_eq!(search_count, 1);

        let selection_query = AssetQuery {
            query: Some("wooden".into()),
            asset_type: Some("model".into()),
            sort: Some("name".into()),
            ..AssetQuery::default()
        };
        let lightweight_ids = query_asset_selections_from_connection(&selection_query, &connection)
            .unwrap()
            .into_iter()
            .map(|selection| selection.id)
            .collect::<Vec<_>>();
        let full_ids = query_assets_from_connection(selection_query, &connection)
            .unwrap()
            .items
            .into_iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>();
        assert_eq!(lightweight_ids, full_ids);

        let first_page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: None,
                collection_id: None,
                limit: Some(1),
                offset: Some(0),
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        let second_page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: None,
                collection_id: None,
                limit: Some(1),
                offset: Some(1),
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(first_page.total, 3);
        assert_eq!(first_page.items.len(), 1);
        assert!(first_page.has_more);
        assert_ne!(first_page.items[0].id, second_page.items[0].id);

        let ascending = query_assets_from_connection(
            AssetQuery {
                sort: Some("name".into()),
                sort_direction: Some("asc".into()),
                limit: Some(10),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|asset| asset.name)
        .collect::<Vec<_>>();
        let descending = query_assets_from_connection(
            AssetQuery {
                sort: Some("name".into()),
                sort_direction: Some("desc".into()),
                limit: Some(10),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|asset| asset.name)
        .collect::<Vec<_>>();
        assert_eq!(
            ascending.iter().rev().cloned().collect::<Vec<_>>(),
            descending
        );

        let (width, height, thumbnail): (i64, i64, String) = connection
            .query_row(
                "SELECT width, height, thumbnail_path FROM assets WHERE name = 'grass'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((width, height), (8, 4));
        assert!(Path::new(&thumbnail).is_file());

        let model_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE asset_type = 'model' AND is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        set_assets_excluded_from_connection(&[model_id], true, &mut connection).unwrap();
        let after_exclusion = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: None,
                collection_id: None,
                limit: Some(10),
                offset: Some(0),
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(after_exclusion.total, 2);
        let removed_page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: Some(imported.id),
                collection_id: None,
                limit: Some(10),
                offset: Some(0),
                excluded: Some(true),
                sort: Some("largest".into()),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(removed_page.total, 1);
        assert_eq!(
            get_pack(&connection, imported.id)
                .unwrap()
                .removed_asset_count,
            1
        );
        set_assets_excluded_from_connection(&[model_id], false, &mut connection).unwrap();
        assert_eq!(
            query_assets_from_connection(
                AssetQuery {
                    query: None,
                    asset_type: None,
                    pack_id: None,
                    collection_id: None,
                    limit: Some(10),
                    offset: Some(0),
                    excluded: None,
                    sort: Some("type".into()),
                    ..AssetQuery::default()
                },
                &connection,
            )
            .unwrap()
            .total,
            3
        );
        set_assets_excluded_from_connection(&[model_id], true, &mut connection).unwrap();
        connection
            .execute(
                "UPDATE packs SET name = 'My Starter Pack' WHERE id = ?1",
                params![imported.id],
            )
            .unwrap();
        let rescanned =
            import_pack_from_path(&mut connection, &pack, Some(&thumbnails), None, &mut |_| {})
                .unwrap();
        assert_eq!(rescanned.asset_count, 2);
        assert_eq!(rescanned.name, "My Starter Pack");

        fs::remove_file(pack.join("impact.wav")).unwrap();
        let rescanned =
            import_pack_from_path(&mut connection, &pack, Some(&thumbnails), None, &mut |_| {})
                .unwrap();
        assert_eq!(rescanned.asset_count, 1);
    }

    #[test]
    fn rescans_preserve_identity_metadata_and_missing_records() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Rename Pack");
        fs::create_dir_all(&pack).unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(pack.join("old-name.png"))
            .unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        let original_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE pack_id = ?1",
                params![imported.id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute("INSERT INTO tags(name) VALUES ('favorite')", [])
            .unwrap();
        connection.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT ?1, id FROM tags WHERE name = 'favorite'",
            params![original_id],
        ).unwrap();

        fs::rename(pack.join("old-name.png"), pack.join("new-name.png")).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        let (renamed_id, relative_path): (i64, String) = connection
            .query_row(
                "SELECT id, relative_path FROM assets WHERE pack_id = ?1",
                params![imported.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(renamed_id, original_id);
        assert_eq!(relative_path, "new-name.png");
        let tag_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1",
                params![original_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_count, 1);

        fs::remove_file(pack.join("new-name.png")).unwrap();
        let rescanned =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        assert_eq!(rescanned.asset_count, 0);
        assert_eq!(rescanned.missing_asset_count, 1);
        let (missing, retained_tags): (bool, i64) = connection.query_row(
            "SELECT missing, (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id) FROM assets WHERE id = ?1",
            params![original_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert!(missing);
        assert_eq!(retained_tags, 1);
    }

    #[test]
    fn manual_classification_overrides_survive_recomputation() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Override Pack");
        fs::create_dir_all(&pack).unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(pack.join("reference.png"))
            .unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        let asset_id: i64 = connection
            .query_row("SELECT id FROM assets", [], |row| row.get(0))
            .unwrap();
        connection.execute(
            "INSERT INTO classification_overrides(asset_id, asset_type, map_role, group_key) VALUES (?1, 'texture', 'color', 'manual:test')",
            params![asset_id],
        ).unwrap();
        recompute_texture_groups(&connection, Some(imported.id)).unwrap();
        apply_classification_overrides(&connection, Some(imported.id)).unwrap();
        let values: (String, Option<String>, String, String) = connection.query_row(
            "SELECT asset_type, map_role, group_key, classification_basis FROM assets WHERE id = ?1",
            params![asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap();
        assert_eq!(
            values,
            (
                "texture".into(),
                Some("color".into()),
                "manual:test".into(),
                "manual-override".into()
            )
        );
        connection
            .execute(
                "UPDATE classification_overrides SET map_role = '__none' WHERE asset_id = ?1",
                params![asset_id],
            )
            .unwrap();
        recompute_texture_groups(&connection, Some(imported.id)).unwrap();
        apply_classification_overrides(&connection, Some(imported.id)).unwrap();
        let cleared_role: Option<String> = connection
            .query_row(
                "SELECT map_role FROM assets WHERE id = ?1",
                params![asset_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cleared_role, None);
    }

    #[test]
    fn import_cancellation_stops_before_writing() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Cancelled Pack");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("asset.txt"), b"asset").unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let cancelled = AtomicBool::new(true);
        let result =
            import_pack_from_path(&mut connection, &pack, None, Some(&cancelled), &mut |_| {});
        assert!(matches!(result, Err(LootboxError::ImportCancelled)));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM packs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn detects_duplicates_by_file_content_across_packs() {
        let temporary = tempfile::tempdir().unwrap();
        let first_pack = temporary.path().join("First Pack");
        let second_pack = temporary.path().join("Second Pack");
        fs::create_dir_all(&first_pack).unwrap();
        fs::create_dir_all(&second_pack).unwrap();
        fs::write(first_pack.join("impact.wav"), b"identical audio bytes").unwrap();
        fs::write(second_pack.join("renamed.wav"), b"identical audio bytes").unwrap();
        fs::write(second_pack.join("different.wav"), b"different audio bytes").unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &first_pack, None, None, &mut |_| {}).unwrap();
        import_pack_from_path(&mut connection, &second_pack, None, None, &mut |_| {}).unwrap();

        let page = query_assets_from_connection(
            AssetQuery {
                duplicates_only: Some(true),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|asset| asset.content_hash.is_some()));
        assert!(page.items.iter().all(|asset| asset.duplicate_count == 2));
        assert!(page
            .items
            .iter()
            .all(|asset| asset.duplicate_locations.len() == 1));
    }

    #[test]
    fn exports_grouped_texture_maps_to_a_godot_project_idempotently() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Surface Pack");
        let project = temporary.path().join("Godot Game");
        fs::create_dir_all(pack.join("Materials/Brick")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("project.godot"),
            b"[application]\nconfig/name=\"Export Test\"\n",
        )
        .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 70, 50, 255]))
            .save(pack.join("Materials/Brick/brick_color.png"))
            .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 255, 255]))
            .save(pack.join("Materials/Brick/brick_normal.png"))
            .unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection
            .execute(
                "INSERT INTO projects(name, root_path) VALUES ('Export Test', ?1)",
                params![path_string(&project)],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        let asset_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE is_primary = 1 AND asset_type = 'texture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let exported_root = project.join("assets/lootbox/surface-pack-1/Materials/Brick");
        fs::create_dir_all(&exported_root).unwrap();
        fs::write(exported_root.join("brick_color.png"), b"project-owned file").unwrap();

        let preview =
            preview_assets_to_godot_from_connection(&connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(preview.selected, 1);
        assert_eq!(preview.related, 1);
        assert_eq!(preview.grouped, 1);
        assert_eq!(preview.dependencies, 0);
        assert_eq!(preview.total_files, 2);
        assert_eq!(preview.conflicts, 1);
        assert_eq!(preview.conflict_files.len(), 1);
        assert_eq!(preview.destination, "res://assets/lootbox");

        let first =
            export_assets_to_godot_from_connection(&mut connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(first.copied, 2);
        assert_eq!(first.unchanged, 0);
        assert_eq!(
            fs::read(exported_root.join("brick_color.png")).unwrap(),
            b"project-owned file"
        );
        assert!(exported_root
            .join(format!("brick_color-lootbox-{asset_id}.png"))
            .is_file());
        assert!(exported_root.join("brick_normal.png").is_file());
        assert!(project
            .join("assets/lootbox/lootbox-manifest.json")
            .is_file());

        let second =
            export_assets_to_godot_from_connection(&mut connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(second.copied, 0);
        assert_eq!(second.unchanged, 2);

        let status = project_status_from_connection(&connection, project_id).unwrap();
        assert_eq!(status.tracked_files, 2);
        assert_eq!(status.up_to_date_files, 2);
        assert_eq!(status.runs.len(), 2);
        assert_eq!(status.runs[0].unchanged_count, 2);

        let unused = query_assets_from_connection(
            AssetQuery {
                unused_by_projects: Some(true),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(unused.total, 0);

        fs::write(
            pack.join("Materials/Brick/brick_normal.png"),
            b"changed source",
        )
        .unwrap();
        let changed_status = project_status_from_connection(&connection, project_id).unwrap();
        assert_eq!(changed_status.source_changed_files, 1);
        assert_eq!(changed_status.up_to_date_files, 1);

        let normal_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE relative_path LIKE '%brick_normal.png'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let edited_project_path: String = connection
            .query_row(
                "SELECT exported_path FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
                params![project_id, normal_id],
                |row| row.get(0),
            )
            .unwrap();
        fs::write(&edited_project_path, b"project edit").unwrap();
        let protected_preview =
            preview_assets_to_godot_from_connection(&connection, project_id, &[asset_id], None)
                .unwrap();
        assert!(protected_preview.conflicts >= 2);
        export_assets_to_godot_from_connection(&mut connection, project_id, &[asset_id], None)
            .unwrap();
        assert_eq!(fs::read(&edited_project_path).unwrap(), b"project edit");
        let replacement_path: String = connection
            .query_row(
                "SELECT exported_path FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
                params![project_id, normal_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(replacement_path, edited_project_path);

        let moved_project = temporary.path().join("Moved Godot Game");
        fs::rename(&project, &moved_project).unwrap();
        let relocated =
            relocate_godot_project_from_connection(&mut connection, project_id, &moved_project)
                .unwrap();
        assert_eq!(relocated.root_path, path_string(&moved_project));
        let relocated_paths = connection
            .prepare("SELECT exported_path FROM project_exports WHERE project_id = ?1")
            .unwrap()
            .query_map(params![project_id], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(relocated_paths
            .iter()
            .all(|path| Path::new(path).starts_with(&moved_project)));
        let relocated_status = project_status_from_connection(&connection, project_id).unwrap();
        assert_eq!(relocated_status.tracked_files, 2);
        assert_eq!(relocated_status.up_to_date_files, 2);
        let removal =
            plan_assets_from_godot_project_removal(&connection, project_id, &[asset_id]).unwrap();
        assert_eq!(removal.preview.remove_files.len(), 2);
        assert!(removal.preview.missing_files.is_empty());
    }

    #[test]
    fn filters_model_export_formats_but_keeps_required_companions() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Model Pack");
        let project = temporary.path().join("Godot Game");
        fs::create_dir_all(pack.join("Models/GLB")).unwrap();
        fs::create_dir_all(pack.join("Models/other-formats/FBX")).unwrap();
        fs::create_dir_all(pack.join("Models/other-formats/OBJ")).unwrap();
        fs::create_dir_all(pack.join("Textures")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("project.godot"),
            b"[application]\nconfig/name=\"Format Test\"\n",
        )
        .unwrap();
        fs::write(pack.join("Models/GLB/crate.glb"), b"glb model").unwrap();
        fs::write(
            pack.join("Models/other-formats/FBX/crate.fbx"),
            b"fbx model",
        )
        .unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/crate.obj"),
            b"mtllib crate.mtl\n",
        )
        .unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/crate.mtl"),
            b"map_Kd crate_diffuse.png\n",
        )
        .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 70, 50, 255]))
            .save(pack.join("Textures/crate_diffuse.png"))
            .unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection
            .execute(
                "INSERT INTO projects(name, root_path) VALUES ('Format Test', ?1)",
                params![path_string(&project)],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        let asset_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE is_primary = 1 AND asset_type = 'model'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let all_formats =
            preview_assets_to_godot_from_connection(&connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(
            all_formats
                .model_formats
                .iter()
                .map(|format| format.extension.as_str())
                .collect::<Vec<_>>(),
            vec!["glb", "fbx", "obj"]
        );
        assert_eq!(
            all_formats.selected_model_formats,
            vec!["fbx", "glb", "obj"]
        );

        let glb = vec!["glb".to_string()];
        let glb_only = preview_assets_to_godot_from_connection(
            &connection,
            project_id,
            &[asset_id],
            Some(&glb),
        )
        .unwrap();
        assert_eq!(glb_only.selected_model_formats, vec!["glb"]);
        assert!(glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.glb")));
        assert!(!glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.fbx")));
        assert!(!glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.obj")));
        assert!(!glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.mtl")));
        assert!(glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate_diffuse.png")));

        let obj = vec!["obj".to_string()];
        let obj_only = preview_assets_to_godot_from_connection(
            &connection,
            project_id,
            &[asset_id],
            Some(&obj),
        )
        .unwrap();
        assert!(obj_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.obj")));
        assert!(obj_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.mtl")));
        assert!(obj_only
            .files
            .iter()
            .any(|file| file.ends_with("crate_diffuse.png")));

        let exported = export_assets_to_godot_from_connection(
            &mut connection,
            project_id,
            &[asset_id],
            Some(&glb),
        )
        .unwrap();
        assert_eq!(exported.copied, glb_only.total_files);
    }

    #[test]
    fn removes_only_unchanged_project_exports_and_keeps_shared_or_modified_files() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Model Pack");
        let project = temporary.path().join("Godot Game");
        for directory in [
            "Models/GLB",
            "Models/other-formats/FBX",
            "Models/other-formats/OBJ",
            "Textures",
        ] {
            fs::create_dir_all(pack.join(directory)).unwrap();
        }
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("project.godot"),
            b"[application]\nconfig/name=\"Removal Test\"\n",
        )
        .unwrap();
        fs::write(pack.join("Models/GLB/crate.glb"), b"crate glb").unwrap();
        fs::write(
            pack.join("Models/other-formats/FBX/crate.fbx"),
            b"crate fbx",
        )
        .unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/crate.mtl"),
            b"map_Kd shared.png\n",
        )
        .unwrap();
        fs::write(pack.join("Models/GLB/barrel.glb"), b"barrel glb").unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/barrel.mtl"),
            b"map_Kd shared.png\n",
        )
        .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([90, 60, 30, 255]))
            .save(pack.join("Textures/shared.png"))
            .unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection
            .execute(
                "INSERT INTO projects(name, root_path) VALUES ('Removal Test', ?1)",
                params![path_string(&project)],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        let crate_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE name = 'crate' AND extension = 'glb'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let barrel_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE name = 'barrel' AND extension = 'glb'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        export_assets_to_godot_from_connection(&mut connection, project_id, &[crate_id], None)
            .unwrap();
        export_assets_to_godot_from_connection(&mut connection, project_id, &[barrel_id], None)
            .unwrap();

        let crate_glb: String = connection
            .query_row(
                "SELECT exported_path FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
                params![project_id, crate_id],
                |row| row.get(0),
            )
            .unwrap();
        fs::write(&crate_glb, b"project edited crate glb").unwrap();

        let preview = plan_assets_from_godot_project_removal(&connection, project_id, &[crate_id])
            .unwrap()
            .preview;
        assert_eq!(preview.selected, 1);
        assert!(preview
            .remove_files
            .iter()
            .any(|path| path.ends_with("crate.fbx")));
        assert!(preview
            .modified_files
            .iter()
            .any(|path| path.ends_with("crate.glb")));
        assert!(preview
            .shared_files
            .iter()
            .any(|path| path.ends_with("shared.png")));

        let result = remove_assets_from_godot_project_from_connection(
            &mut connection,
            project_id,
            &[crate_id],
        )
        .unwrap();
        assert_eq!(result.deleted, 1);
        assert_eq!(result.kept_modified, 1);
        assert_eq!(result.kept_shared, 1);
        assert!(Path::new(&crate_glb).is_file());
        assert_eq!(
            project_summary(&connection, project_id)
                .unwrap()
                .asset_count,
            1
        );

        let manifest =
            fs::read_to_string(project.join("assets/lootbox/lootbox-manifest.json")).unwrap();
        assert!(!manifest.contains("crate.glb"));
        assert!(!manifest.contains("crate.fbx"));
        assert!(manifest.contains("barrel.glb"));
        assert!(manifest.contains("shared.png"));
    }

    #[test]
    fn packaged_release_preview_policy_is_safe() {
        let config = include_str!("../tauri.conf.json");
        let parsed: serde_json::Value = serde_json::from_str(config).unwrap();
        let csp = parsed["app"]["security"]["csp"].as_str().unwrap();
        assert!(csp.contains("connect-src") && csp.contains("blob:") && csp.contains("data:"));
        assert!(csp.contains("asset:") && csp.contains("http://asset.localhost"));
        let model_preview = include_str!("../../src/components/ModelPreview.tsx");
        assert!(model_preview.contains("GLTFLoader"));
        assert!(model_preview.contains("outputColorSpace = THREE.SRGBColorSpace"));
    }

    #[test]
    fn extracts_model_poly_and_vertex_counts() {
        let gltf_json = r#"{
            "accessors": [
                { "count": 24 },
                { "count": 36 }
            ],
            "meshes": [
                {
                    "primitives": [
                        {
                            "attributes": { "POSITION": 0 },
                            "indices": 1,
                            "mode": 4
                        }
                    ]
                }
            ]
        }"#;
        let counts = parse_gltf_json(gltf_json.as_bytes()).unwrap();
        assert_eq!(counts, (12, 24));

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("cube.obj");
        fs::write(
            &obj_path,
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n",
        )
        .unwrap();
        let (triangles, vertices) = model_poly_count(&obj_path, "obj");
        assert_eq!(triangles, Some(2));
        assert_eq!(vertices, Some(4));
    }
}
