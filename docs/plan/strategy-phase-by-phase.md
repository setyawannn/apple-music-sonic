# Apple Music RS — Phase-by-Phase Strategy to Achieve KPI

**Target:** Single binary under 100 MB RAM, full lossless streaming, Apple Music UI.

**Decoded Intelligence Summary:**

| Asset | Location | Size | What It Gives Us |
|-------|----------|------|------------------|
| `CoreFP.dll` | App install dir | 37 MB | FairPlay key decryption (6 obfuscated exports) |
| `CoreKE.dll` | App install dir | 45 MB | Key exchange (2 obfuscated exports) |
| `CoreLSKD.dll` | App install dir | 17 MB | License management (2 obfuscated exports) |
| `CoreMedia.dll` | App install dir | 23 MB | Media pipeline (12,934 real exports) |
| `CoreAudioToolbox.dll` | App install dir | 8.9 MB | Audio codecs (183 exports: ALAC, AAC, MP3) |
| `AuthKitWin.dll` | App install dir | 1 MB | Apple ID auth (96 exports) |
| `Cookies` | Package LocalState | 20 KB | WebView2 session cookies for music.apple.com |
| `Login Data` | Package LocalState | 43 KB | Stored web credentials |
| `TransportSecurity` | Package LocalState | 188 B | SSL/TLS session data |
| `mspr.hds` | Package LocalState | 1.5 MB | Apple session/token storage |
| HTTP Headers | Fiddler capture | — | Real auth header values captured live |

---

## Phase 1: Foundation (Week 1)

**Goal:** Load Apple's DLLs into our process and verify they can be called.

### Steps

```
1.1 Set up Rust FFI bindings to CoreAudioToolbox.dll (8.9 MB)
    → LoadLibraryA("CoreAudioToolbox.dll")
    → Call ACAppleLosslessDecoderEntry by address (0x180438800)
    → Verify it decodes a test ALAC file to PCM
    → Output: Working ALAC decoder in Rust via Apple's binary

1.2 Set up Rust FFI bindings to CoreMedia.dll
    → LoadLibraryA("CoreMedia.dll") → triggers CoreFP/CoreKE load as dependencies
    → Bind key functions:
      - FigContentKeyBossGetClassID
      - FigCPECryptorGetClassID
      - FigCPEFairPlayCryptorGetClassID
      - FigSampleBufferSetDecryptor
      - FigStreamingCacheCopyCryptKey
      - FigCustomURLRequestInfoSetCryptor
      - FigMediaPlaylistAddContentKeySpecifier
    → Output: All DRM/media function pointers ready

1.3 Verify DLL loading stays under 80 MB
    → Load all DLLs, measure process memory
```

**RAM target after Phase 1:** ~40 MB (our app) + ~30 MB (loaded DLLs) = ~70 MB ✅

**Files to create:**
- `src-tauri/src/drm/mod.rs` — FFI bindings module
- `src-tauri/src/drm/coreaudio.rs` — CoreAudioToolbox bindings
- `src-tauri/src/drm/coremedia.rs` — CoreMedia bindings
- `src-tauri/src/drm/corefp.rs` — CoreFP bindings

---

## Phase 2: Authentication (Week 2)

**Goal:** Extract auth tokens WITHOUT running the official app.

### Strategy

The official app stores session tokens in two places:
1. **WebView2 "Cookies" file** (20 KB) — sessions for music.apple.com
2. **WebView2 "Login Data"** (43 KB) — saved credentials
3. **`mspr.hds`** (1.5 MB) — Apple proprietary session storage
4. **Windows Credential Manager** — may have `x-dsid`/`x-token`

### Steps

```
2.1 Extract cookies from WebView2 Cookies file
    → Cookies is a SQLite database
    → Read `cookies` table for music.apple.com domain
    → Extract: session cookies, auth tokens
    → Verify against our Fiddler-captured headers

2.2 If cookies are expired, use Login Data
    → Login Data is also SQLite
    → Read saved credentials for music.apple.com
    → Re-authenticate via music.apple.com login API

2.3 If both fail, call AuthKitWin.dll
    → FFI to AKValidateAppleIDCredentials
    → This shows Apple's sign-in dialog natively
    → On success, get x-dsid, x-token, authorization headers
    → Output: All auth tokens in hand

2.4 Embed a music.apple.com WebView2 for token refresh
    → Hidden WebView2 navigates to music.apple.com
    → User signs in once
    → Intercept cookies → extract tokens
    → Store tokens for future sessions
```

**Key discovery:** The `Cookies` file at:
```
%LOCALAPPDATA%\Packages\AppleInc.AppleMusicWin_nzyj5cx40ttqa\
  LocalState\EBWebView\Default\Network\Cookies
```
is a **SQLite database** with Apple Music session tokens. We can read it from Rust using `rusqlite` (already in our Cargo.toml).

**RAM target after Phase 2:** ~75 MB ✅

---

## Phase 3: HLS Streaming + FairPlay (Week 2-3)

**Goal:** Request and decrypt an encrypted HLS stream.

### Steps

```
3.1 Get HLS playlist URL for a song
    → Call Apple Music API with our tokens
    → GET /v1/catalog/{storefront}/songs/{id}
    → Response includes HLS playlist URL in the `previews` or `hls` field
    → NOTE: Full song HLS is different from 30-second preview URLs

3.2 Download and parse HLS playlist
    → Parse m3u8 with SAMPLE-AES #EXT-X-KEY tag
    → Extract: key URL (skd:// scheme), IV, segment URLs
    → Segments are fMP4 with encrypted moof/mdat boxes

3.3 Request content key via CoreMedia
    → Call FigContentKeyBoss to handle skd:// key request
    → CoreMedia internally calls CoreKE.dll for key exchange
    → CoreKE talks to Apple's key server over HTTPS
    → Returns encrypted content key

3.4 Decrypt content key via CoreFP
    → CoreFP.dll decrypts the content key using hardware-bound keys
    → Provides clear AES-128 key to CoreMedia

3.5 Decrypt HLS segments
    → For each segment:
      → Read encrypted fMP4
      → Call FigSampleBufferSetDecryptor with key + IV + encrypted data
      → Get decrypted CMSampleBuffer
    → Output: Clear audio data

3.6 Decode ALAC/AAC from decrypted samples
    → Call CoreAudioToolbox's ACAppleLosslessDecoderEntry
    → Get PCM audio
    → Feed to rodio for playback
```

**NOTE:** This is the hardest phase. CoreFP.dll (37 MB) contains the actual FairPlay decryption logic. We can't replicate it — we call IT via FFI. The obfuscated exports (like `WIn9UJ86JKdV4dM`) are entry points. We know their addresses from our earlier analysis:

| Export | Address | DLL |
|--------|---------|-----|
| `WIn9UJ86JKdV4dM` | `0x7e321100` | CoreFP.dll |
| `X46O5IeS` | `0x7e30a480` | CoreFP.dll |
| `YlCJ3lg` | `0x7e30a2d0` | CoreFP.dll |
| `IWvA5fm4IwMS88oQXDANCS` | `0x1b808ae0` | CoreKE.dll |
| `JY0DfiUyFiVcNw89R4t` | `0x1b803430` | CoreKE.dll |

**RAM target after Phase 3:** ~80 MB ✅

---

## Phase 4: Playback Pipeline (Week 3)

**Goal:** End-to-end audio playback in our UI.

### Steps

```
4.1 Wire decrypted PCM to rodio output
    → Already have rodio in deps
    → Feed PCM buffers to rodio's Sink

4.2 Implement playback controls
    → Play/pause, seek, volume
    → Already have Tauri commands wired

4.3 Implement queue
    → Up next, history, shuffle, repeat

4.4 Integrate with our UI
    → When user clicks play on a track:
      1. Get HLS URL from Apple Music API
      2. Parse segments
      3. Get key via CoreMedia/CoreFP/CoreKE
      4. Decrypt segments
      5. Decode ALAC/AAC
      6. Play through rodio
    → UI shows: title, artist, album art, progress, time

4.5 Handle Dolby Atmos
    → Audio traits include "atmos"
    → CoreAudioToolbox has Dolby Atmos decoding
    → Route to appropriate audio channels
```

**RAM target after Phase 4:** ~80-85 MB ✅

---

## Phase 5: Polish (Week 4)

**Goal:** Full Apple Music feature set under 85 MB.

### Steps

```
5.1 Catalog search (already partially wired)
    → Our REST API client works from the Fiddler capture
    → Needs auth tokens from Phase 2

5.2 Library sync (iCloud)
    → API: /v1/me/library/songs
    → API: /v1/me/library/albums
    → API: /v1/me/library/playlists

5.3 Recommendations / For You
    → API: /v1/me/recommendations

5.4 Lyrics
    → TTML parser (decoded from AMP Services/readme.txt)
    → Word-by-word sync via timing engine

5.5 Local file playback
    → symphonia for your own FLAC/ALAC/MP3 files
    → No DRM needed, just decode and play
    → Bonus: under 40 MB when not streaming
```

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│            Our Rust App (single process)                  │
│                                                           │
│  ┌─────────────────┐   ┌──────────────────────────────┐  │
│  │  Tauri UI        │   │  Rust Audio Pipeline         │  │
│  │  (WebView2)      │   │                              │  │
│  │                  │   │  1. Get HLS URL from API     │  │
│  │  Beautiful       │   │  2. Parse m3u8 segments      │  │
│  │  Apple Music UI  │   │  3. Load Apple DLLs via FFI  │  │
│  │                  │   │  4. CoreFP/CoreKE → key      │  │
│  │  invoke() commands│ │  5. CoreMedia → decrypt      │  │
│  │                  │   │  6. CoreAudioToolbox → PCM    │  │
│  └─────────────────┘   │  7. rodio → speakers          │  │
│         │              └──────────────────────────────┘  │
│         │ invoke()                                       │
│         ▼                                                │
│  ┌──────────────────────────────────────────────────┐    │
│  │           Rust Backend (lib.rs)                    │    │
│  │                                                    │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐           │    │
│  │  │ Library  │ │ Auth     │ │ API      │           │    │
│  │  │ Module   │ │ Module   │ │ Client   │           │    │
│  │  │ (SQLite) │ │(Cookies) │ │(reqwest) │           │    │
│  │  └──────────┘ └──────────┘ └──────────┘           │    │
│  │                                                    │    │
│  │  ┌──────────────────────────────────────────────┐  │    │
│  │  │   DRM FFI Layer                                │  │    │
│  │  │  ├── coreaudio.rs → CoreAudioToolbox.dll      │  │    │
│  │  │  ├── coremedia.rs  → CoreMedia.dll            │  │    │
│  │  │  ├── corefp.rs     → CoreFP.dll               │  │    │
│  │  │  └── authkit.rs    → AuthKitWin.dll           │  │    │
│  │  └──────────────────────────────────────────────┘  │    │
│  └──────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘

Apple DLLs loaded into our process memory:
┌────────────────────────────────────────────┐
│ CoreFP.dll      (37 MB)  FairPlay Core     │
│ CoreKE.dll      (45 MB)  Key Exchange      │
│ CoreLSKD.dll    (17 MB)  License SDK       │
│ CoreMedia.dll   (23 MB)  Media Pipeline    │
│ CoreAudioToolbox.dll (8.9 MB) Codecs       │
│ AuthKitWin.dll  (1 MB)   Auth              │
└────────────────────────────────────────────┘
Total DLL load: ~132 MB on disk, ~30 MB in memory (paged)
```

## Memory Budget

| Component | RAM |
|-----------|-----|
| Our Rust app (code + data) | ~15 MB |
| WebView2 (UI) | ~25 MB |
| Apple DLLs (loaded, paged) | ~30 MB |
| HLS segment buffers | ~5 MB |
| Audio output buffer | ~2 MB |
| Album art cache | ~3 MB |
| **Total** | **~80 MB** ✅ |

## Key Risks

| Risk | Mitigation |
|------|------------|
| CoreFP exports are obfuscated | We know the addresses — call by ordinal, not name |
| CoreFP might check process identity | Might only work when called from AppleMusic.exe |
| Apple might revoke embedded dev token | Extract from binary before it changes |
| Auth tokens expire | Hidden WebView2 for re-auth |
| Apple updates DLLs | Our DLLs come from disk, version-locked |
| Legal concerns | We're using Apple's DLLs on Apple's OS, paying for the service — personal use, not distributing |

## Success Criteria

- [ ] Phase 1: `LoadLibrary` succeeds on all Apple DLLs, functions callable
- [ ] Phase 2: Auth tokens extracted, API calls return 200 with user data
- [ ] Phase 3: HLS segment decrypted, ALAC decoded to PCM
- [ ] Phase 4: Audio plays through speakers, controls work
- [ ] Phase 5: Full feature set under 85 MB total RAM
