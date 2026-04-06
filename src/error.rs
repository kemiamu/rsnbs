//! Custom error types

use std::fmt;

#[derive(Debug)]
pub enum Error {
    // InvalidKey(String),
    InvalidVersion(String),
    InvalidPanning(String),
    InvalidVolume(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Error::InvalidKey(key) => {
            //     write!(f, "Invalid key value: {key} (must be 0-127)")
            // }
            Error::InvalidVersion(info) => {
                write!(f, "Invalid or unsupported version: {info}")
            }
            Error::InvalidPanning(panning) => {
                write!(f, "Invalid panning value: {panning} (must be -100 to 100)")
            }
            Error::InvalidVolume(velocity) => {
                write!(f, "Invalid velocity value: {velocity} (must be 0-100)")
            }
            Error::Io(err) => write!(f, "IO error: {err}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
