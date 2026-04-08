//! Custom error types

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // InvalidKey(String),
    #[error("Invalid or unsupported version: {0}")]
    InvalidVersion(String),
    #[error("Invalid panning value: {0} (must be -100 to 100)")]
    InvalidPanning(String),
    #[error("Invalid velocity value: {0} (must be 0-100)")]
    InvalidVolume(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Integer conversion error: {0}")]
    TryFromIntError(#[from] std::num::TryFromIntError),
}

pub type Result<T> = std::result::Result<T, Error>;
