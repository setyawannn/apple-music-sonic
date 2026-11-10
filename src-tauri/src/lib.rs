pub mod audio;
pub mod library;
pub mod api;
pub mod drm;

use api::{ApiClient, AuthManager, AuthState, CatalogCharts, CatalogSearchResult, CloudPlaylist, SearchSuggestions, StoreAlbum, StoreSong, SyncStats};
use audio::PlaybackStatus;
use library::{Album, Artist, Playlist, Track};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

/// Application state shared across all commands
pub struct AppState {
    pub library: Mutex<library::Library>,
    pub audio_engine: audio::Engine,
    pub api_client: ApiClient,
}

// ─── Library Commands ───────────────────────────────────────────────

#[tauri::command]
fn scan_library(state: State<AppState>) -> Result<usize, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.scan_music_folder().map_err(|e| e.to_string())?;
    Ok(lib.track_count())
}

#[tauri::command]
fn get_albums(state: State<AppState>) -> Result<Vec<Album>, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    Ok(lib.albums())
}

#[tauri::command]
fn get_tracks(state: State<AppState>) -> Result<Vec<Track>, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    Ok(lib.tracks())
}

#[tauri::command]
fn get_artists(state: State<AppState>) -> Result<Vec<Artist>, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    Ok(lib.artists())
}

#[tauri::command]
fn get_playlists(state: State<AppState>) -> Result<Vec<Playlist>, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    Ok(lib.playlists())
}

#[tauri::command]
fn get_track_count(state: State<AppState>) -> Result<usize, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    Ok(lib.track_count())
}

#[tauri::command]
fn create_playlist(state: State<AppState>, name: String, description: String) -> Result<Playlist, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.create_playlist(&name, &description).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_to_playlist(state: State<AppState>, playlist_id: String, track_id: String) -> Result<(), String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.add_track_to_playlist(&playlist_id, &track_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_from_playlist(state: State<AppState>, playlist_id: String, track_id: String) -> Result<(), String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.remove_track_from_playlist(&playlist_id, &track_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_playlist(state: State<AppState>, playlist_id: String) -> Result<(), String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.delete_playlist(&playlist_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn import_playlist(state: State<AppState>, file_path: String) -> Result<Playlist, String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.import_playlist_file(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_playlist(state: State<AppState>, playlist_id: String, out_path: String) -> Result<(), String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    lib.export_playlist_file(&playlist_id, &out_path).map_err(|e| e.to_string())
}

// ─── Playback Commands ──────────────────────────────────────────────

#[tauri::command]
fn play_track(state: State<AppState>, track_id: String) -> Result<(), String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    let all_tracks = lib.tracks();
    let track = all_tracks
        .iter()
        .find(|t| t.id == track_id)
        .cloned()
        .ok_or_else(|| format!("Track with id '{}' not found in library", track_id))?;

    {
        let mut queue = state.audio_engine.queue.lock().map_err(|e| e.to_string())?;
        if queue.is_empty() {
            let idx = all_tracks.iter().position(|t| t.id == track_id);
            queue.set_queue(all_tracks, idx);
        } else {
            queue.play_track(track.clone());
        }
    }

    state.audio_engine.play_track_file(
        std::path::PathBuf::from(&track.file_path),
        track.id,
        track.title,
        track.artist,
        track.duration_secs,
    )
}

#[tauri::command]
fn play_album(state: State<AppState>, album_id: String) -> Result<(), String> {
    let lib = state.library.lock().map_err(|e| e.to_string())?;
    let albums = lib.albums();
    let album = albums
        .iter()
        .find(|a| a.id == album_id)
        .ok_or_else(|| format!("Album with id '{}' not found", album_id))?;

    if album.tracks.is_empty() {
        return Err("Album contains no tracks".to_string());
    }

    let first_track = album.tracks[0].clone();

    {
        let mut queue = state.audio_engine.queue.lock().map_err(|e| e.to_string())?;
        queue.set_queue(album.tracks.clone(), Some(0));
    }

    state.audio_engine.play_track_file(
        std::path::PathBuf::from(&first_track.file_path),
        first_track.id,
        first_track.title,
        first_track.artist,
        first_track.duration_secs,
    )
}

#[tauri::command]
fn pause_playback(state: State<AppState>) -> Result<(), String> {
    state.audio_engine.pause()
}

#[tauri::command]
fn resume_playback(state: State<AppState>) -> Result<(), String> {
    state.audio_engine.resume()
}

#[tauri::command]
fn stop_playback(state: State<AppState>) -> Result<(), String> {
    state.audio_engine.stop()
}

#[tauri::command]
fn seek_to(state: State<AppState>, position_secs: f64) -> Result<(), String> {
    state.audio_engine.seek(position_secs)
}

#[tauri::command]
fn set_volume(state: State<AppState>, volume: f32) -> Result<(), String> {
    state.audio_engine.set_volume(volume)
}

#[tauri::command]
fn get_playback_status(state: State<AppState>) -> Result<PlaybackStatus, String> {
    let status = state.audio_engine.status.lock().map_err(|e| e.to_string())?;
    Ok(status.clone())
}

#[tauri::command]
fn next_track(state: State<AppState>) -> Result<Option<String>, String> {
    let next = {
        let mut queue = state.audio_engine.queue.lock().map_err(|e| e.to_string())?;
        queue.next()
    };

    if let Some(track) = next {
        state.audio_engine.play_track_file(
            std::path::PathBuf::from(&track.file_path),
            track.id.clone(),
            track.title,
            track.artist,
            track.duration_secs,
        )?;
        Ok(Some(track.id))
    } else {
        state.audio_engine.stop()?;
        Ok(None)
    }
}

#[tauri::command]
fn prev_track(state: State<AppState>) -> Result<Option<String>, String> {
    let prev = {
        let mut queue = state.audio_engine.queue.lock().map_err(|e| e.to_string())?;
        queue.prev()
    };

    if let Some(track) = prev {
        state.audio_engine.play_track_file(
            std::path::PathBuf::from(&track.file_path),
            track.id.clone(),
            track.title,
            track.artist,
            track.duration_secs,
        )?;
        Ok(Some(track.id))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn set_repeat_mode(state: State<AppState>, mode: String) -> Result<(), String> {
    let repeat = match mode.to_lowercase().as_str() {
        "one" => audio::queue::RepeatMode::One,
        "all" => audio::queue::RepeatMode::All,
        _ => audio::queue::RepeatMode::Off,
    };
    let mut queue = state.audio_engine.queue.lock().map_err(|e| e.to_string())?;
    queue.set_repeat_mode(repeat);
    let mut status = state.audio_engine.status.lock().map_err(|e| e.to_string())?;
    status.repeat_mode = repeat;
    Ok(())
}

#[tauri::command]
fn set_shuffle(state: State<AppState>, enable: bool) -> Result<(), String> {
    let mut queue = state.audio_engine.queue.lock().map_err(|e| e.to_string())?;
    queue.set_shuffle(enable);
    let mut status = state.audio_engine.status.lock().map_err(|e| e.to_string())?;
    status.is_shuffle = enable;
    Ok(())
}

// ─── Apple Music API & Auth Commands ────────────────────────────────

/// Ask Apple whether a Music-User-Token is actually live.
/// A token can be non-empty and well-formed yet already revoked, which is exactly
/// what MusicKit hands back from a cached (expired) web session.
async fn validate_music_user_token(dev_token: &str, user_token: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    else {
        return false;
    };

    match client
        .get("https://amp-api.music.apple.com/v1/me/library/songs?limit=1")
        .header("Authorization", format!("Bearer {}", dev_token))
        .header("Music-User-Token", user_token)
        .header("Origin", "https://music.apple.com")
        .header("Referer", "https://music.apple.com/")
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

#[tauri::command]
fn probe_report(data: String) {
    println!("[PROBE] {}", data);
}

// ─── Full-song streaming player ─────────────────────────────────────
//
// The license challenge Apple requires can only be produced by a PlayReady CDM,
// so decryption cannot be reimplemented in Rust. Instead we host Apple's own
// MusicKit player in a hidden WebView2 that shares the signed-in auth profile.
// Windows' PlayReady CDM does the DRM; we only drive the player and read state.

const PLAYER_LABEL: &str = "player";
const PLAYER_STATE_PORT: u16 = 38471;

/// Last state blob seen on the loopback channel, to log transitions only.
static LAST_PLAYER_STATE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

const PLAYER_BOOTSTRAP: &str = r#"
(function () {
  var PORT = 38471;
  window.__mkReady = new Promise(function (res) { window.__mkResolve = res; });
  function got() {
    try {
      if (window.MusicKit && typeof window.MusicKit.getInstance === 'function') {
        var m = window.MusicKit.getInstance();
        if (m) { window.__mkResolve(m); return true; }
      }
    } catch (e) {}
    return false;
  }
  if (!got()) {
    var tries = 0;
    var t = setInterval(function () {
      tries += 1;
      if (got() || tries > 200) clearInterval(t);
    }, 300);
  }
  window.__mkDo = async function (cmd, arg, nonce) {
    // Each user action carries a nonce; the retry burst re-sends the same nonce,
    // so only the first attempt runs. A fresh click gets a fresh nonce.
    if (nonce) {
      if (window.__mkNonce === nonce) return;
      window.__mkNonce = nonce;
    }
    var m = await window.__mkReady;
    if (cmd === 'play') {
      await m.setQueue({ song: String(arg) });
      return m.play();
    }
    if (cmd === 'pause') return m.pause();
    if (cmd === 'resume') return m.play();
    if (cmd === 'stop') return m.stop();
    if (cmd === 'seek') return m.seekToTime(Number(arg));
  };
  function report() {
    try {
      var m = window.MusicKit.getInstance();
      var item = m.nowPlayingItem || null;
      var attrs = (item && item.attributes) || null;
      var body = JSON.stringify({
        playbackState: m.playbackState,
        isPlaying: !!m.isPlaying,
        position: m.currentPlaybackTime || 0,
        duration: m.currentPlaybackDuration || 0,
        id: item && item.id ? item.id : null,
        title: attrs ? attrs.name : null,
        artist: attrs ? attrs.artistName : null,
        artwork: attrs && attrs.artwork ? attrs.artwork.url : null
      });
      fetch('http://127.0.0.1:' + PORT + '/state', { method: 'POST', mode: 'no-cors', body: body }).catch(function () {});
    } catch (e) {}
  }
  setInterval(report, 1000);
})();
"#;

/// Create (once) the hidden playback webview, sharing the signed-in profile so
/// Apple's page is already authenticated and its PlayReady CDM can licence audio.
fn ensure_player(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(PLAYER_LABEL) {
        return Ok(w);
    }

    let profile = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("auth_profile");
    let _ = std::fs::create_dir_all(&profile);

    let url: tauri::Url = "https://music.apple.com/id/"
        .parse()
        .map_err(|e: <tauri::Url as std::str::FromStr>::Err| e.to_string())?;

    tauri::WebviewWindowBuilder::new(app, PLAYER_LABEL, tauri::WebviewUrl::External(url))
        .title("Apple Music Player")
        .visible(false)
        .inner_size(480.0, 320.0)
        .initialization_script(PLAYER_BOOTSTRAP)
        .additional_browser_args(
            "--autoplay-policy=no-user-gesture-required \
             --disable-background-timer-throttling \
             --disable-renderer-backgrounding \
             --disable-backgrounding-occluded-windows",
        )
        .data_directory(profile)
        .build()
        .map_err(|e| e.to_string())
}

/// Drive the hidden player. Repeats a few times because the page may still be
/// loading when the first command arrives; the bootstrap de-duplicates plays.
async fn player_command(app: &tauri::AppHandle, cmd: &str, arg: Option<String>) -> Result<(), String> {
    let window = ensure_player(app)?;
    let has_arg = arg.is_some();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let js = match arg {
        Some(a) => format!(
            "if (window.__mkDo) {{ window.__mkDo('{}', '{}', '{}').catch(function(e){{console.error('mk', e);}}); }}",
            cmd, a, nonce
        ),
        None => format!(
            "if (window.__mkDo) {{ window.__mkDo('{}', null, '{}').catch(function(e){{console.error('mk', e);}}); }}",
            cmd, nonce
        ),
    };
    println!("[PLAYER] cmd={} has_arg={} nonce={}", cmd, has_arg, nonce);

    for attempt in 0..6 {
        if app.get_webview_window(PLAYER_LABEL).is_none() {
            println!("[PLAYER] window vanished at attempt {}", attempt);
            return Err("player window closed".to_string());
        }
        if let Err(e) = window.eval(&js) {
            println!("[PLAYER] eval failed at attempt {}: {}", attempt, e);
            return Err(e.to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    }
    Ok(())
}

#[tauri::command]
async fn stream_play(app: tauri::AppHandle, song_id: String) -> Result<(), String> {
    player_command(&app, "play", Some(song_id)).await
}

#[tauri::command]
async fn stream_pause(app: tauri::AppHandle) -> Result<(), String> {
    player_command(&app, "pause", None).await
}

#[tauri::command]
async fn stream_resume(app: tauri::AppHandle) -> Result<(), String> {
    player_command(&app, "resume", None).await
}

#[tauri::command]
async fn stream_stop(app: tauri::AppHandle) -> Result<(), String> {
    player_command(&app, "stop", None).await
}

#[tauri::command]
async fn stream_seek(app: tauri::AppHandle, position: f64) -> Result<(), String> {
    player_command(&app, "seek", Some(position.to_string())).await
}

/// Tiny loopback endpoint the hidden player POSTs its state to. Tauri's IPC is
/// unavailable on remote origins, so this is the only reliable channel back.
fn start_state_listener(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PLAYER_STATE_PORT)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[PLAYER] state listener bind failed on {}: {}", PLAYER_STATE_PORT, e);
                return;
            }
        };
        println!("[PLAYER] state listener on 127.0.0.1:{}", PLAYER_STATE_PORT);

        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                continue;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 16384];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                if let Some(idx) = request.find("\r\n\r\n") {
                    let body = request[idx + 4..].trim().to_string();
                    if !body.is_empty() {
                        {
                            let mut last = LAST_PLAYER_STATE.lock().unwrap();
                            if last.as_deref() != Some(body.as_str()) {
                                println!("[PLAYER] state {}", body);
                                *last = Some(body.clone());
                            }
                        }
                        let _ = app.emit("player-state", body);
                    }
                }
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            });
        }
    });
}

#[tauri::command]
async fn start_apple_sign_in(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("apple_auth") {
        let _ = existing.set_focus();
        return Ok(());
    }

    let url: tauri::Url = "https://music.apple.com/login"
        .parse()
        .map_err(|e: <tauri::Url as std::str::FromStr>::Err| e.to_string())?;

    let auth_manager = state.api_client.auth.clone();
    let app_handle = app.clone();
    let dev_token = auth_manager.developer_token();

    // The sign-in window gets its own WebView2 profile. The app's main profile keeps a
    // cached Apple session, and MusicKit happily replays that dead token — which made
    // sign-in "succeed" instantly with an expired token and never prompt for credentials.
    let auth_profile = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("auth_profile");
    let _ = std::fs::remove_dir_all(&auth_profile);
    std::fs::create_dir_all(&auth_profile).map_err(|e| e.to_string())?;

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        "apple_auth",
        tauri::WebviewUrl::External(url),
    )
    .title("Sign in with Apple ID")
    .inner_size(520.0, 720.0)
    .center()
    .resizable(true)
    .data_directory(auth_profile)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window.show();

    // `media-user-token` is HttpOnly, so injected JavaScript can never read it — that is
    // why the old `document.cookie` poller only ever picked up MusicKit's cached token.
    // Reading the WebView2 cookie jar directly gets the real, freshly minted cookie.
    let _ = window.set_focus();

    tauri::async_runtime::spawn(async move {
        // ~10 minutes: long enough for 2FA and a slow Apple ID round trip.
        for _ in 0..600u32 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            // The user closed the window — nothing left to watch.
            if app_handle.get_webview_window("apple_auth").is_none() {
                return;
            }

            let Ok(jar) = window.cookies() else { continue };
            let Some(cookie) = jar.iter().find(|c| c.name() == "media-user-token") else {
                continue;
            };
            let token = cookie.value().trim().to_string();
            if token.len() < 20 {
                continue;
            }

            // Apple is the only authority on whether this token is alive. A dead token
            // looks perfectly valid locally, so never persist one that isn't confirmed.
            if !validate_music_user_token(&dev_token, &token).await {
                continue;
            }

            if let Ok(jar) = window.cookies() {
                if let Some(sf) = jar
                    .iter()
                    .find(|c| matches!(c.name(), "geo" | "x-apple-store-front"))
                    .map(|c| c.value().trim().to_string())
                    .filter(|v| v.len() == 2)
                {
                    let _ = auth_manager.set_storefront(sf, auth_manager.language());
                }
            }

            if auth_manager.set_user_token(Some(token)).is_err() {
                continue;
            }

            let auth_state = AuthState {
                is_authenticated: true,
                storefront: auth_manager.storefront(),
                language: auth_manager.language(),
                user_token_masked: auth_manager.masked_user_token(),
            };
            let _ = app_handle.emit("auth-success", auth_state);
            if let Some(w) = app_handle.get_webview_window("apple_auth") {
                let _ = w.close();
            }
            return;
        }
    });

    Ok(())
}

#[tauri::command]
fn set_user_session(state: State<AppState>, token: String) -> Result<AuthState, String> {
    state.api_client.auth.set_user_token(Some(token)).map_err(|e| e.to_string())?;
    get_auth_state(state)
}

#[tauri::command]
fn sign_out(state: State<AppState>) -> Result<(), String> {
    state.api_client.auth.sign_out().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_auth_state(state: State<AppState>) -> Result<AuthState, String> {
    let auth = &state.api_client.auth;
    Ok(AuthState {
        is_authenticated: auth.is_authenticated(),
        storefront: auth.storefront(),
        language: auth.language(),
        user_token_masked: auth.masked_user_token(),
    })
}

#[tauri::command]
fn set_storefront(state: State<AppState>, storefront: String, language: String) -> Result<(), String> {
    state.api_client.auth.set_storefront(storefront, language).map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_catalog(
    state: State<'_, AppState>,
    query: String,
    types: Option<String>,
    limit: Option<u32>,
) -> Result<CatalogSearchResult, String> {
    state
        .api_client
        .search_catalog(&query, types.as_deref(), limit.unwrap_or(25))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_search_suggestions(
    state: State<'_, AppState>,
    query: String,
) -> Result<SearchSuggestions, String> {
    state
        .api_client
        .get_search_suggestions(&query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_catalog_album(state: State<'_, AppState>, id: String) -> Result<StoreAlbum, String> {
    state
        .api_client
        .get_catalog_album(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_charts(
    state: State<'_, AppState>,
    types: Option<String>,
    limit: Option<u32>,
) -> Result<CatalogCharts, String> {
    state
        .api_client
        .get_charts(types.as_deref(), limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_personal_library(state: State<'_, AppState>) -> Result<Vec<StoreSong>, String> {
    state
        .api_client
        .get_personal_library_songs()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_recommendations(state: State<'_, AppState>) -> Result<Vec<StoreAlbum>, String> {
    state
        .api_client
        .get_personal_recommendations()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_cloud_playlists(state: State<'_, AppState>) -> Result<Vec<CloudPlaylist>, String> {
    state
        .api_client
        .get_user_playlists()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_cloud_playlist_tracks(
    state: State<'_, AppState>,
    playlist_id: String,
) -> Result<Vec<StoreSong>, String> {
    state
        .api_client
        .get_playlist_tracks(&playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn sync_cloud_library(state: State<'_, AppState>) -> Result<SyncStats, String> {
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

#[tauri::command]
async fn play_stream_track(
    state: State<'_, AppState>,
    track_id: String,
    stream_url: String,
    title: String,
    artist: String,
    duration_secs: Option<f64>,
) -> Result<(), String> {
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
}

// ─── Native FFI & Session Bridge Commands ───────────────────────────

use api::local_session::LocalSessionReader;
use drm::{get_current_process_memory, AppLocator, CoreAudioBridge, CoreFpBridge, CoreMediaBridge};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NativeBridgeStatusPayload {
    pub is_installed: bool,
    pub install_path: Option<String>,
    pub version: Option<String>,
    pub coreaudio_ready: bool,
    pub coremedia_ready: bool,
    pub corefp_ready: bool,
    pub current_ram_mb: f64,
    pub session_detected: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImportSessionResult {
    pub success: bool,
    pub has_user_token: bool,
    pub storefront: Option<String>,
    pub message: String,
}

#[tauri::command]
fn get_native_bridge_status(_state: State<AppState>) -> Result<NativeBridgeStatusPayload, String> {
    let locator = AppLocator::new();
    let install_res = locator.locate();

    let mut is_installed = false;
    let mut install_path = None;
    let mut version = None;
    let mut coreaudio_ready = false;
    let mut coremedia_ready = false;
    let mut corefp_ready = false;
    let mut session_detected = false;

    if let Ok(install) = install_res {
        is_installed = true;
        install_path = Some(install.install_dir.to_string_lossy().to_string());
        version = Some(install.version);

        if let Ok(ca) = CoreAudioBridge::load_from_dir(&install.install_dir) {
            coreaudio_ready = ca.is_operational();
        }

        if let Ok(cm) = CoreMediaBridge::load_from_dir(&install.install_dir) {
            coremedia_ready = (cm.fig_content_key_boss_get_class_id as usize) != 0;
        }

        if let Ok(fp) = CoreFpBridge::load_from_dir(&install.install_dir) {
            corefp_ready = fp.resolved_count() == 6;
        }

        let reader = LocalSessionReader::new();
        if let Ok(session) = reader.extract_from_local_state(&install.local_state_dir) {
            session_detected = session.media_user_token.is_some() || session.itctx.is_some();
        }
    }

    let ram_mb = get_current_process_memory()
        .map(|m| m.working_set_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    Ok(NativeBridgeStatusPayload {
        is_installed,
        install_path,
        version,
        coreaudio_ready,
        coremedia_ready,
        corefp_ready,
        current_ram_mb: ram_mb,
        session_detected,
    })
}

#[tauri::command]
fn import_local_apple_session(state: State<AppState>) -> Result<ImportSessionResult, String> {
    let locator = AppLocator::new();
    let install = locator.locate().map_err(|e| e.to_string())?;

    let reader = LocalSessionReader::new();
    let session = reader
        .extract_from_local_state(&install.local_state_dir)
        .map_err(|e| e.to_string())?;

    let mut has_user_token = false;
    if let Some(mut_token) = session.media_user_token.as_ref().or(session.itctx.as_ref()) {
        state
            .api_client
            .auth
            .set_user_token(Some(mut_token.clone()))
            .map_err(|e| e.to_string())?;
        has_user_token = true;
    }

    if let Some(storefront) = &session.storefront {
        let _ = state
            .api_client
            .auth
            .set_storefront(storefront.clone(), "en-US".to_string());
    }

    Ok(ImportSessionResult {
        success: true,
        has_user_token,
        storefront: session.storefront,
        message: "Successfully imported Apple Music session from local SQLite cookies".to_string(),
    })
}

#[tauri::command]
fn run_native_bridge_diagnostics(_state: State<AppState>) -> Result<String, String> {
    let locator = AppLocator::new();
    let install = locator.locate().map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    lines.push(format!("Apple Music Version: {}", install.version));
    lines.push(format!("Install Dir: {}", install.install_dir.display()));
    lines.push(format!("LocalState Dir: {}", install.local_state_dir.display()));

    let ca = CoreAudioBridge::load_from_dir(&install.install_dir).map_err(|e| e.to_string())?;
    lines.push(format!("CoreAudio Operational: {}", ca.is_operational()));
    lines.push(format!("CoreAudio Decoders: {:?}", ca.decoder_status()));

    let cm = CoreMediaBridge::load_from_dir(&install.install_dir).map_err(|e| e.to_string())?;
    lines.push(format!("FigContentKeyBoss Class ID: {}", cm.get_key_boss_class_id()));

    let fp = CoreFpBridge::load_from_dir(&install.install_dir).map_err(|e| e.to_string())?;
    lines.push(format!("CoreFP Ordinals Resolved: {}/6", fp.resolved_count()));

    let mem = get_current_process_memory().map_err(|e| e.to_string())?;
    lines.push(format!("Process Working Set: {:.2} MB", mem.working_set_bytes as f64 / (1024.0 * 1024.0)));

    Ok(lines.join("\n"))
}

// ─── Application Lifecycle ──────────────────────────────────────────

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
            println!("[SETUP] Inside setup callback...");
            match app.get_webview_window("main") {
                Some(main_window) => {
                    println!("[SETUP] Found main window: label={}", main_window.label());
                    println!("[SETUP] HWND: {:?}", main_window.hwnd());
                    println!("[SETUP] is_visible: {:?}", main_window.is_visible());
                    println!("[SETUP] outer_position: {:?}", main_window.outer_position());
                    println!("[SETUP] outer_size: {:?}", main_window.outer_size());
                    let mw = main_window.clone();
                    main_window.on_window_event(move |event| {
                        println!("[WINDOW EVENT] {:?}", event);
                    });
                    if let Err(e) = mw.show() {
                        eprintln!("[SETUP] Failed to show window: {:?}", e);
                    }
                    let _ = mw.unminimize();
                    let _ = mw.set_focus();
                    println!("[SETUP] After show: is_visible: {:?}", mw.is_visible());

                    // Hidden PlayReady-backed player state channel.
                    start_state_listener(app.handle().clone());
                }
                None => {
                    eprintln!("[SETUP] ERROR: No webview window with label 'main' found!");
                }
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let _ = state.api_client.auth.refresh_developer_token_from_web().await;
                }
            });

            Ok(())
        })
        .manage(AppState {
            library: Mutex::new(library::Library::new()),
            audio_engine: audio::Engine::new(),
            api_client,
        })
        .invoke_handler(tauri::generate_handler![
            // Diagnostics
            probe_report,
            // Streaming player
            stream_play,
            stream_pause,
            stream_resume,
            stream_stop,
            stream_seek,
            // Library
            scan_library,
            get_albums,
            get_tracks,
            get_artists,
            get_playlists,
            get_track_count,
            create_playlist,
            add_to_playlist,
            remove_from_playlist,
            delete_playlist,
            import_playlist,
            export_playlist,
            // Playback
            play_track,
            play_album,
            pause_playback,
            resume_playback,
            stop_playback,
            seek_to,
            set_volume,
            get_playback_status,
            next_track,
            prev_track,
            set_repeat_mode,
            set_shuffle,
            // Apple Music API & Auth
            start_apple_sign_in,
            set_user_session,
            sign_out,
            get_auth_state,
            set_storefront,
            search_catalog,
            get_search_suggestions,
            get_catalog_album,
            get_charts,
            get_personal_library,
            get_recommendations,
            get_cloud_playlists,
            get_cloud_playlist_tracks,
            sync_cloud_library,
            play_stream_track,
            // Native FFI & Session Bridge
            get_native_bridge_status,
            import_local_apple_session,
            run_native_bridge_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
