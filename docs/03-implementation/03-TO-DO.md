# Apple Music RS — To-Do

> Last updated: 2026-11-13
> **Scope: clone Apple Music only.** Local file playback is parked
> (`04-PARKED-LOCAL-PLAYBACK.md`) — not on this list, not being deleted.

Each item names the file to touch and the definition of done. Work top to bottom.

---

## P0 — Make playback truthful — ✅ DONE (2026-11-13)

### P0.1 Stop waiting on a signal that never arrives — ✅
`player_command` dispatches the job and returns `Ok(())`. The `PLAYER_READY` wait and the
30 s timeout are gone. Verified: `stream_play` resolves immediately and the icon no longer
snaps back.

### P0.2 Give the player a CSP-proof state channel — ✅
The title channel was tried and **failed** (WebView2 does not mirror `document.title`). The
cookie channel works: the page writes `mk_state`, `start_state_poller` reads
`window.cookies()` every 250 ms and emits `player-state`. Verified live — real position,
duration, title, artist, artwork.

### P0.3 Wire seek to the streaming player — ✅
`app.js` branches on `state.streamingActive` → `stream_seek`, re-anchors `streamAnchor`,
updates the fill. Ignored when duration is unknown instead of snapping to 0.

### P0.4 Remove the dead-player probes — ✅
Loopback listener, `/debug` handler, probe loop, `probe_report` (48 → 47 commands),
`PLAYER_READY`, `PLAYER_STATE_PORT`, and the `player_probe` capability entry are all gone.

### P0.5 Auth profile lifecycle — ✅
`start_apple_sign_in` closes the `player` window, waits for the profile lock to release,
wipes the profile, and re-warms the player after success.

**Outcome:** the playback bar now reports the player's own state. This is the foundation P1
and P2 build on.

---

## P1 — Real transport — ✅ DONE (2026-11-13)

### P1.1 Queue model — ✅ A
MusicKit owns the queue. `stream_play` takes the ids of the visible list
(`queue: Vec<String>`) plus `startWith`; the bootstrap calls
`setQueue({ songs, startWith })`. Verified live: `queueLength` reports 10 for an album
queue and next/previous now have somewhere to go.

### P1.2 New commands + bootstrap branches — ✅
`stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat`, `stream_volume`
(52 commands total). `__mkDo` branches: `skipToNextItem`, `skipToPreviousItem`
(restart-if->3s, Apple's own rule), `m.shuffleMode`, `m.repeatMode`, `m.volume`.
`publish()` now also reports `album`, `shuffle`, `repeat`, `volume`, `queueLength`, so
the buttons repaint from the player's state instead of a local guess.
Verified live: `cmd=volume` moves the player's own `volume` value and the bar follows.

### P1.3 Stop
`stream_stop` exists and reaches `m.stop()`. There is no stop control in the UI because
Apple Music has none either (transport is play/pause + next/prev).

---

## P2 — Surface the Apple library — ✅ DONE (2026-11-13)

### P2.1 Library tab = iCloud library — ✅
`renderLibrary` / `renderAlbums` / `renderArtists` load `get_personal_library` and derive
albums/artists client-side (`deriveLibraryAlbums`, `deriveLibraryArtists`). The Library
badge shows the iCloud song count. The scanner and local renderers remain parked in the
codebase and are no longer reachable from the main views.

### P2.2 Album detail view — ✅
`showAlbumDetail` → `get_catalog_album` → header (artwork, year, song count, total time)
plus Play and Shuffle pills and a full track list. Any catalog album card opens it;
`Play` builds a MusicKit queue, `Shuffle` does the same with shuffle mode on. iCloud
album cards open `showLibraryAlbumDetail`, artist cards open `showArtistDetail`.

### P2.3 Cloud playlists — ✅
`get_cloud_playlists` cards open `showCloudPlaylistDetail` →
`get_cloud_playlist_tracks`, and every row plays through the queue path. Local playlist
cards still open the parked local detail view; that route is not promoted in the UI.
`sync_cloud_library` is still registered but unused by the UI.

### P2.4 Welcome/first-run — ✅
`renderWelcome` is a sign-in CTA when signed out and a "Go to Listen Now" shortcut when
signed in. The scanner is no longer offered.

### P2.5 Browse + Radio — ✅ / honest gap
`renderBrowse` shows Top Albums and Top Songs from `get_charts`. Radio needs Apple's
stations API, which is not wired up; `renderRadio` says so instead of showing a fake page.

---

## P3 — Polish

- Transport state machine: idle / loading / playing / paused / error, with a visible
  "buffering" state. Real state is now available, so this is finally possible.
- Unavailable-track errors surfaced in the UI, not just `console.error`.
- Restore last track, position, and volume on launch.
- Space to play/pause, arrows to seek, Windows media keys.
- Now Playing view and a queue drawer.
- Radio: Apple stations API.
- Library album grouping currently keys on `artist + album name` because the library song
  model carries no collection id; add one if Apple returns it for library songs.

---

## P4 — Decide the parked feature

Only after P0–P3: develop local file playback, or delete it. Both paths are written out in
`04-PARKED-LOCAL-PLAYBACK.md`.

---

## Guardrails while working

- Never route new UI into `scan_library` / `play_track` / `seek_to`.
- Never extend `drm/**`.
- Never add a playback readiness handshake; the page queue already orders commands.
- Never use loopback `fetch` or `document.title` for state — both are proven dead. Use the
  `mk_state` cookie.
- Run `cargo check --no-default-features` after every `lib.rs` change.
- One player, one state source. If a change needs a second source of truth, stop and fix the
  first one.
