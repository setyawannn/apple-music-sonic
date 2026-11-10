# Apple Music RS — How to Build & Run

## Prerequisites

- Windows 10/11 (x64)
- Rust 1.81+
- Node.js 18+
- Apple Music installed from Microsoft Store (for streaming features)
- Apple Music subscription (for streaming — you pay Apple, not us)

## Quick Start

```bash
# 1. Clone and enter the project
cd apple-music-rs

# 2. Install frontend dependencies
npm install

# 3. Build and run (development mode with hot reload)
cargo tauri dev
```

This compiles the Rust backend (5,655 lines across 22 files), bundles the frontend with Vite (1,258 lines JS + 1,011 lines CSS), and opens a native window.

## Production Build

```bash
npm run build
cargo tauri build
```

Output: `src-tauri/target/release/app.exe` (~14 MB)

## Running Tests

```bash
# All unit tests (16 tests across 5 modules)
cd src-tauri && cargo test

# Specific module tests
cargo test loc_pos_        # AppLocator positive tests
cargo test fp_pos_         # CoreFP positive tests
cargo test ca_pos_         # CoreAudio positive tests
cargo test cm_pos_         # CoreMedia positive tests

# Acceptance tests
cargo test --test library_acceptance_tests
cargo test --test audio_acceptance_tests
cargo test --test api_acceptance_tests
cargo test --test ffi_acceptance_tests
cargo test --test streaming_acceptance_tests
```

## Using the App

### Local Music
1. Launch the app
2. Click "Scan My Music" — it scans your `~/Music` folder
3. Browse albums, artists, songs
4. Click a song to play

### Apple Music Catalog
1. Click the "Import Session" button (auto-detects your official app login)
2. OR click "Sign In" and log in with your Apple ID in the WebView2 window
3. Search for artists/albums/songs
4. Click a catalog song → plays **30-second preview** (full streaming not yet implemented)

### Diagnostics
Run `run_native_bridge_diagnostics` to verify:
- Apple Music version detected
- CoreAudioToolbox loaded
- CoreMedia loaded  
- CoreFP loaded (6/6 ordinals)
- Current memory usage

## What Doesn't Work Yet

- **Full song streaming** — HLS + FairPlay engine not built (see `3-WHAT-IS-MISSING.md`)
- **Lossless playback from catalog** — same blocker
- **Dolby Atmos** — needs investigation
- **Lyrics** — TTML parser not implemented yet

## Project Commands

```bash
npm run dev          # Frontend only in browser (no Tauri)
cargo check          # Check Rust compilation
cargo test           # Run all tests
cargo tauri dev      # Full app with hot reload
cargo tauri build    # Production build
```

## Quick File Reference

| If you want to... | Edit this file |
|-------------------|---------------|
| Add a new API endpoint | `src-tauri/src/api/client.rs` |
| Change playback behavior | `src-tauri/src/audio/mod.rs` |
| Add a new Tauri command | `src-tauri/src/lib.rs` |
| Change the UI | `index.html`, `src/styles/app.css`, `src/js/app.js` |
| Fix DLL loading | `src-tauri/src/drm/locator.rs`, `loader.rs` |
| Fix authentication | `src-tauri/src/api/auth.rs` |
| Fix session import | `src-tauri/src/api/local_session.rs` |
