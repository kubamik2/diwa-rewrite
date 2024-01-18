pub type DynError = Box<dyn std::error::Error + Send + Sync>;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    #[error("")]
    EnvFile,
    #[error("")]
    EnvVarsMissing { var: Vec<String> }
}