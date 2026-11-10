# Apple Music API — Captured Endpoints & Data Contracts

> Captured live from Apple Music Windows app v1.1792.39682.0 via Fiddler Everywhere
> Date: 2026-11-09

---

## 1. Base URLs

| Environment | URL | Notes |
|------------|-----|-------|
| Primary API | `https://amp-api.music.apple.com` | Main API, catalog, library, recommendations |
| Edge API | `https://amp-api-edge.music.apple.com` | Search suggestions, faster CDN |
| Album Art | `https://is1-ssl.mzstatic.com` | Image CDN |
| Audio Preview | `https://audio-ssl.itunes.apple.com` | 30-second AAC previews (`.plus.aac.ep.m4a`) |
| Telemetry | `https://xp.apple.com/report/2/xp_its_music_main` | Usage analytics |

## 2. Authentication Headers

Every request to `amp-api.music.apple.com` includes these headers:

| Header | Example Value | Description |
|--------|--------------|-------------|
| `authorization` | `[REDACTED]` | Bearer token (MusicKit JWT) |
| `x-dsid` | `19032862749` | Directory Services ID (user identifier) |
| `x-token` | `AwIAAAECAAH70AAAAABq8XQ0...` | Media token (base64, session auth) |
| `x-apple-cuid` | `4109634f74d27a187b932e845d4e3cd3` | Client unique ID |
| `x-apple-i-md` | `AAAABQAAABCdiXxI+Xa3n2EaEnyvCsmwAAAAAg==` | Media device ID (base64) |
| `x-apple-i-md-m` | `67RJZy6isCqDcLTALc6h...` | Media device master key |
| `x-apple-i-md-rinfo` | `84215040` | Media device rotation info |
| `x-apple-store-front` | `143476-2,42 t:musicstore1` | Storefront (143476 = Indonesia, 2 = en) |
| `x-apple-tz` | `25200` | Timezone offset in seconds |
| `user-agent` | `Music/1.7 (Windows 10.0.26300 x64; x64) Chromium/157.0.4341.42 build/155 (dt:2)` | Desktop app identifier |
| `accept-language` | `en-us` | Language |
| `cookie` | `[REDACTED]` | Session cookies |

## 3. API Endpoints Captured

### 3.1 Social Profile
```
GET /v1/me/social/profile
  ?extend=followState
  &format[resources]=map
  &include=followers,pending-followers,social-profile
  &l=en-GB
  &platform=desktop
  &with=nonOnboarded
```
**Response**: `personal-social-profiles`, `social-profiles` resources with handle, name, URL, follow state

### 3.2 Catalog Search Suggestions (Live Search)
```
GET /v1/catalog/{storefront}/search/suggestions
  ?omit[resource]=autos
  &kinds=terms,topResults
  &with=naturalLanguage
  &limit=5
  &limit[results:topResults]=10
  &format[resources]=map
  &term={query}
  &platform=desktop
  &types=activities,albums,apple-curators,artists,curators,editorial-items,music-movies,music-videos,playlists,songs,stations,tv-episodes,tv-shows,uploaded-videos,record-labels
  &l=en-GB
```
**Response**: `suggestions` array with `terms` (search suggestions) and `topResults` (content links)

### 3.3 Catalog Search (Full)
```
GET /v1/catalog/{storefront}/search
  ?art[url]=f
  &extend=artistUrl
  &fields[albums]=artistName,artistUrl,artwork,contentRating,editorialArtwork,editorialNotes,name,playParams,releaseDate,trackCount,url
  &format[resources]=map
  &l=en-GB
  &limit=25
  &omit[resource]=autos
  &platform=desktop
  &term={query}
  &types=albums,apple-curators,artists,curators,editorial-items,music-videos,playlists,songs,stations
```
**Types available**: `activities`, `albums`, `apple-curators`, `artists`, `curators`, `editorial-items`, `music-movies`, `music-videos`, `playlists`, `songs`, `stations`, `tv-episodes`, `tv-shows`, `uploaded-videos`, `record-labels`

### 3.4 Album Detail
```
GET /v1/catalog/{storefront}/albums/{id}
  ?art[url]=f
  &extend=editorialArtwork,editorialVideo,extendedAssetUrls,offers
  &fields[artists]=name,url
  &fields[curators]=name
  &fields[record-labels]=name,url
  &format[resources]=map
  &include=record-labels,artists
  &include[music-videos]={includeId}
  &include[playlists]=curator
  &include[songs]=artists,composers,albums
  &l=en-GB
  &meta[albums:tracks]=popularity
  &platform=desktop
  &views=appears-on,audio-extras,more-by-artist,other-versions,related-videos,video-extras,you-might-also-like
```

### 3.5 Song Recommendations (Playlist)
```
POST /v1/me/recommendations/suggested
  ?meta=source
  &limit=20
  &format[resources]=map
  &types=songs
  &contexts=playlist-suggested-songs

Body: {
  "offered": {
    "suggested": [
      {"id": "{songId}", "type": "songs", "meta": {"impressed": true}},
      ...
    ]
  },
  "targetContent": {
    "type": "library-playlists",
    "id": "{playlistId}"
  }
}
```

## 4. Resource Data Models

### Song Resource
```json
{
  "id": "1828560357",
  "type": "songs",
  "attributes": {
    "albumArtistName": "LANY",
    "albumName": "Soft",
    "artistName": "LANY",
    "artwork": {
      "bgColor": "000003",
      "hasP3": false,
      "height": 1400,
      "textColor1": "d8c9c0",
      "textColor2": "76c3e7",
      "textColor3": "ada19a",
      "textColor4": "5e9cb9",
      "url": "https://is1-ssl.mzstatic.com/image/thumb/.../{w}x{h}bb.jpg",
      "width": 1400
    },
    "audioLocale": "en-US",
    "audioTraits": ["atmos", "lossless", "lossy-stereo", "spatial"],
    "composerName": "Paul Jason Klein, Steph Jones & David Hodges",
    "contentRating": "explicit",   // optional
    "discNumber": 1,
    "durationInMillis": 178629,
    "genreNames": ["Pop", "Music"],
    "hasLyrics": true,
    "hasTimeSyncedLyrics": true,
    "isAppleDigitalMaster": false,
    "isMasteredForItunes": false,
    "isVocalAttenuationAllowed": true,
    "isrc": "USA2P2536900",
    "name": "Soft",
    "playParams": {
      "id": "{catalogId}",
      "kind": "song"
    },
    "previews": [
      {
        "url": "https://audio-ssl.itunes.apple.com/...mzaf_2103939372791718282.plus.aac.ep.m4a"
      }
    ],
    "releaseDate": "2025-05-30",
    "trackNumber": 1,
    "url": "https://music.apple.com/id/album/soft/1828560356?i=1828560357"
  }
}
```

### Audio Traits (Quality Levels)
| Trait | Meaning | Bitrate/Sample Rate |
|-------|---------|-------------------|
| `lossy-stereo` | AAC (Standard) | 256 kbps |
| `lossless` | ALAC Lossless | Up to 48 kHz |
| `hi-res-lossless` | Hi-Res Lossless | Up to 192 kHz / 24-bit |
| `atmos` | Dolby Atmos | Spatial audio |
| `spatial` | Spatial Audio | Head tracking capable |

### Album Resource
```json
{
  "id": "1435482172",
  "type": "albums",
  "attributes": {
    "artistName": "LANY",
    "artwork": { "...same format..." },
    "audioTraits": ["atmos", "lossless", "lossy-stereo", "spatial"],
    "contentRating": "explicit",
    "copyright": "℗ 2024...",
    "editorialArtwork": { "...various artwork crops..." },
    "editorialNotes": { "short": "...", "standard": "..." },
    "genreNames": ["Pop", "Music"],
    "isCompilation": false,
    "isComplete": true,
    "isMasteredForItunes": false,
    "isPrerelease": false,
    "isSingle": false,
    "name": "Malibu Nights",
    "offers": [{ "type": "buy", "price": 9.99 }],
    "playParams": { "id": "{catalogId}", "kind": "album" },
    "recordLabel": "Interscope Records",
    "releaseDate": "2018-10-05",
    "trackCount": 10,
    "upc": "0602577948405",
    "url": "https://music.apple.com/id/album/malibu-nights/1435482172"
  }
}
```

## 5. Streaming URLs (Audio Previews)

Preview URLs follow this pattern:
```
https://audio-ssl.itunes.apple.com/itunes-assets/AudioPreview{version}/v4/{hash}/mzaf_{id}.plus.aac.ep.m4a
```

Full streaming (lossless) goes through `CoreMedia.dll` + `CoreAudioToolbox.dll` using HLS with FairPlay DRM. The URLs are fetched internally by Apple's native networking stack.

## 6. Storefront Codes

The app uses `143476-2` which means:
- `143476` = Indonesia store
- `2` = English language variant
- Common storefronts: `143441` (US), `143444` (UK), `143476` (ID)

## 7. Key Observations for Rust Implementation

1. **All catalog/library API calls** go through `amp-api.music.apple.com`
2. **All search suggestions** go through `amp-api-edge.music.apple.com` (faster edge)
3. **Authentication** requires: `authorization` (Bearer JWT), `x-dsid`, `x-token`, `x-apple-store-front`
4. **Response format** is always `format[resources]=map` — resources keyed by ID
5. **Images** use `{w}x{h}bb.jpg` template in the URL (e.g., `300x300bb.jpg`)
6. **Audio previews** are AAC files (30s clips)
7. **Telemetry** goes to `xp.apple.com` (can be ignored)
8. **No streaming URLs visible** — streaming is handled natively by `CoreMedia.dll` with HLS/FairPlay
