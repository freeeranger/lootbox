use std::{sync::Mutex, time::Duration};
use rodio::Source;
use tauri::State;

use crate::{
    audio::*,
    error::{LootboxError, Result},
    state::*,
};

#[tauri::command]
pub async fn get_audio_duration(path: String) -> Result<f64> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(open_audio_decoder(&path)?
            .total_duration()
            .unwrap_or_default()
            .as_secs_f64())
    })
    .await
    .map_err(|error| LootboxError::Audio(error.to_string()))?
}

#[tauri::command]
pub async fn get_audio_analysis(path: String) -> Result<AudioAnalysis> {
    tauri::async_runtime::spawn_blocking(move || {
        const BUCKETS: usize = 240;
        let decoder = open_audio_decoder(&path)?;
        let duration = decoder.total_duration().unwrap_or_default();
        let channels = decoder.channels().get() as usize;
        let total_frames =
            (duration.as_secs_f64() * f64::from(decoder.sample_rate().get())) as usize;
        let mut peaks = vec![0.0_f32; BUCKETS];
        if total_frames > 0 && channels > 0 {
            for (index, sample) in decoder.enumerate() {
                let frame = index / channels;
                let bucket = (frame.saturating_mul(BUCKETS) / total_frames).min(BUCKETS - 1);
                peaks[bucket] = peaks[bucket].max(sample.abs());
            }
        }
        Ok(AudioAnalysis {
            duration_seconds: duration.as_secs_f64(),
            peaks,
        })
    })
    .await
    .map_err(|error| LootboxError::Audio(error.to_string()))?
}

#[tauri::command]
pub fn toggle_audio(path: String, state: State<'_, Mutex<AudioPlayback>>) -> Result<AudioStatus> {
    let mut playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    let is_current = playback.path.as_deref() == Some(path.as_str());
    if is_current {
        if let Some(player) = playback.player.as_ref() {
            if player.empty() {
                start_audio(&mut playback, path)?;
            } else if player.is_paused() {
                player.play();
            } else {
                player.pause();
            }
        } else {
            start_audio(&mut playback, path)?;
        }
    } else {
        start_audio(&mut playback, path)?;
    }
    Ok(audio_status(&playback))
}

#[tauri::command]
pub fn get_audio_status(state: State<'_, Mutex<AudioPlayback>>) -> Result<AudioStatus> {
    let playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    Ok(audio_status(&playback))
}

#[tauri::command]
pub fn seek_audio(
    path: String,
    position_seconds: f64,
    state: State<'_, Mutex<AudioPlayback>>,
) -> Result<AudioStatus> {
    let mut playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    if playback.path.as_deref() != Some(path.as_str()) || playback.player.is_none() {
        start_audio(&mut playback, path)?;
    }
    if let Some(player) = playback.player.as_ref() {
        player
            .try_seek(Duration::from_secs_f64(position_seconds.max(0.0)))
            .map_err(|error| LootboxError::Audio(error.to_string()))?;
    }
    Ok(audio_status(&playback))
}

#[tauri::command]
pub fn stop_audio(path: String, state: State<'_, Mutex<AudioPlayback>>) -> Result<()> {
    let mut playback = state
        .lock()
        .map_err(|_| LootboxError::Audio("Playback state is unavailable".into()))?;
    if playback.path.as_deref() == Some(path.as_str()) {
        if let Some(player) = playback.player.take() {
            player.stop();
        }
        playback.path = None;
        playback.duration = Duration::ZERO;
    }
    Ok(())
}

