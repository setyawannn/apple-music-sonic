# Audio quality ceiling — why this app streams AAC 256, not lossless

> Status: **measured, not assumed** (2026-11-14)
> Short answer: Apple *does* give us the lossless stream, but WebView2 has no ALAC decoder,
> so the app cannot play it. The ceiling is AAC 256 kbps on this architecture.

## What Apple actually hands us

A `GET /v1/catalog/{storefront}/songs/{id}?extend=extendedAssetUrls` with the app's own
developer token + `Media-User-Token` returns five asset URLs. `enhancedHls` is the interesting
one: fetching it returned a master playlist with **four audio renditions**, including real ALAC:

```
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio-stereo-256",NAME="songEnhanced"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio-alac-stereo-44100-24",NAME="songEnhanced",SAMPLE-RATE=44100,BIT-DEPTH=24
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio-stereo-128",NAME="songEnhanced"
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio-HE-stereo-64",NAME="songEnhanced"
#EXT-X-STREAM-INF:AVERAGE-BANDWIDTH=258613,CODECS="mp4a.40.2",AUDIO="audio-stereo-256"
#EXT-X-STREAM-INF:AVERAGE-BANDWIDTH=1511922,CODECS="alac",AUDIO="audio-alac-stereo-44100-24"
```

So a lossless track ("I Wanna Be Yours", `audioTraits: ["lossless","lossy-stereo"]`) offers
**ALAC 24-bit / 44.1 kHz at ~1.6 Mbps** next to AAC 256 at ~260 kbps. The master playlist also
carries three key systems per rendition — Widevine, FairPlay, **and PlayReady**:

```
#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,KEYFORMAT="urn:uuid:edef8ba9-…"     (Widevine)
#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,KEYFORMAT="com.apple.streamingkeydelivery"
#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,KEYFORMAT="com.microsoft.playready"
```

That is why this looked reachable: the ALAC rendition exists and Windows has a PlayReady CDM.

## Why it still doesn't play

Probed inside the running hidden player (a real WebView2 at `music.apple.com`), results logged
as `[PROBE]`:

| Check | Result | Meaning |
|---|---|---|
| `MediaSource.isTypeSupported('audio/mp4; codecs="alac"')` | **false** | Chromium's MP4/MSE pipeline has no ALAC decoder |
| `MediaSource.isTypeSupported('audio/mp4; codecs="mp4a.40.2"')` | true | AAC-LC is fine |
| `MediaSource.isTypeSupported('audio/mp4; codecs="flac"')` | true | FLAC is fine — ALAC is not |
| `requestMediaKeySystemAccess('com.microsoft.playready', alac)` | **NotSupportedError** | the CDM also refuses an ALAC content type |
| `requestMediaKeySystemAccess('com.microsoft.playready', aac)` | ok | PlayReady itself works |
| `requestMediaKeySystemAccess('com.widevine.alpha', aac)` | ok | so does Widevine |
| `requestMediaKeySystemAccess('com.apple.fps', aac)` | NotSupportedError | no FairPlay in WebView2 |

**Both ends refuse ALAC**: no decoder, and the EME stack rejects the content type before a key
session can even open. There is no flag, no `codecs` spelling, and no MusicKit setting that
changes this — MusicKit JS's only bitrate constant is `HIGH` (256 kbps).

And no, you cannot "just decrypt it yourself": EME never exposes keys or PCM to JavaScript.
Any ALAC pipeline would have to sit *below* the CDM, i.e. outside the browser.

## What lossless would actually cost

| Route | Verdict |
|---|---|
| MusicKit JS / WebView2 | **Impossible.** Measured above. |
| In-page WASM ALAC decoder + `enhancedHls` | **Impossible.** The bytes are FairPlay/PlayReady-encrypted; WASM would receive ciphertext. |
| Native Windows media stack using `enhancedHls` | **Plausible but large.** HLS demux + `IMFContentDecryptionModule` (PlayReady CENC) + ALAC decode + a media sink. Weeks of work, replaces the whole player, and re-opens the parked `drm/**` code that was abandoned for good reason. |
| Apple's own Apple Music app for Windows | **Works today, free.** Apple decodes ALAC with its own CDM. |

## What this project does instead

It plays the `audio-stereo-256` rendition through MusicKit — the same 256 kbps AAC the Apple
Music web player serves — and spends its effort on the parts it *can* own: a real Apple Music
UI, iCloud library, two-way playlist editing, a truthful transport, queue and Up Next.

That is the trade. If lossless is the requirement, use Apple's Windows app; if the app
experience is the requirement, this is the ceiling and it is a deliberate one.

## If this is revisited

Do not start from "MusicKit might expose lossless" — it does not. Start by proving
`MediaSource.isTypeSupported('audio/mp4; codecs="alac"')` in whatever host you plan to use.
If that is `false` (today it is), the only remaining path is a native CDM client, and the
`enhancedHls` URLs above are the input it would need.
