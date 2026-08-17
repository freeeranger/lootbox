use rusqlite::{params_from_iter, Connection};

use crate::{
    error::Result,
    models::AssetQuery,
};

pub fn rebuild_search_index(connection: &Connection) -> Result<()> {
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

pub fn sync_search_index_for_assets(connection: &Connection, asset_ids: &[i64]) -> Result<()> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    for chunk in asset_ids.chunks(500) {
        let id_in_clause = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let delete_sql = format!("DELETE FROM assets_fts WHERE asset_id IN ({id_in_clause})");
        connection.execute(&delete_sql, params_from_iter(chunk.iter().copied()))?;

        let insert_sql = format!(
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
            WHERE a.id IN ({id_in_clause}) AND a.is_primary = 1 AND a.excluded = 0 AND a.missing = 0
            GROUP BY a.id
            "#
        );
        connection.execute(&insert_sql, params_from_iter(chunk.iter().copied()))?;
    }
    Ok(())
}

pub fn fts_query(input: &str) -> Option<String> {
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

pub fn asset_query_filter(request: &AssetQuery) -> (String, Vec<rusqlite::types::Value>) {
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

pub fn asset_query_order(request: &AssetQuery) -> String {
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

