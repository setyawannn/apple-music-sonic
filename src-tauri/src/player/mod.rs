//! The hidden MusicKit player: window lifecycle, command queue, state poller,
//! restore-on-launch persistence, and the transport commands the UI calls.

use crate::{AppError, AppResult};
use tauri::{Emitter, Manager};

mod cookie;

use cookie::{base64_decode, percent_decode};

#[cfg(windows)]
mod media;

/// Route Windows media keys (and the now-playing flyout) to this app.
#[cfg(windows)]
pub(crate) use media::register as register_media_keys;
/// Mirror the current track into the Windows flyout.
#[cfg(windows)]
pub(crate) use media::update_now_playing;

/// Media keys are a Windows-only convenience; nothing to do elsewhere.
#[cfg(not(windows))]
pub(crate) fn register_media_keys(_: &tauri::AppHandle) -> AppResult<()> {
    Ok(())
}

/// No system now-playing surface off Windows.
#[cfg(not(windows))]
pub(crate) fn update_now_playing(_: &serde_json::Value) {}

// ─── Full-song streaming player ─────────────────────────────────────
//
// The license challenge Apple requires can only be produced by a PlayReady CDM,
// so decryption cannot be reimplemented in Rust. Instead we host Apple's own
// MusicKit player in a hidden WebView2 that shares the signed-in auth profile.
// Windows' PlayReady CDM does the DRM; we only drive the player and read state.

pub(crate) const PLAYER_LABEL: &str = "player";

const PLAYER_BOOTSTRAP: &str = include_str!("bootstrap.js");

/// Create (once) the hidden playback webview, sharing the signed-in profile so
/// Apple's page is already authenticated and its PlayReady CDM can licence audio.
pub(crate) fn ensure_player(app: &tauri::AppHandle) -> AppResult<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(PLAYER_LABEL) {
        return Ok(w);
    }

    let profile = app.path().app_local_data_dir()?.join("auth_profile");
    let _ = std::fs::create_dir_all(&profile);

    let url = "https://music.apple.com/id/"
        .parse::<tauri::Url>()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;

    tauri::WebviewWindowBuilder::new(app, PLAYER_LABEL, tauri::WebviewUrl::External(url))
        .title("Apple Music Player")
        .visible(false)
        .inner_size(480.0, 320.0)
        .initialization_script(PLAYER_BOOTSTRAP)
        .additional_browser_args(
            "--autoplay-policy=no-user-gesture-required \
             --disable-background-timer-throttling \
             --disable-renderer-backgrounding \
             --disable-backgrounding-occluded-windows \
             --disable-features=BlockInsecurePrivateNetworkRequests,PrivateNetworkAccessSendPreflights,PrivateNetworkAccessRespectPreflightResults",
        )
        .data_directory(profile)
        .build()
        .map_err(AppError::from)
}

/// Drive the hidden player. Dispatch and return: the page owns a command queue, so
/// a command issued before MusicKit is live is parked and executed the moment the
/// player resolves. There is no readiness handshake and no retry, by design — the
/// old "wait for /ready" loop could never be satisfied because music.apple.com's
/// CSP blocks loopback fetches.
pub(crate) async fn player_command(
    app: &tauri::AppHandle,
    cmd: &str,
    arg: Option<String>,
) -> AppResult<()> {
    let window = ensure_player(app)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());

    let job = serde_json::json!({ "cmd": cmd, "arg": arg, "nonce": nonce });
    let js = format!(
        "(function(j){{ (window.__mkQueue = window.__mkQueue || []).push(j);          if (window.__mkDrain) window.__mkDrain(); }})({});",
        job
    );

    window.eval(&js)?;
    println!("[PLAYER] cmd={} nonce={}", cmd, nonce);
    Ok(())
}

#[tauri::command]
pub async fn stream_play(
    app: tauri::AppHandle,
    song_id: String,
    queue: Option<Vec<String>>,
    shuffle: Option<bool>,
) -> AppResult<()> {
    // The queue travels with the click so MusicKit can own playback order: next,
    // previous, shuffle and repeat then operate on the list the user was looking at,
    // exactly like the real app. A single song is just a one-item queue.
    let mut songs: Vec<String> = queue.unwrap_or_default();
    songs.retain(|id| !id.is_empty());
    if !songs.iter().any(|id| id == &song_id) {
        songs.insert(0, song_id.clone());
    }
    let payload = serde_json::json!({
        "startWith": song_id,
        "songs": songs,
        "shuffle": shuffle,
    });
    // Remember the queue (not just the song) so the next launch can rebuild it.
    if let Some(path) = player_queue_path(&app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let blob = serde_json::json!({ "songs": songs, "shuffle": shuffle });
        let _ = std::fs::write(path, blob.to_string());
    }
    player_command(&app, "play", Some(payload.to_string())).await
}

#[tauri::command]
pub async fn stream_pause(app: tauri::AppHandle) -> AppResult<()> {
    player_command(&app, "pause", None).await
}

#[tauri::command]
pub async fn stream_resume(app: tauri::AppHandle) -> AppResult<()> {
    player_command(&app, "resume", None).await
}

#[tauri::command]
pub async fn stream_stop(app: tauri::AppHandle) -> AppResult<()> {
    player_command(&app, "stop", None).await
}

#[tauri::command]
pub async fn stream_next(app: tauri::AppHandle) -> AppResult<()> {
    player_command(&app, "next", None).await
}

#[tauri::command]
pub async fn stream_previous(app: tauri::AppHandle) -> AppResult<()> {
    player_command(&app, "previous", None).await
}

#[tauri::command]
pub async fn stream_shuffle(app: tauri::AppHandle, enabled: bool) -> AppResult<()> {
    player_command(
        &app,
        "shuffle",
        Some(if enabled { "1" } else { "0" }.to_string()),
    )
    .await
}

#[tauri::command]
pub async fn stream_repeat(app: tauri::AppHandle, mode: String) -> AppResult<()> {
    // 0 = off, 1 = one, 2 = all — MusicKit.RepeatMode's own values.
    let value = match mode.as_str() {
        "One" | "one" | "1" => "1",
        "All" | "all" | "2" => "2",
        _ => "0",
    };
    player_command(&app, "repeat", Some(value.to_string())).await
}

#[tauri::command]
pub async fn stream_volume(app: tauri::AppHandle, volume: f64) -> AppResult<()> {
    player_command(&app, "volume", Some(volume.clamp(0.0, 1.0).to_string())).await
}

#[tauri::command]
pub async fn stream_seek(app: tauri::AppHandle, position: f64) -> AppResult<()> {
    player_command(&app, "seek", Some(position.to_string())).await
}

/// Jump to an absolute index in MusicKit's queue (used by the Up Next drawer).
#[tauri::command]
pub async fn stream_jump_to(app: tauri::AppHandle, index: u32) -> AppResult<()> {
    player_command(&app, "queuejump", Some(index.to_string())).await
}

/// Where the hidden player's last state is remembered between launches.
fn player_state_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("player_state.json"))
}

/// Where the last queue sent to the player is remembered between launches.
fn player_queue_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("player_queue.json"))
}

/// Forwards the hidden player's state to the UI.
///
/// The page writes a compact JSON blob into the `mk_state` cookie. Nothing else
/// about the page can reach us: music.apple.com's CSP blocks loopback `fetch()`,
/// WebView2 does not mirror `document.title` to the native window title, and
/// Tauri IPC is unavailable on remote origins. `document.cookie` is first-party
/// and CSP-immune, and `window.cookies()` is the same API the sign-in flow uses
/// to pull `media-user-token` — so it is known to work on this webview.
pub(crate) fn start_state_poller(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last = String::new();
        let mut last_queue = String::new();
        // Throttle disk writes: only when the song changes or playback moves ~5s.
        let mut saved_id = String::new();
        let mut saved_pos = f64::NAN;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;

            let Some(window) = app.get_webview_window(PLAYER_LABEL) else {
                // Player closed; forget the last blob so a rebuild re-emits state.
                last.clear();
                continue;
            };
            let Ok(jar) = window.cookies() else { continue };
            let Some(raw) = jar
                .iter()
                .find(|c| c.name() == "mk_state")
                .map(|c| c.value().to_string())
            else {
                continue;
            };

            // WebView2 may hand back the value already decoded; accept either form.
            let json = if raw.starts_with('{') {
                raw
            } else {
                percent_decode(&raw)
            };
            if !json.starts_with('{') || json == last {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
                continue;
            };

            // Remember the current track so the next launch can restore the bar.
            let id = value.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let pos = value
                .get("position")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if !id.is_empty() && (id != saved_id || (pos - saved_pos).abs() >= 5.0) {
                if let Some(path) = player_state_path(&app) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(path, &json);
                }
                saved_id = id.to_string();
                saved_pos = pos;
            }

            println!("[PLAYER] state {}", json);
            last = json.clone();
            update_now_playing(&value);
            let _ = app.emit("player-state", json);

            // Up Next rides in a second cookie: base64(JSON) with the current item
            // and the next few. Emit only when the list actually changes.
            if let Some(queue_json) = jar
                .iter()
                .find(|c| c.name() == "mk_queue")
                .map(|c| c.value().to_string())
                .and_then(|raw| base64_decode(&raw))
                .filter(|s| s.starts_with('{'))
            {
                if queue_json != last_queue
                    && serde_json::from_str::<serde_json::Value>(&queue_json).is_ok()
                {
                    let count = serde_json::from_str::<serde_json::Value>(&queue_json)
                        .ok()
                        .and_then(|v| v.get("items").and_then(|i| i.as_array().map(|a| a.len())))
                        .unwrap_or(0);
                    println!("[PLAYER] queue items={}", count);
                    last_queue = queue_json.clone();
                    let _ = app.emit("player-queue", queue_json);
                }
            }
        }
    });
}

/// Returns the last track and queue that were playing, for restore-on-launch.
///
/// The frontend uses this to repaint the playback bar (paused) and to rebuild the
/// queue if the user hits play before choosing anything else. `None` means nothing
/// was ever played on this machine.
#[tauri::command]
pub fn get_last_player_state(app: tauri::AppHandle) -> AppResult<Option<serde_json::Value>> {
    let Some(state_path) = player_state_path(&app) else {
        return Ok(None);
    };
    let Ok(state_raw) = std::fs::read_to_string(state_path) else {
        return Ok(None);
    };
    let Ok(state_val) = serde_json::from_str::<serde_json::Value>(&state_raw) else {
        return Ok(None);
    };
    if state_val
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
    {
        return Ok(None);
    }

    let queue = player_queue_path(&app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    let songs = queue
        .as_ref()
        .and_then(|v| v.get("songs").cloned())
        .unwrap_or_else(|| serde_json::Value::Array(vec![]));
    let shuffle = queue
        .as_ref()
        .and_then(|v| v.get("shuffle").cloned())
        .unwrap_or(serde_json::Value::Null);

    Ok(Some(serde_json::json!({
        "state": state_val,
        "queue": songs,
        "shuffle": shuffle,
    })))
}
