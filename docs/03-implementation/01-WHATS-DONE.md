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
| Personal iCloud songs | `get_personal_library` | ✅ Library / Albums / Artists, preloaded at sign-in |
| Cloud playlists | `get_cloud_playlists` | ✅ cards + sidebar |
| Cloud playlist tracks | `get_cloud_playlist_tracks` | ✅ opens and plays through the queue path |
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
| Browse (Top Albums / Top Songs) | ✅ |
| Radio | ❌ honest placeholder (needs Apple stations API) |
| Loading spinner + error toast (transport state machine) | ✅ |
| Resume last track/queue on launch | ✅ state live-verified; queue rebuild compile-verified |
| Keyboard shortcuts (Space, arrows, `M`) | ✅ |
| Windows media keys | ❌ |
| Up Next queue drawer (with jump-to-item) | ✅ |
| Now Playing full-screen view | ❌ |
| Lyrics | ❌ |
| Settings panel | ❌ |

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

## 8. Tests — 130 pass, 0 fail (2026-11-13)

| Suite | Tests | What it actually covers |
|-------|-------|-------------------------|
| Unit (in-module) | 50 | cookie parsing, percent-decode of the state channel, DLL loader/locator/bindings |
| `api_acceptance_tests` | 16 | models, JSON shapes, error paths |
| `audio_acceptance_tests` | 16 | parked local engine |
| `ffi_acceptance_tests` | 16 | abandoned DLL bindings (flaky under load: they stage DLLs into a shared dir) |
| `library_acceptance_tests` | 16 | parked SQLite library |
| `streaming_acceptance_tests` | 16 | HLS parsers + parked stream cache |

**Nothing tests the live streaming path** — no test touches the hidden webview, the JS
command queue, the cookie poller, or the frontend. Verification for those is a dev run with
`[PLAYER] state {...}` lines in the log.

## Summary

| Area | State |
|------|-------|
| Auth ($0) | ✅ done |
| Full-song streaming | ✅ works |
| Playback state truth | ✅ done (P0) |
| Transport (next/prev/shuffle/repeat/volume) | ✅ done (P1, queue model A) |
| Catalog browse/search/charts/recs | ✅ done |
| Album detail, iCloud library, cloud playlists | ✅ done (P2) |
| Loading/error state, keyboard shortcuts, resume on launch | ✅ done (P3, partial) |
| Browse / Radio | ✅ Browse · ❌ Radio (stations API) |
| Local file playback | ⏸ parked |
| Native decryption | ⚰ abandoned |

The app is a working Apple Music player whose interface matches what the player is doing:
the queue is the list you clicked, and the transport bar reports MusicKit's own state.
P3 is well underway: loading/error states, keyboard shortcuts, resume-on-launch and the Up Next
queue drawer are in; media keys and a full Now Playing view remain.
