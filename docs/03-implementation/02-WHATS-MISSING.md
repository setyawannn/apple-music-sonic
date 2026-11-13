# Apple Music RS — What's Missing (MVP gaps)

> Last updated: 2026-11-13
> Scope: only what blocks "clone Apple Music". Local file playback is parked, not listed.

---

## Gap 1 — The player cannot talk back (P0, blocking everything)

**Symptom.** No real position, duration, or now-playing data. Progress is
`Date.now() - streamAnchor`. The play icon can revert ~30 s after pressing play.

**Cause.** `PLAYER_BOOTSTRAP` reports state with
`fetch('http://127.0.0.1:38471/state')`. music.apple.com's CSP `connect-src` blocks it before
it leaves the page. Across every recorded run the listener received **zero** requests, so
`PLAYER_READY` never becomes true and `player_command` waits 30 s
(`lib.rs:465-473`) then returns `Err`. The frontend's `catch` (`app.js:1084-1087`) resets the
optimistic state.

**Fix.** Two independent steps:

1. Delete the readiness wait. The page queue already handles ordering, so
   `player_command` should dispatch and return `Ok(())` immediately.
2. Add a CSP-proof channel. Cheapest: the injected script writes a compact JSON string to
   `document.title`, and Rust polls `webview.title()` every 250 ms, emitting `player-state`.
   No network, no capability change. Alternative: `window.ipc.postMessage` via Tauri's
   internals (more moving parts).

**Evidence:** `docs/04-bugs/03-PLAYER-STATE-CHANNEL.md`

## Gap 2 — Seeking hits the wrong player (P0)

`app.js:1205-1218` always invokes `seek_to` with `state.playback.duration`, which belongs to
the parked local engine. Clicking the progress bar during a stream snaps the bar to 0 % while
audio keeps playing.

**Fix.** Branch on `state.streamingActive` → `invoke("stream_seek", { position })` and
re-anchor `streamAnchor`. Requires Gap 1 for a truthful duration.
**Evidence:** `docs/04-bugs/04-PROGRESS-BAR-SEEK.md`

## Gap 3 — No transport for the streamed player (P1)

| Control | Today | Needed |
|---------|-------|--------|
| Next | `next_track` → parked local engine | MusicKit `skipToNextItem` / queue |
| Previous | `prev_track` → parked local engine | MusicKit `skipToPreviousItem` |
| Shuffle | `set_shuffle` → parked local engine | MusicKit `shuffleMode` |
| Repeat | `set_repeat_mode` → parked local engine | MusicKit `repeatMode` |
| Stop | never called | `stream_stop` on the UI |

**Fix.** Add `stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat` commands that
push jobs through the existing queue mechanism. `PLAYER_BOOTSTRAP.__mkDo` needs matching
branches. Playback order needs a queue concept: either let MusicKit own the queue
(`setQueue` with a song list, then skip) or keep one song at a time and compute neighbours in
Rust.

## Gap 4 — "Your Library" is not your Apple library (P1)

The sidebar's Library/Albums/Artists views read the **parked local SQLite** database
(`app.js:660-745`). `get_personal_library` exists and returns the user's iCloud songs but is
never called.

**Fix.** Repoint those views at `get_personal_library` + `get_cloud_playlists`. Keep the
parked renderers compiled but unreachable, or gate the nav behind a feature flag until P4.

## Gap 5 — No album detail view (P2)

`get_catalog_album` is implemented; clicking an album card does nothing.

**Fix.** New view: header (artwork, title, artist, year, track count), track list from the
album payload, click-to-play, "play album" that queues all tracks.

## Gap 6 — Cloud playlists are half-wired (P2)

`get_cloud_playlists` renders names; `get_cloud_playlist_tracks` exists but the "Sync iCloud"
button only shows counts (`sync_cloud_library` at `lib.rs:791` counts and discards).

**Fix.** Render cloud playlists with artwork and open a track list; make sync either persist
or disappear.

## Gap 7 — No loading or error surface (P2)

- Clicking a song gives no "buffering" indication; if playback fails you get a silent icon flip.
- `stream_play` returning `Err` is only logged to the console.

**Fix.** A small state machine in the transport bar: idle → loading → playing → error, driven
by Gap 1's real state.

## Gap 8 — Nothing survives restart (P3)

Session persists; last track, queue, and volume do not.

**Fix.** Persist a tiny `player_state.json` (last song id, position, volume) next to
`session.json` and restore on launch.

## Gap 9 — No keyboard or media-key control (P3)

**Fix.** Space to toggle, arrows to seek, and Win32 media keys via a global shortcut plugin
or a WebView2 key handler.

## Gap 10 — Audio ceiling (won't fix on this architecture)

AAC 256 kbps. Lossless needs either a native CDM integration or a different playback host.
Out of scope for the MVP; document it in the UI as a known limitation rather than pretending
otherwise.

---

## Out of scope (deliberately)

Downloads/offline · lyrics · Dolby Atmos · equalizer · Discord presence · tray controls ·
multi-account · queue drag-and-drop · Last.fm scrobbling.

## Parked, decision later

Local file playback — develop or delete. See `04-PARKED-LOCAL-PLAYBACK.md`. Not a gap.
