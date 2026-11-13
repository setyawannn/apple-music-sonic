# Apple Music RS

> A personal Apple Music client for Windows, built with Rust + Tauri.
> Uses your own Apple Music subscription. Costs $0 in developer fees.

## What this is

A hobby project: reproduce the Apple Music desktop experience in a small native shell, using
the subscription you already pay for. No Apple Developer Program, no paid APIs, no telemetry.

**MVP:** sign in → browse and search the catalog → play full songs, with a playback bar that
matches what the player is actually doing.

## Status

| Area | State |
|------|-------|
| Apple ID sign-in ($0, web token + cookie capture) | ✅ works |
| Full-song playback (hidden MusicKit player + Windows PlayReady) | ✅ works |
| Play / pause / resume / seek | ✅ works |
| Truthful progress, duration, now-playing metadata | ✅ works |
| Browse: Listen Now, Browse/Charts, Search + typeahead | ✅ works |
| Next / previous / shuffle / repeat / volume while streaming | ✅ works (MusicKit owns the queue) |
| Album detail (catalog + iCloud), iCloud library views | ✅ works |
| Cloud playlists | ✅ works |
| Loading spinner, error toast, resume-on-launch | ✅ works |
| Keyboard shortcuts (Space, arrows, `M`) | ✅ works |
| Radio stations | ❌ not wired (needs Apple's stations API) |
| Local file playback | ⏸ parked — see `docs/03-implementation/04-PARKED-LOCAL-PLAYBACK.md` |
| Lossless / Atmos | ⛔ out of reach on this architecture (AAC 256 ceiling) |

Known bugs and their fixes: [`docs/04-bugs/`](docs/04-bugs/).
Ordered work plan: [`docs/03-implementation/03-TO-DO.md`](docs/03-implementation/03-TO-DO.md).

## How it plays music

Apple encrypts streams with FairPlay, and the license challenge can only be produced by a real
CDM. So the app does not try to decrypt anything. It hosts Apple's own MusicKit player in a
**hidden WebView2** that shares your signed-in profile, drives it with injected JavaScript,
and lets the **PlayReady CDM** that ships with Windows handle the rest.

```
play:  click a song → invoke("stream_play", { songId, queue }) → hidden webview
       → MusicKit.setQueue({ songs, startWith }) → play() → PlayReady → audio
state: page → document.cookie("mk_state") → Rust polls the cookie jar → "player-state" event → UI
```

Because the clicked list becomes the queue, next/previous/shuffle/repeat are MusicKit's own,
and the transport bar mirrors `shuffleMode`, `repeatMode` and `volume` from the same event.

Commands are queued inside the page and carry a nonce, so a click that arrives before MusicKit
has loaded still runs, exactly once — and no command ever waits for a readiness handshake.
State comes back through a cookie because the page's CSP blocks loopback HTTP and WebView2 does
not mirror `document.title`.

## Requirements

- Windows 10/11 (x64)
- Rust 1.81+ and Node.js 18+ to build
- An Apple Music subscription to play anything

## Build & run

```bash
npm install
npm run tauri dev        # development, hot reload
npm run tauri build      # MSI / NSIS installers
```

Production binary: `src-tauri/target/release/app.exe` (~20 MB).

## Project layout

```
index.html                  UI shell (sidebar, main view, playback bar)
src/js/app.js               UI logic and Tauri command calls
src/styles/app.css          Apple-style dark theme
src-tauri/src/lib.rs        53 Tauri commands, hidden player, cookie state poller, auth, lifecycle
src-tauri/src/api/          REST client, auth, session import
src-tauri/src/audio/        parked local engine (symphonia + rodio)
src-tauri/src/library/      parked local library (SQLite)
src-tauri/src/drm/          abandoned native-decrypt attempt (diagnostics only)
src-tauri/capabilities/     window permissions for the remote player webview
docs/                       strategy, architecture, gaps, bugs
```

## Documentation

| Start here | |
|---|---|
| `docs/01-getting-started/01-STRATEGY.md` | What this is and the plan |
| `docs/01-getting-started/02-HOW-TO-RUN.md` | Build, sign in, test, troubleshoot |
| `docs/02-architecture/HOW-IT-WORKS.md` | The real flows, including the hidden player |
| `docs/03-implementation/03-TO-DO.md` | P0–P4 work items |
| `AGENTS.md` | Rules for agents working in this repo |

## Scope

Personal use only. It borrows the public MusicKit developer token that Apple ships in its own
web bundle and relies on your existing subscription and the CDM already installed on Windows.
It is not distributed, and it does not circumvent the subscription.

## License

MIT
