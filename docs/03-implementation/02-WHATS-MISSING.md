# Apple Music RS — What's Missing (MVP gaps)

> Last updated: 2026-11-13 (after P0 shipped)
> Scope: only what blocks "clone Apple Music". Local file playback is parked, not listed.

**P0 is done** — playback state is truthful, seek works, and `stream_play` no longer hangs.
Details: `docs/04-bugs/03-PLAYER-STATE-CHANNEL.md` and `04-PROGRESS-BAR-SEEK.md`.

---

## Gap 1 — No transport for the streamed player (P1, next)

| Control | Today | Needed |
|---------|-------|--------|
| Next | `next_track` → parked local engine | MusicKit `skipToNextItem` / queue |
| Previous | `prev_track` → parked local engine | MusicKit `skipToPreviousItem` |
| Shuffle | `set_shuffle` → parked local engine | MusicKit `shuffleMode` |
| Repeat | `set_repeat_mode` → parked local engine | MusicKit `repeatMode` |
| Stop | `stream_stop` exists, never called from the UI | wire the control |

**Fix.** Add `stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat` commands that
push jobs through the existing queue mechanism, plus matching `PLAYER_BOOTSTRAP.__mkDo`
branches. Playback order needs a queue concept: let MusicKit own it
(`setQueue({ songs: [...] })` then skip) rather than keeping one song at a time — a
single-song queue has nothing to skip to. `publish()` should also report `shuffleMode` and
`repeatMode` so the buttons reflect the player instead of a local guess.

## Gap 2 — "Your Library" is not your Apple library (P1)

The sidebar's Library/Albums/Artists views read the **parked local SQLite** database
(`app.js:660-745`). `get_personal_library` exists and returns the user's iCloud songs but is
never called.

**Fix.** Repoint those views at `get_personal_library` + `get_cloud_playlists`. Keep the
parked renderers compiled but unreachable, or gate the nav behind a feature flag until P4.

## Gap 3 — No album detail view (P2)

`get_catalog_album` is implemented; clicking an album card does nothing.

**Fix.** New view: header (artwork, title, artist, year, track count), track list from the
album payload, click-to-play, "play album" that queues all tracks (first consumer of the
P1 queue).

## Gap 4 — Cloud playlists are half-wired (P2)

`get_cloud_playlists` renders names; `get_cloud_playlist_tracks` exists but the "Sync iCloud"
button only shows counts (`sync_cloud_library` counts and discards).

**Fix.** Render cloud playlists with artwork and open a track list; make sync either persist
or disappear.

## Gap 5 — No loading or error surface (P2)

- Clicking a song gives no "buffering" indication; if playback fails you get a silent icon flip.
- Playback errors are only logged to the console.

**Fix.** A state machine in the transport bar: idle → loading → playing → error. Now
tractable, because real state arrives.

## Gap 6 — Nothing survives restart (P3)

Session persists; last track, queue, and volume do not.

**Fix.** Persist a tiny `player_state.json` (last song id, position, volume) next to
`session.json` and restore on launch.

## Gap 7 — No keyboard or media-key control (P3)

**Fix.** Space to toggle, arrows to seek, and Win32 media keys via a global shortcut plugin
or a WebView2 key handler.

## Gap 8 — Audio ceiling (won't fix on this architecture)

AAC 256 kbps. Lossless needs either a native CDM integration or a different playback host.
Out of scope for the MVP; document it in the UI as a known limitation rather than pretending
otherwise.

---

## Recently closed

| Gap | Was | Now |
|-----|-----|-----|
| State channel | CSP killed loopback `fetch`; UI ran on a local clock; every command hung 30 s | Cookie channel `mk_state` → `start_state_poller` → `player-state`; commands return immediately |
| Seek | Progress bar drove the parked local engine and snapped to 0 % | Branches to `stream_seek`, re-anchors the smoothing clock |
| Sign-in vs player profile | Sign-in wiped the profile under a live player | Player closed first, re-warmed after success |

## Out of scope (deliberately)

Downloads/offline · lyrics · Dolby Atmos · equalizer · Discord presence · tray controls ·
multi-account · queue drag-and-drop · Last.fm scrobbling.

## Parked, decision later

Local file playback — develop or delete. See `04-PARKED-LOCAL-PLAYBACK.md`. Not a gap.
