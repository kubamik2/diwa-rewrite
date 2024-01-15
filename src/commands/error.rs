use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum VoiceError {
    #[error("manager doesn't exist??")]
    ManagerNone,
    #[error("user not connected to voice")]
    UserNotInVoice
}