# Apple Music RS — System Design

> Last updated: 2026-11-13
> Runtime truth lives in `HOW-IT-WORKS.md`. This file describes structure.

---

## Layers

```
┌─────────────────────────────────────────────────────────────────────┐
│ Presentation — WebView2, local page                                 │
│   index.html · src/styles/app.css · src/js/app.js                    │
│   Views: Listen Now · Browse · Radio · Library · Playlists ·         │
│          Albums · Artists                    (+ persistent transport) │
└──────────────────────────┬──────────────────────────────────────────┘
                           │  invoke() / listen()
┌──────────────────────────▼──────────────────────────────────────────┐
│ Tauri command layer — src-tauri/src/lib.rs                            │
│   47 commands, window lifecycle, event emit                          │
└──────┬───────────────────┬──────────────────────┬───────────────────┘
       │                   │                      │
┌──────▼──────┐   ┌────────▼─────────┐   ┌────────▼──────────────────┐
│  api/       │   │  hidden player    │   │  parked: audio/ library/  │
│  reqwest +  │   │  WebView2 at      │   │  abandoned: drm/          │
│  auth       │   │  music.apple.com  │   │                           │
└─────────────┘   └────────┬─────────┘   └───────────────────────────┘
                           │
                    Windows PlayReady CDM
```

## Module graph

```
main.rs
└── lib.rs  (AppState { library, audio_engine, api_client })
     ├── api/      AuthManager · ApiClient · LocalSessionReader · models
     ├── audio/    Engine · QueueManager · SymphoniaSource · StreamCache   [PARKED]
     ├── library/  Library · Database · Scanner · ArtworkManager          [PARKED]
     └── drm/      AppLocator · NativeLibrary · CoreAudio/CoreMedia/CoreFP [ABANDONED]
```

Only `lib.rs`, `api/`, and the hidden-player code are in the product's critical path.
`audio/` and `library/` are retained for compilation and reference; `drm/` feeds two
diagnostics commands.

## Frontend state (in-memory, no store library)

```js
state = {
  auth: { isAuthenticated, storefront, userTokenMasked },
  tracks, albums, artists, playlists, cloudPlaylists,   // library views
  playback: { isPlaying, currentTrackId, duration, position, volume,
              isShuffle, repeatMode },                   // PARKED local player
  streamingActive, streamIsPlaying, streamPosition,
  streamDuration, streamAnchor       // hidden player (re-anchored from real reports)
}
```

`syncPlaybackStatus()` runs every 250 ms. If `streamingActive` it takes the streaming branch
and returns; otherwise it polls `get_playback_status` from the parked local engine. The
streaming branch's numbers now come from the `player-state` event (cookie channel), and
`streamAnchor` is re-set on every report, so the local clock only smooths gaps between them.
The local fork remains a wart until P2 stops routing UI into the parked engine.

## Data flow (current)

| User action | Path |
|-------------|------|
| Click catalog song | optimistic state → `invoke("stream_play")` → `player_command` → JS queue → MusicKit |
| Play/pause reflect | page `publish()` → `mk_state` cookie → `start_state_poller` → `emit("player-state")` |
| Click play/pause | `stream_pause` / `stream_resume` (or local commands when the local engine owns playback) |
| Seek | progress bar → `stream_seek` → `seekToTime()` |
| Search | `invoke("search_catalog")` → `amp-api.music.apple.com` |
| Load Listen Now | `get_recommendations` |
| Load Browse | `get_charts` |
| Playlists | `get_cloud_playlists` (+ parked `get_playlists`) |
| Sign in | WebView2 window → cookie poll → validate → persist → emit `auth-success` |
| Import session | cookie SQLite → validate → persist |

### Not wired yet (backend exists)

`get_personal_library` · `get_catalog_album` · `sync_cloud_library` — see
`docs/03-implementation/02-WHATS-MISSING.md`. Transport (next/prev/shuffle/repeat) is the P1
work and does not have backend commands yet either.

## Concurrency model

- Tauri commands run on the async runtime (Tokio) or on a blocking thread for pure sync work.
- `AppState` holds `Mutex<library::Library>` and an `audio::Engine`; the streaming path does
  not touch either.
- `start_state_poller` runs one Tokio task that polls `window.cookies()` every 250 ms and
  emits `player-state`. There is no inbound port any more.
- The hidden player is driven with `webview.eval()`. There is no shared lock between the
  player and the local engine.

## Design decisions

1. **Borrow Apple's CDM instead of reimplementing it.** A license challenge requires a real
   CDM; the native FFI route cannot produce audio. Cost: AAC 256 ceiling, extra process.
2. **Queue commands inside the page.** Removes the "is it ready yet?" retry loop and makes
   cold-start clicks work.
3. **Nonce per user action.** Repeat clicks work; duplicate delivery is idempotent.
4. **Pre-warm the player at startup.** Moves page-load latency off the first click.
5. **Optimistic UI on click.** The playback bar updates instantly even when the player is
   still waking up.
6. **Frontend-driven rendering.** Rust is a command backend; no SSR, no framework.
7. **Park, don't delete, local playback.** Keeps the build green and the decision reversible.
8. **State out of the page via cookie.** CSP blocks loopback `fetch`; WebView2 does not mirror
   `document.title`. A first-party cookie polled by Rust is the only route that actually
   works — and it needs no new dependency or capability.

## Boundaries and permissions

| Boundary | Rule |
|----------|------|
| Main window | local page; CSP relaxed to `null` |
| Hidden player | remote origin `https://music.apple.com/*`; needs `capabilities/player.json` |
| Player state | `mk_state` cookie (first-party, Secure, max-age 300 s); polled every 250 ms |
| Secrets on disk | `%APPDATA%\AppleMusicRS\session.json`; never committed |
| WebView2 profile | `%LOCALAPPDATA%\com.applemusicrs.desktop\auth_profile` |

## Performance — targets vs measurements

| Metric | Target | Measured |
|--------|--------|----------|
| Binary size | <15 MB | ~20 MB release (historical figure) |
| RAM idle | 40–60 MB | **not measured** |
| RAM playing | 60–100 MB | **not measured** |
| Startup | <1 s | not measured |
| First-click audio | instant | ~4–6 s on a cold player, faster after pre-warm |
| Command latency | <100 ms | `stream_play` returns immediately after `eval`; audio starts when MusicKit loads |
| State freshness | ≤500 ms | page publishes every 500 ms + on events; Rust polls every 250 ms |

Do not repeat the old "5–7× less memory than the official app" claim without measuring it.
That number came from a native-decoder design that was never built.
