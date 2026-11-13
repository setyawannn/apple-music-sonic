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

// ─── Full-song streaming player ─────────────────────────────────────
//
// The license challenge Apple requires can only be produced by a PlayReady CDM,
// so decryption cannot be reimplemented in Rust. Instead we host Apple's own
// MusicKit player in a hidden WebView2 that shares the signed-in auth profile.
// Windows' PlayReady CDM does the DRM; we only drive the player and read state.

const PLAYER_LABEL: &str = "player";

const PLAYER_BOOTSTRAP: &str = r#"
(function () {
  window.__mkReady = new Promise(function (res) { window.__mkResolve = res; });
  window.__mkQueue = window.__mkQueue || [];

  function got() {
    try {
      if (window.MusicKit && typeof window.MusicKit.getInstance === 'function') {
        var m = window.MusicKit.getInstance();
        if (m) { window.__mkResolve(m); return true; }
      }
    } catch (e) {}
    return false;
  }

  // State leaves the page through a cookie. music.apple.com's CSP blocks fetch()
  // to 127.0.0.1, and WebView2 does not mirror document.title to the native
  // window title, but `document.cookie` is first-party and CSP-immune. Rust polls
  // the webview cookie jar (the same API the sign-in flow uses) and re-emits the
  // value as `player-state`.
  function publish() {
    try {
      var m = window.MusicKit.getInstance();
      var item = m.nowPlayingItem || null;
      var attrs = (item && item.attributes) || null;
      var json = JSON.stringify({
        isPlaying: !!m.isPlaying,
        position: m.currentPlaybackTime || 0,
        duration: m.currentPlaybackDuration || 0,
        id: item && item.id ? item.id : null,
        title: attrs ? attrs.name : null,
        artist: attrs ? attrs.artistName : null,
        artwork: attrs && attrs.artwork ? attrs.artwork.url : null,
        album: attrs ? attrs.albumName : null,
        shuffle: typeof m.shuffleMode === 'number' ? m.shuffleMode : 0,
        repeat: typeof m.repeatMode === 'number' ? m.repeatMode : 0,
        volume: typeof m.volume === 'number' ? m.volume : null,
        queueLength: m.queue && typeof m.queue.length === 'number' ? m.queue.length : 0
      });
      document.cookie = 'mk_state=' + encodeURIComponent(json) + '; path=/; max-age=300; Secure; SameSite=Lax';
      publishQueue(m);
    } catch (e) {}
  }

  // The Up Next list rides in its own cookie, base64-encoded so quotes/braces do
  // not blow past the per-cookie size limit. Only the current item and the next 40
  // are sent, and only when they actually change.
  var __lastQueueSig = '';
  function publishQueue(m) {
    try {
      var q = m.queue;
      var items = (q && q.items) ? q.items : q;
      if (!items || typeof items.length !== 'number') return;
      var idx = 0;
      if (q && typeof q.position === 'number') {
        idx = q.position;
      } else {
        var cur = m.nowPlayingItem;
        if (cur) {
          for (var k = 0; k < items.length; k++) {
            if (items[k] && items[k].id === cur.id) { idx = k; break; }
          }
        }
      }
      var slice = [];
      for (var j = idx; j < items.length && slice.length < 40; j++) {
        var it = items[j];
        var a = (it && it.attributes) || {};
        slice.push({
          i: it && it.id ? it.id : null,
          n: a.name || '',
          a: a.artistName || '',
          d: a.durationInMillis || 0
        });
      }
      var sig = idx + '|' + slice.map(function (x) { return x.i; }).join(',');
      if (sig === __lastQueueSig) return;
      __lastQueueSig = sig;
      var payload = btoa(unescape(encodeURIComponent(JSON.stringify({ base: idx, items: slice }))));
      if (payload) document.cookie = 'mk_queue=' + payload + '; path=/; max-age=300; Secure; SameSite=Lax';
    } catch (e) {}
  }

  window.__mkDo = async function (cmd, arg, nonce) {
    // Each user action carries a nonce; a re-eval carries the same one,
    // so only the first attempt runs. A fresh click gets a fresh nonce.
    if (nonce) {
      if (window.__mkNonce === nonce) return;
      window.__mkNonce = nonce;
    }
    var m = await window.__mkReady;
    if (cmd === 'play') {
      // arg is JSON: { startWith, songs[] }. MusicKit owns the queue so that
      // next/previous/shuffle/repeat behave like Apple Music instead of replaying
      // one song forever.
      var spec = null;
      try { spec = JSON.parse(String(arg)); } catch (e) { spec = null; }
      var songs = spec && Array.isArray(spec.songs) ? spec.songs.filter(Boolean) : [];
      var startWith = (spec && spec.startWith) || songs[0] || String(arg);
      // Apple's album "Shuffle" is the same queue with shuffle already on; a plain
      // album Play turns it off. A bare track click leaves the mode untouched.
      if (spec && spec.shuffle === true) {
        try { m.shuffleMode = 1; } catch (e) {}
      } else if (spec && spec.shuffle === false) {
        try { m.shuffleMode = 0; } catch (e) {}
      }
      if (songs.length > 1) {
        try {
          await m.setQueue({ songs: songs, startWith: startWith });
        } catch (e) {
          await m.setQueue({ songs: songs });
          var idx = songs.indexOf(startWith);
          if (idx > 0) await m.changeToMediaAtIndex(idx);
        }
      } else {
        await m.setQueue({ song: String(startWith) });
      }
      return m.play();
    }
    if (cmd === 'pause') return m.pause();
    if (cmd === 'resume') return m.play();
    if (cmd === 'stop') return m.stop();
    if (cmd === 'seek') return m.seekToTime(Number(arg));
    if (cmd === 'next') return m.skipToNextItem();
    if (cmd === 'queuejump') return m.changeToMediaAtIndex(Number(arg));
    if (cmd === 'previous') {
      // Apple's own rule: restart the track if we are more than 3s in.
      if (typeof m.currentPlaybackTime === 'number' && m.currentPlaybackTime > 3) {
        return m.seekToTime(0);
      }
      return m.skipToPreviousItem();
    }
    if (cmd === 'shuffle') { m.shuffleMode = Number(arg) ? 1 : 0; return; }
    if (cmd === 'repeat') { m.repeatMode = Number(arg) || 0; return; }
    if (cmd === 'volume') {
      m.volume = Math.max(0, Math.min(1, Number(arg)));
      return;
    }
  };

  function run(job) {
    window.__mkDo(job.cmd, job.arg, job.nonce)
      .then(publish)
      .catch(function (e) { console.error('mk', e); });
  }

  // Runs every command queued so far, including ones Rust pushed before this
  // script existed in the current document.
  window.__mkDrain = function () {
    var jobs = window.__mkQueue.splice(0);
    jobs.forEach(run);
  };

  window.__mkReady.then(function (m) {
    // Publish on real player transitions instead of waiting for the next tick.
    ['playbackStateDidChange', 'nowPlayingItemDidChange', 'playbackTimeDidChange'].forEach(function (name) {
      try { m.addEventListener(name, publish); } catch (e) {}
    });
    publish();
    setInterval(publish, 500);
  });

  if (!got()) {
    var tries = 0;
    var t = setInterval(function () {
      tries += 1;
      if (got() || tries > 200) clearInterval(t);
    }, 300);
  }

  window.__mkDrain();
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
             --disable-backgrounding-occluded-windows \
             --disable-features=BlockInsecurePrivateNetworkRequests,PrivateNetworkAccessSendPreflights,PrivateNetworkAccessRespectPreflightResults",
        )
        .data_directory(profile)
        .build()
        .map_err(|e| e.to_string())
}

/// Drive the hidden player. Dispatch and return: the page owns a command queue, so
/// a command issued before MusicKit is live is parked and executed the moment the
/// player resolves. There is no readiness handshake and no retry, by design — the
/// old "wait for /ready" loop could never be satisfied because music.apple.com's
/// CSP blocks loopback fetches.
async fn player_command(app: &tauri::AppHandle, cmd: &str, arg: Option<String>) -> Result<(), String> {
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

    window.eval(&js).map_err(|e| e.to_string())?;
    println!("[PLAYER] cmd={} nonce={}", cmd, nonce);
    Ok(())
}

#[tauri::command]
async fn stream_play(
    app: tauri::AppHandle,
    song_id: String,
    queue: Option<Vec<String>>,
    shuffle: Option<bool>,
) -> Result<(), String> {
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
async fn stream_next(app: tauri::AppHandle) -> Result<(), String> {
    player_command(&app, "next", None).await
}

#[tauri::command]
async fn stream_previous(app: tauri::AppHandle) -> Result<(), String> {
    player_command(&app, "previous", None).await
}

#[tauri::command]
async fn stream_shuffle(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    player_command(&app, "shuffle", Some(if enabled { "1" } else { "0" }.to_string())).await
}

#[tauri::command]
async fn stream_repeat(app: tauri::AppHandle, mode: String) -> Result<(), String> {
    // 0 = off, 1 = one, 2 = all — MusicKit.RepeatMode's own values.
    let value = match mode.as_str() {
        "One" | "one" | "1" => "1",
        "All" | "all" | "2" => "2",
        _ => "0",
    };
    player_command(&app, "repeat", Some(value.to_string())).await
}

#[tauri::command]
async fn stream_volume(app: tauri::AppHandle, volume: f64) -> Result<(), String> {
    player_command(&app, "volume", Some(volume.clamp(0.0, 1.0).to_string())).await
}

#[tauri::command]
async fn stream_seek(app: tauri::AppHandle, position: f64) -> Result<(), String> {
    player_command(&app, "seek", Some(position.to_string())).await
}

/// Jump to an absolute index in MusicKit's queue (used by the Up Next drawer).
#[tauri::command]
async fn stream_jump_to(app: tauri::AppHandle, index: u32) -> Result<(), String> {
    player_command(&app, "queuejump", Some(index.to_string())).await
}

/// Percent-decode an `encodeURIComponent`-style cookie value (`%7B` → `{`).
/// Invalid escapes are passed through untouched rather than dropped.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decode standard base64 (the form the page's `btoa` produces). `None` on any
/// invalid character, so a corrupt cookie is dropped rather than half-decoded.
fn base64_decode(input: &str) -> Option<String> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = val(c)?;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    String::from_utf8(out).ok()
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
fn start_state_poller(app: tauri::AppHandle) {
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
            let Some(raw) = jar.iter().find(|c| c.name() == "mk_state").map(|c| c.value().to_string())
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
            let pos = value.get("position").and_then(|v| v.as_f64()).unwrap_or(0.0);
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
                if queue_json != last_queue && serde_json::from_str::<serde_json::Value>(&queue_json).is_ok() {
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
fn get_last_player_state(app: tauri::AppHandle) -> Result<Option<serde_json::Value>, String> {
    let Some(state_path) = player_state_path(&app) else {
        return Ok(None);
    };
    let Ok(state_raw) = std::fs::read_to_string(state_path) else {
        return Ok(None);
    };
    let Ok(state_val) = serde_json::from_str::<serde_json::Value>(&state_raw) else {
        return Ok(None);
    };
    if state_val.get("id").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
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

#[cfg(test)]
mod state_channel_tests {
    use super::{base64_decode, percent_decode};

    #[test]
    fn decodes_base64_queue_cookie() {
        // btoa('{"base":0,"items":[]}') === 'eyJiYXNlIjowLCJpdGVtcyI6W119'
        let encoded = "eyJiYXNlIjowLCJpdGVtcyI6W119";
        assert_eq!(base64_decode(encoded).unwrap(), r#"{"base":0,"items":[]}"#);
    }

    #[test]
    fn base64_decode_rejects_garbage() {
        assert!(base64_decode("not base64!!").is_none());
    }

    #[test]
    fn decodes_encoded_json() {
        let json = r#"{"isPlaying":true,"position":12.5}"#;
        let encoded: String = json
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() {
                    (b as char).to_string()
                } else {
                    format!("%{:02X}", b)
                }
            })
            .collect();
        assert_eq!(percent_decode(&encoded), json);
    }

    #[test]
    fn leaves_invalid_escapes_alone() {
        assert_eq!(percent_decode("100%25 %zz %"), "100% %zz %");
        assert_eq!(percent_decode("plain"), "plain");
    }
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

    // The hidden player runs on this same profile. Wiping it underneath a live
    // webview leaves the player holding a destroyed session, so close it first;
    // ensure_player() rebuilds it against the fresh profile when needed.
    if let Some(player) = app.get_webview_window(PLAYER_LABEL) {
        let _ = player.close();
        // Give WebView2 a moment to release its lock on the profile directory.
        for _ in 0..20 {
            if app.get_webview_window(PLAYER_LABEL).is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

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

            // Pre-warm the player on the fresh profile so the next click streams.
            if let Err(e) = ensure_player(&app_handle) {
                eprintln!("[AUTH] player re-warm failed: {}", e);
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
    let result = state.api_client.get_user_playlists().await;
    match &result {
        Ok(list) => println!("[PLAYLIST] loaded {} playlists", list.len()),
        Err(e) => eprintln!("[PLAYLIST] error: {}", e),
    }
    result.map_err(|e| e.to_string())
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

/// Create a playlist in the user's Apple Music library, optionally pre-filled.
/// Returns the new playlist id.
#[tauri::command]
async fn create_cloud_playlist(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
    track_ids: Option<Vec<String>>,
) -> Result<String, String> {
    let id = state
        .api_client
        .create_library_playlist(&name, description.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    if let Some(ids) = track_ids {
        if !ids.is_empty() {
            state
                .api_client
                .add_tracks_to_library_playlist(&id, &ids)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(id)
}

#[tauri::command]
async fn rename_cloud_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    name: String,
) -> Result<(), String> {
    state
        .api_client
        .rename_library_playlist(&playlist_id, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_cloud_playlist(state: State<'_, AppState>, playlist_id: String) -> Result<(), String> {
    state
        .api_client
        .delete_library_playlist(&playlist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_to_cloud_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    track_ids: Vec<String>,
) -> Result<(), String> {
    state
        .api_client
        .add_tracks_to_library_playlist(&playlist_id, &track_ids)
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

                    // Start forwarding the hidden player's state to the UI, then
                    // pre-warm the player: paying the page load here, while the user
                    // is browsing, is what keeps the first click instant.
                    start_state_poller(app.handle().clone());
                    if let Err(e) = ensure_player(app.handle()) {
                        eprintln!("[SETUP] player pre-warm failed: {}", e);
                    }

                    // The hidden player is a real window, so Tauri would keep the app
                    // alive after the main window closes. Shut down explicitly instead.
                    let quitting = mw.clone();
                    mw.on_window_event(move |event| {
                        if let tauri::WindowEvent::Destroyed = event {
                            let _ = quitting.app_handle().exit(0);
                        }
                    });
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
            // Streaming player
            stream_play,
            stream_pause,
            stream_resume,
            stream_stop,
            stream_seek,
            stream_next,
            stream_previous,
            stream_shuffle,
            stream_repeat,
            stream_volume,
            get_last_player_state,
            stream_jump_to,
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
            create_cloud_playlist,
            rename_cloud_playlist,
            delete_cloud_playlist,
            add_to_cloud_playlist,
            play_stream_track,
            // Native FFI & Session Bridge
            get_native_bridge_status,
            import_local_apple_session,
            run_native_bridge_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
