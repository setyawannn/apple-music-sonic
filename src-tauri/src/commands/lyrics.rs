//! Lyrics for the currently playing song.
//!
//! The UI only knows the id the hidden player reported, which may be a library id
//! (`i.…`). Lyrics are catalog-scoped, so this command bridges the two namespaces and
//! then fetches Apple's TTML. "No lyrics" is `Ok(None)`, never an error: an
//! instrumental is not a failure.

use crate::api::Lyrics;
use crate::{AppResult, AppState};
use tauri::State;

#[tauri::command]
pub async fn get_lyrics(
    state: State<'_, AppState>,
    song_id: String,
    catalog_id: Option<String>,
) -> AppResult<Option<Lyrics>> {
    // Prefer the catalog id the player already published (bootstrap.js reads it from
    // `playParams.catalogId`), and only pay for a lookup when it is missing.
    let resolved = match catalog_id.filter(|id| !id.trim().is_empty()) {
        Some(id) => Some(id),
        None => state.api_client.resolve_catalog_id(&song_id).await?,
    };

    let Some(catalog_id) = resolved else {
        return Ok(None);
    };

    Ok(state.api_client.get_song_lyrics(&catalog_id).await?)
}
