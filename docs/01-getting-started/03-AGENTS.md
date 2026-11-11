# AGENTS — Apple Music RS

## Quick Facts

- **22 Rust files**, 5,655 lines, 38 backend commands, 47 total Tauri commands
- **~20 MB** release binary, ~80 MB RAM target
- **$0 budget** — no Apple Developer account, no paid APIs
- **Streaming works** via hidden MusicKit WebView2 + Windows PlayReady CDM
- **Local playback** via symphonia + rodio

## Document Structure

```
docs/
├── 01-getting-started/
│   ├── 01-STRATEGY.md       ← The master strategy
│   ├── 02-HOW-TO-RUN.md     ← Build & run instructions
│   └── 03-AGENTS.md         ← This file
├── 02-architecture/
│   ├── HOW-IT-WORKS.md      ← How the app actually works
│   └── SYSTEM-DESIGN.md     ← System architecture
├── 03-implementation/
│   ├── 01-WHATS-DONE.md     ← Complete implementation audit
│   ├── 02-WHATS-MISSING.md  ← Gap analysis
│   └── 03-TO-DO.md          ← Priority to-do list
├── 04-bugs/
│   ├── 01-DELAYED-PLAY.md   ← 5-15s play delay (unfixed)
│   └── 02-WINDOW-INIT-FIX.md← Window initialization fix (solved)
└── 05-reference/
    ├── 01-REA-DECODE.md     ← Original REA decode report
    ├── 02-FULL-DECODE-REPORT.md ← Complete decode report
    └── 03-API-CONTRACTS.md  ← Live API contracts from Fiddler
```

## Architecture (50 words)

User clicks a catalog song → `invoke("stream_play")` → hidden WebView2 at music.apple.com receives JS command → `MusicKit.play()` → Windows PlayReady CDM handles FairPlay → audio plays. Every second, the hidden player POSTs state to a local TCP server → Rust emits `player-state` event → frontend updates UI.

## Key Principle

**No Apple Developer account. No $99/year.** Public MusicKit token scraped from web JS bundle. Hidden WebView2 uses user's existing subscription. Personal hobby project only.

## Files Reference

| File | Lines | Purpose |
|------|-------|---------|
| `src-tauri/src/lib.rs` | 1004 | 47 commands, hidden player, auth, lifecycle |
| `src-tauri/src/audio/mod.rs` | 403 | Local playback engine |
| `src-tauri/src/audio/queue.rs` | 212 | Queue management |
| `src-tauri/src/audio/source.rs` | 193 | symphonia decoder |
| `src-tauri/src/audio/stream.rs` | 263 | HLS parser + stream cache |
| `src-tauri/src/drm/locator.rs` | 598 | Find/stage Apple DLLs |
| `src-tauri/src/drm/loader.rs` | 319 | LoadLibrary |
| `src-tauri/src/drm/coreaudio.rs` | 233 | CoreAudioToolbox FFI |
| `src-tauri/src/drm/coremedia.rs` | 198 | CoreMedia FFI |
| `src-tauri/src/drm/corefp.rs` | 168 | CoreFP FFI |
| `src-tauri/src/api/client.rs` | 474 | REST API client |
| `src-tauri/src/api/local_session.rs` | 397 | Cookie import |
| `src-tauri/src/api/auth.rs` | 166 | Auth manager |
| `src-tauri/src/library/db.rs` | 399 | SQLite |
| `src/js/app.js` | 1349 | Frontend |
| `src/styles/app.css` | 1011 | Theme |
