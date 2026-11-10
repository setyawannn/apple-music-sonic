# Apple Music DRM & Streaming Pipeline — Deep Analysis

> Analysis of FairPlay DRM implementation in Apple Music Windows App v1.1792.39682.0

## The Problem

The official Apple Music app streams **all full-length audio encrypted** using Apple's FairPlay DRM.
The public REST API only returns **30-second AAC previews** (`.plus.aac.ep.m4a`).
Full songs are delivered via HLS (HTTP Live Streaming) with **SAMPLE-AES encryption**
(AES-128 CBC with sample-level encryption pattern).

## DRM Component Architecture

```
AppleMusic.exe
    │
    ├── AMP.Services.dll (29MB) ── Streaming orchestration
    │       │
    │       ├── Calls CoreMedia.dll for media pipeline
    │       ├── Calls CoreAudioToolbox.dll for audio decoding
    │       └── Uses FairPlay for DRM
    │
    ├── CoreMedia.dll (23MB, 12,934 exports) ── Apple's media framework
    │       │
    │       ├── FigContentKeyBoss* ── Key request management
    │       ├── FigCPECryptor* ── Content Protection Encryption
    │       ├── FigCPEFairPlayCryptor ── FairPlay-specific cryptor
    │       ├── FigSampleBufferSetDecryptor ── Decrypts sample buffers
    │       ├── FigStreamingCache* ── Streaming cache + decryption
    │       ├── FigCustomURLRequestInfo* ── Key request URL handling
    │       ├── FigMediaPlaylist* ── HLS playlist + key mapping
    │       ├── FigPKD* ── Protection Key Database
    │       ├── FigSecureStopManager ── Secure stop (license persistence)
    │       ├── FigH264/HEVCBridge_LocateSliceHeaderForHLSfMP4EncryptableNAL ── CBCS decryption
    │       └── PKDCopyDefaultKeyIDFromSinfArray ── Key ID extraction from sinf atoms
    │
    ├── CoreFP.dll (37MB) ── FairPlay Core (PROTECTED)
    │       └── 6 obfuscated exports: WIn9UJ86JKdV4dM, X46O5IeS, YlCJ3lg, dku592fbFAj, fdjkDSAFjklaf2s, lxpgvVMLd0S7uRl
    │
    ├── CoreKE.dll (45MB) ── Key Exchange (PROTECTED)
    │       └── 2 obfuscated exports: IWvA5fm4IwMS88oQXDANCS, JY0DfiUyFiVcNw89R4t
    │
    └── CoreLSKD.dll (17MB) ── License SDK (PROTECTED)
            └── 2 obfuscated exports: BBdyFX9JObVXUh8aGhB0aD3, Mym0f1araacCU5xeSFjr
```

## Streaming Flow

```
1. User clicks play on a song
2. AMP.Services.dll requests HLS playlist URL from Apple's API
3. HLS playlist is fetched (contains AES-128 SAMPLE-AES encrypted segments)
4. CoreMedia.dll (FigContentKeyBoss) sends key request:
   → Custom URL scheme (skd://) handled by FigCustomURLRequestInfo
   → Request goes to Apple's FairPlay key server
5. CoreKE.dll (Key Exchange):
   → Generates device-specific key exchange payload
   → Communicates with Apple's key server
   → Receives encrypted content key
6. CoreFP.dll (FairPlay Core):
   → Decrypts content key using device's hardware key
   → Provides clear key to CoreMedia
7. CoreLSKD.dll (License SDK):
   → Manages license persistence (offline playback)
   → Handles secure stop (license return)
8. CoreAudioToolbox.dll:
   → Decrypts SAMPLE-AES segments using clear key
   → Decodes ALAC/AAC from decrypted segments
   → Outputs PCM audio via rodio-equivalent pipeline
```

## Why Reverse Engineering DRM is Extremely Hard

| Challenge | Details |
|-----------|---------|
| **Obfuscated exports** | All FairPlay DLLs use random-looking function names |
| **No debug symbols** | Stripped binaries, no PDB files |
| **Hardware binding** | FairPlay keys are bound to device hardware (TPM-like) |
| **Anti-tamper** | CoreFP.dll likely has integrity checks |
| **Legal risk** | Circumventing DRM may violate DMCA and Copyright Law |
| **Key server** | Requires Apple's backend — can't be replicated |

## Available Alternatives (That Work)

| Approach | Effort | Legality | Result |
|----------|--------|----------|--------|
| **A. MusicKit JS + Web Playback** | 1-2 days | ✅ Authorized | Play full songs via Apple's SDK |
| **B. Apple Music API + Local files** | 3-5 days | ✅ Clean | Local music first, streaming later |
| **C. Lossless from CD/Downloads** | Already done | ✅ Clean | Buy/download ALAC files |
| **D. DRM stripping (DO NOT)** | - | ❌ Illegal | DMCA violation |

## Recommendation: Use MusicKit JS

Apple provides a legitimate way to play full songs in a custom app:

1. **Get an Apple Developer account** ($99/year)
2. **Create a MusicKit identifier** in Developer Portal
3. **Generate a MusicKit private key** for JWT signing
4. **Use MusicKit JS** in the WebView2 to play songs
5. The WebView2 handles FairPlay automatically — **Apple's SDK does all the DRM work**

The flow:
```
Tauri WebView2 → MusicKit JS → Apple's CDN (HLS + FairPlay)
                                    ↓
                          WebView2 handles decrypt natively
                                    ↓
                          Audio plays automatically
```

This means:
- ✅ Full lossless playback
- ✅ Dolby Atmos
- ✅ All audio traits from the API
- ✅ No DRM reverse engineering
- ✅ 100% legal
- ✅ Apple handles the hard part

## If You Still Want Local Playback

For **locally owned music files**, use:
- **symphonia** — decodes ALAC, FLAC, AAC, MP3 without DRM
- **Local file scanning** — read ID3/MP4 tags, extract cover art
- **M3U/PLS playlists** — import/export standard formats

## Files to Update in Our Project

The current `api/mod.rs` already has the `StoreSong` and `StoreAlbum` models ready.
The next step is implementing MusicKit auth and using Apple's JS SDK.

**Bottom line:** We can't reverse engineer FairPlay. But we don't need to — Apple's MusicKit JS SDK does everything we need, legally and efficiently.
