# Apple Music RS — What's Missing (MVP gaps)

> Last updated: 2026-11-15 (P0–P3 shipped)

> Scope: only what blocks "clone Apple Music". Local file playback is parked, not listed.

**P0, P1, P2 and P3 are done.** What is left is Radio (needs Apple's stations API), library
album grouping (needs a collection id), and playlist track removal (no working endpoint).
Details: `03-TO-DO.md`, `01-WHATS-DONE.md`, and `docs/04-bugs/`.

---

## Gap 1 — Radio has no backend

`renderRadio` is an honest placeholder. Live stations need Apple's stations API
(`/v1/catalog/{storefront}/stations` plus station playback params), which is not implemented.

**Fix.** Add a `get_stations` command and queue a station id through `stream_play`, or keep the
placeholder explicit.

## Gap 2 — Removing a track from a playlist

Create/rename/delete playlist and add-tracks all work. **Removing** a track does not: Apple
returns 404 for `DELETE …/tracks/{id}` and for the `relationships/tracks` form, on both
`amp-api` and `api` hosts (measured). There is no UI for it.

**Fix.** Find the endpoint Apple's own web client uses (likely a different header set), or
accept it permanently and say so in the UI.

## Gap 3 — Library album grouping is name-based

`deriveLibraryAlbums` keys on `artist + album name` because `StoreSong` carries no collection
id. Two same-named albums by one artist would merge.

**Fix.** Extend the library song parse to keep the collection id/name from Apple's response
and group on that.

## Gap 4 — Audio ceiling (won't fix on this architecture)

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
| Keyboard (P3) | Mouse only | Space / arrows / `M` + hardware media keys |
| Media keys + flyout (P3) | OS media keys did nothing; the Windows flyout showed nothing | SMTC bound to the main window (`player/media.rs`); buttons map to `player_command`, flyout mirrors `player-state` |
| Now Playing (P3) | No full-screen view | `#now-playing-view` painted from `state.stream*`; opens from the bar artwork/title, `Esc` closes |
| Queue drawer (P3) | Queue length shown, contents hidden | `mk_queue` cookie (base64) → `player-queue`; Up Next drawer with click-to-jump (verified live: `items=5`) |
| Playlists (P2.6) | Read-only cards; "New Playlist" wrote to the parked local DB | Library playlists are the real ones; create/rename/delete/add-track write to Apple Music (measured live), local playlist UI removed |

## Out of scope (deliberately)

Downloads/offline · lyrics · Dolby Atmos · equalizer · Discord presence · tray controls ·
multi-account · queue drag-and-drop · Last.fm scrobbling.

## Parked, decision later

Local file playback — develop or delete. See `04-PARKED-LOCAL-PLAYBACK.md`. Not a gap.
