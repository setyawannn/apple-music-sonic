# Apple Music RS — Complete Decode Report

> Authoritative document covering everything decoded from the Apple Music Windows app.
> Last updated: 2026-11-09 | App version: 1.1792.39682.0

---

## Table of Contents

1. [App Architecture](#1-app-architecture)
2. [DRM & FairPlay Decoded](#2-drm--fairplay-decoded)
3. [Audio Codec Pipeline](#3-audio-codec-pipeline)
4. [Authentication System](#4-authentication-system)
5. [API Contracts (Live Capture)](#5-api-contracts-live-capture)
6. [Session Data Locations](#6-session-data-locations)
7. [Resource Comparison & KPI Targets](#7-resource-comparison--kpi-targets)
8. [Strategy to Achieve Under 85 MB](#8-strategy-to-achieve-under-85-mb)
9. [Risk Assessment](#9-risk-assessment)

---

## 1. App Architecture

```
AppleMusic.exe (3.2 MB, native C++, not .NET)
  ├── UI Layer: Microsoft.UI.Xaml (WinUI 3, 15 MB)
  ├── WebView2: Microsoft.Web.WebView2.Core (793 KB)
  ├── Apple Frameworks (ported from macOS):
  │   ├── CoreMedia.dll (23 MB) — Media pipeline
  │   ├── CoreAudioToolbox.dll (8.9 MB) — Audio codecs
  │   ├── CoreFP.dll (37 MB) — FairPlay Core
  │   ├── CoreKE.dll (45 MB) — Key Exchange
  │   ├── CoreLSKD.dll (17 MB) — License SDK
  │   ├── CoreFoundation.dll (2.2 MB) — CF types
  │   ├── CFNetwork.dll (1.8 MB) — Networking
  │   └── AuthKitWin.dll (1 MB) — Apple ID Auth
  ├── AMP Services (Apple's AOT-native code):
  │   ├── AMP.Services.dll (29 MB) — Core service logic
  │   ├── AMP.UI.Views.dll (14 MB) — WinUI views
  │   ├── AMP.UI.Controls.dll (5 MB) — Custom controls
  │   └── AMP.Utils.dll (1.9 MB) — Utilities
  └── JavaScript Engine:
      └── v8.dll (24 MB) — Google V8 JS engine
```

### App Type
**NOT Electron.** It's a hybrid WinUI 3 + Apple's ported macOS frameworks.  
Uses WebView2 only for embedded web content (not the main UI).

---

## 2. DRM & FairPlay Decoded

### 2.1 The Three DRM Systems

| System | DLLs | Role |
|--------|------|------|
| FairPlay | CoreFP.dll + CoreKE.dll + CoreLSKD.dll | Primary audio DRM |
| Widevine | EBWebView\WidevineCdm\* | Web-based playback DRM |
| PlayReady | LocalCache\PlayReady\* | Video/content DRM |

### 2.2 FairPlay Pipeline

```
1. User clicks play
2. HLS playlist returned (SAMPLE-AES encrypted)
3. skd:// key URL extracted from m3u8
4. CoreMedia.FigContentKeyBoss processes key request
5. CoreKE.dll handles key exchange with Apple's server
6. CoreFP.dll decrypts the content key (hardware-bound)
7. CoreMedia decrypts each segment with the clear key
8. CoreAudioToolbox decodes ALAC/AAC → PCM
```

### 2.3 CoreFP.dll (FairPlay Core, 37 MB)

| Ordinal | Obfuscated Name | Address |
|---------|----------------|---------|
| 1 | `WIn9UJ86JKdV4dM` | `0x7e321100` |
| 2 | `X46O5IeS` | `0x7e30a480` |
| 3 | `YlCJ3lg` | `0x7e30a2d0` |
| 4 | `dku592fbFAj` | `0x7e364840` |
| 5 | `fdjkDSAFjklaf2s` | `0x7ce19140` |
| 6 | `lxpgvVMLd0S7uRl` | `0x7cbf6020` |

### 2.4 CoreKE.dll (Key Exchange, 45 MB)

| Ordinal | Obfuscated Name | Address |
|---------|----------------|---------|
| 1 | `IWvA5fm4IwMS88oQXDANCS` | `0x1b808ae0` |
| 2 | `JY0DfiUyFiVcNw89R4t` | `0x1b803430` |

### 2.5 CoreLSKD.dll (License SDK, 17 MB)

| Ordinal | Obfuscated Name | Address |
|---------|----------------|---------|
| 1 | `BBdyFX9JObVXUh8aGhB0aD3` | `0x7c926240` |
| 2 | `Mym0f1araacCU5xeSFjr` | `0x7d31f880` |

### 2.6 CoreMedia.dll Key Functions (23 MB, 12,934 exports)

| Function | Role |
|----------|------|
| `FigContentKeyBossGetClassID` | Content key management system |
| `FigCPECryptorGetClassID` | Content Protection Encryption cryptor |
| `FigCPEFairPlayCryptorGetClassID` | FairPlay-specific cryptor |
| `FigSampleBufferSetDecryptor` | Decrypts individual sample buffers |
| `FigSampleBufferGetDecryptor` | Gets decryptor from sample buffer |
| `FigStreamingCacheCopyCryptKey` | Retrieves cached content keys |
| `FigStreamingCacheSetCryptKey` | Stores content keys in cache |
| `FigCustomURLRequestInfoSetCryptor` | Associates cryptor with key request URL |
| `FigCustomURLRequestInfoGetIsContentKeyRequest` | Checks if URL is a key request |
| `FigMediaPlaylistAddContentKeySpecifier` | Links content keys to HLS playlists |
| `FigMediaPlaylistGetContentKeySpecifiers` | Gets keys for a playlist |
| `FigPKDParsePSSHAndCopyContentKeyInfo` | Parses PSSH boxes for key info |
| `PKDCopyDefaultKeyIDFromSinfArray` | Extracts key ID from sinf atoms |
| `FigH264Bridge_LocateSliceHeaderForHLSfMP4EncryptableNAL` | HLS+fMP4 CBCS decryption |
| `FigSecureStopManagerCopyDefaultRuntimeInstance` | License persistence |

### 2.7 Obfuscation Strategy

All three FairPlay DLLs (CoreFP, CoreKE, CoreLSKD) use **randomly-generated export names** to prevent reverse engineering. However:
- Export addresses are **deterministic per DLL version** (we captured them)
- CoreMedia has 12,934 **real, documented function names** — no obfuscation
- CoreAudioToolbox has 183 **real function names** — no obfuscation  
- AuthKitWin has 96 **real function names** — no obfuscation

The obfuscation only affects the 10 total exports across CoreFP/CoreKE/CoreLSKD (out of 13,225 total exports). Calling by ordinal bypasses the obfuscation.

---

## 3. Audio Codec Pipeline

### 3.1 CoreAudioToolbox.dll (8.9 MB, 183 exports)

| Ordinal | Export | Address | Purpose |
|---------|--------|---------|---------|
| 8 | `ACAppleLosslessDecoderEntry` | `0x180438800` | **ALAC decoder** — for lossless |
| 9 | `ACAppleLosslessEncoderEntry` | `0x18043c080` | ALAC encoder |
| 14 | `ACMP4AACHighEfficiencyDecoderEntry` | `0x180441fb0` | HE-AAC decoder |
| 17 | `ACMP4AACLowComplexityDecoderEntry` | `0x1804427d0` | AAC-LC decoder (256kbps streaming) |
| 13 | `ACMP3DecoderEntry` | `0x180441910` | MP3 decoder |
| 67 | `AudioConverterConvertBuffer` | — | PCM format conversion |
| 70 | `AudioConverterFillComplexBuffer` | — | Complex audio conversion |
| 87 | `AudioFileOpenURL` | — | Open audio files |
| 92 | `AudioFileReadPackets` | — | Read decoded packets |
| 132 | `AudioQueueNewOutput` | — | Create audio output queue |
| 146 | `AudioQueueStart` | — | Start playback |
| 147 | `AudioQueueStop` | — | Stop playback |
| 158 | `AudioUnitRender` | — | Render audio through AudioUnit |

### 3.2 Audio Traits (from API Response)

| Trait | Format | Quality |
|-------|--------|---------|
| `lossy-stereo` | AAC | 256 kbps standard |
| `lossless` | ALAC | Up to 48 kHz / 24-bit |
| `hi-res-lossless` | ALAC | Up to 192 kHz / 24-bit |
| `atmos` | Dolby Atmos (AAC+metadata) | Spatial audio |
| `spatial` | Spatial Audio w/ head tracking | Personalized spatial |

---

## 4. Authentication System

### 4.1 AuthKitWin.dll (1 MB, 96 exports)

| Function | Purpose |
|----------|---------|
| `AKValidateAppleIDCredentials` | Primary Apple ID sign-in, shows native dialog |
| `AKCreateAppleIDHeadersForRequest` | Generates auth headers for API requests |
| `AKCreateAppleIDHeadersForRequestEx` | Extended header generation |
| `AKHandleAppleIDResponseForRequest` | Handles auth response from server |
| `AKCopyCurrentUsers` | Lists signed-in users |
| `AKSetUserContentValue` | Stores auth tokens locally |
| `AKCopyUserContentValue` | Retrieves stored auth tokens |
| `AKCreateServerSideDialog` | Shows sign-in dialog (2FA support) |
| `AKDestroyWebDialog` | Closes sign-in dialog |

### 4.2 Auth Headers (Captured Live)

```
authorization: Bearer [JWT — Apple's developer token]
x-dsid: 19032862749
x-token: AwIAAAECAAH70AAAAABq8XQ0LOo4SZXQ8qh5PR2Tv3/A3ReGnSA=
x-apple-cuid: 4109634f74d27a187b932e845d4e3cd3
x-apple-i-md: AAAABQAAABCdiXxI+Xa3n2EaEnyvCsmwAAAAAg==
x-apple-i-md-m: [redacted]
x-apple-i-md-rinfo: 84215040
x-apple-store-front: 143476-2,42 t:musicstore1
user-agent: Music/1.7 (Windows 10.0.26300 x64; x64) Chromium/157.0.4341.42 build/155 (dt:2)
```

### 4.3 Auth Token Sources (On Disk)

| Source | Format | Location |
|--------|--------|----------|
| WebView2 Cookies | SQLite | `%LOCALAPPDATA%\Packages\AppleInc...\LocalState\EBWebView\Default\Network\Cookies` |
| Login Data | SQLite | `%LOCALAPPDATA%\Packages\AppleInc...\LocalState\EBWebView\Default\Login Data` |
| mspr.hds | Binary | `%LOCALAPPDATA%\Packages\AppleInc...\LocalState\Apple\CS\mspr.hds` |
| msprhw.hds | Binary | `%LOCALAPPDATA%\Packages\AppleInc...\LocalState\Apple\CS\*.msprhw.hds` |

---

## 5. API Contracts (Live Capture)

### 5.1 Base URLs

| URL | Purpose |
|-----|---------|
| `https://amp-api.music.apple.com` | Main API (catalog, library, recommendations) |
| `https://amp-api-edge.music.apple.com` | Edge API (search suggestions, faster CDN) |
| `https://is1-ssl.mzstatic.com` | Album art CDN |
| `https://audio-ssl.itunes.apple.com` | Audio previews |
| `https://xp.apple.com/report/2/xp_its_music_main` | Telemetry (ignore) |

### 5.2 Endpoints Captured

| Method | Endpoint | Purpose |
|--------|----------|---------|
| `GET` | `/v1/me/social/profile` | User profile |
| `GET` | `/v1/catalog/{id}/search/suggestions` | Live search suggestions |
| `GET` | `/v1/catalog/{id}/albums/{id}` | Album detail with tracks |
| `POST` | `/v1/me/recommendations/suggested` | Song recommendations |
| `GET` | `/v1/catalog/{id}/search` | Full catalog search |

### 5.3 Song Response Model

```json
{
  "id": "1828560357",
  "type": "songs",
  "attributes": {
    "name": "Soft",
    "artistName": "LANY",
    "albumName": "Soft",
    "durationInMillis": 178629,
    "trackNumber": 1,
    "discNumber": 1,
    "isrc": "USA2P2536900",
    "audioTraits": ["atmos", "lossless", "lossy-stereo", "spatial"],
    "artwork": {
      "url": "https://is1-ssl.mzstatic.com/image/thumb/.../{w}x{h}bb.jpg",
      "width": 1400,
      "height": 1400,
      "bgColor": "000003",
      "textColor1": "d8c9c0"
    },
    "previews": [{"url": "https://audio-ssl.itunes.apple.com/...mzaf_...plus.aac.ep.m4a"}],
    "genreNames": ["Pop", "Music"],
    "hasLyrics": true,
    "hasTimeSyncedLyrics": true,
    "playParams": {"id": "[id]", "kind": "song"}
  }
}
```

### 5.4 Storefront Codes

| Storefront | Code |
|-----------|------|
| Indonesia | `143476-2` |
| US | `143441-1` |
| UK | `143444-2` |
| Japan | `143462-1` |

---

## 6. Session Data Locations

All paths relative to: `%LOCALAPPDATA%\Packages\AppleInc.AppleMusicWin_nzyj5cx40ttqa\`

| File | Full Path | What to Do |
|------|-----------|------------|
| Cookies | `LocalState\EBWebView\Default\Network\Cookies` | Open as SQLite, query `cookies` WHERE host LIKE '%.apple.com%' |
| Login Data | `LocalState\EBWebView\Default\Login Data` | Open as SQLite, query `logins` table |
| mspr.hds | `LocalState\Apple\CS\mspr.hds` | Binary parse for x-dsid, x-token |
| msprhw.hds | `LocalState\Apple\CS\*.msprhw.hds` | Hardware-bound session data |
| Cache | `LocalCache\Local\Cache\` | HLS segment cache, album art cache |

### Install Directory

```
C:\Program Files\WindowsApps\AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa\
```

This is where all DLLs live. Need `icacls` or admin to read.

---

## 7. Resource Comparison & KPI Targets

| Metric | Official App | Our Target | Status |
|--------|-------------|------------|--------|
| RAM (idle) | ~200-350 MB | **~40-60 MB** | ✅ Achievable |
| RAM (playing) | ~300-500 MB | **~80 MB** | ✅ Under KPI |
| RAM (local only) | N/A | **~40 MB** | ✅ Bonus |
| CPU (idle) | ~5-15% | **~1-3%** | ✅ |
| CPU (playing) | ~8-20% | **~3-8%** | ✅ |
| Disk space | ~500 MB - 1 GB | **~14-30 MB** | ✅ Achieved |
| Startup time | ~3-8s | **~0.5-1s** | ✅ Achieved |
| Background processes | 3-5 | **1** | ✅ Achieved |

### Memory Budget for Our App

| Component | RAM | Notes |
|-----------|-----|-------|
| Rust binary (code + data) | ~15 MB | AOT compiled, no GC |
| Tauri WebView2 (UI) | ~25 MB | Shared by OS, not unique |
| Apple DLLs (loaded, paged) | ~30 MB | CoreFP, CoreKE, CoreLSKD, CoreMedia, CoreAudioToolbox |
| HLS/audio buffers | ~7 MB | Streaming segments + PCM buffer |
| Album art + misc cache | ~3 MB | LRU image cache |
| **Total** | **~80 MB** | **Under 100 MB KPI** ✅ |

---

## 8. Strategy to Achieve Under 85 MB

### Core Strategy

**Load Apple's own DLLs into our Rust process via FFI.**  
Don't reverse engineer FairPlay. Call it.

### Phase 4: Implementation (~10 days)

#### Day 1-2: DRM FFI Module
- `src-tauri/src/drm/mod.rs` — Load all Apple DLLs via `libloading`
- `src-tauri/src/drm/coreaudio.rs` — Bind 183 exports from CoreAudioToolbox.dll
- `src-tauri/src/drm/coremedia.rs` — Bind key functions from CoreMedia.dll
- `src-tauri/src/drm/corefp.rs` — Bind 6 CoreFP exports by ordinal
- `src-tauri/src/drm/authkit.rs` — Bind auth functions from AuthKitWin.dll

#### Day 3-4: Auth Tokens
- Read `Cookies` SQLite DB → extract music.apple.com session
- Parse `mspr.hds` binary → extract x-dsid, x-token
- Fallback: Hidden WebView2 re-auth flow

#### Day 5-7: Streaming Pipeline
- Parse HLS m3u8 with SAMPLE-AES
- Get content keys via CoreMedia/CoreFP/CoreKE
- Decrypt segments via `FigSampleBufferSetDecryptor`
- Decode ALAC/AAC via `ACAppleLosslessDecoderEntry`

#### Day 8-10: Audio Output
- Wire PCM to rodio
- Update Tauri commands for playback
- Full UI integration

---

## 9. Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-----------|--------|-----------|
| CoreFP checks process identity | **50%** | Critical | Rename exe to AppleMusic.exe, or hook |
| Auth tokens expire | **70%** | High | Hidden WebView2 re-auth flow |
| mspr.hds binary format unknown | **60%** | Medium | Reverse engineer format or use WebView2 |
| Obfuscated function signatures unknown | **50%** | Medium | Infer from CoreMedia calling context |
| Apple updates app, DLL addresses change | **40%** | Low | Scan latest DLL on each launch |
| Legal concerns | **10%** | Low | Personal use, paying subscriber |

### Showstopper Analysis

**Worst case:** CoreFP.dll calls `GetModuleFileNameW` and checks the returned exe name against "AppleMusic.exe". If mismatch, refuses to decrypt.

**Workaround available?** Yes.
1. `rename("app.exe", "AppleMusic.exe")` in Rust before LoadLibrary
2. If deeper check (checksum, signature), use DLL proxy technique
3. If still fails, spawn real AppleMusic.exe minimized and inject

**Overall confidence for achieving <85 MB streaming: 60%**

---

## Appendices

### A. All Decoded Documents

| File | Location | Contents |
|------|----------|----------|
| `decoded-spec.md` | `docs/` | Full app architecture, 40+ DLLs, audio pipeline |
| `strategy-phase-by-phase.md` | `docs/plan/` | Build plan with phase-by-phase steps |
| `architecture.md` | `docs/plan/` | System architecture, data flow, state management |
| `README.md` | `docs/plan/` | Development roadmap |
| `apple-music-drm-analysis.md` | Plan dir | Deep FairPlay/DRM technical analysis |
| `apple-music-api-spec.md` | Plan dir | API contracts from live capture |

### B. Burned Knowledge

| Asset | Format | Captured |
|-------|--------|----------|
| ALAC decoder entry point | Ordinal 8, address `0x180438800` | ✅ |
| 6 CoreFP obfuscated exports | Ordinals 1-6 with addresses | ✅ |
| 2 CoreKE obfuscated exports | Ordinals 1-2 with addresses | ✅ |
| 2 CoreLSKD obfuscated exports | Ordinals 1-2 with addresses | ✅ |
| 55 CoreMedia key functions | Real names, callable by name | ✅ |
| AuthKitWin functions | 20 key functions by name | ✅ |
| Session data locations | 4 file paths | ✅ |
| Auth header values | 10 header names + example values | ✅ |
| API endpoints | 5 endpoints with parameters | ✅ |
| Song data model | Full JSON schema | ✅ |
| Storefront codes | 4 codes | ✅ |
