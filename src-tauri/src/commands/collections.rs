use rusqlite::{params, OptionalExtension};
use tauri::State;

use crate::{
    db::*,
    error::{LootboxError, Result},
    models::*,
    state::AppState,
};

#[tauri::command]
pub fn add_tag(asset_id: i64, name: String, state: State<'_, AppState>) -> Result<()> {
    add_tags(vec![asset_id], name, state).map(|_| ())
}

#[tauri::command]
pub fn add_tags(asset_ids: Vec<i64>, name: String, state: State<'_, AppState>) -> Result<Vec<i64>> {
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
    sync_search_index_for_assets(&connection, &changed)?;
    Ok(changed)
}

#[tauri::command]
pub fn remove_tag(asset_id: i64, name: String, state: State<'_, AppState>) -> Result<()> {
    remove_tags(vec![asset_id], name, state).map(|_| ())
}

#[tauri::command]
pub fn remove_tags(asset_ids: Vec<i64>, name: String, state: State<'_, AppState>) -> Result<Vec<i64>> {
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
    sync_search_index_for_assets(&connection, &changed)?;
    Ok(changed)
}

#[tauri::command]
pub fn create_collection(name: String, state: State<'_, AppState>) -> Result<CollectionSummary> {
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
pub fn set_collection_membership(
    asset_id: i64,
    collection_id: i64,
    included: bool,
    state: State<'_, AppState>,
) -> Result<()> {
    set_collection_memberships(vec![asset_id], collection_id, included, state).map(|_| ())
}

#[tauri::command]
pub fn set_collection_memberships(
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
pub fn delete_collection(collection_id: i64, state: State<'_, AppState>) -> Result<()> {
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

