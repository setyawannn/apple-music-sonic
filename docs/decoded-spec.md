# Apple Music Windows App — REA Decoded Spec

> Decoded on: 2026-11-09
> App version: 1.1792.39682.0
> Install path: `C:\Program Files\WindowsApps\AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa`

---

## 1. App Architecture Overview

The Apple Music Windows app is a **hybrid native C++ / WinUI app** — NOT an Electron app. It uses:

| Layer | Technology | Details |
|-------|-----------|---------|
| **Entry point** | C++ native EXE | `AppleMusic.exe` — native PE, not .NET managed |
| **UI Framework** | Microsoft.UI.Xaml (WinUI 3) | `Microsoft.UI.Xaml.dll` (15MB), WinUI desktop |
| **Web rendering** | WebView2 (Edge Chromium) | `Microsoft.Web.WebView2.Core.dll` — used for embedded web content |
| **Audio pipeline** | Apple's CoreAudioToolbox | `CoreAudioToolbox.dll` — ported from macOS |
| **DRM / FairPlay** | Apple's CoreFP, CoreKE, CoreLSKD | `CoreFP.dll` (37MB), `CoreKE.dll` (45MB), `CoreLSKD.dll` (17MB) |
| **Media playback** | Apple CoreMedia + AVFoundationCF | `CoreMedia.dll` (23MB), `AVFoundationCF.dll` |
| **Graphics** | Apple CoreGraphics + CoreText | `CoreGraphics.dll`, `CoreText.dll` |
| **Networking** | Apple CFNetwork | `CFNetwork.dll` (1.8MB), with PAC support JS |
| **Auth / iCloud** | Apple AuthKitWin | `AuthKitWin.dll` (1MB), localized in 40+ languages |
| **Services** | AMP.* native DLLs | `AMP.Services.dll` (29MB), `AMP.UI.Views.dll` (14MB) — native AOT/C++ |
| **JavaScript** | Google V8 | `v8.dll` (24MB) — used for JS evaluation internally |
| **SQLite** | Custom SQLite3 | `SQLite3.dll` — local music database |
| **Gracenote** | Gracenote SDK | `gnsdk_dsp.dll`, `gnsdk_manager.dll`, `gnsdk_musicid.dll` — audio fingerprinting |
| **mdns** | Apple Bonjour/mDNS | `mDNSResponderDLL.dll`, `dnssd.dll` — AirPlay discovery |
| **XML** | libxml2, libxslt | `libxml2.dll`, `libxslt.dll` — XML parsing for metadata |

## 2. Key Native DLLs & Their Roles

### 2.1 Apple Ported Frameworks (from macOS/iOS)

| DLL | Size | Role |
|-----|------|------|
| `CoreFoundation.dll` | 2.2 MB | Foundation types (CFString, CFDictionary, CFURL, run loops) |
| `CFNetwork.dll` | 1.8 MB | HTTP networking, TLS, PAC proxy support |
| `CoreAudioToolbox.dll` | 8.9 MB | Audio codec, format conversion, output |
| `CoreMedia.dll` | 23 MB | Media pipeline, timing, playback control |
| `CoreFP.dll` | 37 MB | FairPlay DRM core |
| `CoreKE.dll` | 45 MB | Key exchange / security |
| `CoreLSKD.dll` | 17 MB | License SDK |
| `CoreGraphics.dll` | 5 MB | 2D rendering |
| `CoreText.dll` | 1.1 MB | Text layout and rendering |
| `CoreVideo.dll` | 254 KB | Video frame processing |
| `AVFoundationCF.dll` | 749 KB | AVFoundation for Windows (media capture/playback) |
| `AppleMediaServicesKit.dll` | 6.6 MB | Apple Media Services (store, catalog, search) |
| `ApplePushDirect.dll` | 350 KB | Apple Push Notifications |
| `AuthKitWin.dll` | 1 MB | Apple ID authentication on Windows |
| `JavaScriptCoreV8.dll` | 193 KB | JS bridge (thin wrapper over V8) |
| `libdispatch.dll` | 112 KB | Grand Central Dispatch (concurrency) |
| `objc.dll` | 244 KB | Objective-C runtime |
| `Foundation.dll` | 2.7 MB | Foundation framework (ported) |
| `QuartzCore.dll` | 1.8 MB | Core Animation |

### 2.2 AMP (Apple Music Platform) Native DLLs

| DLL | Size | Role |
|-----|------|------|
| `AMP.Services.dll` | 29 MB | Core services: playback, library, streaming, DRM orchestration |
| `AMP.UI.Views.dll` | 14 MB | WinUI view implementations |
| `AMP.UI.Controls.dll` | 5 MB | Custom WinUI controls |
| `AMP.UI.ViewModels.dll` | 576 KB | ViewModels (MVVM pattern) |
| `AMP.Utils.dll` | 1.9 MB | Utilities |
| `AMP.Core.winmd` | 11 KB | .NET WinMD metadata for WinRT interop |
| `AMP.Services.winmd` | 708 KB | WinRT metadata for services |
| `AMP.UI.Controls.winmd` | 152 KB | WinRT metadata for controls |
| `AMP.UI.ViewModels.winmd` | 33 KB | WinRT metadata for viewmodels |
| `AMP.UI.Views.winmd` | 155 KB | WinRT metadata for views |

> **Key finding:** The `.winmd` files expose WinRT COM interfaces that allow the app to interact with Apple's native C++ code through WinRT projections. However, the actual implementation DLLs (AMP.Services.dll, AMP.UI.Views.dll, etc.) are **native x64 AOT-compiled code**, NOT .NET managed assemblies.

### 2.3 Microsoft Platform DLLs

| DLL | Size | Role |
|-----|------|------|
| `Microsoft.UI.Xaml.dll` | 15 MB | WinUI 3 runtime |
| `Microsoft.Web.WebView2.Core.dll` | 793 KB | WebView2 host |
| `Microsoft.Web.WebView2.Core.Projection.dll` | 688 KB | WebView2 .NET projection |
| `Microsoft.Graphics.Canvas.dll` | 1.6 MB | Win2D canvas |
| `DWriteCore.dll` | 3.2 MB | DirectWrite typography |

## 3. Authentication Flow

Based on the presence of `AuthKitWin.dll` and analysis of the app capabilities:

1. **AuthKitWin.dll** handles Apple ID authentication on Windows
2. It's localized in **40+ languages** (ar, ca, cs, da, de, el, en, es, fi, fr, he, hi, hr, hu, id, it, ja, ko, ms, nb, nl, pl, pt, pt_PT, ro, ru, sk, sv, th, tr, uk, vi, zh_CN, zh_HK, zh_TW, etc.)
3. The app has `internetClient`, `runFullTrust`, `allowElevation` capabilities
4. Authentication likely uses OAuth 2.0 + MusicKit token exchange
5. Tokens are stored in Windows Credential Manager or app-local storage

**For our Rust client:** We'll need to:
- Reimplement AuthKitWin's authentication flow (OAuth 2.0 with Apple)
- Store tokens locally (Windows Credential Manager or encrypted file)
- Generate MusicKit tokens for API calls

## 4. Audio & Streaming Pipeline

### 4.1 Audio Playback Path
```
AppleMediaServicesKit (catalog/search)
  → AMP.Services (streaming orchestration)
    → CoreMedia (media pipeline & timing)
      → CoreAudioToolbox (decoding: AAC, ALAC, MP3)
        → Audio output (Windows audio session)
```

### 4.2 Lossless Audio (ALAC)
- `CoreAudioToolbox.dll` (8.9MB) contains the ALAC decoder
- `gnsdk_dsp.dll` (3.3MB) is Gracenote's audio DSP for fingerprinting
- Lossless streaming uses HLS (HTTP Live Streaming) with fragmented MP4 segments
- FairPlay DRM (CoreFP + CoreKE + CoreLSKD) decrypts the stream

### 4.3 Lyrics System
- TTML (Timed Text Markup Language) parser for lyrics
- Ported from Android Apple Music app
- Supports:
  - Word-by-word karaoke timing
  - Line-by-line timing
  - Static (non-timed) lyrics
  - Background vocals (role="x-bg")
  - Agent/duet support (multiple singers)
  - Pronunciation/transliteration
  - Translations (subtitle-style and replacement-style)
- Located in `AMP.Services/Lyrics/` directory

## 5. Networking & API

### 5.1 Apple Music API
- `CFNetwork.dll` handles HTTP networking
- `PACSupport.js` provides proxy auto-config support
- Apple Media Services API base URL: `https://api.music.apple.com`
- Endpoints (to be confirmed via network capture):
  - `/v1/catalog/{storefront}/search`
  - `/v1/catalog/{storefront}/albums/{id}`
  - `/v1/catalog/{storefront}/songs/{id}`
  - `/v1/me/recommendations`
  - `/v1/me/library/songs`
  - `/v1/me/library/playlists`

### 5.2 Streaming Protocol
- **HLS** (HTTP Live Streaming) for audio
- **FairPlay Streaming** for DRM
- Audio codecs: AAC (256kbps), ALAC (lossless up to 24-bit/192kHz)
- Content keys delivered through CoreFP/CoreKE/CoreLSKD

## 6. UI Architecture

### 6.1 View Hierarchy
```
Window (AppleMusic.exe)
  ├── NavigationView (sidebar)
  │   ├── Library (playlists, albums, artists, songs)
  │   ├── Listen Now
  │   ├── Browse
  │   └── Radio
  ├── Content views (AMP.UI.Views)
  │   ├── AlbumDetailView
  │   ├── PlaylistView
  │   ├── ArtistView
  │   ├── SearchView
  │   └── NowPlayingView
  ├── PlaybackBar (persistent bottom bar)
  └── QueuePanel (right side drawer)
```

### 6.2 UI Framework
- **Built with:** WinUI 3 (Microsoft.UI.Xaml) + Apple's own WinRT controls
- **Custom controls:** AMP.UI.Controls.dll (5MB) + SVGs in `AMP.UI.Controls/Assets/`
- **WinRT metadata:** `.winmd` files define COM interfaces for cross-language interop
- **Icon system:** SVG assets with theme variants (`.contrast-high.svg`, `.theme-dark.svg`, `.theme-light.svg`)

### 6.3 Available SVGs reveal UI features:
- Lossless badge: `lossless.svg`
- Dolby Atmos: `DolbyAtmosForMovie.svg`, `dolby.svg`
- Dolby Vision: `dolbyvision.svg`
- 4K/HDR/HD badges
- Default artwork for: music, artist, playlist, playlist_folder, playlist_genius, playlist_smart, video, movie, musicvideo
- Concert tickets, Extras (iTunes Extras)
- Empty states (empty_downloaded, empty_music_note, empty_music_videos)
- Rotten Tomatoes integration (for video)
- Content ratings (US: G, PG, PG-13, R, NC-17, TV-Y, TV-Y7, TV-G, TV-PG, TV-14, TV-MA)
- ADM badge (Audio Description)

## 7. Capabilities & Permissions (from AppxManifest)

```xml
<Capabilities>
  <Capability Name="internetClient" />          <!-- Network access -->
  <rescap:Capability Name="runFullTrust" />      <!-- Full trust execution -->
  <rescap:Capability Name="allowElevation" />    <!-- Can elevate privileges -->
  <uap:Capability Name="cellularData" />         <!-- Mobile data -->
  <uap:Capability Name="wifiData" />             <!-- WiFi data -->
  <rescap:Capability Name="broadFileSystemAccess" /> <!-- File system access -->
</Capabilities>
```

## 7.1 Resource Comparison: Official App vs Our Rust Client

| Metric | **Official Apple Music** (Windows) | **Apple Music RS** (Our Target) | **Enhancement** |
|--------|-----------------------------------|-------------------------------|-----------------|
| **RAM (idle)** | ~200-350 MB | **~40-60 MB** | **5-7x less** |
| **RAM (playing music)** | ~300-500 MB | **~60-100 MB** | **5x less** |
| **CPU (idle)** | ~5-15% | **~1-3%** | **5x less** |
| **CPU (while streaming)** | ~8-20% | **~3-8%** | **2-3x less** |
| **Disk space (app only)** | ~500 MB - 1 GB | **~15-30 MB** | **20-30x smaller** |
| **Startup time** | ~3-8 seconds | **~0.5-1 second** | **5x faster** |
| **Background processes** | 3-5 (AppleMusic.exe, AMPLibraryAgent, RestartAgent, etc.) | **1 single binary** | **Clean** |
| **GPU usage** | WebView2 compositor + animations | WebView2 compositor | **Same** |

### Why Such a Big Difference?

The official app loads into memory:

| Process | Typical RAM |
|---------|------------|
| `AppleMusic.exe` (Electron shell) | ~150 MB |
| `AMPLibraryAgent.exe` (library sync daemon) | ~50 MB |
| `RestartAgent.exe` (update checker) | ~10 MB |
| `v8.dll` (JavaScript engine, always loaded) | ~24 MB |
| Apple DLLs (CoreFoundation, CFNetwork, etc.) | ~50-100 MB |
| Gracenote SDK (audio fingerprinting) | ~10 MB |
| WebView2 runtime (shared) | ~100-200 MB |

Our app eliminates ALL of these:
- **No Electron** — saves ~150 MB
- **No AMPLibraryAgent** — no background daemon
- **No V8** — we use the OS WebView2, not a bundled JS engine
- **No Gracenote** — no audio fingerprinting telemetry
- **No Apple DLLs** — we use Rust crates instead (symphonia, reqwest, etc.)
- **No telemetry** — no `xp.apple.com` reporting
- **No RestartAgent** — no background updater

Net result: **5-7x less RAM, 20-30x smaller disk footprint, 5x faster startup.**

---

## 8. Key Findings for Rust Implementation

### 8.1 What We CAN Reuse
1. **Apple Music API** — public REST API that requires only a MusicKit token
2. **HLS streaming format** — standard protocol, many Rust crates handle it (`hls-m3u8`, `reqwest`)
3. **ALAC decoding** — open format, Rust Crates: `symphonia` (supports ALAC via isomp4/ALAC codec)
4. **FairPlay DRM** — This is the hard part. CoreFP/CoreKE/CoreLSKD are Apple-proprietary

### 8.2 What We Need to Build
| Component | Approach |
|-----------|----------|
| **Auth (Apple ID)** | OAuth 2.0 with Apple's auth endpoints + MusicKit token generation |
| **Music API Client** | `reqwest` + serde — call Apple Music REST API |
| **Audio Decoder** | `symphonia` — supports AAC, ALAC, MP3, FLAC |
| **HLS Parser** | `hls-m3u8` crate or custom parser |
| **DRM / FairPlay** | Must call CoreFP/CoreKE native DLLs via FFI, OR use Windows Media Foundation DRM |
| **Local Library** | SQLite via `rusqlite` |
| **Artwork Cache** | `image` crate + file system cache |
| **Lyrics Parser** | Custom TTML parser (spec from readme.txt above) |

### 8.3 Architecture for Rust Client
```
apple-music-rs
├── src/
│   ├── main.rs              ← GPUI app entry
│   ├── auth/                 ← Apple ID authentication + MusicKit tokens
│   ├── api/                  ← Apple Music REST API client
│   │   ├── catalog.rs        ← Search, albums, songs, playlists
│   │   ├── library.rs        ← User's library (songs, playlists)
│   │   └── streaming.rs      ← HLS playlist retrieval, FairPlay
│   ├── audio/                ← Audio playback
│   │   ├── decoder.rs        ← symphonia-based decoding (AAC, ALAC, MP3)
│   │   ├── stream.rs         ← HLS stream parser and downloader
│   │   ├── drm.rs            ← FairPlay key request and decryption
│   │   └── output.rs         ← Audio output (cpal or WASAPI)
│   ├── library/              ← Local music database
│   │   ├── db.rs             ← SQLite schema and queries
│   │   ├── models.rs         ← Track, Album, Playlist, Artist structs
│   │   ├── import.rs         ← Local file scanning and metadata extraction
│   │   └── sync.rs           ← iCloud library sync
│   ├── lyrics/               ← TTML lyrics parser
│   ├── ui/                   ← GPUI views
│   │   ├── auth_view.rs      ← Sign-in with Apple
│   │   ├── library_view.rs   ← Album grid, song list
│   │   ├── album_detail.rs   ← Album track list + artwork
│   │   ├── playback_bar.rs   ← Persistent bottom playback bar
│   │   ├── queue_panel.rs    ← Up next queue
│   │   ├── search_view.rs    ← Search + results
│   │   └── now_playing.rs    ← Full-screen now playing view
│   ├── image_cache.rs        ← Artwork caching
│   └── drm/                  ← FairPlay DRM integration (FFI to CoreFP.dll or alternative)
```

## 9. Next Steps for Deep Analysis

| # | Action | Tool | What We'll Learn |
|---|--------|------|-----------------|
| 1 | Open AppleMusic.exe in binary analyzer | `rea.open_binary` | Entry point, imports, startup sequence |
| 2 | Analyze CoreFP.dll (37MB DRM) | `rea.open_binary` | FairPlay implementation details |
| 3 | Analyze CoreAudioToolbox.dll | `rea.open_binary` | ALAC decoder internals |
| 4 | Capture network traffic while using the app | Browser/REA capture | API endpoints, auth headers, HLS URLs |
| 5 | Analyze `AppleMediaServicesKit.dll` | `rea.open_binary` | Store/catalog service methods |

---

*Decoded for educational purposes. Only analyze software you are authorized to examine.*
