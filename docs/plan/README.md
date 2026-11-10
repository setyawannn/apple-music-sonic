# Development Plan — Apple Music RS

## Phases & Milestones

### Phase 1: Foundation ✅ Complete
- [x] Decode official Apple Music app architecture (REA)
- [x] Map all 40+ DLLs and their roles
- [x] Capture live API traffic (Fiddler)
- [x] Document all API endpoints, auth headers, data models
- [x] Scaffold Tauri + Rust project
- [x] Design Apple Music UI (dark theme, sidebar, grid, playback bar)
- [x] Wire 15 Tauri commands (library, audio, API)

### Phase 2: Local Playback ✅ Complete
- [x] Implement symphonia audio decoder
- [x] Wire rodio audio output
- [x] Queue management (up-next, history, shuffle, repeat)
- [x] Gapless playback support
- [x] Volume control with system mixer integration
- [x] Automated Acceptance Test Suite (16 positive & negative tests)
- [ ] Media key support (keyboard play/pause/next/prev)

### Phase 3: Library Management ✅ Complete
- [x] SQLite schema (tracks, albums, artists, playlists with WAL mode)
- [x] CRUD operations for library items
- [x] Metadata extraction from audio files via Symphonia (zero bloat)
- [x] Album artwork extraction and caching (embedded & folder art)
- [x] M3U/M3U8 and PLS playlist import/export
- [x] Incremental fast scanner (<100ms for 10k tracks)
- [x] Automated Acceptance Test Suite (16 positive & negative tests)

### Phase 4: Apple Music API ✅ Complete
- [x] Apple ID official native webview sign-in flow (`https://music.apple.com/login`)
- [x] Automated MUT (`media-user-token`) & storefront extraction and session persistence
- [x] Anonymous bearer token fallback with connection pooling (`reqwest`)
- [x] Catalog search (songs, albums, artists) with debounced live suggestions
- [x] Personalized recommendations and iCloud music library fetching
- [x] Top charts (albums & songs) with artwork template resolution
- [x] Automated Acceptance Test Suite (16 positive & negative tests)

### Phase 5: Streaming & iCloud Sync ✅ Complete
- [x] HLS master & media playlist parser (variant bandwidth & codec detection)
- [x] Audio stream download + decode (AAC 256kbps & Lossless ALAC)
- [x] High-performance StreamCache with LRU pruning (<200MB budget)
- [x] Click-to-play on Search Results, Top Charts & iCloud Playlists
- [x] iCloud library sync & playlist models integration
- [x] Automated Acceptance Test Suite (16 positive & negative tests)

### Phase 5B: Native FFI & Local Session Bridge ✅ Complete
- [x] App package locator & environment discovery (WindowsApp AppModel HKCU registry)
- [x] Local SQLite Cookies session extractor (Immutable URI mode, zero-lock collision)
- [x] Safe Native DLL dynamic loader with process working set RAM monitor (<85 MB)
- [x] CoreAudioToolbox binding (ALAC Lossless Decoder Ordinal 8, AAC Ordinal 17, MP3 Ordinal 13)
- [x] CoreMedia real-name symbol binding (`FigContentKeyBossGetClassID` execution)
- [x] CoreFP FairPlay obfuscated ordinals resolution (all 6 ordinals verified)
- [x] Tauri IPC commands (`get_native_bridge_status`, `import_local_apple_session`, `run_native_bridge_diagnostics`)
- [x] Automated Acceptance Test Suite (`tests/ffi_acceptance_tests.rs`, 16 positive & negative tests)

### Phase 6: Polish
- [ ] Time-synced lyrics (TTML parser from decoded spec)
- [ ] Dolby Atmos / Spatial Audio
- [ ] Keyboard shortcuts
- [ ] System tray with playback controls
- [ ] Desktop notifications
- [ ] Custom accent colors
- [ ] Multiple library paths
- [ ] Equalizer

## Decoded Knowledge Base

### Key Files in `docs/`

| File | Contents |
|------|----------|
| `decoded-spec.md` | Full reverse engineering report — 40+ DLLs, audio pipeline, auth, API |
| `plan/architecture.md` | System architecture, data flow, state management, module deps |

### REA-Discovered Apple Music Internals

| Component | Location | Role |
|-----------|----------|------|
| `CoreAudioToolbox.dll` | `AMP.Services/` | Audio codecs — 183 exports including ALAC, AAC, MP3 |
| `CoreMedia.dll` | Root | Media pipeline, timing, playback control |
| `CoreFP.dll` | Root | FairPlay DRM core (37 MB) |
| `AuthKitWin.dll` | Root | Apple ID authentication, 40+ languages |
| `CFNetwork.dll` | Root | HTTP networking with TLS |
| `AppleMediaServicesKit.dll` | Root | Catalog/search/store services |
| `AMP.Services.dll` | Root | Core streaming service (29 MB, native AOT) |

### API Contracts (Live Captured)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `amp-api.music.apple.com/v1/me/social/profile` | GET | User profile |
| `amp-api.music.apple.com/v1/catalog/{id}/search` | GET | Full catalog search |
| `amp-api-edge.music.apple.com/v1/catalog/{id}/search/suggestions` | GET | Live search suggestions |
| `amp-api.music.apple.com/v1/catalog/{id}/albums/{id}` | GET | Album detail with tracks |
| `amp-api.music.apple.com/v1/me/recommendations/suggested` | POST | Song recommendations |

## Resource Targets

| Metric | Official App | Our Target |
|--------|-------------|------------|
| RAM (idle) | 200-350 MB | **40-60 MB** |
| RAM (playing) | 300-500 MB | **60-100 MB** |
| CPU (idle) | 5-15% | **1-3%** |
| CPU (playing) | 8-20% | **3-8%** |
| Binary size | 500 MB+ | **~14 MB** |
| Startup time | 3-8s | **<1s** |
| Background processes | 3-5 | **1** |

## Build & Run

```bash
# Development (hot reload)
cargo tauri dev

# Production build
cargo tauri build

# Frontend only (browser preview)
npm run dev
```
