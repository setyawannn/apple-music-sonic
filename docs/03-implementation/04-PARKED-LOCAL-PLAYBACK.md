# Parked Feature — Local File Playback

> **Status: PARKED — do not build, do not delete**
> Decided: 2026-11-13
> Owner: personal project (single maintainer)

---

## The decision

Local file playback is **frozen in place**. The code stays in the repository, it keeps
compiling, and nobody invests further work in it. It does not get a vote in MVP design
decisions, and it does not get deleted yet.

The MVP is: **clone the Apple Music client** — sign in with an existing Apple Music
subscription, browse/search/charts, open albums, and play full songs, plus the user's own
iCloud library and cloud playlists. Nothing else.

---

## What exactly is parked

| Path | What it does |
|------|--------------|
| `src-tauri/src/library/**` (6 files, 1,006 lines) | Scans `%USERPROFILE%\Music`, extracts tags, stores an index in SQLite, caches artwork, imports/exports M3U/PLS |
| `src-tauri/src/audio/{mod,queue,source}.rs` (808 lines) | symphonia decoder → rodio output, local queue, shuffle, repeat |
| `src-tauri/src/audio/stream.rs` (263 lines) | HLS parser + stream cache, used only by 30-second previews |
| `src-tauri/src/drm/**` (5 files, 1,538 lines) | Abandoned native FairPlay attempt; now only feeds a diagnostics panel |
| `play_stream_track` in `lib.rs` | Plays a 30-second AAC preview clip |
| 12 library commands + 12 local playback commands | Registered in `generate_handler!` |

Local playback is **not** an optimization. It is a separate feature: "play audio files I
already own as files". It shares no runtime code with Apple streaming.

---

## Why it is parked rather than deleted

1. The streaming MVP is not finished, so deleting now would be a distraction.
2. Removing `library/` and `audio/` would also remove a working reference for queue,
   shuffle, and repeat semantics that the MusicKit transport still has to reproduce.
3. `rusqlite` must stay in any case — `api/local_session.rs` uses it to read WebView2's
   cookie database for session import. So deleting these modules saves less than it looks.

## Why it is parked rather than developed

The parked modules currently cost us design clarity. `syncPlaybackStatus` in
`src/js/app.js` is a two-headed function: a streaming branch that returns early and a local
branch that polls `get_playback_status` every 250 ms. The progress bar wires seeking to the
local engine only. Every transport change has to be written or guarded twice. Until the
Apple path is complete and stable, adding more local surface makes that worse, not better.

---

## Interim rules (apply to every future change)

- Do **not** delete `library/`, `audio/`, or `drm/`.
- Do **not** add new local features, commands, or UI.
- Do **not** wire new UI entry points into the scanner (`scan_library`).
- Local commands stay registered so the build keeps compiling.
- The frontend stops **routing** users into the local path; the parked code simply becomes
  unreachable at runtime.
- New transport work targets the hidden MusicKit player only.

---

## Revisit — two options, decide later

Only after the MVP works end to end (auth, browse/charts, search, album detail, iCloud
library, cloud playlists, transport: next/prev/shuffle/repeat, and a truthful playback
state).

**Option A — Develop it.** "Play files you own" alongside Apple streaming. Requires solving
the UX question of one transport bar serving two players, and finishing the multi-source
state model.

**Option B — Delete it.** Removes roughly 3,600 lines plus `symphonia`, `rodio`, `walkdir`,
`uuid`, `image`, the local half of the SQLite schema, the 12 library commands, the 12 local
playback commands, and the `drm/` diagnostics commands. Keep `rusqlite` (cookie import),
`thiserror` (`local_session.rs`), `anyhow`, `chrono`, `tracing`.

Both options are reversible today. That is the point of parking.
