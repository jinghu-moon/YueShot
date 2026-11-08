use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid screenshot artifact: {0}")]
    Invalid(String),
    #[error("screenshot history database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("screenshot history data is invalid: {0}")]
    Data(String),
}
