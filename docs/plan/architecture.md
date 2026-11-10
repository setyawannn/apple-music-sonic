# Agent Architecture — Apple Music RS

## System Design

```
┌─────────────────────────────────────────────────────────┐
│                    Tauri Shell (Window)                  │
│  ┌──────────────┬────────────────────────────────────┐  │
│  │              │                                    │  │
│  │   Sidebar    │       Content Area                 │  │
│  │   ┌──────┐   │   ┌──────────────────────────┐    │  │
│  │   │Library│   │   │  Album Grid / Track List  │    │  │
│  │   │Listen │   │   │                          │    │  │
│  │   │Browse │   │   │  ┌───┐ ┌───┐ ┌───┐      │    │  │
│  │   │Radio  │   │   │  │ A │ │ B │ │ C │      │    │  │
│  │   │       │   │   │  └───┘ └───┘ └───┘      │    │  │
│  │   │Albums │   │   │                          │    │  │
│  │   │Artists│   │   │  Search Results           │    │  │
│  │   │       │   │   │  Playlists                │    │  │
│  │   └──────┘   │   └──────────────────────────┘    │  │
│  └──────────────┴────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────┐  │
│  │           Playback Bar (persistent)                 │  │
│  │  ┌──────┬───────────┬──────────────────────────┐   │  │
│  │  │ Art  │  Title    │  ◄◄  ►II  ►►  ████░░   │   │  │
│  │  │      │  Artist   │  0:42 ─────── 3:27  🔊  │   │  │
│  │  └──────┴───────────┴──────────────────────────┘   │  │
│  └────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
         ▲                          │
         │     invoke() commands    │
         │                          ▼
┌─────────────────────────────────────────────────────────┐
│                Rust Backend (src-tauri/)                  │
│                                                          │
│  ┌───────────┐  ┌──────────┐  ┌──────────────────┐      │
│  │  Library  │  │  Audio   │  │  Apple Music API  │      │
│  │  Module   │  │  Engine  │  │  Client           │      │
│  │           │  │          │  │                   │      │
│  │ • SQLite  │  │• symphonia│ │• reqwest HTTP     │      │
│  │ • Models  │  │• rodio   │  │• Catalog search   │      │
│  │ • Import  │  │• Queue   │  │• Auth tokens      │      │
│  │ • Search  │  │• Volume  │  │• Recommendations  │      │
│  └───────────┘  └──────────┘  └──────────────────┘      │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │            Image Cache + Utilities                │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Data Flow

```
User Action (click, search, play)
    │
    ▼
Frontend JS (src/js/app.js)
    │
    ├── invoke("scan_library")          → Rust: scans ~/Music
    ├── invoke("get_albums")            → Rust: returns Vec<Album>
    ├── invoke("play_track", {id})      → Rust: symphonia decoder → rodio output
    ├── invoke("search_catalog", {...})  → Rust: reqwest → amp-api.music.apple.com
    ├── invoke("set_volume", {vol})     → Rust: updates audio state
    └── invoke("seek_to", {pos})        → Rust: seeks in current track
```

## Component Communication

```
┌──────────────────────────────────────────────┐
│                Frontend (WebView2)             │
│                                               │
│  State: tracks[], albums[], playlists[]       │
│  UI: Reactivity via manual re-render          │
│  IPC: @tauri-apps/api/core → invoke()         │
└──────────────────┬───────────────────────────┘
                   │ JSON serialization
                   ▼
┌──────────────────────────────────────────────┐
│            Rust Backend (Tauri)                │
│                                               │
│  State: AppState { library, audio, api }      │
│  Concurrency: Mutex<T> for shared state       │
│  Threading: Tokio async runtime               │
└──────────────────────────────────────────────┘
```

## State Management

| Component | State | Storage |
|-----------|-------|---------|
| Library | `Vec<Track>`, `Vec<Album>`, `Vec<Playlist>` | In-memory + SQLite (future) |
| Audio | `PlaybackStatus { state, track, position, volume }` | In-memory `Mutex` |
| API | `Client { base_url, storefront, language }` | In-memory `Mutex` |
| UI | Current view, search query, selected item | Frontend JS memory |

## Module Dependency Graph

```
main.rs
  └── lib.rs (Tauri builder, command handlers)
       ├── audio/  ← Engine, PlaybackStatus
       ├── library/ ← Track, Album, Playlist, scan, grouping
       └── api/    ← Client, StoreSong, StoreAlbum
```

## Key Design Decisions

1. **Frontend-driven UI**: HTML/CSS in WebView2 handles all rendering. Rust is strictly backend.
2. **Command pattern**: Every user action maps to exactly one `#[tauri::command]`.
3. **Mutex-guarded state**: Shared state uses `std::sync::Mutex` for thread safety.
4. **Async-ready**: Tokio runtime available for HTTP requests and audio decoding.
5. **Minimal IPC payloads**: Only serialized JSON across the bridge — no DOM access from Rust.

## Security Boundaries

```
User Input (search, volume)
  → Frontend (sanitized by JS)
    → invoke() (serialized as JSON)
      → Tauri IPC (validated by schema)
        → Rust command (type-safe)
          → System (file I/O, audio output)
```

- `tauri-plugin-fs`: scoped file access
- `tauri-plugin-dialog`: OS-native file picker
- CSP set to `null` (relaxed for local development)

## Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Startup time | <1s | `time cargo tauri dev` |
| RAM idle | ~40-60 MB | Task Manager |
| RAM playing | ~60-100 MB | Task Manager |
| CPU idle | <3% | Task Manager |
| CPU decoding | <10% | Task Manager |
| Binary size | <15 MB | `ls -lh src-tauri/target/release/` |
| Library scan (10k tracks) | <5s | Manual timing |
| UI frame rate | 60fps | Chrome DevTools |
