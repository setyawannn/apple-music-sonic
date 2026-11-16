//! Apple Music RS — Tauri command surface and application lifecycle.
//!
//! Playback is hosted, not reimplemented: a hidden WebView2 runs Apple's own MusicKit
//! player on `music.apple.com` and Windows PlayReady handles DRM. See
//! `docs/02-architecture/HOW-IT-WORKS.md`.

pub mod api;
pub mod audio;
pub mod commands;
pub mod drm;
pub mod error;
pub mod library;
pub mod player;
pub mod state;

use api::{ApiClient, AuthManager};
use std::sync::Arc;
use tauri::Manager;

pub use error::{AppError, AppResult};
pub use state::AppState;

use commands::{auth, catalog, diagnostics, local, lyrics, playlist};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let auth_mgr = Arc::new(AuthManager::new(AuthManager::default_path()));
    let api_client = ApiClient::new(auth_mgr);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .setup(|app| {
            match app.get_webview_window("main") {
                Some(main_window) => {
                    let mw = main_window.clone();
                    if let Err(e) = mw.show() {
                        eprintln!("[SETUP] Failed to show window: {e:?}");
                    }
                    let _ = mw.unminimize();
                    let _ = mw.set_focus();

                    // Forward the hidden player's state to the UI, then pre-warm the
                    // player: paying the page load here, while the user is browsing,
                    // is what keeps the first click instant.
                    player::start_state_poller(app.handle().clone());
                    if let Err(e) = player::ensure_player(app.handle()) {
                        eprintln!("[SETUP] player pre-warm failed: {e}");
                    }

                    // Media keys are routed by the OS, so this is registered once at
                    // startup; a refused registration (another app owns them) is not fatal.
                    if let Err(e) = player::register_media_keys(app.handle()) {
                        eprintln!("[SETUP] media keys unavailable: {e}");
                    }

                    // The hidden player is a real window, so Tauri would keep the app
                    // alive after the main window closes. Shut down explicitly instead.
                    let quitting = mw.clone();
                    mw.on_window_event(move |event| {
                        if let tauri::WindowEvent::Destroyed = event {
                            quitting.app_handle().exit(0);
                        }
                    });
                }
                None => eprintln!("[SETUP] ERROR: no window labelled 'main'"),
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let _ = state
                        .api_client
                        .auth
                        .refresh_developer_token_from_web()
                        .await;
                }
            });

            Ok(())
        })
        .manage(AppState::new(
            library::Library::new(),
            audio::Engine::new(),
            api_client,
        ))
        .invoke_handler(tauri::generate_handler![
            // Streaming player
            player::stream_play,
            player::stream_pause,
            player::stream_resume,
            player::stream_stop,
            player::stream_seek,
            player::stream_next,
            player::stream_previous,
            player::stream_shuffle,
            player::stream_repeat,
            player::stream_volume,
            player::get_last_player_state,
            player::stream_jump_to,
            // Wallet: the parked local engine, kept reachable
            local::scan_library,
            local::get_albums,
            local::get_tracks,
            local::get_artists,
            local::get_playlists,
            local::get_track_count,
            local::create_playlist,
            local::add_to_playlist,
            local::remove_from_playlist,
            local::delete_playlist,
            local::import_playlist,
            local::export_playlist,
            local::play_track,
            local::play_album,
            local::pause_playback,
            local::resume_playback,
            local::stop_playback,
            local::seek_to,
            local::set_volume,
            local::get_playback_status,
            local::next_track,
            local::prev_track,
            local::set_repeat_mode,
            local::set_shuffle,
            local::play_stream_track,
            // Apple Music API & auth
            auth::start_apple_sign_in,
            auth::set_user_session,
            auth::sign_out,
            auth::get_auth_state,
            auth::set_storefront,
            // Catalog browse, search, iCloud library
            catalog::search_catalog,
            catalog::get_search_suggestions,
            catalog::get_catalog_album,
            catalog::get_charts,
            catalog::get_personal_library,
            catalog::get_recommendations,
            // Cloud playlists (read + write)
            playlist::get_cloud_playlists,
            playlist::get_cloud_playlist_tracks,
            playlist::sync_cloud_library,
            playlist::create_cloud_playlist,
            playlist::rename_cloud_playlist,
            playlist::delete_cloud_playlist,
            playlist::add_to_cloud_playlist,
            // Lyrics for the playing song
            lyrics::get_lyrics,
            // Native-bridge diagnostics (abandoned path)
            diagnostics::get_native_bridge_status,
            diagnostics::import_local_apple_session,
            diagnostics::run_native_bridge_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Tauri application");
}
