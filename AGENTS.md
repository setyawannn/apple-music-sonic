# Apple Music RS — Agent Instructions

**Project:** Single Rust binary (~14 MB, ~80 MB RAM) playing Apple Music + local files.
**Budget:** $0. No Apple Developer Program. Public MusicKit token scraped from web.
**Status:** 5,655 lines Rust, 22 files, 38 Tauri commands. Streaming engine not built yet.

## Quick Facts

- ✅ **Local playback** works — MP3/FLAC/ALAC/AAC via symphonia → rodio
- ✅ **Library management** works — SQLite, scanner, metadata, artwork, M3U/PLS
- ✅ **Apple Music API** works — search, charts, recommendations, library sync
- ✅ **Authentication** works — hardcoded dev token + auto-refresh + WebView2 sign-in + cookie import
- ✅ **DLL loading** works — CoreAudioToolbox, CoreMedia, CoreFP all load and bind
- ❌ **Full song streaming** does NOT work — HLS + FairPlay pipeline not built

## What to Build Next

The only missing piece is the streaming pipeline. See `3-WHAT-IS-MISSING.md`.

Priority order:
1. Wire HLS URL fetch from API
2. Wire FigContentKeyBoss for content key
3. Wire CoreFP ordinals for key decryption
4. Wire FigSampleBufferSetDecryptor for segment decryption
5. Wire CoreAudio ALAC decoder for PCM output
6. Connect to UI play button

## File Map

```
apple-music-rs/
├── index.html                    ← Apple Music UI (176 lines)
├── src/
│   ├── js/app.js                 ← Frontend logic (1,258 lines)
│   └── styles/app.css            ← Apple Music theme (1,011 lines)
├── src-tauri/
│   └── src/
│       ├── lib.rs                ← 38 commands, lifecycle (798 lines)
│       ├── audio/
│       │   ├── mod.rs            ← Engine, worker thread (403 lines)
│       │   ├── queue.rs          ← Queue, shuffle, repeat (212 lines)
│       │   ├── source.rs         ← symphonia decoder (193 lines)
│       │   └── stream.rs         ← HLS parser, cache (263 lines)
│       ├── api/
│       │   ├── mod.rs            ← Re-exports
│       │   ├── auth.rs           ← Dev token, sign-in (166 lines)
│       │   ├── client.rs         ← REST API client (474 lines)
│       │   ├── models.rs         ← Data models (101 lines)
│       │   └── local_session.rs  ← Cookie import (397 lines)
│       ├── drm/
│       │   ├── mod.rs            ← Re-exports
│       │   ├── locator.rs        ← Find install, stage DLLs (598 lines)
│       │   ├── loader.rs         ← LoadLibrary, pin (319 lines)
│       │   ├── coreaudio.rs      ← CoreAudioToolbox FFI (233 lines)
│       │   ├── coremedia.rs      ← CoreMedia FFI (198 lines)
│       │   └── corefp.rs         ← CoreFP FFI (168 lines)
│       └── library/
│           ├── mod.rs            ← Models (190 lines)
│           ├── scanner.rs        ← File scanner (107 lines)
│           ├── db.rs             ← SQLite (399 lines)
│           ├── metadata.rs       ← Audio tags (143 lines)
│           ├── artwork.rs        ← Album art (71 lines)
│           └── playlist.rs       ← M3U/PLS (96 lines)
└── docs/
    ├── 1-STRATEGY.md             ← This project's direction
    ├── 2-WHAT-IS-IMPLEMENTED.md  ← Complete audit
    ├── 3-WHAT-IS-MISSING.md      ← Streaming gap analysis
    ├── 4-HOW-TO-RUN.md           ← Build/run instructions
    ├── decoded-spec.md           ← Original REA report
    └── plan/                      ← Planning docs
```

## Running

```bash
npm install && cargo tauri dev
```

## Key Principle

**We do NOT pay for anything.** The MusicKit token comes from music.apple.com's public JS bundle. The DRM DLLs come from the official app's install directory. The user already pays Apple for their subscription.
