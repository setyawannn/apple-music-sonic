# Apple Music RS — What's Missing

Only one thing stops the app from playing full lossless songs: **the FairPlay streaming engine.**

---

## The Missing Pipeline

Everything below exists ✅ but hasn't been wired into a single end-to-end flow:

```
1. Get HLS URL for a song
   → API endpoint: we call search_catalog, get song metadata
   → BUT: we never fetch the full-song HLS playlist URL
   → NEED: investigate which API parameter returns full HLS vs 30s preview
   → File: api/client.rs — needs a get_stream_url(song_id) method

2. Parse the HLS m3u8
   → PARSER EXISTS: stream.rs HlsParser
   → parse_master() — finds best quality variant
   → parse_media() — lists encrypted segments
   → BUT: never called for streaming

3. Request content key
   → CoreMedia bridge has FigContentKeyBoss function pointer
   → Takes an skd:// URL from the m3u8 #EXT-X-KEY tag
   → BUT: never called
   → NEED: a function that calls FigContentKeyBoss with the key URL

4. Decrypt content key
   → CoreFP bridge has all 6 obfuscated ordinals
   → CoreKE handles key exchange protocol
   → BUT: never called
   → NEED: a function that passes encrypted key through CoreFP ordinals

5. Decrypt HLS segments
   → FigSampleBufferSetDecryptor pointer exists in CoreMedia bridge
   → Takes: AES key, IV, encrypted data → clear CMSampleBuffer
   → BUT: never called
   → NEED: a function that downloads segment → decrypts → returns PCM

6. Decode ALAC/AAC
   → CoreAudio bridge has ACAppleLosslessDecoderEntry at ordinal 8
   → Takes: encoded data → PCM audio
   → BUT: never called
   → NEED: pass decrypted segment to the ALAC decoder

7. Play through rodio
   → Audio engine already accepts PCM and plays through rodio
   → WOULD WORK immediately once PCM arrives
```

## Why It's Hard

| Challenge | Impact |
|-----------|--------|
| FigContentKeyBoss parameter types unknown | We have the function pointer but don't know the exact C struct types |
| CoreFP ordinal signatures unknown | We know 6 exported addresses but not their calling conventions |
| Apple's key server requires valid device attestation | Might reject requests from non-Apple processes |
| API field for full HLS URL needs discovery | The 30s preview URL is in `previews[0].url` but full HLS URL may be elsewhere |

## What We Know

- The developer token works (scraped from music.apple.com, used for API calls)
- The user token works (imported from official app's cookies)
- The DLLs load and function pointers bind
- The HLS parser works for both master playlists and media segments
- The audio engine works with local files and could play decrypted streams

## Next Steps (When Ready to Build)

1. Add `get_stream_url(song_id)` to `api/client.rs` — try different API parameters
2. Create `streaming::decrypt_pipeline` module that:
   a. Downloads HLS master playlist
   b. Selects best variant
   c. Downloads media playlist
   d. For each segment: download → pass to FigSampleBufferSetDecryptor → pass to CoreAudio ALAC decoder → feed PCM to rodio
3. Connect to UI: modify `play_track` for catalog songs to use streaming pipeline instead of local file path
