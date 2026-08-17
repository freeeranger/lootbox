pub mod audio;
pub mod commands;
pub mod db;
pub mod error;
pub mod godot;
pub mod ingest;
pub mod models;
pub mod state;

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, VecDeque},
    fs,
    sync::{
        atomic::AtomicBool,
        Arc, Mutex,
    },
};

use tauri::Manager;

pub use commands::*;
pub use error::*;
pub use models::*;
pub use state::*;

use crate::db::{
    clean_thumbnail_cache_from_connection, create_rotating_backup, initialize_database,
    SCHEMA_VERSION,
};
use crate::ingest::{hash_unhashed_assets, migrate_classification};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_linux_webview();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data_directory)?;
            let database_path = app_data_directory.join("lootbox.db");
            let database_existed = database_path.is_file();
            let state = AppState {
                database_path,
                thumbnail_directory: app_data_directory.join("thumbnails"),
                backup_directory: app_data_directory.join("backups"),
                diagnostic_log_path: app_data_directory.join("logs/lootbox.log"),
                write_queue: Arc::new(Mutex::new(())),
                import_cancellations: Arc::new(Mutex::new(HashMap::new())),
                diagnostics: Arc::new(Mutex::new(VecDeque::new())),
                hashing_library: Arc::new(AtomicBool::new(false)),
            };
            fs::create_dir_all(&state.thumbnail_directory)?;
            let mut connection = state.connect().map_err(|error| error.to_string())?;
            let old_schema_version: i64 = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap_or_default();
            if database_existed && old_schema_version < SCHEMA_VERSION {
                create_rotating_backup(&state, &connection, "before-migration")
                    .map_err(|error| error.to_string())?;
            }
            initialize_database(&connection).map_err(|error| error.to_string())?;
            migrate_classification(&mut connection).map_err(|error| error.to_string())?;
            create_rotating_backup(&state, &connection, "startup")
                .map_err(|error| error.to_string())?;
            clean_thumbnail_cache_from_connection(&state, &connection)
                .map_err(|error| error.to_string())?;
            state.record("info", "startup", "Database initialized and cache checked");

            // Asset protocol access is scoped at runtime. Restore access for cached
            // thumbnails and previously imported packs on every launch.
            let asset_scope = app.asset_protocol_scope();
            asset_scope.allow_directory(&state.thumbnail_directory, true)?;
            let mut statement = connection.prepare("SELECT root_path FROM packs")?;
            let roots = statement.query_map([], |row| row.get::<_, String>(0))?;
            for root in roots.flatten() {
                let _ = asset_scope.allow_directory(root, true);
            }
            drop(statement);
            let hashing_state = state.clone();
            app.manage(state);
            app.manage(Mutex::new(AudioPlayback::default()));
            tauri::async_runtime::spawn_blocking(move || {
                match hash_unhashed_assets(&hashing_state) {
                    Ok(count) if count > 0 => hashing_state.record(
                        "info",
                        "content-hashing",
                        format!("Hashed {count} existing assets"),
                    ),
                    Ok(_) => {}
                    Err(error) => {
                        hashing_state.record("error", "content-hashing", error.to_string())
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import_pack,
            cancel_import,
            save_model_thumbnail,
            get_library_snapshot,
            query_assets,
            query_asset_selections,
            get_filter_options,
            add_tag,
            add_tags,
            remove_tag,
            remove_tags,
            create_collection,
            set_collection_membership,
            set_collection_memberships,
            delete_collection,
            add_godot_project,
            relocate_godot_project,
            remove_project,
            get_project_status,
            preview_assets_to_godot,
            export_assets_to_godot,
            preview_remove_assets_from_godot_project,
            remove_assets_from_godot_project,
            hash_library,
            remove_pack,
            rename_pack,
            set_assets_excluded,
            set_classification_override,
            reset_classification_override,
            restore_classification_overrides,
            purge_missing_assets,
            relocate_pack,
            get_cache_status,
            clean_thumbnail_cache,
            clear_thumbnail_cache,
            regenerate_image_thumbnails,
            create_metadata_backup,
            restore_metadata_backup,
            get_diagnostics,
            log_diagnostic,
            open_asset,
            reveal_asset,
            get_audio_duration,
            get_audio_analysis,
            toggle_audio,
            get_audio_status,
            seek_audio,
            stop_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lootbox");
}

#[cfg(target_os = "linux")]
fn configure_linux_webview() {
    // WebKitGTK's DMABUF renderer currently crashes at startup on some hybrid/NVIDIA
    // Wayland systems. Respect an explicit value so users can re-enable it with `=0`
    // after their driver or WebKit version fixes the upstream issue.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_webview() {}
