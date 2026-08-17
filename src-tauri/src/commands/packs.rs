use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use rusqlite::{params, Connection};
use tauri::{ipc::Channel, Manager, State};

use crate::{
    db::*,
    error::{LootboxError, Result},
    ingest::*,
    models::*,
    state::AppState,
};

#[tauri::command]
pub fn cancel_import(job_id: String, state: State<'_, AppState>) -> Result<()> {
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
pub async fn import_pack(
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

#[tauri::command]
pub fn get_library_snapshot(state: State<'_, AppState>) -> Result<LibrarySnapshot> {
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

#[tauri::command]
pub fn remove_pack(pack_id: i64, state: State<'_, AppState>) -> Result<()> {
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
pub fn rename_pack(pack_id: i64, name: String, state: State<'_, AppState>) -> Result<()> {
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
pub fn purge_missing_assets(pack_id: i64, state: State<'_, AppState>) -> Result<()> {
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
pub fn get_filter_options(state: State<'_, AppState>) -> Result<FilterOptions> {
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

#[tauri::command]
pub async fn relocate_pack(
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

