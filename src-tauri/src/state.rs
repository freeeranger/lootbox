use std::{
    collections::{HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::AtomicBool,
        Arc, Mutex,
    },
    time::Duration,
};

use rodio::{MixerDeviceSink, Player};
use rusqlite::Connection;
use serde::Serialize;

use crate::{
    db::register_collations,
    error::Result,
    ingest::{rotate_log_if_needed, unix_timestamp},
    models::DiagnosticEntry,
};

#[derive(Clone)]
pub struct AppState {
    pub database_path: PathBuf,
    pub thumbnail_directory: PathBuf,
    pub backup_directory: PathBuf,
    pub diagnostic_log_path: PathBuf,
    pub write_queue: Arc<Mutex<()>>,
    pub import_cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    pub diagnostics: Arc<Mutex<VecDeque<DiagnosticEntry>>>,
    pub hashing_library: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct AudioPlayback {
    pub device: Option<MixerDeviceSink>,
    pub player: Option<Player>,
    pub path: Option<String>,
    pub duration: Duration,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub path: Option<String>,
    pub playing: bool,
    pub position_seconds: f64,
    pub duration_seconds: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAnalysis {
    pub duration_seconds: f64,
    pub peaks: Vec<f32>,
}

pub fn audio_status(playback: &AudioPlayback) -> AudioStatus {
    let player = playback.player.as_ref();
    AudioStatus {
        path: playback.path.clone(),
        playing: player.is_some_and(|player| !player.is_paused() && !player.empty()),
        position_seconds: player
            .map(Player::get_pos)
            .unwrap_or_default()
            .min(playback.duration)
            .as_secs_f64(),
        duration_seconds: playback.duration.as_secs_f64(),
    }
}

impl AppState {
    pub fn connect(&self) -> Result<Connection> {
        let connection = Connection::open(&self.database_path)?;
        register_collations(&connection)?;
        connection.busy_timeout(Duration::from_secs(15))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(connection)
    }

    pub fn record(&self, level: &str, context: &str, message: impl Into<String>) {
        let entry = DiagnosticEntry {
            timestamp: unix_timestamp(),
            level: level.to_string(),
            context: context.to_string(),
            message: message.into(),
        };
        if let Ok(mut entries) = self.diagnostics.lock() {
            entries.push_back(entry.clone());
            while entries.len() > 500 {
                entries.pop_front();
            }
        }
        if let Some(parent) = self.diagnostic_log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if rotate_log_if_needed(&self.diagnostic_log_path).is_ok() {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.diagnostic_log_path)
            {
                let _ = writeln!(
                    file,
                    "{}\t{}\t{}\t{}",
                    entry.timestamp,
                    entry.level,
                    entry.context,
                    entry.message.replace('\n', " ")
                );
            }
        }
    }
}

