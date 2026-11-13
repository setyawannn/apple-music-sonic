# Apple Music RS — How It Works

> Last updated: 2026-11-13
> Everything here describes the code as it actually is, not as it was planned.

---

## What the app does

A Tauri v2 desktop app on Windows that plays Apple Music full songs using **your own
subscription**, and browses the catalog through Apple's public REST API.

```
┌───────────────────────────────────────────────────────────────┐
│  main window (WebView2, local page)                            │
│   index.html + src/js/app.js + src/styles/app.css              │
│   sidebar · search · charts · album grid · playback bar        │
│                      │  invoke()                               │
│                      ▼                                         │
│  Rust backend (src-tauri/src/lib.rs)                           │
│   • api/     REST client, auth, session import                 │
│   • player   hidden WebView2 at music.apple.com (label "player")│
│   • audio/   PARKED local engine (symphonia + rodio)           │
│   • library/ PARKED local library (SQLite)                     │
│   • drm/     ABANDONED native decrypt attempt (diagnostics)    │
└───────────────────────────────────────────────────────────────┘
                       │
                       ▼  Windows PlayReady CDM
                 music.apple.com  (audio out)
```

There is **no audio engine of our own in the streaming path.** Apple's page does the
playing; we drive it.

---

## The streaming path

### 1. A song is clicked

`src/js/app.js` reads `data-catalog-id` off the track row and:

```js
state.streamingActive  = true;      // optimistic UI
state.streamIsPlaying  = true;
state.streamPosition   = 0;
state.streamAnchor     = Date.now();
invoke("stream_play", { songId: catalogId });
```

Title, artist, and artwork are written to the playback bar **before** the command is sent,
so the UI never sits empty while the player wakes up.

### 2. Rust hands the command to the hidden page

`stream_play` → `player_command()` in `lib.rs`:

- `ensure_player()` returns the existing `player` webview, or builds it if missing.
  It is normally already built: `setup()` calls `ensure_player()` to **pre-warm** it while
  you browse, so the first click does not pay the page-load cost.
- The command is pushed into a page-side queue and the page is told to drain it:

```js
(function (j) { (window.__mkQueue = window.__mkQueue || []).push(j);
                if (window.__mkDrain) window.__mkDrain(); })({ cmd, arg, nonce });
```

### 3. The page owns the ordering

`PLAYER_BOOTSTRAP` (an initialization script) runs at document start in the hidden webview:

```js
window.__mkReady  = new Promise(res => window.__mkResolve = res);  // resolves when MusicKit exists
window.__mkQueue  = [];                                           // survives page load
window.__mkDo     = async (cmd, arg, nonce) => {
  if (nonce && window.__mkNonce === nonce) return;   // same command re-delivered → ignore
  window.__mkNonce = nonce;
  const m = await window.__mkReady;                  // parks until MusicKit is live
  if (cmd === "play")   { await m.setQueue({ song: String(arg) }); return m.play(); }
  if (cmd === "pause")  return m.pause();
  if (cmd === "resume") return m.play();
  if (cmd === "stop")   return m.stop();
  if (cmd === "seek")   return m.seekToTime(Number(arg));
};
window.__mkDrain = () => window.__mkQueue.splice(0).forEach(job => run(job));
```

Two design points worth keeping:

- **The queue absorbs the cold start.** A command issued before the page has MusicKit is
  parked in `__mkQueue` and executed the moment `__mkReady` resolves. That is why the old
  blind `for attempt in 0..6 { sleep(1200ms) }` loop could be deleted.
- **The nonce makes re-delivery idempotent.** Clicking the same song twice sends two
  different nonces, so repeat clicks work; re-delivering one command sends the same nonce,
  so it runs once.

### 4. PlayReady does the rest

`MusicKit.play()` starts Apple's own player inside WebView2. The page requests a license,
Windows' PlayReady CDM answers the challenge, and decrypted audio goes straight to the
system mixer. We never see ciphertext, keys, or PCM.

---

## Why a hidden browser (and what it costs)

`src-tauri/src/drm/**` is the record of the alternative: load Apple's DLLs into our process
and decrypt natively. The DLLs load, the ordinals resolve, the unit tests pass — and it never
played a note. A license `challenge` must come from a real CDM; Apple's key server rejects
anything else.

So the app borrows one. Consequences, stated plainly:

| Cost | Detail |
|------|--------|
| Audio ceiling | **AAC 256 kbps**. WebView2 exposes no ALAC decoder (`mse.alac = false`) and EME output cannot be read back as PCM, so "lossless" is unreachable on this path. |
| Memory | A hidden Chromium instance. The old "~80 MB total" target is **not measured** and should be treated as aspirational. |
| State feedback | The page's CSP blocks our loopback channel, so the UI currently guesses position. See below. |
| Startup | Mitigated by pre-warming the player during `setup()`. |

Probe results recorded on this machine:

```
com.microsoft.playready : ok / ok / ok      ← used
com.widevine.alpha      : ok / NotSupportedError / ok
com.apple.fps           : NotSupportedError ×3
MSE                     : aac = true, alac = false, ec3 = false
```

---

## Playback state — intended vs actual

**Intended design.** The page reports once every 250 ms and on real transitions
(`playbackStateDidChange`, `nowPlayingItemDidChange`, `playbackTimeDidChange`) by POSTing
JSON to a loopback server:

```
POST http://127.0.0.1:38471/state   ← position, duration, id, title, artist, artwork
POST http://127.0.0.1:38471/ready   ← page can accept commands
```

`start_state_listener()` receives it and emits the `player-state` Tauri event, which
`app.js` listens for.

**Actual reality.** Every request is blocked by music.apple.com's CSP before it leaves the
page. Across all recorded dev sessions the server has received exactly zero `/state`,
`/ready`, or `/debug` requests. Therefore:

- `PLAYER_READY` is always `false`
- `player_command` waits 30 s on a notification that never fires, then returns `Err`
- the frontend's optimistic state survives until that `Err` lands and resets it
- progress is `Date.now() - streamAnchor`, and duration comes from catalog metadata

This is the top P0 bug: `docs/04-bugs/03-PLAYER-STATE-CHANNEL.md`.

---

## Authentication

All three paths end with a `media-user-token` string, persisted to
`%APPDATA%\AppleMusicRS\session.json` by `AuthManager`.

1. **Developer token (always required).** Apple's *public* MusicKit JWT is committed in
   `api/auth.rs` and refreshed at startup by scraping `music.apple.com`'s JS bundle. This is
   not a user credential and does not expire often (~Jan 2027 as of writing).
2. **Import session.** `api/local_session.rs` opens the official app's WebView2 cookie
   database (`...\EBWebView\Default\Network\Cookies`) read-only and extracts
   `media-user-token`. Works only if you signed in there before.
3. **Sign in window.** `start_apple_sign_in` opens `music.apple.com/login` in its own
   WebView2, then polls `window.cookies()` once per second for `media-user-token`, validates
   it against Apple, and closes on success.

⚠️ The sign-in flow **deletes the shared `auth_profile` directory** before opening the login
window. The hidden player uses that same directory. Until the profile lifecycle is fixed,
restart the app after signing in. See bug 03.

---

## Command surface (48 commands)

| Group | Count | Commands |
|-------|-------|----------|
| Streaming player | 5 | `stream_play`, `stream_pause`, `stream_resume`, `stream_stop`, `stream_seek` |
| Apple Music API + auth | 15 | `start_apple_sign_in`, `set_user_session`, `sign_out`, `get_auth_state`, `set_storefront`, `search_catalog`, `get_search_suggestions`, `get_catalog_album`, `get_charts`, `get_personal_library`, `get_recommendations`, `get_cloud_playlists`, `get_cloud_playlist_tracks`, `sync_cloud_library`, `play_stream_track` |
| Native bridge + diagnostics | 3 | `get_native_bridge_status`, `import_local_apple_session`, `run_native_bridge_diagnostics` |
| Diagnostics | 1 | `probe_report` |
| **Parked** local playback | 12 | `play_track`, `play_album`, `pause_playback`, `resume_playback`, `stop_playback`, `seek_to`, `set_volume`, `get_playback_status`, `next_track`, `prev_track`, `set_repeat_mode`, `set_shuffle` |
| **Parked** local library | 12 | `scan_library`, `get_albums`, `get_tracks`, `get_artists`, `get_playlists`, `get_track_count`, `create_playlist`, `add_to_playlist`, `remove_from_playlist`, `delete_playlist`, `import_playlist`, `export_playlist` |

The two parked groups stay registered so the build and the frontend keep compiling. They are
not part of the product.

---

## Security boundaries

- Main window: local page, `security.csp: null` in `tauri.conf.json`.
- Hidden player: remote origin, so it needs `src-tauri/capabilities/player.json`
  (`windows: ["player", "player_probe"]`, `remote.urls: ["https://music.apple.com/*"]`).
- The loopback listener binds `127.0.0.1:38471` only, accepts any POST body, and never
  authenticates the caller. It is a local state pipe for a local-only app — do not widen it.
- Secrets never enter git: `session.json`, `auth_profile/`, `*.log`, `*.db` are ignored.

---

## Known limits (not bugs, just ceilings)

- AAC 256 kbps maximum; no lossless, no Atmos.
- Apple UI features not implemented: lyrics, queue drawer, downloads, radio.
- Single window, single account, Windows only.

---

## File map

| Path | Lines | Role |
|------|-------|------|
| `src-tauri/src/lib.rs` | 1,115 | 48 commands, `PLAYER_BOOTSTRAP`, `ensure_player`, `player_command`, state listener, auth windows, lifecycle |
| `src-tauri/src/api/client.rs` | 474 | REST: search, suggestions, album, charts, library, playlists |
| `src-tauri/src/api/local_session.rs` | 397 | WebView2 cookie import |
| `src-tauri/src/api/auth.rs` | 166 | Developer token, session persistence, storefront |
| `src-tauri/src/api/models.rs` | 101 | Store models |
| `src/js/app.js` | 1,349 | All UI logic and command calls |
| `src/styles/app.css` | 1,011 | Apple-style dark theme |
| `index.html` | 176 | Shell: sidebar, main view, playback bar |
| `src-tauri/src/audio/**`, `library/**` | 2,077 | **PARKED** local engine + library |
| `src-tauri/src/drm/**` | 1,538 | **ABANDONED** native decrypt attempt |
| `src-tauri/capabilities/{default,player}.json` | — | Window permissions |
