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

`src/js/app.js` reads `data-catalog-id` off the track row, collects the ids of every row in
the same list, and:

```js
state.streamingActive  = true;      // optimistic UI
state.streamIsPlaying  = true;
state.streamPosition   = 0;
state.streamAnchor     = Date.now();
invoke("stream_play", { songId: catalogId, queue: idsOfVisibleList, shuffle: null });
```

The queue is the list the user is looking at (album, search result, playlist, library
songs), so MusicKit's next/previous/shuffle/repeat move through exactly that list. Title,
artist, and artwork are written to the playback bar **before** the command is sent, so the
UI never sits empty while the player wakes up.

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
  if (cmd === "play") {
    const spec = JSON.parse(String(arg));             // { startWith, songs, shuffle }
    if (spec.songs.length > 1) {
      await m.setQueue({ songs: spec.songs, startWith: spec.startWith });
    } else {
      await m.setQueue({ song: String(spec.startWith) });
    }
    if (spec.shuffle === true)  m.shuffleMode = 1;    // album Shuffle / Play
    if (spec.shuffle === false) m.shuffleMode = 0;
    return m.play();
  }
  if (cmd === "pause")    return m.pause();
  if (cmd === "resume")   return m.play();
  if (cmd === "stop")     return m.stop();
  if (cmd === "seek")     return m.seekToTime(Number(arg));
  if (cmd === "next")     return m.skipToNextItem();
  if (cmd === "previous") {
    if (m.currentPlaybackTime > 3) return m.seekToTime(0);  // Apple's own rule
    return m.skipToPreviousItem();
  }
  if (cmd === "shuffle") { m.shuffleMode = Number(arg) ? 1 : 0; return; }
  if (cmd === "repeat")  { m.repeatMode  = Number(arg) || 0; return; }
  if (cmd === "volume")  { m.volume = Math.max(0, Math.min(1, Number(arg))); return; }
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
| State feedback | Works, but the only path out of the page is a cookie (CSP blocks fetch, WebView2 blocks the title route). See below. |
| Startup | Mitigated by pre-warming the player during `setup()`. |

Probe results recorded on this machine:

```
com.microsoft.playready : ok / ok / ok      ← used
com.widevine.alpha      : ok / NotSupportedError / ok
com.apple.fps           : NotSupportedError ×3
MSE                     : aac = true, alac = false, ec3 = false
```

---

## Playback state — the cookie channel

The page reports on real transitions (`playbackStateDidChange`, `nowPlayingItemDidChange`,
`playbackTimeDidChange`) and on a 500 ms tick. It cannot report over HTTP: music.apple.com's
CSP blocks loopback `fetch()`. It cannot report through the window title either — WebView2 does
not mirror `document.title` to the native title (measured; the title stayed
`"Apple Music Player"`).

It reports through a **cookie**, which is first-party and CSP-immune:

```js
document.cookie = 'mk_state=' + encodeURIComponent(json) +
                  '; path=/; max-age=300; Secure; SameSite=Lax';
```

```json
{"isPlaying":true,"position":17,"duration":207,"id":"1696166346",
 "title":"XXL","artist":"LANY","album":"a beautiful blur",
 "artwork":"https://is1-ssl.mzstatic.com/...",
 "shuffle":0,"repeat":0,"volume":0.52,"queueLength":10}
```

The transport fields are why the shuffle/repeat buttons and the volume bar show the player's
real state rather than a local guess, and `queueLength` is how we know the click actually
created a multi-song queue.

`start_state_poller()` runs a 250 ms task in Rust that:

1. reads `window.cookies()` (the same API the sign-in flow uses for `media-user-token`),
2. finds `mk_state`, accepting either a percent-encoded or already-decoded value,
3. rejects anything that is not JSON, dedupes by full string,
4. emits the `player-state` Tauri event, which `app.js` listens for,
5. writes the last track's state to `player_state.json` (throttled to song changes and ~5 s
   position steps) so the next launch can restore it — a report with a null `id` never
   overwrites the file.

Because the payload is deduped, a paused player emits nothing and a playing one emits once per
second. The frontend still smooths between reports with `Date.now() - streamAnchor`, but the
anchor is re-set from every real report, so it cannot drift.

## Transport state machine

The bar must never claim to be playing while Apple's player is still buffering, so the
frontend runs an explicit phase: `idle → loading → playing ⇄ paused`, or `error`.

- `optimisticallyPlay()` enters `loading` and starts a 12 s watchdog.
- The first `player-state` for that song id clears `loading`; reports for a *different* song
  (the previous track) are ignored while a load is pending.
- If the watchdog fires, the phase becomes `error` and a toast explains it. Before this, a
  failed song just flipped the icon with a `console.error`.
- The play button shows a spinner (`#play-spinner`) while loading.

Title/artist/artwork still paint optimistically before the command is sent, so the bar is
never empty; only the *transport* state waits for truth.

## Keyboard shortcuts

`app.js` binds a global keydown (ignored while typing or while a modal is open):

| Key | Action |
|-----|--------|
| Space | Play / pause |
| ← / → | Seek −5 s / +5 s |
| ↑ / ↓ | Volume +5 % / −5 % |
| ⌘/Ctrl + ← / → | Previous / next track |
| `M` | Mute toggle |

Media keys are not bound: they need a native/global hook, tracked in `02-WHATS-MISSING.md`.

## Restore on launch

`stream_play` writes the queue to `player_queue.json`; the poller writes the last state to
`player_state.json` (both under `%APPDATA%\com.applemusicrs.desktop\`). `get_last_player_state`
returns them together. On launch `restoreLastSession()` repaints the bar **paused** (Apple
Music does not auto-play on launch) and keeps the queue; the first press of play rebuilds the
queue via `stream_play` and seeks back to the saved position.

## Up Next — the queue drawer

`m.queue` can be thousands of items, and JSON's quotes/braces would blow WebView2's per-cookie
limit, so the page publishes only the current item plus the next 40, **base64-encoded**, in a
second cookie `mk_queue`, and only when the list actually changes:

```js
document.cookie = 'mk_queue=' + btoa(unescape(encodeURIComponent(json))) + '; path=/; …';
```

`start_state_poller` decodes it (`base64_decode`) and emits `player-queue`. The `#queue-btn`
badge shows the length; the drawer renders each row and highlights the current `id` from
`player-state`. Clicking a row invokes `stream_jump_to`, which runs
`m.changeToMediaAtIndex(index)` — an absolute MusicKit queue index (the drawer's `base` plus
the row offset). Verified live: `[PLAYER] queue items=5` after playback started.

The queue list is intentionally sliced from the *current* position: that is what "Up Next"
means, and it keeps the payload small. Previous items reappear as you skip back.

The old design — a loopback TCP listener on `127.0.0.1:38471` with a `/ready` handshake and a
30 s command timeout — was deleted: it never received a single packet, and the timeout made the
play icon flip back after 30 s. History and evidence: `docs/04-bugs/03-PLAYER-STATE-CHANNEL.md`.

---

## Playlists — read and write

Playlists are the user's **Apple Music library playlists**. The local SQLite playlists
(`library/**`) are parked and no longer reachable from the UI.

Reading: `get_cloud_playlists` → `/v1/me/library/playlists`; opening one calls
`get_cloud_playlist_tracks`. Rows play through the same MusicKit queue path as everything else.

Writing (all verified against a live session during a scratch create→add→rename→delete probe):

| Action | Request | Result |
|--------|---------|--------|
| Create | `POST /v1/me/library/playlists?l={lang}` `{attributes:{name}}` | 201 + new `p.…` id |
| Rename | `PUT /v1/me/library/playlists/{id}` `{attributes:{name}}` — **no `?l=`** | 204 |
| Delete | `DELETE /v1/me/library/playlists/{id}` | 204 |
| Add tracks | `POST /v1/me/library/playlists/{id}/tracks?l={lang}` `{data:[{id,type}]}` | 204 |
| Remove track | — | ❌ 404 (not exposed) |

Two quirks are load-bearing and easy to regress:

- **Rename must not carry the `?l=` query parameter.** With it, Apple returns 500; without it,
  204. Create/add/delete *do* take `?l=`.
- **Track `type` depends on the id.** Catalog ids (numeric) use `type: "songs"`; the user's
  library songs (`i.` prefix) use `type: "library-songs"` — see `library_track_type`.

**Remove-track is not supported** by this API: `DELETE …/tracks/{id}` and
`…/relationships/tracks` both 404, on both hosts. The UI therefore does not offer it.

On the frontend, every catalog track row gets a `+` button injected by a single
`MutationObserver` (`decorateTrackRows`) rather than by editing each row template; the `+`
opens a picker of the user's editable playlists. Create and rename share one modal
(`openPlaylistFormModal`); `⋯` on a `can_edit` playlist opens Rename/Delete.

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

The sign-in flow **deletes the shared `auth_profile` directory** before opening the login
window, and the hidden player uses that same directory. `start_apple_sign_in` therefore closes
the `player` window first, waits for WebView2 to release the profile lock, and re-warms the
player after a successful sign-in.

---

## Command surface (58 commands)

| Group | Count | Commands |
|-------|-------|----------|
| Streaming player | 12 | `stream_play`, `stream_pause`, `stream_resume`, `stream_stop`, `stream_seek`, `stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat`, `stream_volume`, `get_last_player_state`, `stream_jump_to` |
| Apple Music API + auth | 19 | `start_apple_sign_in`, `set_user_session`, `sign_out`, `get_auth_state`, `set_storefront`, `search_catalog`, `get_search_suggestions`, `get_catalog_album`, `get_charts`, `get_personal_library`, `get_recommendations`, `get_cloud_playlists`, `get_cloud_playlist_tracks`, `sync_cloud_library`, `create_cloud_playlist`, `rename_cloud_playlist`, `delete_cloud_playlist`, `add_to_cloud_playlist`, `play_stream_track` |
| Native bridge + diagnostics | 3 | `get_native_bridge_status`, `import_local_apple_session`, `run_native_bridge_diagnostics` |
| **Parked** local playback | 12 | `play_track`, `play_album`, `pause_playback`, `resume_playback`, `stop_playback`, `seek_to`, `set_volume`, `get_playback_status`, `next_track`, `prev_track`, `set_repeat_mode`, `set_shuffle` |
| **Parked** local library | 12 | `scan_library`, `get_albums`, `get_tracks`, `get_artists`, `get_playlists`, `get_track_count`, `create_playlist`, `add_to_playlist`, `remove_from_playlist`, `delete_playlist`, `import_playlist`, `export_playlist` |

The two parked groups stay registered so the build and the frontend keep compiling. They are
not part of the product.

---

## Security boundaries

- Main window: local page, `security.csp: null` in `tauri.conf.json`.
- Hidden player: remote origin, so it needs `src-tauri/capabilities/player.json`
  (`windows: ["player"]`, `remote.urls: ["https://music.apple.com/*"]`).
- The `mk_state` cookie is the only data path out of the player page: first-party,
  same-site, `Secure`, `max-age=300`, path `/`. It carries no credentials — only playback
  position and now-playing metadata.
- Secrets never enter git: `session.json`, `auth_profile/`, `*.log`, `*.db` are ignored.

---

## Known limits (not bugs, just ceilings)

- AAC 256 kbps maximum; no lossless, no Atmos.
- Apple UI features not implemented: lyrics, downloads, radio stations.
- Single window, single account, Windows only.

---

## File map

| Path | Lines | Role |
|------|-------|------|
| `src-tauri/src/lib.rs` | 1,450 | 58 commands, `PLAYER_BOOTSTRAP` (queue + transport + publish + Up Next), `ensure_player`, `player_command`, `start_state_poller` (state + restore persistence), `base64_decode`, playlist write commands, auth windows, lifecycle |
| `src-tauri/src/api/client.rs` | 662 | REST: search, suggestions, album, charts, library, playlists (read + create/rename/delete/add-tracks) |
| `src-tauri/src/api/local_session.rs` | 397 | WebView2 cookie import |
| `src-tauri/src/api/auth.rs` | 166 | Developer token, session persistence, storefront |
| `src-tauri/src/api/models.rs` | 101 | Store models |
| `src/js/app.js` | 2,357 | All UI logic and command calls (iCloud library, album detail, queue-aware playback, transport state machine, Up Next drawer, playlist read/write, keyboard shortcuts, resume) |
| `src/styles/app.css` | 1,407 | Apple-style dark theme |
| `index.html` | 176 | Shell: sidebar, main view, playback bar |
| `src-tauri/src/audio/**`, `library/**` | 2,077 | **PARKED** local engine + library |
| `src-tauri/src/drm/**` | 1,538 | **ABANDONED** native decrypt attempt |
| `src-tauri/capabilities/{default,player}.json` | — | Window permissions |
