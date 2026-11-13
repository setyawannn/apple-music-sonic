# Apple Music RS — What's Missing (MVP gaps)

> Last updated: 2026-11-13 (after P1 + P2 shipped, P3 in progress)
> Scope: only what blocks "clone Apple Music". Local file playback is parked, not listed.

**P0, P1 and P2 are done**; **P3 is in progress** (transport state machine, error surface,
resume-on-launch and keyboard shortcuts are in). Details: `03-TO-DO.md`, `01-WHATS-DONE.md`,
and `docs/04-bugs/`.

---

## Gap 1 — Windows media keys (P3)

In-app keyboard shortcuts work (Space, arrows, `M`), but the hardware media keys do not:
they need a global shortcut plugin or a native hook, not a WebView2 key handler.

**Fix.** Add `tauri-plugin-global-shortcut` and bind MediaPlayPause / MediaNextTrack /
MediaPrevTrack, or register a Win32 hotkey.

## Gap 2 — Now Playing view (P3)

Apple's full-screen Now Playing is not built (the Up Next queue drawer **is**: see the closed
table). The transport bar shows the queue length and opens the drawer.

**Fix.** A Now Playing view driven by the existing `player-state` plus the `mk_queue` items.

## Gap 3 — Radio has no backend (P3)

`renderRadio` is an honest placeholder. Live stations need Apple's stations API
(`/v1/catalog/{storefront}/stations` plus station playback params), which is not implemented.

**Fix.** Add a `get_stations` command and queue a station id through `stream_play`, or keep the
placeholder explicit.

## Gap 4 — Library album grouping is name-based

`deriveLibraryAlbums` keys on `artist + album name` because `StoreSong` carries no collection
id. Two same-named albums by one artist would merge.

**Fix.** Extend the library song parse to keep the collection id/name from Apple's response
and group on that.

## Gap 5 — Audio ceiling (won't fix on this architecture)

AAC 256 kbps. Lossless needs either a native CDM integration or a different playback host.
Out of scope for the MVP; document it in the UI as a known limitation rather than pretending
otherwise.

---

## Recently closed

| Gap | Was | Now |
|-----|-----|-----|
| State channel | CSP killed loopback `fetch`; UI ran on a local clock; every command hung 30 s | Cookie channel `mk_state` → `start_state_poller` → `player-state`; commands return immediately |
| Seek | Progress bar drove the parked local engine and snapped to 0 % | Branches to `stream_seek`, re-anchors the smoothing clock |
| Sign-in vs player profile | Sign-in wiped the profile under a live player | Player closed first, re-warmed after success |
| Transport (P1) | next/prev/shuffle/repeat drove only the parked local engine | `stream_next/previous/shuffle/repeat/volume` → MusicKit; queue is the clicked list; state reported back |
| iCloud library (P2) | Library/Albums/Artists read the parked SQLite DB | `get_personal_library` → grouped client-side; badge shows iCloud song count |
| Album detail (P2) | Clicking an album card did nothing | `get_catalog_album` → header + Play/Shuffle queue + track list; iCloud albums get a derived detail view |
| Cloud playlists (P2) | Names only; tracks unreachable | Artwork cards open the track list; rows play through the queue |
| Welcome screen (P2) | "Scan My Music" local-scanner CTA | Sign-in CTA / Go to Listen Now |
| Loading/error surface (P3) | Failed songs flipped the icon silently | `streamPhase` state machine, play-button spinner, error toast, 12 s watchdog |
| Resume on launch (P3) | Only the session survived a restart | `player_state.json` + `player_queue.json`; bar restores paused, play rebuilds the queue |
| Keyboard (P3) | Mouse only | Space / arrows / `M` (media keys still open) |
| Queue drawer (P3) | Queue length shown, contents hidden | `mk_queue` cookie (base64) → `player-queue`; Up Next drawer with click-to-jump (verified live: `items=5`) |

## Out of scope (deliberately)

Downloads/offline · lyrics · Dolby Atmos · equalizer · Discord presence · tray controls ·
multi-account · queue drag-and-drop · Last.fm scrobbling.

## Parked, decision later

Local file playback — develop or delete. See `04-PARKED-LOCAL-PLAYBACK.md`. Not a gap.
