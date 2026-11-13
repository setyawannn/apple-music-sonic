# AGENTS — Apple Music RS (`apple-music-sonic`)

Guidance for AI agents and future-me working in this repository.

## Project identity

A personal Windows desktop client that **clones the Apple Music app experience** on top of
the user's existing Apple Music subscription. Rust + Tauri v2, no paid infrastructure.

**MVP scope — this is the whole product:**

1. Sign in with an Apple ID ($0 — no Apple Developer Program)
2. Browse: Listen Now (recommendations), Browse/Charts
3. Search catalog with typeahead
4. Album detail view
5. Your Library = the user's **iCloud** library
6. Cloud playlists
7. Play full songs, with working transport (play/pause, next/prev, seek, shuffle, repeat)
   and a playback bar that tells the truth

**Explicit non-goals (for now):**

- Local file playback is **PARKED** — see `docs/03-implementation/04-PARKED-LOCAL-PLAYBACK.md`.
  Do not build it, do not delete it, do not route UI into it.
- No downloads/offline, no lyrics, no Atmos, no equalizer, no Discord presence.

## Quick facts

- **23 Rust files, 5,889 lines**, **48 Tauri commands** registered in `generate_handler!`
- **~20 MB** release binary; RAM target **~80 MB** (not yet measured — treat old claims as aspirational)
- Frontend: `index.html`, `src/js/app.js` (1,349 lines), `src/styles/app.css` (1,011 lines)
- Streaming works: hidden MusicKit WebView2 + Windows PlayReady CDM
- Auth works: developer token scraped from Apple's web bundle + `media-user-token` cookie

## How streaming works (the one architecture fact that matters)

```
click a catalog song
  → invoke("stream_play", { songId })
  → Rust evals a job into the hidden "player" webview (music.apple.com/id/)
  → page's MusicKit instance: setQueue({ song }) then play()
  → Windows PlayReady CDM decrypts FairPlay
  → audio plays
```

- Hidden window label: `player`, created by `ensure_player()` in `src-tauri/src/lib.rs`,
  pre-warmed during setup so the first click is fast.
- It shares the signed-in profile at
  `%LOCALAPPDATA%\com.applemusicrs.desktop\auth_profile`.
- Commands are **queued in the page** (`window.__mkQueue`), so a command issued before
  MusicKit loads still runs. This replaced the old blind retry loop.
- Every command carries a **nonce**; re-dispatch of the same nonce is a no-op and a fresh
  click always gets a fresh nonce.

**Native Rust decryption is abandoned.** The license challenge can only be produced by a
CDM. `src-tauri/src/drm/**` is dead weight that only feeds `get_native_bridge_status` and
`run_native_bridge_diagnostics`.

## Hard rules

1. **$0.** No Apple Developer Program, no paid APIs, no paid services.
2. **Never commit credentials.** `session.json`, `auth_profile/`, and the user's
   `media-user-token` stay out of git (see `.gitignore`). Apple's *public* MusicKit
   developer token in `api/auth.rs` is not a user credential.
3. **Personal hobby project only.** Do not present it as a distributable product, and do not
   strip that framing from the docs.
4. **Do not delete parked modules** (`library/`, `audio/`, `drm/`) and do not extend them.
5. **One player.** New playback features target the hidden MusicKit player, never the
   symphonia/rodio engine.

## Environment gotchas (learned the hard way)

- music.apple.com's CSP blocks `fetch()` to `http://127.0.0.1:38471`, so the loopback state
  channel has **never** received a packet. UI state is currently a local clock. See
  `docs/04-bugs/03-PLAYER-STATE-CHANNEL.md`.
- Because of that, `player_command` currently waits up to 30 s for a `/ready` signal that
  never arrives, then returns `Err`. This is a known P0 bug, not intended behavior.
- The hidden player is a real window. Tauri keeps the app alive while it exists, so the main
  window's `Destroyed` handler calls `app_handle().exit(0)`.
- After sign-in the auth profile directory is wiped. The hidden player must be closed or
  recreated around sign-in, otherwise it keeps a dead session.
- Do not touch `tauri.conf.json`'s `security.csp: null` without checking the injected
  bootstrap script; the main window is a local page, the hidden player is remote.
- Shell quirk on this machine: PowerShell rejects some compound commands; run simple,
  separate commands.

## Where things live

| Path | Purpose |
|------|---------|
| `src-tauri/src/lib.rs` (1,115) | 48 commands, hidden player + bootstrap script, auth windows, state listener, lifecycle |
| `src-tauri/src/api/auth.rs` (166) | Developer token, session persistence, storefront |
| `src-tauri/src/api/client.rs` (474) | REST client: search, album, charts, library, playlists |
| `src-tauri/src/api/local_session.rs` (397) | Imports session from WebView2 cookie SQLite |
| `src-tauri/src/api/models.rs` (101) | Store models |
| `src-tauri/src/audio/**` (1,071) | **PARKED** local engine |
| `src-tauri/src/library/**` (1,006) | **PARKED** local library + SQLite |
| `src-tauri/src/drm/**` (1,538) | **ABANDONED** native decrypt attempt (diagnostics only) |
| `src-tauri/capabilities/player.json` | Remote capability for the `player` webview |
| `src/js/app.js` (1,349) | All UI logic |

## Docs map

```
docs/
├── 01-getting-started/
│   ├── 01-STRATEGY.md            ← What this project is, and the plan
│   ├── 02-HOW-TO-RUN.md          ← Build, run, test
│   └── 03-AGENTS.md              ← This file (mirror of /AGENTS.md)
├── 02-architecture/
│   ├── HOW-IT-WORKS.md           ← The real request/response flows
│   └── SYSTEM-DESIGN.md          ← Modules, state, boundaries
├── 03-implementation/
│   ├── 01-WHATS-DONE.md          ← Verified working surface
│   ├── 02-WHATS-MISSING.md       ← MVP gaps, with file/line evidence
│   ├── 03-TO-DO.md               ← Ordered work plan (P0…P4)
│   └── 04-PARKED-LOCAL-PLAYBACK.md ← Frozen feature + revisit options
├── 04-bugs/
│   ├── 01-DELAYED-PLAY.md        ← Play delay: fixed parts + what still hangs
│   ├── 02-WINDOW-INIT-FIX.md     ← Blank-window fix (solved, historical)
│   ├── 03-PLAYER-STATE-CHANNEL.md← Dead state channel + 30 s ready wait
│   └── 04-PROGRESS-BAR-SEEK.md   ← Seek hits the wrong player
└── 05-reference/                 ← Decode reports, API contracts (see README there)
```

## Build & verify

```bash
npm install
cargo check --no-default-features        # from src-tauri/
npm run tauri dev                        # full app, hot reload

cd src-tauri
cargo test --no-default-features         # unit tests
cargo test --no-default-features --test api_acceptance_tests
```

Whenever you change `lib.rs`, run `cargo check` before claiming done — this codebase has a
history of half-applied edits.
