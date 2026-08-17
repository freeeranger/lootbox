use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::Path,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    db::rebuild_search_index,
    error::Result,
    ingest::textures::*,
};

pub fn classify_extension(extension: &str) -> &'static str {
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

pub fn classify_asset_type(relative_path: &Path, extension: &str) -> &'static str {
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

pub fn is_model_format_directory(name: &str) -> bool {
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

pub fn model_variant_group(relative_path: &Path, asset_type: &str, extension: &str) -> Option<String> {
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

pub fn recompute_primary_assets(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
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

pub fn apply_classification_overrides(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
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

pub fn resource_name(value: &str) -> Option<String> {
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

pub fn extract_resource_names(path: &Path, extension: &str) -> HashSet<String> {
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

pub fn recompute_asset_dependencies(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
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

pub fn migrate_classification(connection: &mut Connection) -> Result<()> {
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

pub fn image_dimensions(path: &Path, asset_type: &str) -> (Option<i64>, Option<i64>) {
    if asset_type != "image" && asset_type != "texture" {
        return (None, None);
    }
    imagesize::size(path)
        .map(|size| (Some(size.width as i64), Some(size.height as i64)))
        .unwrap_or((None, None))
}

pub fn parse_gltf_json(json_bytes: &[u8]) -> Option<(i64, i64)> {
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

pub fn glb_poly_count(path: &Path) -> Option<(i64, i64)> {
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

pub fn gltf_poly_count(path: &Path) -> Option<(i64, i64)> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    let reader = BufReader::new(file);
    let val: serde_json::Value = serde_json::from_reader(reader).ok()?;
    let json_bytes = serde_json::to_vec(&val).ok()?;
    parse_gltf_json(&json_bytes)
}

pub fn obj_poly_count(path: &Path) -> Option<(i64, i64)> {
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

pub fn model_poly_count(path: &Path, extension: &str) -> (Option<i64>, Option<i64>) {
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

