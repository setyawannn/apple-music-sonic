//! PARKED local-file commands (symphonia/rodio engine + SQLite library).
//!
//! Kept registered so the parked modules stay reachable, but no shipped UI routes here:
//! streaming goes through `crate::player`. Do not extend this file — see
//! `docs/03-implementation/04-PARKED-LOCAL-PLAYBACK.md`.

use crate::audio::PlaybackStatus;
use crate::library::{Album, Artist, Playlist, Track};
use crate::{AppError, AppResult, AppState};
use tauri::State;

// ─── Library Commands ───────────────────────────────────────────────

#[tauri::command]
pub fn scan_library(state: State<AppState>) -> AppResult<usize> {
    let lib = state.library()?;
    lib.scan_music_folder()?;
    Ok(lib.track_count())
}

#[tauri::command]
pub fn get_albums(state: State<AppState>) -> AppResult<Vec<Album>> {
    let lib = state.library()?;
    Ok(lib.albums())
}

#[tauri::command]
pub fn get_tracks(state: State<AppState>) -> AppResult<Vec<Track>> {
    let lib = state.library()?;
    Ok(lib.tracks())
}

#[tauri::command]
pub fn get_artists(state: State<AppState>) -> AppResult<Vec<Artist>> {
    let lib = state.library()?;
    Ok(lib.artists())
}

#[tauri::command]
pub fn get_playlists(state: State<AppState>) -> AppResult<Vec<Playlist>> {
    let lib = state.library()?;
    Ok(lib.playlists())
}

#[tauri::command]
pub fn get_track_count(state: State<AppState>) -> AppResult<usize> {
    let lib = state.library()?;
    Ok(lib.track_count())
}

#[tauri::command]
pub fn create_playlist(
    state: State<AppState>,
    name: String,
    description: String,
) -> AppResult<Playlist> {
    let lib = state.library()?;
    Ok(lib.create_playlist(&name, &description)?)
}

#[tauri::command]
pub fn add_to_playlist(
    state: State<AppState>,
    playlist_id: String,
    track_id: String,
) -> AppResult<()> {
    let lib = state.library()?;
    Ok(lib.add_track_to_playlist(&playlist_id, &track_id)?)
}

#[tauri::command]
pub fn remove_from_playlist(
    state: State<AppState>,
    playlist_id: String,
    track_id: String,
) -> AppResult<()> {
    let lib = state.library()?;
    Ok(lib.remove_track_from_playlist(&playlist_id, &track_id)?)
}

#[tauri::command]
pub fn delete_playlist(state: State<AppState>, playlist_id: String) -> AppResult<()> {
    let lib = state.library()?;
    Ok(lib.delete_playlist(&playlist_id)?)
}

#[tauri::command]
pub fn import_playlist(state: State<AppState>, file_path: String) -> AppResult<Playlist> {
    let lib = state.library()?;
    Ok(lib.import_playlist_file(&file_path)?)
}

#[tauri::command]
pub fn export_playlist(
    state: State<AppState>,
    playlist_id: String,
    out_path: String,
) -> AppResult<()> {
    let lib = state.library()?;
    Ok(lib.export_playlist_file(&playlist_id, &out_path)?)
}

// ─── Playback Commands ──────────────────────────────────────────────

#[tauri::command]
pub fn play_track(state: State<AppState>, track_id: String) -> AppResult<()> {
    let lib = state.library()?;
    let all_tracks = lib.tracks();
    let track = all_tracks
        .iter()
        .find(|t| t.id == track_id)
        .cloned()
        .ok_or_else(|| {
            AppError::InvalidInput(format!("track '{}' is not in the local library", track_id))
        })?;

    {
        let mut queue = state.audio_engine.queue.lock()?;
        if queue.is_empty() {
            let idx = all_tracks.iter().position(|t| t.id == track_id);
            queue.set_queue(all_tracks, idx);
        } else {
            queue.play_track(track.clone());
        }
    }

    state
        .audio_engine
        .play_track_file(
            std::path::PathBuf::from(&track.file_path),
            track.id,
            track.title,
            track.artist,
            track.duration_secs,
        )
        .map_err(AppError::Audio)
}

#[tauri::command]
pub fn play_album(state: State<AppState>, album_id: String) -> AppResult<()> {
    let lib = state.library()?;
    let albums = lib.albums();
    let album = albums.iter().find(|a| a.id == album_id).ok_or_else(|| {
        AppError::InvalidInput(format!("album '{}' is not in the local library", album_id))
    })?;

    if album.tracks.is_empty() {
        return Err(AppError::InvalidInput("album contains no tracks".into()));
    }

    let first_track = album.tracks[0].clone();

    {
        let mut queue = state.audio_engine.queue.lock()?;
        queue.set_queue(album.tracks.clone(), Some(0));
    }

    state
        .audio_engine
        .play_track_file(
            std::path::PathBuf::from(&first_track.file_path),
            first_track.id,
            first_track.title,
            first_track.artist,
            first_track.duration_secs,
        )
        .map_err(AppError::Audio)
}

#[tauri::command]
pub fn pause_playback(state: State<AppState>) -> AppResult<()> {
    state.audio_engine.pause().map_err(AppError::Audio)
}

#[tauri::command]
pub fn resume_playback(state: State<AppState>) -> AppResult<()> {
    state.audio_engine.resume().map_err(AppError::Audio)
}

#[tauri::command]
pub fn stop_playback(state: State<AppState>) -> AppResult<()> {
    state.audio_engine.stop().map_err(AppError::Audio)
}

#[tauri::command]
pub fn seek_to(state: State<AppState>, position_secs: f64) -> AppResult<()> {
    state
        .audio_engine
        .seek(position_secs)
        .map_err(AppError::Audio)
}

#[tauri::command]
pub fn set_volume(state: State<AppState>, volume: f32) -> AppResult<()> {
    state
        .audio_engine
        .set_volume(volume)
        .map_err(AppError::Audio)
}

#[tauri::command]
pub fn get_playback_status(state: State<AppState>) -> AppResult<PlaybackStatus> {
    let status = state.audio_engine.status.lock()?;
    Ok(status.clone())
}

#[tauri::command]
pub fn next_track(state: State<AppState>) -> AppResult<Option<String>> {
    let next = {
        let mut queue = state.audio_engine.queue.lock()?;
        queue.next()
    };

    if let Some(track) = next {
        state
            .audio_engine
            .play_track_file(
                std::path::PathBuf::from(&track.file_path),
                track.id.clone(),
                track.title,
                track.artist,
                track.duration_secs,
            )
            .map_err(AppError::Audio)?;
        Ok(Some(track.id))
    } else {
        state.audio_engine.stop().map_err(AppError::Audio)?;
        Ok(None)
    }
}

#[tauri::command]
pub fn prev_track(state: State<AppState>) -> AppResult<Option<String>> {
    let prev = {
        let mut queue = state.audio_engine.queue.lock()?;
        queue.prev()
    };

    if let Some(track) = prev {
        state
            .audio_engine
            .play_track_file(
                std::path::PathBuf::from(&track.file_path),
                track.id.clone(),
                track.title,
                track.artist,
                track.duration_secs,
            )
            .map_err(AppError::Audio)?;
        Ok(Some(track.id))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn set_repeat_mode(state: State<AppState>, mode: String) -> AppResult<()> {
    let repeat = match mode.to_lowercase().as_str() {
        "one" => crate::audio::queue::RepeatMode::One,
        "all" => crate::audio::queue::RepeatMode::All,
        _ => crate::audio::queue::RepeatMode::Off,
    };
    let mut queue = state.audio_engine.queue.lock()?;
    queue.set_repeat_mode(repeat);
    let mut status = state.audio_engine.status.lock()?;
    status.repeat_mode = repeat;
    Ok(())
}

#[tauri::command]
pub fn set_shuffle(state: State<AppState>, enable: bool) -> AppResult<()> {
    let mut queue = state.audio_engine.queue.lock()?;
    queue.set_shuffle(enable);
    let mut status = state.audio_engine.status.lock()?;
    status.is_shuffle = enable;
    Ok(())
}

#[tauri::command]
pub async fn play_stream_track(
    state: State<'_, AppState>,
    track_id: String,
    stream_url: String,
    title: String,
    artist: String,
    duration_secs: Option<f64>,
) -> AppResult<()> {
    state
        .audio_engine
        .play_stream_url(
            track_id,
            stream_url,
            title,
            artist,
            duration_secs.unwrap_or(30.0),
        )
        .await
        .map_err(AppError::Audio)
}
