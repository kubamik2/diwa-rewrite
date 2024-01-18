use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum VoiceError {
    #[error("")]
    NoManager,
    #[error("You are not connected to a voice channel")]
    NotConnected,
    #[error("You are connected to a different voice channel")]
    DifferentVoiceChannel,
    #[error("Couldn't join the voice channel")]
    Join(songbird::error::JoinError)
}

#[derive(Debug, ThisError)]
pub enum CommandError {
    #[error("{0}")]
    Voice(#[from] VoiceError),
    #[error("")]
    Input(#[from] songbird::input::error::Error),
    #[error("")]
    Serenity(#[from] serenity::Error),
    #[error("")]
    Track(#[from] songbird::error::TrackError),
    #[error("{0}")]
    Conversion(#[from] crate::convert_query::ConversionError),
    #[error("")]
    EmptyPlaylist,
    #[error("{0}")]
    Metadata(#[from] crate::metadata::MetadataError),
    #[error("Invalid query")]
    InvalidQuery,
}

impl From<songbird::error::JoinError> for CommandError {
    fn from(value: songbird::error::JoinError) -> Self {
        Self::Voice(VoiceError::Join(value))
    }
}