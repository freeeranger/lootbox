use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    error::{LootboxError, Result},
    ingest::{hash_file, path_string},
    models::*,
};

pub fn project_summary(connection: &Connection, project_id: i64) -> Result<ProjectSummary> {
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

pub fn godot_project_name(root: &Path) -> String {
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

pub fn relocate_godot_project_from_connection(
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

pub fn project_status_from_connection(
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

pub fn safe_export_relative_path(path: &str) -> Result<PathBuf> {
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

pub fn safe_project_component(value: &str) -> String {
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

pub fn collision_export_path(path: &Path, asset_id: i64, attempt: usize) -> PathBuf {
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

pub fn export_destination_conflicts(
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

pub fn safe_collision_destination(
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

pub struct GodotExportSelection {
    pub physical_ids: HashSet<i64>,
    pub grouped_ids: HashSet<i64>,
    pub dependency_ids: HashSet<i64>,
    pub selected: usize,
    pub model_formats: Vec<GodotModelFormat>,
    pub selected_model_formats: Vec<String>,
}

pub fn collect_godot_export_selection(
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

pub fn preview_assets_to_godot_from_connection(
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

pub fn ensure_godot_export_root(root: &Path) -> Result<PathBuf> {
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

pub fn project_path_contains_symlink(root: &Path, path: &Path) -> Result<bool> {
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

pub fn write_godot_manifest(
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

pub fn export_assets_to_godot_from_connection(
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

pub struct GodotProjectRemovalFile {
    pub path: PathBuf,
    pub expected_hash: String,
}

pub struct GodotProjectRemovalPlan {
    pub preview: GodotProjectRemovalPreview,
    pub delete_files: Vec<GodotProjectRemovalFile>,
    pub untrack_ids: Vec<i64>,
}

pub fn tracked_project_export(
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

pub fn safe_tracked_project_path(export_root: &Path, tracked_path: &str) -> Result<(PathBuf, String)> {
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

pub fn project_tracks_dependency_owner(
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

pub fn plan_assets_from_godot_project_removal(
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

pub fn remove_assets_from_godot_project_from_connection(
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

