# Apple Music RS — To-Do

> Last updated: 2026-11-13
> **Scope: clone Apple Music only.** Local file playback is parked
> (`04-PARKED-LOCAL-PLAYBACK.md`) — not on this list, not being deleted.

Each item names the file to touch and the definition of done. Work top to bottom; P0 items
are prerequisites for testing anything below them.

---

## P0 — Make playback truthful

### P0.1 Stop waiting on a signal that never arrives
**File:** `src-tauri/src/lib.rs` (`player_command`)
- Dispatch the job and return `Ok(())` immediately.
- Remove the `PLAYER_READY` wait and the 30 s timeout, or keep them as pure observation with
  no `Err` path.
- Keep `PLAYER_READY` / `/ready` if useful for logging, but never block on it.

**Done when:** `stream_play` resolves in milliseconds and the optimistic UI state is never
reset by a timeout.

### P0.2 Give the player a CSP-proof state channel
**Files:** `lib.rs` (`PLAYER_BOOTSTRAP`, `start_state_listener`), `src/js/app.js`
- In the bootstrap, write state to `document.title` on transitions and on a 250–500 ms tick:
  `{playing, position, duration, id, title, artist, artwork}`.
- In Rust, poll `webview.title()` on a 250 ms task, dedupe by value, emit `player-state`.
- Frontend listener already exists (`app.js:186-219`) and needs no change if the payload
  shape is preserved.
- Keep the TCP listener temporarily for debugging, then delete it and the `/debug` probe loop.

**Done when:** the playback bar shows the player's real position and duration, artwork and
title come from the page, and pausing updates the icon without a click.

### P0.3 Wire seek to the streaming player
**File:** `src/js/app.js` (`progressBar` handler)
- If `state.streamingActive`: `invoke("stream_seek", { position })`, set
  `streamPosition`, re-anchor `streamAnchor`, update the fill.
- Otherwise keep the existing `seek_to` path (dormant local engine).

**Done when:** clicking the bar mid-stream jumps the audio and the bar stays where you put it.

### P0.4 Remove the dead-player probes
**File:** `lib.rs`
- Delete the 5-second `probe_*` loop and the `/debug` handler once P0.2 lands.

---

## P1 — Real transport

### P1.1 Queue model
Decide and implement one of:
- **A.** MusicKit owns the queue: `setQueue({ songs: [...] })` for album/playlist playback,
  then `skipToNextItem` / `skipToPreviousItem`.
- **B.** One song at a time, with Rust computing neighbours from the current view.

A is less code and matches Apple's semantics; recommend A.

### P1.2 New commands + bootstrap branches
`stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat`, plus matching
`__mkDo` cases (`skipToNextItem`, `skipToPreviousItem`, `m.shuffleMode`, `m.repeatMode`).

**Done when:** next/prev/shuffle/repeat work while streaming and the shuffle/repeat buttons
reflect the player's state.

### P1.3 Stop
Wire a stop control to `stream_stop`, then clear streaming state in the frontend.

---

## P2 — Surface the Apple library

### P2.1 Library tab = iCloud library
Replace the local-DB renderers with `get_personal_library` (songs) and derive albums/artists
client-side or with a small backend grouping. Do not delete the parked renderers.

### P2.2 Album detail view
New view driven by `get_catalog_album`: artwork, metadata, track list, play-all, per-track
play. Clicking an album card anywhere opens it.

### P2.3 Cloud playlists
`get_cloud_playlists` with artwork → open → `get_cloud_playlist_tracks` → play. Decide the
fate of `sync_cloud_library` (persist or remove).

### P2.4 Welcome/first-run
Replace "Scan My Music" with the sign-in path. Keep the scanner reachable only from the
parked code path, not the main flow.

---

## P3 — Polish

- Transport state machine: idle / loading / playing / paused / error, with a visible
  "buffering" state (depends on P0.2).
- Non-256 kbps / unavailable track errors surfaced in the UI, not just `console.error`.
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
- Run `cargo check --no-default-features` after every `lib.rs` change.
- One player, one state source. If a change needs a second source of truth, stop and fix the
  first one.
