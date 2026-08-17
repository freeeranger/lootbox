use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use tauri::State;

use crate::{
    db::*,
    error::{LootboxError, Result},
    ingest::*,
    models::*,
    state::AppState,
};

#[tauri::command]
pub async fn query_assets(request: AssetQuery, state: State<'_, AppState>) -> Result<AssetPage> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        query_assets_from_connection(request, &connection)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
pub async fn query_asset_selections(
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

pub fn query_asset_selections_from_connection(
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

pub fn query_assets_from_connection(request: AssetQuery, connection: &Connection) -> Result<AssetPage> {
    let (where_clause, mut values) = asset_query_filter(&request);
    let count_sql = format!("SELECT COUNT(*) FROM assets a WHERE {where_clause}");
    let total = connection.query_row(&count_sql, params_from_iter(values.iter()), |row| {
        row.get::<_, i64>(0)
    })?;

    let sql = format!(
        r#"
        SELECT
            a.id, a.pack_id, p.name, a.name, a.relative_path, a.absolute_path,
            a.extension, a.asset_type, a.size_bytes, a.modified_at, a.width, a.height,
            a.triangles, a.vertices, a.thumbnail_path,
            a.file_type, a.usage, a.map_role, a.resolution,
            a.classification_confidence, a.classification_basis, a.missing,
            EXISTS(SELECT 1 FROM classification_overrides override WHERE override.asset_id = a.id),
            a.content_hash, a.group_key
        FROM assets a
        JOIN packs p ON p.id = a.pack_id
        WHERE {where_clause}
        ORDER BY {}
        LIMIT ? OFFSET ?
        "#,
        asset_query_order(&request)
    );

    let limit = request.limit.unwrap_or(160).clamp(1, 10_000);
    let offset = request.offset.unwrap_or(0).max(0);
    values.push(limit.into());
    values.push(offset.into());

    struct RowBase {
        id: i64,
        pack_id: i64,
        pack_name: String,
        name: String,
        relative_path: String,
        absolute_path: String,
        extension: String,
        asset_type: String,
        size_bytes: i64,
        modified_at: i64,
        width: Option<i64>,
        height: Option<i64>,
        triangles: Option<i64>,
        vertices: Option<i64>,
        thumbnail_path: Option<String>,
        file_type: String,
        usage: Option<String>,
        map_role: Option<String>,
        resolution: Option<String>,
        classification_confidence: i64,
        classification_basis: String,
        missing: bool,
        manual_classification: bool,
        content_hash: Option<String>,
        group_key: Option<String>,
    }

    let mut statement = connection.prepare(&sql)?;
    let base_rows: Vec<RowBase> = statement
        .query_map(params_from_iter(values), |row| {
            Ok(RowBase {
                id: row.get(0)?,
                pack_id: row.get(1)?,
                pack_name: row.get(2)?,
                name: row.get(3)?,
                relative_path: row.get(4)?,
                absolute_path: row.get(5)?,
                extension: row.get(6)?,
                asset_type: row.get(7)?,
                size_bytes: row.get(8)?,
                modified_at: row.get(9)?,
                width: row.get(10)?,
                height: row.get(11)?,
                triangles: row.get(12)?,
                vertices: row.get(13)?,
                thumbnail_path: row.get(14)?,
                file_type: row.get(15)?,
                usage: row.get(16)?,
                map_role: row.get(17)?,
                resolution: row.get(18)?,
                classification_confidence: row.get(19)?,
                classification_basis: row.get(20)?,
                missing: row.get(21)?,
                manual_classification: row.get(22)?,
                content_hash: row.get(23)?,
                group_key: row.get(24)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if base_rows.is_empty() {
        return Ok(AssetPage {
            items: Vec::new(),
            total,
            has_more: offset < total,
        });
    }

    let asset_ids: Vec<i64> = base_rows.iter().map(|row| row.id).collect();
    let id_in_clause = asset_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // Batch fetch tags
    let mut tags_by_asset: HashMap<i64, Vec<String>> = HashMap::new();
    {
        let tags_sql = format!(
            "SELECT at.asset_id, t.name FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id IN ({id_in_clause})"
        );
        let mut stmt = connection.prepare(&tags_sql)?;
        let rows = stmt.query_map(params_from_iter(asset_ids.iter().copied()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for entry in rows {
            let (asset_id, tag_name) = entry?;
            tags_by_asset.entry(asset_id).or_default().push(tag_name);
        }
    }

    // Batch fetch collections
    let mut collections_by_asset: HashMap<i64, Vec<i64>> = HashMap::new();
    {
        let coll_sql = format!(
            "SELECT asset_id, collection_id FROM collection_assets WHERE asset_id IN ({id_in_clause})"
        );
        let mut stmt = connection.prepare(&coll_sql)?;
        let rows = stmt.query_map(params_from_iter(asset_ids.iter().copied()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for entry in rows {
            let (asset_id, collection_id) = entry?;
            collections_by_asset.entry(asset_id).or_default().push(collection_id);
        }
    }

    // Batch fetch variants for items with group_key
    let mut variants_by_group: HashMap<(i64, String), Vec<AssetVariant>> = HashMap::new();
    let group_keys: Vec<(i64, String)> = base_rows
        .iter()
        .filter_map(|row| row.group_key.as_ref().map(|k| (row.pack_id, k.clone())))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if !group_keys.is_empty() {
        let mut variant_sql = String::from(
            "SELECT pack_id, group_key, id, extension, asset_type, file_type, usage, map_role, resolution, triangles, vertices, absolute_path, relative_path, size_bytes FROM assets WHERE group_key IS NOT NULL AND ("
        );
        for (i, _) in group_keys.iter().enumerate() {
            if i > 0 {
                variant_sql.push_str(" OR ");
            }
            variant_sql.push_str("(pack_id = ? AND group_key = ?)");
        }
        variant_sql.push(')');
        let mut variant_params = Vec::new();
        for (pack_id, group_key) in &group_keys {
            variant_params.push(rusqlite::types::Value::from(*pack_id));
            variant_params.push(rusqlite::types::Value::from(group_key.clone()));
        }
        let mut stmt = connection.prepare(&variant_sql)?;
        let rows = stmt.query_map(params_from_iter(variant_params), |row| {
            let pack_id: i64 = row.get(0)?;
            let group_key: String = row.get(1)?;
            let variant = AssetVariant {
                id: row.get(2)?,
                extension: row.get(3)?,
                asset_type: row.get(4)?,
                file_type: row.get(5)?,
                usage: row.get(6)?,
                map_role: row.get(7)?,
                resolution: row.get(8)?,
                triangles: row.get(9)?,
                vertices: row.get(10)?,
                absolute_path: row.get(11)?,
                relative_path: row.get(12)?,
                size_bytes: row.get(13)?,
            };
            Ok(((pack_id, group_key), variant))
        })?;
        for entry in rows {
            let (key, variant) = entry?;
            variants_by_group.entry(key).or_default().push(variant);
        }
    }

    // Batch fetch dependencies
    let mut resources_by_asset: HashMap<i64, Vec<AssetResource>> = HashMap::new();
    {
        let dep_sql = format!(
            r#"
            SELECT
                d.owner_asset_id, r.id, r.name, r.extension, r.asset_type, r.file_type,
                r.usage, r.map_role, r.resolution, r.triangles, r.vertices,
                r.absolute_path, r.relative_path, r.size_bytes, r.thumbnail_path
            FROM asset_dependencies d
            JOIN assets r ON r.id = d.dependency_asset_id
            WHERE d.owner_asset_id IN ({id_in_clause})
            "#
        );
        let mut stmt = connection.prepare(&dep_sql)?;
        let rows = stmt.query_map(params_from_iter(asset_ids.iter().copied()), |row| {
            let owner_id: i64 = row.get(0)?;
            let resource = AssetResource {
                id: row.get(1)?,
                name: row.get(2)?,
                extension: row.get(3)?,
                asset_type: row.get(4)?,
                file_type: row.get(5)?,
                usage: row.get(6)?,
                map_role: row.get(7)?,
                resolution: row.get(8)?,
                triangles: row.get(9)?,
                vertices: row.get(10)?,
                absolute_path: row.get(11)?,
                relative_path: row.get(12)?,
                size_bytes: row.get(13)?,
                thumbnail_path: row.get(14)?,
            };
            Ok((owner_id, resource))
        })?;
        for entry in rows {
            let (owner_id, resource) = entry?;
            resources_by_asset.entry(owner_id).or_default().push(resource);
        }
    }

    // Batch fetch duplicates for items with content_hash
    let mut duplicate_locations_by_hash: HashMap<String, Vec<DuplicateLocation>> = HashMap::new();
    let mut duplicate_count_by_hash: HashMap<String, i64> = HashMap::new();
    let hashes: Vec<String> = base_rows
        .iter()
        .filter_map(|row| row.content_hash.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if !hashes.is_empty() {
        let hash_in_clause = hashes.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let dup_sql = format!(
            r#"
            SELECT copy.content_hash, copy.id, copy_pack.name, copy.relative_path, copy.absolute_path, copy.size_bytes
            FROM assets copy
            JOIN packs copy_pack ON copy_pack.id = copy.pack_id
            WHERE copy.content_hash IN ({hash_in_clause}) AND copy.missing = 0
            "#
        );
        let mut stmt = connection.prepare(&dup_sql)?;
        let rows = stmt.query_map(params_from_iter(hashes.iter().map(|h| h.as_str())), |row| {
            let hash: String = row.get(0)?;
            let dup = DuplicateLocation {
                id: row.get(1)?,
                pack_name: row.get(2)?,
                relative_path: row.get(3)?,
                absolute_path: row.get(4)?,
                size_bytes: row.get(5)?,
            };
            Ok((hash, dup))
        })?;
        for entry in rows {
            let (hash, dup) = entry?;
            *duplicate_count_by_hash.entry(hash.clone()).or_insert(0) += 1;
            duplicate_locations_by_hash.entry(hash).or_default().push(dup);
        }
    }

    let assets: Vec<Asset> = base_rows
        .into_iter()
        .map(|row| {
            let variants = row
                .group_key
                .as_ref()
                .and_then(|k| variants_by_group.remove(&(row.pack_id, k.clone())))
                .unwrap_or_default();
            let resources = resources_by_asset.remove(&row.id).unwrap_or_default();
            let tags = tags_by_asset.remove(&row.id).unwrap_or_default();
            let collection_ids = collections_by_asset.remove(&row.id).unwrap_or_default();
            let (duplicate_count, duplicate_locations) = if let Some(ref hash) = row.content_hash {
                let count = duplicate_count_by_hash.get(hash).copied().unwrap_or(0);
                let locs = duplicate_locations_by_hash
                    .get(hash)
                    .map(|list| list.iter().filter(|d| d.id != row.id).cloned().collect())
                    .unwrap_or_default();
                (count, locs)
            } else {
                (0, Vec::new())
            };

            Asset {
                id: row.id,
                pack_id: row.pack_id,
                pack_name: row.pack_name,
                name: row.name,
                relative_path: row.relative_path,
                absolute_path: row.absolute_path,
                extension: row.extension,
                asset_type: row.asset_type,
                file_type: row.file_type,
                usage: row.usage,
                map_role: row.map_role,
                resolution: row.resolution,
                classification_confidence: row.classification_confidence,
                classification_basis: row.classification_basis,
                missing: row.missing,
                manual_classification: row.manual_classification,
                content_hash: row.content_hash,
                duplicate_count,
                duplicate_locations,
                size_bytes: row.size_bytes,
                modified_at: row.modified_at,
                width: row.width,
                height: row.height,
                triangles: row.triangles,
                vertices: row.vertices,
                thumbnail_path: row.thumbnail_path,
                variants,
                resources,
                tags,
                collection_ids,
            }
        })
        .collect();

    let has_more = offset + (assets.len() as i64) < total;
    Ok(AssetPage {
        items: assets,
        total,
        has_more,
    })
}

#[tauri::command]
pub async fn hash_library(state: State<'_, AppState>) -> Result<usize> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || hash_unhashed_assets(&state))
        .await
        .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
pub fn set_assets_excluded(
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

pub fn set_assets_excluded_from_connection(
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
    sync_search_index_for_assets(connection, asset_ids)
}

#[tauri::command]
pub fn set_classification_override(
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
    sync_search_index_for_assets(&connection, &asset_ids)?;
    Ok(snapshots)
}

#[tauri::command]
pub fn reset_classification_override(
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
    for asset_id in &asset_ids {
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
    sync_search_index_for_assets(&connection, &asset_ids)?;
    Ok(snapshots)
}

#[tauri::command]
pub fn restore_classification_overrides(
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
    let asset_ids: Vec<i64> = snapshots.iter().map(|s| s.asset_id).collect();
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
    sync_search_index_for_assets(&connection, &asset_ids)
}

pub fn run_open_command(path: &str, reveal: bool) -> Result<()> {
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
pub fn open_asset(path: String) -> Result<()> {
    run_open_command(&path, false)
}

#[tauri::command]
pub fn reveal_asset(path: String) -> Result<()> {
    run_open_command(&path, true)
}

