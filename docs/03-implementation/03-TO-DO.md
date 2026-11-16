# Apple Music RS — To-Do

> Last updated: 2026-11-15
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
`stream_next`, `stream_previous`, `stream_shuffle`, `stream_repeat`, `stream_volume` (58
commands today, after P3 added `get_last_player_state`/`stream_jump_to` and the playlist
writes). `__mkDo` branches: `skipToNextItem`, `skipToPreviousItem`
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

### P2.6 Playlist sync (two-way) — ✅ DONE (2026-11-13)
Playlists are the user's Apple Music library playlists; the local SQLite playlist UI is gone
and `get_playlists` is no longer called. Four write commands (`create_cloud_playlist`,
`rename_cloud_playlist`, `delete_cloud_playlist`, `add_to_cloud_playlist`) hit Apple's
`/v1/me/library/playlists` endpoints, all measured live (create 201, rename 204, delete 204,
add 204). Frontend: create/rename share a modal, `⋯` gives Rename/Delete for `can_edit`
playlists, and a `+` on any track row opens a playlist picker. Verified via a scratch
create→add→rename→delete probe (and the scratch playlist was deleted).

**Not done:** removing a track from a playlist — Apple returns 404 on every form of the
endpoint (see `HOW-IT-WORKS`). Left out rather than faking it.

---

## P3 — Polish — ✅ DONE (2026-11-15)

### P3.1 Transport state machine + error surface — ✅ DONE (2026-11-13)
`app.js` tracks `streamPhase` (idle / loading / playing / paused / error). A play command
enters `loading`; the first real `player-state` for that song clears it, and a 12 s watchdog
turns a song that never starts into `error`. The play button shows a spinner while loading and
a dismissible toast reports failures (previously only `console.error`). Verified: `node
--check`, `npm run build`, `cargo check`, full test suite.

### P3.2 Resume on launch — ✅ DONE (2026-11-13, queue path compile-verified)
`start_state_poller` writes the last track's state to
`%APPDATA%\com.applemusicrs.desktop\player_state.json` (throttled to song changes and ~5 s
steps); `stream_play` writes the queue to `player_queue.json`. New command
`get_last_player_state` (58 commands at the time) returns both. On launch `restoreLastSession()` repaints
the bar paused and remembers the queue; the first press of play rebuilds the queue and seeks
back to the saved position. Live-verified: the state file was written with the real track and
the null-id reset report was correctly ignored. The queue file/rebuild path has not been
observed live yet.

### P3.3 Keyboard shortcuts — ✅ DONE (2026-11-13)
Space toggles play/pause, ←/→ seek ±5 s, ↑/↓ adjust volume ±5 %, ⌘/Ctrl + ←/→ skip tracks,
`M` mutes. Ignored while typing or while a modal is open.

### P3.4 Up Next queue drawer — ✅ DONE (2026-11-13)
The page publishes the current item + next 40 as base64 JSON in a second cookie `mk_queue`;
`start_state_poller` decodes it (new `base64_decode`) and emits `player-queue`. The transport
bar's queue button shows the length; the drawer lists the rows, highlights the playing one, and
clicking a row calls `stream_jump_to` → `changeToMediaAtIndex` (58 commands at the time). Verified live:
`[PLAYER] queue items=5`.

### P3.5 Windows media keys + now-playing flyout — ✅ DONE (2026-11-15)
`player/media.rs` binds `ISystemMediaTransportControlsInterop` to the **main window** (the
flyout must point at the window the user is looking at, not the hidden player) and maps OS
button presses to the same `player_command`s the UI sends: resume/pause/stop/next/previous.
The flyout's title/artist/album/artwork/status mirror the `player-state` blob, so it lies
exactly as much as the transport bar does — which is never. No global-shortcut plugin was
added; SMTC is the platform API for this.

Two traps cost a crash each, both documented in `AGENTS.md`: the interop interface is
**missing from the `windows` crate metadata**, so `InteropVtbl` is declared by hand (7 slots),
and the activation factory must be requested **by the interop IID**. Asking for `IUnknown`
returns the object's primary vtable, and reading a fixed slot from it access-violates
(`0xc0000005`). Registration failure is logged and non-fatal, so the app still runs without
media keys. Dep: target-gated `windows = "0.62"` (same version Tauri uses, so `HWND` unifies).

### P3.6 Full-screen Now Playing view — ✅ DONE (2026-11-15)
Click the transport bar's artwork or title → `#now-playing-view`; `Esc` or the chevron closes
it. It is a **mirror, never a second source of truth**: `paintNowPlaying()` reads `state.stream*`
(filled by `applyPlayerState`), its buttons forward to the bar's own handlers, and
`seekToFraction` / `setVolumeFraction` are shared by both bars. Artwork is guarded by URL so
the ~1 Hz state ticks never rebuild the `<img>` (the flicker rule from bug 06).

### Remaining (not transport work)
- Radio: Apple stations API (`renderRadio` is an honest placeholder).
- Library album grouping currently keys on `artist + album name` because the library song
  model carries no collection id; add one if Apple returns it for library songs.
- Removing a track from a playlist: Apple returns 404 on every endpoint form.

---

## P4 — Karaoke lyrics — ✅ DONE (2026-11-17)

### P4.0 Measure the contract — ✅ DONE
Probed the live session before writing code (probe deleted afterwards):
- `GET /v1/catalog/{sf}/songs/{catalogId}/lyrics?l={lang}` → 200 with `data[0].attributes.ttml`.
- `GET …/songs/{i.…}/lyrics` → **404**; lyrics are catalog-scoped.
- `playParams.catalogId` **exists** on library songs (`i.4YBNbkzHx3QbVDr` → `1850019038`).
- Sweep of 40 library songs: **36 `itunes:timing="Line"`**, **3 `"None"`**, **1 404**,
  **0 with word/syllable spans**. Two real payloads saved as fixtures.

### P4.1 Parse — ✅ DONE
`src-tauri/src/api/lyrics.rs` (hand-written XML tokenizer, no new dependency) →
`Lyrics { lines, timing, word_timed, language, leading_silence_ms }`. Handles `<p begin end>`,
timed `<span>`s with `ttm:role="x-bg"`, `<br/>`, entities, CDATA/comments, and the mixed
`44.851` / `1:04.163` / `1:02:03.5` time formats. Lines without timed spans get interpolated
word windows (proportional to word length); `word_timed` records which is which. 12 tests over
the captured fixtures.

### P4.2 Fetch — ✅ DONE
`ApiClient::get_song_lyrics` (404 → `Ok(None)`) and `ApiClient::resolve_catalog_id` (library id →
catalog twin). Command `get_lyrics(song_id, catalog_id)` — 59 commands today. `bootstrap.js`
publishes `playParams.catalogId` in `mk_state` so the common path needs no lookup.

### P4.3 Paint — ✅ DONE
Lyrics pane inside `#now-playing-view`, replacing the artwork column (`.np-stage.lyrics-on`),
`L` or the header button toggles it. The DOM is built once per track; a `requestAnimationFrame`
loop reads the smoothed `state.streamAnchor` and flips `sung` / `current` classes only when the
active line or word changes; the loop stops when the view closes or playback pauses. Clicking a
line seeks to it. Unsynchronised lyrics render as a static block, and a 404 renders
"Lyrics aren't available" — no invented content.

### P4.4 Docs — ✅ DONE
TTML contract recorded in `docs/05-reference/03-API-CONTRACTS.md` §3.6; lyrics promoted from
the non-goal list into the MVP in `AGENTS.md` (+ mirror), `README.md`, `01-STRATEGY.md`;
`[lyrics]` diagnostics in the console log.

---

## P5 — Decide the parked feature

Develop local file playback, or delete it. Both paths are written out in
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
- Lyrics: never scrape the hidden page for them, never rebuild the lyric DOM per frame (build
  once, toggle classes), and never present interpolated word windows as Apple's timing.
