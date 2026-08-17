use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use rusqlite::{params, Connection};

use crate::{
    error::Result,
    ingest::classifier::{classify_extension, model_variant_group},
};

pub fn normalized_texture_token(value: &str) -> String {
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

pub fn texture_map_role(value: &str) -> Option<&'static str> {
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

pub fn is_texture_directory(value: &str) -> bool {
    let token = normalized_texture_token(value);
    matches!(
        token.as_str(),
        "texture" | "textures" | "map" | "maps" | "texture_maps" | "material_maps"
    ) || texture_map_role(&token).is_some()
}

pub fn is_resolution_directory(value: &str) -> bool {
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

pub fn texture_stem_and_role(stem: &str) -> (String, Option<&'static str>) {
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

pub fn texture_resolution(relative_path: &Path) -> Option<String> {
    relative_path.parent()?.components().find_map(|component| {
        let value = component.as_os_str().to_string_lossy();
        is_resolution_directory(&value).then(|| normalized_texture_token(&value))
    })
}

pub fn texture_directory_evidence(relative_path: &Path) -> (Option<&'static str>, bool) {
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

pub fn texture_group_key(relative_path: &Path) -> String {
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

pub fn recompute_texture_groups(connection: &Connection, pack_id: Option<i64>) -> Result<()> {
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

