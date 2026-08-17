use std::{
    fs,
    path::{Path, PathBuf},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rayon::prelude::*;
use rusqlite::{params, Connection};
use tauri::State;

use crate::{
    db::*,
    error::{LootboxError, Result},
    ingest::*,
    models::*,
    state::AppState,
};

#[tauri::command]
pub fn get_cache_status(state: State<'_, AppState>) -> Result<CacheStatus> {
    cache_status_from_connection(&state, &state.connect()?)
}

#[tauri::command]
pub fn clean_thumbnail_cache(state: State<'_, AppState>) -> Result<CacheStatus> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clean_thumbnail_cache_from_connection(&state, &state.connect()?)
}

#[tauri::command]
pub fn clear_thumbnail_cache(state: State<'_, AppState>) -> Result<CacheStatus> {
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
pub async fn regenerate_image_thumbnails(state: State<'_, AppState>) -> Result<CacheStatus> {
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
pub fn create_metadata_backup(
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
pub fn restore_metadata_backup(path: String, state: State<'_, AppState>) -> Result<()> {
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
pub fn get_diagnostics(state: State<'_, AppState>) -> Result<Vec<DiagnosticEntry>> {
    Ok(state
        .diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect())
}

#[tauri::command]
pub fn log_diagnostic(
    level: String,
    context: String,
    message: String,
    state: State<'_, AppState>,
) -> Result<()> {
    state.record(&level, &context, message);
    Ok(())
}

#[tauri::command]
pub async fn save_model_thumbnail(
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

pub fn save_model_thumbnail_from_state(
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

