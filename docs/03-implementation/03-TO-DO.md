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

## P1 — Real transport (next)

### P1.1 Queue model
Decide and implement one of:
- **A.** MusicKit owns the queue: `setQueue({ songs: [...] })` for album/playlist playback,
  then `skipToNextItem` / `skipToPreviousItem`.
- **B.** One song at a time, with Rust computing neighbours from the current view.

A is less code and matches Apple's semantics; recommend A. It also gives next/prev something
real to skip within, which a single-song queue does not.

### P1.2 New commands + bootstrap branches
`stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat`, plus matching `__mkDo`
cases (`skipToNextItem`, `skipToPreviousItem`, `m.shuffleMode`, `m.repeatMode`).

Reflect the result: `publish()` should also report `shuffleMode` and `repeatMode` so the
buttons show the player's state instead of a local guess.

**Done when:** next/prev/shuffle/repeat work while streaming and the buttons stay in sync.

### P1.3 Stop
Wire a stop control to `stream_stop`, then clear streaming state in the frontend.

---

## P2 — Surface the Apple library

### P2.1 Library tab = iCloud library
Replace the local-DB renderers with `get_personal_library` (songs) and derive albums/artists
client-side or with a small backend grouping. Do not delete the parked renderers.

### P2.2 Album detail view
New view driven by `get_catalog_album`: artwork, metadata, track list, play-all, per-track
play. Clicking an album card anywhere opens it. Play-all is the natural first consumer of the
P1.1 queue.

### P2.3 Cloud playlists
`get_cloud_playlists` with artwork → open → `get_cloud_playlist_tracks` → play. Decide the
fate of `sync_cloud_library` (persist or remove).

### P2.4 Welcome/first-run
Replace "Scan My Music" with the sign-in path. Keep the scanner reachable only from the
parked code path, not the main flow.

---

## P3 — Polish

- Transport state machine: idle / loading / playing / paused / error, with a visible
  "buffering" state. Real state is now available, so this is finally possible.
- Unavailable-track errors surfaced in the UI, not just `console.error`.
- Restore last track, position, and volume on launch.
- Space to play/pause, arrows to seek, Windows media keys.
- Now Playing view and a queue drawer.

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
