# Apple Music RS — What's Done

> Last updated: 2026-11-13 (after P0 shipped)
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
| Next / previous | ❌ | no MusicKit queue command exists yet (P1) |
| Shuffle / repeat | ❌ | backend commands drive only the parked local engine (P1) |
| Audio quality | ⚠️ | AAC 256 kbps ceiling (no ALAC decoder in WebView2, EME output not readable) |

## 3. Apple Music catalog API — ✅ backend / ⚠️ UI

Backend methods in `api/client.rs` are implemented and exercised:

| Capability | Command | Wired to UI |
|------------|---------|-------------|
| Catalog search | `search_catalog` | ✅ |
| Live typeahead | `get_search_suggestions` | ✅ |
| Album detail + tracklist | `get_catalog_album` | ❌ no album view |
| Charts | `get_charts` | ✅ |
| Recommendations | `get_recommendations` | ✅ (Listen Now) |
| Personal iCloud songs | `get_personal_library` | ❌ never called |
| Cloud playlists | `get_cloud_playlists` | ✅ partial |
| Cloud playlist tracks | `get_cloud_playlist_tracks` | ✅ partial |
| Library sync | `sync_cloud_library` | ❌ never called; also only counts, does not persist |

## 4. UI shell — ✅

| Item | Status |
|------|--------|
| Apple-style dark theme, sidebar, album grid, persistent transport | ✅ |
| Playback bar with artwork, title, artist, progress, volume | ✅ |
| Optimistic click feedback | ✅ |
| Real progress and play/pause sync | ✅ |
| Now Playing full-screen view | ❌ |
| Queue drawer | ❌ |
| Lyrics | ❌ |
| Settings panel | ❌ |
| Keyboard shortcuts / media keys | ❌ |

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
| Transport (next/prev/shuffle/repeat) | ❌ P1 |
| Catalog browse/search/charts/recs | ✅ done |
| Album detail, iCloud library | ⚠️ backend done, UI missing |
| Local file playback | ⏸ parked |
| Native decryption | ⚰ abandoned |

The app is a working Apple Music player whose interface now matches what the player is doing.
Next milestone: real transport (P1).
