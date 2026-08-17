use std::{
    cmp::Ordering as CmpOrdering,
    collections::HashSet,
    fs,
    path::Path,
};

use rusqlite::{params, Connection};

use crate::{
    db::search::rebuild_search_index,
    error::Result,
    ingest::{path_string, unix_timestamp},
    models::{CacheStatus, PackSummary},
    state::AppState,
};

pub fn natural_name_cmp(left: &str, right: &str) -> CmpOrdering {
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

pub fn register_collations(connection: &Connection) -> Result<()> {
    connection.create_collation("LOOTBOX_NATURAL", natural_name_cmp)?;
    Ok(())
}

pub const SCHEMA_VERSION: i64 = 6;

pub const IMAGE_THUMBNAIL_VERSION: i64 = 2;

pub const MODEL_THUMBNAIL_VERSION: i64 = 4;

pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

pub fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|name| name == column))
}

pub fn add_column_if_missing(
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

pub fn initialize_database(connection: &Connection) -> Result<()> {
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

pub fn create_backup(connection: &Connection, destination: &Path) -> Result<String> {
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

pub fn create_rotating_backup(
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

pub fn cache_status_from_connection(state: &AppState, connection: &Connection) -> Result<CacheStatus> {
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

pub fn clean_thumbnail_cache_from_connection(
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
    connection.execute_batch("BEGIN TRANSACTION;")?;
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
    connection.execute_batch("COMMIT;")?;
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
    connection.execute_batch("BEGIN TRANSACTION;")?;
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
    connection.execute_batch("COMMIT;")?;
    cache_status_from_connection(state, connection)
}

pub fn get_pack(connection: &Connection, pack_id: i64) -> Result<PackSummary> {
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

