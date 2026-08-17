use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum LootboxError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("The selected folder does not exist or is not a directory")]
    InvalidDirectory,
    #[error("Could not open this item with the operating system")]
    OpenFailed,
    #[error("The import worker stopped unexpectedly")]
    ImportWorker,
    #[error("Import cancelled")]
    ImportCancelled,
    #[error("Invalid thumbnail data")]
    InvalidThumbnail,
    #[error("Audio error: {0}")]
    Audio(String),
    #[error("Pack name cannot be empty")]
    InvalidPackName,
    #[error("That folder does not match this pack: {0}")]
    InvalidPackLocation(String),
    #[error("Invalid backup: {0}")]
    InvalidBackup(String),
    #[error("Invalid Godot project: {0}")]
    InvalidGodotProject(String),
    #[error("Project export failed: {0}")]
    ProjectExport(String),
}

impl Serialize for LootboxError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LootboxError>;

