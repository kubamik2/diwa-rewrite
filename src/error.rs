pub type Error = Box<dyn std::error::Error + Send + Sync>;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    #[error("\"{entry}\" Env Variable Missing")]
    MissingEnvEntry { entry: String },
    #[error("{entries:?} Env Variables Missing")]
    MissingEnvEntries { entries: Vec<String> },
    #[error("\"{content}\" Content Type Is Not Supported")]
    UnsupportedContent { content: String },
    #[error("\"{value}\" object is missing")]
    MissingValue { value: String },
    #[error("empty playlist")]
    EmptyPlaylist
}