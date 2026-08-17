use std::{fs::File, path::Path};
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

use crate::{
    error::{LootboxError, Result},
    state::*,
};

pub fn open_audio_decoder(path: &str) -> Result<Decoder<std::io::BufReader<File>>> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "mp3" | "ogg" | "wav") {
        return Err(LootboxError::Audio(
            "Only MP3, OGG, and WAV playback is supported for now".into(),
        ));
    }
    let file = File::open(path)?;
    Decoder::try_from(file).map_err(|error| LootboxError::Audio(error.to_string()))
}

pub fn start_audio(playback: &mut AudioPlayback, path: String) -> Result<()> {
    let decoder = open_audio_decoder(&path)?;
    let duration = decoder.total_duration().unwrap_or_default();
    if playback.device.is_none() {
        playback.device = Some(
            DeviceSinkBuilder::open_default_sink()
                .map_err(|error| LootboxError::Audio(error.to_string()))?,
        );
    }
    if let Some(player) = playback.player.take() {
        player.stop();
    }
    let player = Player::connect_new(
        playback
            .device
            .as_ref()
            .expect("audio device was initialized")
            .mixer(),
    );
    player.append(decoder);
    player.play();
    playback.player = Some(player);
    playback.path = Some(path);
    playback.duration = duration;
    Ok(())
}

