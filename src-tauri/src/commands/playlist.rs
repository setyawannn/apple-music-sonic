//! Two-way sync for the user's Apple Music library playlists.

use crate::api::{CloudPlaylist, StoreSong, SyncStats};
use crate::{AppResult, AppState};
use tauri::State;

#[tauri::command]
pub async fn get_cloud_playlists(state: State<'_, AppState>) -> AppResult<Vec<CloudPlaylist>> {
    let result = state.api_client.get_user_playlists().await;
    match &result {
        Ok(list) => println!("[PLAYLIST] loaded {} playlists", list.len()),
        Err(e) => eprintln!("[PLAYLIST] error: {}", e),
    }
    Ok(result?)
}

#[tauri::command]
pub async fn get_cloud_playlist_tracks(
    state: State<'_, AppState>,
    playlist_id: String,
) -> AppResult<Vec<StoreSong>> {
    let result = state.api_client.get_playlist_tracks(&playlist_id).await;
    match &result {
        Ok(tracks) => println!("[PLAYLIST] {} tracks in {}", tracks.len(), playlist_id),
        Err(e) => eprintln!("[PLAYLIST] tracks error: {}", e),
    }
    Ok(result?)
}

/// Create a playlist in the user's Apple Music library, optionally pre-filled.
/// Returns the new playlist id.
#[tauri::command]
pub async fn create_cloud_playlist(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
    track_ids: Option<Vec<String>>,
) -> AppResult<String> {
    let id = state
        .api_client
        .create_library_playlist(&name, description.as_deref())
        .await?;
    if let Some(ids) = track_ids {
        if !ids.is_empty() {
            state
                .api_client
                .add_tracks_to_library_playlist(&id, &ids)
                .await?;
        }
    }
    Ok(id)
}

#[tauri::command]
pub async fn rename_cloud_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    name: String,
) -> AppResult<()> {
    Ok(state
        .api_client
        .rename_library_playlist(&playlist_id, &name)
        .await?)
}

#[tauri::command]
pub async fn delete_cloud_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
) -> AppResult<()> {
    Ok(state
        .api_client
        .delete_library_playlist(&playlist_id)
        .await?)
}

#[tauri::command]
pub async fn add_to_cloud_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    track_ids: Vec<String>,
) -> AppResult<()> {
    Ok(state
        .api_client
        .add_tracks_to_library_playlist(&playlist_id, &track_ids)
        .await?)
}

#[tauri::command]
pub async fn sync_cloud_library(state: State<'_, AppState>) -> AppResult<SyncStats> {
    let mut stats = SyncStats::default();

    match state.api_client.get_user_playlists().await {
        Ok(playlists) => {
            stats.playlists_synced = playlists.len();
            for p in &playlists {
                stats.tracks_synced += p.tracks.len();
            }
        }
        Err(e) => {
            stats.error = Some(e.to_string());
        }
    }

    Ok(stats)
}
