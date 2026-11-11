# Apple Music RS — Strategy

**Goal:** Single Rust binary (~14 MB) that plays Apple Music lossless streaming + local files.
**RAM target:** ~80 MB total (vs official app's 300-500 MB)
**Budget:** $0 — no Apple Developer account, no paid APIs.

---

## The Core Strategy

**Load Apple's own DLLs into our process via FFI. Steal the public MusicKit token from music.apple.com's JavaScript bundle.**

We do NOT reverse engineer FairPlay. We do NOT pay $99/year. Apple's web app publicly embeds a MusicKit JWT. We scrape it on startup. Apple's DRM DLLs are on disk — we load them.

## What Already Works (38 Commands, 22 Rust Files, 5,655 Lines)

### Local Music Playback
- symphonia decoder → rodio output
- Supports MP3, FLAC, ALAC, AAC, WAV, OGG
- Play/pause/seek/volume
- Queue with shuffle and repeat (off/one/all)
- Auto-advance to next track
- Album play (queue all tracks)

### Library Management
- SQLite database (tracks, albums, artists, playlists)
- walkdir scanner for ~/Music
- Metadata extraction from audio files
- Album artwork extraction and caching
- M3U/PLS playlist import and export

### Apple Music API (Free, No Subscription Needed)
- Catalog search (songs, albums, artists)
- Search suggestions (live typeahead)
- Album detail with track list
- Charts / Browse
- Recommendations / For You
- Personal library sync
- Cloud playlists

### Authentication ($0)
- Hardcoded Apple Music developer token (scraped from web JS)
- Auto-refresh on startup (scrapes music.apple.com bundle)
- WebView2 sign-in window that intercepts media-user-token cookie
- Import session from official app's SQLite cookies database

### DLL Loading (FairPlay Bridge)
- AppLocator finds Apple Music install, stages DLLs to %LOCALAPPDATA%/bridge
- CoreAudioToolbox.dll loaded — ALAC decoder at ordinal 8 (0x180438800)
- CoreMedia.dll loaded — FigContentKeyBoss, FigCPECryptor, FigCPEFairPlayCryptor by name
- CoreFP.dll loaded — all 6 obfuscated ordinals resolved
- Microsoft DLLs filtered out during staging to avoid WebView2 conflicts

### Diagnostics
- `get_native_bridge_status` returns: install path, version, DLL state, RAM usage, session detection
- `run_native_bridge_diagnostics` returns detailed text report

## What's Missing (The Streaming Engine)

The HLS parser exists. The DLLs load. The function pointers bind. But nobody wired the actual streaming pipeline:

```
User clicks a catalog song
  → Get song ID
  → Fetch HLS playlist URL  ❌ NOT IMPLEMENTED
  → Parse m3u8               ✅ PARSER EXISTS but not called
  → Get content key           ❌ NOT IMPLEMENTED
  → Decrypt key via CoreFP    ❌ NOT IMPLEMENTED
  → Decrypt segments          ❌ NOT IMPLEMENTED
  → Decode ALAC/AAC           ❌ POINTERS BOUND but not called
  → Play through rodio        ✅ WOULD WORK
```

The current `play_stream_track` command downloads a URL to a file and plays it with symphonia. This works for 30-second AAC previews but not for encrypted HLS streams.

## Development Priority

| Priority | Item | Effort |
|----------|------|--------|
| 🔴 P0 | Build the HLS streaming engine (FairPlay decrypt → decode → play) | ~5 days |
| 🟡 P1 | UI polish — wire search results to play action | 1 day |
| 🟢 P2 | Lyrics display (TTML parser from decoded spec) | 2 days |
| 🟢 P2 | Dolby Atmos support | 3 days |

## Key Files Reference

| File | Lines | What It Does |
|------|-------|-------------|
| `src-tauri/src/lib.rs` | 798 | 38 Tauri commands, auth WebView2, diagnostics, lifecycle |
| `src-tauri/src/api/auth.rs` | 166 | Hardcoded dev token, auto-refresh, sign-in/out |
| `src-tauri/src/api/client.rs` | 474 | REST API client (search, catalog, charts, library) |
| `src-tauri/src/api/local_session.rs` | 397 | Read official app's cookies for auto-login |
| `src-tauri/src/drm/locator.rs` | 598 | Find install dir, stage DLLs from WindowsApps |
| `src-tauri/src/drm/loader.rs` | 319 | LoadLibrary via libloading, pin in memory |
| `src-tauri/src/drm/coreaudio.rs` | 233 | CoreAudioToolbox — ALAC/AAC/MP3 decoder bindings |
| `src-tauri/src/drm/coremedia.rs` | 198 | CoreMedia — FigContentKeyBoss, FigCPECryptor bindings |
| `src-tauri/src/drm/corefp.rs` | 168 | CoreFP — 6 obfuscated ordinals by ordinal |
| `src-tauri/src/audio/mod.rs` | 403 | Playback engine, audio worker thread |
| `src-tauri/src/audio/stream.rs` | 263 | HLS parser (master + media) + stream cache |
| `src-tauri/src/audio/source.rs` | 193 | symphonia decoder source |
| `src-tauri/src/audio/queue.rs` | 212 | Queue manager with shuffle/repeat |
| `src-tauri/src/library/db.rs` | 399 | SQLite CRUD operations |
| `src/index.html` | 176 | Apple Music UI |
| `src/styles/app.css` | 1011 | Apple Music dark theme CSS |
| `src/js/app.js` | 1258 | Frontend logic, invokes backend commands |

## File Count: 22 Rust files, 3 frontend files, 10+ docs

**Total Rust code: ~5,655 lines. Total project: ~8,000+ lines.**
