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

Note: the sign-in flow closes the hidden player and deletes the shared `auth_profile`
directory before opening the login window, then re-warms the player on success. No restart is
needed.

## Using the app

1. Sign in (above). `Listen Now` populates from your recommendations.
2. **Search** the catalog; results render as track rows and album cards.
3. Click a song → full song streams through the hidden player.
4. The playback bar shows the player's own position and duration; play/pause, mute, and
   seeking all work while streaming.
5. **Next / previous / shuffle / repeat / volume** act on the list you clicked: that list
   becomes MusicKit's queue, and the transport bar mirrors the player's own state.
6. **Library / Albums / Artists** read your iCloud library; album cards open a detail view
   with Play and Shuffle; cloud playlists open and play.

### What you can expect to be rough

- **Media keys are not bound.** In-app shortcuts work (Space, arrows, `M`), but the hardware
  media keys need a native hook. (P3)
- **No full-screen Now Playing view yet.** The Up Next drawer is in the transport bar, though.
- **Radio** is an honest placeholder: live stations need Apple's stations API.
- **The playback bar lags reality by up to ~250 ms.** The player publishes state on events
  and on a timer; the frontend smooths between reports.
- **Ceiling is AAC 256 kbps.** Lossless is not reachable through WebView2.

All of the above are tracked in `docs/03-implementation/02-WHATS-MISSING.md`.

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

**Verified 2026-11-13: 130 tests pass, 0 fail.**

| Suite | Tests |
|-------|-------|
| Unit tests (`src/`, in-module) | 50 |
| `api_acceptance_tests` | 16 |
| `audio_acceptance_tests` | 16 |
| `ffi_acceptance_tests` | 16 |
| `library_acceptance_tests` | 16 |
| `streaming_acceptance_tests` | 16 |

**Read this before trusting the green.** No test covers the shipped streaming path: the
hidden WebView2 player, the JS command queue, the cookie state poller, or the frontend.
The suites verify data models, parsers, the parked local engine, the abandoned FFI bindings,
and helper logic (including the percent-decoder used by the state channel).
`ffi_acceptance_tests` passing says the DLL bindings resolve — not that decryption works,
because it never did. Treat a green run as "nothing structurally broken", not as "playback
works". Verify streaming from a dev run instead: `[PLAYER] state {...}` lines mean the
channel is alive.

Note: PowerShell reports `Exit code: 1` for these runs because cargo writes progress to
stderr. Judge by the `test result:` lines. `ffi_acceptance_tests` can fail while another
build or app instance is staging the same DLLs into `%LOCALAPPDATA%\bridge`; re-run it.

## Troubleshooting

| Symptom | Likely cause / action |
|---------|----------------------|
| Window never appears | Known and fixed historically — see `docs/04-bugs/02-WINDOW-INIT-FIX.md`. Confirm the setup hook still calls `show()` + `set_focus()`. |
| Audio plays, UI says nothing | The player is not signed in, so `publish()` has no `nowPlayingItem`. Sign in; the player is rebuilt on success. |
| UI updates then freezes | Check the log for `[PLAYER] state` lines. None means the cookie is not arriving — confirm `mk_state` is written and that the poller is running. |
| Nothing plays, no error | The hidden player is not signed in. Sign in again (no restart needed). |
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
