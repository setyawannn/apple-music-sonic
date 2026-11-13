# Apple Music RS — How to Build & Run

> Last updated: 2026-11-13

## Prerequisites

| Requirement | Why |
|-------------|-----|
| Windows 10/11 (x64) | WebView2 + PlayReady CDM are Windows-only |
| Rust 1.81+ (2021 edition) | Backend |
| Node.js 18+ | Vite frontend build |
| An Apple Music subscription | Playback uses your own account |
| WebView2 runtime | Ships with Windows 11 and modern Windows 10 |

**You do NOT need** the official Apple Music desktop app to stream. It is only used by the
diagnostics commands that stage Apple's DLLs — and that whole native-decrypt path is
abandoned. If you never open the diagnostics panel, you never need it installed.

## Quick start

```bash
npm install

# Full app, hot reload (Vite dev server + Tauri)
npm run tauri dev

# Production build
npm run build
cd src-tauri && cargo build --release
```

Output: `src-tauri/target/release/app.exe` (~20 MB). Installers via
`npm run tauri build` produce MSI/NSIS bundles.

## First run — signing in

Three paths exist, in order of convenience:

1. **Import session** — `import_local_apple_session` reads the `media-user-token` cookie out
   of an existing WebView2 profile (`%LOCALAPPDATA%\Packages\AppleInc.*\...\Network\Cookies`).
   Instant, no interaction, requires that you have signed in somewhere before.
2. **Sign in** — opens a WebView2 window at `music.apple.com/login`. The backend polls the
   cookie jar every second for `media-user-token`, validates it against Apple, then persists
   it to `%APPDATA%\AppleMusicRS\session.json`.
3. **Set session token manually** — `set_user_session` with a Music-User-Token string.

Note: the sign-in flow deletes the shared `auth_profile` directory before opening the login
window. The hidden player uses that same directory, so **restart the app after signing in**
until `docs/04-bugs/03-PLAYER-STATE-CHANNEL.md` is fixed.

## Using the app

1. Sign in (above). `Listen Now` populates from your recommendations.
2. **Search** the catalog; results render as track rows and album cards.
3. Click a song → full song streams through the hidden player.
4. The playback bar's play/pause toggles the stream.

### What you can expect to be rough

- **Progress is a local clock, not the player's clock.** The page's CSP blocks the loopback
  state channel, so position/duration come from catalog metadata + elapsed time.
- **Seeking does nothing while streaming.** The progress bar still calls the local engine.
- **Next / previous / shuffle / repeat do nothing while streaming.** They call the parked
  local engine.
- **The playback icon can revert ~30 s after you press play.** `stream_play` waits 30 s for a
  readiness signal that never arrives, then rejects, and the frontend resets its optimistic
  state. Audio keeps playing.
- **Ceiling is AAC 256 kbps.** Lossless is not reachable through WebView2.

All of the above are tracked in `docs/04-bugs/` and `docs/03-implementation/02-WHATS-MISSING.md`.

## Running tests

```bash
cd src-tauri

cargo test --no-default-features                                   # unit tests
cargo test --no-default-features --test api_acceptance_tests       # one suite
cargo test --no-default-features --test audio_acceptance_tests
cargo test --no-default-features --test library_acceptance_tests
cargo test --no-default-features --test streaming_acceptance_tests
cargo test --no-default-features --test ffi_acceptance_tests
```

`--no-default-features` matches how the dev server and release build are configured.

**Verified 2026-11-13: 128 tests pass, 0 fail.**

| Suite | Tests |
|-------|-------|
| Unit tests (`src/`, in-module) | 48 |
| `api_acceptance_tests` | 16 |
| `audio_acceptance_tests` | 16 |
| `ffi_acceptance_tests` | 16 |
| `library_acceptance_tests` | 16 |
| `streaming_acceptance_tests` | 16 |

**Read this before trusting the green.** No test covers the shipped streaming path: the
hidden WebView2 player, the JS command queue, the loopback state channel, or the frontend.
The suites verify data models, parsers, the parked local engine, the abandoned FFI bindings,
and helper logic. `ffi_acceptance_tests` passing says the DLL bindings resolve — not that
decryption works, because it never did. Treat a green run as "nothing structurally broken",
not as "playback works".

Note: PowerShell reports `Exit code: 1` for these runs because cargo writes progress to
stderr. Judge by the `test result:` lines.

## Troubleshooting

| Symptom | Likely cause / action |
|---------|----------------------|
| Window never appears | Known and fixed historically — see `docs/04-bugs/02-WINDOW-INIT-FIX.md`. Confirm the setup hook still calls `show()` + `set_focus()`. |
| Audio plays, UI says nothing | Expected until the state channel is fixed. See bug 03. |
| Play icon flips back after ~30 s | Expected — the 30 s ready-wait rejects. See bug 03. |
| `state listener bind failed on 38471` | Another instance is running. Kill `app.exe` and relaunch. |
| Nothing plays, no error | The hidden player is not signed in. Restart the app after signing in. |
| `401 Unauthorized` from the API | Music-User-Token expired. Sign in again. |

## Project commands

```bash
npm run dev:web        # frontend only in a browser (no Tauri IPC)
npm run build          # vite build → dist/
cargo check            # fast Rust type check (run from src-tauri/)
cargo tauri dev        # full app with hot reload
cargo tauri build      # production MSI/NSIS
```

## Where to edit

| Goal | File |
|------|------|
| Add/change a Tauri command | `src-tauri/src/lib.rs` |
| Change streaming behavior | `PLAYER_BOOTSTRAP` / `player_command` in `src-tauri/src/lib.rs` |
| Add an API endpoint | `src-tauri/src/api/client.rs` |
| Change the UI | `index.html`, `src/js/app.js`, `src/styles/app.css` |
| Fix auth / session | `src-tauri/src/api/auth.rs`, `local_session.rs` |
| Remote webview permissions | `src-tauri/capabilities/player.json` |
| Local playback | **Parked.** Do not edit — see `docs/03-implementation/04-PARKED-LOCAL-PLAYBACK.md`. |
