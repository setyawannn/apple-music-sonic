# Apple Music RS — What's Done

> Last updated: 2026-11-13 (after P1 + P2 shipped; P3 in progress)
> Status claims here are verified against the code or a recorded run. Optimistic claims from
> earlier versions of this document were removed.

**Legend:** ✅ verified working · ⚠️ works but approximate/fragile · ❌ not wired · ⏸ parked · ⚰ abandoned

---

## 1. Authentication — ✅

| Item | Evidence |
|------|----------|
| Public MusicKit developer token committed | `api/auth.rs` `DEFAULT_APPLE_MUSIC_DEV_TOKEN` (not a user credential) |
| Token refresh at startup | `refresh_developer_token_from_web()` scrapes music.apple.com's JS bundle |
| Apple ID sign-in window | `start_apple_sign_in` → WebView2 at `music.apple.com/login` |
| `media-user-token` capture | polls `window.cookies()` once per second; **HttpOnly, so JS injection cannot read it** — the cookie jar is the only route |
| Token validation before persisting | `validate_music_user_token` against Apple; dead tokens are never saved |
| Session persistence | `%APPDATA%\AppleMusicRS\session.json` |
| Session import from official app | `import_local_apple_session` reads the WebView2 cookie SQLite |
| Storefront/language | `set_storefront`, persisted |
| Sign out | `sign_out` clears token + disk |
| Player survives sign-in | profile closed before the wipe, re-warmed after success |

Verified end to end: `/v1/me/library/songs` returned 200 with a live session.

## 2. Streaming playback — ✅

| Item | Status | Evidence |
|------|--------|----------|
| Full-song playback | ✅ | user confirmed tracks play from 0:00, not 30 s previews |
| Hidden player webview | ✅ | `ensure_player()`, label `player`, `visible(false)` |
| Pre-warm at startup | ✅ | `setup()` calls `ensure_player()` |
| Page-side command queue | ✅ | `window.__mkQueue` + `__mkDrain` in `PLAYER_BOOTSTRAP` |
| Nonce dedupe | ✅ | fresh click = fresh nonce; re-dispatch = no-op |
| Commands never block | ✅ | `player_command` dispatches and returns `Ok(())` |
| Play / pause / resume / stop | ✅ | `stream_*` commands |
| Truthful state feedback | ✅ | `mk_state` cookie → `start_state_poller` → `player-state`; real position, duration, title, artist, artwork verified in `dev-run.log` |
| Seek | ✅ | progress bar branches to `stream_seek` while streaming |
| Next / previous | ✅ | MusicKit queue: `stream_next` → `skipToNextItem`; `stream_previous` restarts the song if >3 s in, else `skipToPreviousItem` |
| Shuffle / repeat | ✅ | `stream_shuffle` / `stream_repeat` set `shuffleMode` / `repeatMode`; `player-state` reports them back so the buttons stay truthful |
| Volume | ✅ | `stream_volume` sets `m.volume`; reported back and mirrored in the volume bar (verified live: `cmd=volume`, value changed) |
| Queue | ✅ | the clicked list's ids ride with `stream_play`; `setQueue({ songs, startWith })`. Verified live: `queueLength: 10` |
| Audio quality | ⚠️ | AAC 256 kbps ceiling (no ALAC decoder in WebView2, EME output not readable) |

## 3. Apple Music catalog API — ✅

Backend methods in `api/client.rs` are implemented and exercised:

| Capability | Command | Wired to UI |
|------------|---------|-------------|
| Catalog search | `search_catalog` | ✅ |
| Live typeahead | `get_search_suggestions` | ✅ |
| Album detail + tracklist | `get_catalog_album` | ✅ album detail view (Play + Shuffle queue) |
| Charts | `get_charts` | ✅ Listen Now fallback + Browse |
| Recommendations | `get_recommendations` | ✅ (Listen Now) |
| Personal iCloud songs | `get_personal_library` | ✅ Library / Albums / Artists, preloaded at sign-in; **paged** (bug 08) |
| Cloud playlists (read) | `get_cloud_playlists` | ✅ Playlists tab + sidebar; auto-synced (no manual button). Note: must omit `include=tracks` (400) — see bug 05 |
| Cloud playlist tracks | `get_cloud_playlist_tracks` | ✅ detail with Play/Shuffle, rows play via the queue; **paged**, logs `[PLAYLIST] N tracks` |
| Create / rename / delete playlist | `create_cloud_playlist`, `rename_cloud_playlist`, `delete_cloud_playlist` | ✅ writes to Apple Music (measured: 201/204/204) |
| Add tracks to a playlist | `add_to_cloud_playlist` | ✅ `+` on any track row → picker (measured: 204) |
| Remove a track from a playlist | — | ❌ Apple exposes no endpoint (404 on every form) |
| Library sync | `sync_cloud_library` | ❌ registered, no UI; counts only, does not persist |

## 4. UI shell — ✅

| Item | Status |
|------|--------|
| Apple-style dark theme, sidebar, album grid, persistent transport | ✅ |
| Playback bar with artwork, title, artist, progress, volume | ✅ |
| Optimistic click feedback | ✅ |
| Real progress, play/pause, next/prev, shuffle, repeat, volume sync | ✅ |
| Library / Albums / Artists from the iCloud library | ✅ |
| Album detail (catalog) and derived iCloud album detail | ✅ |
| Playlist detail, create/rename/delete, add-to-playlist picker | ✅ |
| Browse (Top Albums / Top Songs) | ✅ |
| Radio | ❌ honest placeholder (needs Apple stations API) |
| Loading spinner + error toast (transport state machine) | ✅ |
| Resume last track/queue on launch | ✅ state live-verified; queue rebuild compile-verified |
| Keyboard shortcuts (Space, arrows, `M`, `L`) | ✅ |
| Windows media keys + now-playing flyout (SMTC) | ✅ |
| Up Next queue drawer (with jump-to-item) | ✅ |
| Now Playing full-screen view (art, timeline, transport, volume) | ✅ |
| Loading spinner clears on real playback, even when Apple reports a different id | ✅ (bug 09) |
| Karaoke lyrics in Now Playing (line-timed, word highlight, click-to-seek, `L` toggle) | ✅ (P4) |
| Settings panel | ❌ |

## 4b. Lyrics — ✅ (P4, 2026-11-17)

| Item | Status |
|------|--------|
| `GET /v1/catalog/{sf}/songs/{id}/lyrics?l={lang}` (undocumented) | ✅ measured live; `data[0].attributes.ttml` |
| TTML parser (`api/lyrics.rs`) | ✅ `<p begin end>`, `<span begin end>`, `ttm:role="x-bg"`, `<br/>`, entities, `M:SS.mmm` + bare seconds |
| Line timing | ✅ all 36 timed tracks in the sweep |
| Word timing from Apple | ❌ never observed (0/39); parser supports it, interpolation fills the gap |
| Unsynced lyrics (`itunes:timing="None"`) | ✅ shown as a static block |
| No lyrics (404) | ✅ `Option<Lyrics>` → "Lyrics aren't available" |
| Library id → catalog id | ✅ `playParams.catalogId` in `mk_state`, with `resolve_catalog_id` fallback |
| Rendering | ✅ lyric DOM built once per track; per-frame class flips only; rAF clock off `state.streamAnchor` |
| Honesty flag | ✅ `Lyrics.word_timed` distinguishes Apple's spans from interpolated windows |

## 5. Diagnostics — ✅ but low value

`get_native_bridge_status` and `run_native_bridge_diagnostics` report Apple's install path,
staged DLLs, and memory. They exist to support the abandoned FFI path and are not needed to
stream. The old `probe_report` command was deleted with the loopback channel.

## 6. Parked — local file playback ⏸

Fully implemented and previously working: `.MP3/.FLAC/.ALAC/.AAC/.WAV/.OGG` decode via
symphonia → rodio, queue with shuffle/repeat, SQLite library of `%USERPROFILE%\Music`,
artwork cache, M3U/PLS import/export. **Frozen.** Do not build on it, do not delete it.
See `04-PARKED-LOCAL-PLAYBACK.md`.

## 7. Abandoned — native FairPlay decryption ⚰

`drm/**` stages Apple's DLLs out of `WindowsApps` into `%LOCALAPPDATA%\bridge`, resolves
`CoreAudioToolbox`/`CoreMedia`/`CoreFP` exports by name and ordinal, and guards
`SetDllDirectoryW` with an RAII scope. It loads. It never decrypted anything: the license
challenge must come from a real CDM. Kept only because two diagnostics commands reference it.

## 8. Tests — 156 pass, 0 fail (2026-11-17)

| Suite | Tests | What it actually covers |
|-------|-------|-------------------------|
| Unit (in-module) | 71 | cookie parsing, percent-decode of the state channel, `AppError` serialization, SMTC button ids, DLL loader/locator/bindings, **TTML lyrics parser (12)** |
| `api_acceptance_tests` | 21 | models, JSON shapes, error paths, list pagination (`next` cursor + offset fallback), lyrics fetch/404/catalog-id resolution |
| `audio_acceptance_tests` | 16 | parked local engine (serializes the shared WASAPI device) |
| `ffi_acceptance_tests` | 16 | abandoned DLL bindings (serializes the shared staged-DLL directory) |
| `library_acceptance_tests` | 16 | parked SQLite library |
| `streaming_acceptance_tests` | 16 | HLS parsers + parked stream cache |

The parser tests run against **fixtures captured from a live session**
(`src-tauri/tests/fixtures/lyrics-line.ttml`, `lyrics-plain.ttml`), so a change in Apple's
TTML shape fails a test instead of quietly breaking the pane.

**Nothing tests the live streaming path** — no test touches the hidden webview, the JS
command queue, the cookie poller, or the frontend. Verification for those is a dev run with
`[PLAYER] state {...}` lines in the log.

The two parked suites share process-global resources (one WASAPI device, one staged-DLL
directory), so they take a process-wide lock around those resources. `cargo test` is now
stable under the default parallel runner, not only with `--test-threads=1`.

Code health: `cargo fmt --check` is clean under `rustfmt.toml` (100 columns), and
`cargo clippy --no-default-features` reports no warnings. Every command returns
`AppResult<T>`; `AppError` (`error.rs`) is typed and serialized as a plain message so the
existing toasts keep working. `AppState` is in `state.rs`, commands live in `commands/`, and
the hidden player lives in `player/` (`bootstrap.js`, `cookie.rs`, `media.rs`).

## Summary

| Area | State |
|------|-------|
| Auth ($0) | ✅ done |
| Full-song streaming | ✅ works |
| Playback state truth | ✅ done (P0) |
| Transport (next/prev/shuffle/repeat/volume) | ✅ done (P1, queue model A) |
| Catalog browse/search/charts/recs | ✅ done |
| Album detail, iCloud library, cloud playlists (read + write) | ✅ done (P2) |
| Loading/error state, keyboard shortcuts, resume on launch | ✅ done (P3) |
| Windows media keys, Windows now-playing flyout | ✅ done (P3) |
| Full-screen Now Playing view | ✅ done (P3) |
| Karaoke lyrics in Now Playing | ✅ done (P4) |
| Browse / Radio | ✅ Browse · ❌ Radio (stations API) |
| Local file playback | ⏸ parked |
| Native decryption | ⚰ abandoned |

The app is a working Apple Music player whose interface matches what the player is doing:
the queue is the list you clicked, and the transport bar reports MusicKit's own state.
P3 is complete: loading/error states, keyboard shortcuts, resume-on-launch, the Up Next
queue drawer, Windows media keys with the SMTC flyout, and the full-screen Now Playing view
are all in. What remains is not transport work: Radio needs Apple's stations API, and library
album grouping needs a collection id the library song model does not carry.
