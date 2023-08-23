use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum VoiceError {
    #[error("manager doesn't exist??")]
    ManagerNone,
    #[error("user not connected to voice")]
    UserNotInVoice,
    #[error("user not connected to a guild voice channel")]
    UserNotInGuildVoice
}
