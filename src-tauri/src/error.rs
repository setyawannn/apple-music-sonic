//! Typed error handling for the command surface.
//!
//! Every command returns [`AppResult`], never a bare `String`. Internally the error is a
//! typed enum so call sites can describe *what* failed; at the IPC boundary it is
//! serialized as a plain message, which is what the frontend already displays.

use serde::{Serialize, Serializer};
use std::sync::PoisonError;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    /// Any internal failure carrying context: parked-library SQL, Apple Music REST calls,
    /// DRM bridge loading. The contained `anyhow` error already names the operation.
    #[error("{0}")]
    Internal(#[from] anyhow::Error),

    /// The hidden MusicKit webview could not be created, driven, or read.
    #[error("player unavailable: {0}")]
    Player(String),

    /// A shared `Mutex` was poisoned by a panicking holder.
    #[error("internal state lock was poisoned")]
    Poisoned,

    /// The PARKED local audio engine (symphonia/rodio) reports plain strings.
    #[error("audio engine: {0}")]
    Audio(String),

    /// The ABANDONED native DRM bridge reports plain strings.
    #[error("native bridge: {0}")]
    Native(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

impl<T> From<PoisonError<T>> for AppError {
    fn from(_: PoisonError<T>) -> Self {
        Self::Poisoned
    }
}

impl AppError {
    /// Stable machine-readable discriminant, handy for logging and future UI branching.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Internal(_) => "internal",
            Self::Player(_) => "player",
            Self::Poisoned => "poisoned",
            Self::Audio(_) => "audio",
            Self::Native(_) => "native",
            Self::InvalidInput(_) => "invalid_input",
            Self::Io(_) => "io",
            Self::Json(_) => "json",
            Self::Tauri(_) => "tauri",
        }
    }
}

/// Tauri serializes command errors for the frontend. The UI shows a toast with the
/// message, so serialize a message string rather than a struct (`String(err)` in JS
/// would otherwise render as `[object Object]`).
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_a_plain_message() {
        let err = AppError::Player("window closed".into());
        assert_eq!(
            serde_json::to_string(&err).unwrap(),
            "\"player unavailable: window closed\""
        );
    }

    #[test]
    fn poison_error_has_a_readable_code() {
        let err = AppError::Poisoned;
        assert_eq!(err.code(), "poisoned");
        assert_eq!(err.to_string(), "internal state lock was poisoned");
    }

    #[test]
    fn io_errors_convert_without_boilerplate() {
        let err = AppError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "nope"));
        assert_eq!(err.code(), "io");
    }
}
