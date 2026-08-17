use std::fs;
use rusqlite::params;
use tauri::State;

use crate::{
    error::{LootboxError, Result},
    godot::*,
    ingest::path_string,
    models::*,
    state::AppState,
};

#[tauri::command]
pub fn add_godot_project(path: String, state: State<'_, AppState>) -> Result<ProjectSummary> {
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
pub fn relocate_godot_project(
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

#[tauri::command]
pub fn remove_project(project_id: i64, state: State<'_, AppState>) -> Result<()> {
    let _guard = state
        .write_queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state
        .connect()?
        .execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
    Ok(())
}

#[tauri::command]
pub async fn get_project_status(project_id: i64, state: State<'_, AppState>) -> Result<ProjectStatus> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = state.connect()?;
        project_status_from_connection(&connection, project_id)
    })
    .await
    .map_err(|_| LootboxError::ImportWorker)?
}

#[tauri::command]
pub async fn preview_assets_to_godot(
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
pub async fn export_assets_to_godot(
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
pub async fn preview_remove_assets_from_godot_project(
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
pub async fn remove_assets_from_godot_project(
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

