//! Process-wide state handed to every Tauri command.

use crate::api::ApiClient;
use crate::audio;
use crate::library::Library;
use crate::{AppError, AppResult};
use std::sync::{Mutex, MutexGuard};

pub struct AppState {
    /// PARKED local library (SQLite). Reachable, not routed to by the shipped UI.
    library: Mutex<Library>,
    /// PARKED local audio engine (symphonia + rodio). Same status.
    pub audio_engine: audio::Engine,
    pub api_client: ApiClient,
}

impl AppState {
    pub fn new(library: Library, audio_engine: audio::Engine, api_client: ApiClient) -> Self {
        Self {
            library: Mutex::new(library),
            audio_engine,
            api_client,
        }
    }

    /// Lock the parked library, mapping mutex poisoning to a typed error.
    ///
    /// Keeping the field private and handing out a guard is what stops every command
    /// from repeating `lock().map_err(|e| e.to_string())`.
    pub fn library(&self) -> AppResult<MutexGuard<'_, Library>> {
        self.library.lock().map_err(AppError::from)
    }
}
