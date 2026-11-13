# Bug 05 — Playlists never appeared (`include=tracks` → HTTP 400)

> Status: **fixed** (2026-11-13)
> Symptom: signed in, but the Playlists tab and sidebar were empty.

## Symptom

After signing in, the Playlists view showed the empty state and the sidebar showed no
playlists — even though the account clearly had playlists. Nothing told the user *why*: the
failure was caught in `app.js` and only `console.warn`-ed, which never reaches the Rust log.

## Root cause

`ApiClient::get_user_playlists()` requested playlists with an `include=tracks` query
parameter:

```
GET /v1/me/library/playlists?limit=100&l=en-GB&include=tracks
```

Apple rejects that combination for **library** playlists with **HTTP 400 Bad Request** and an
empty body. Because the command returned `Err`, the frontend kept `state.cloudPlaylists` empty
and rendered nothing.

Measured against a live session:

| Request | Result |
|---------|--------|
| `…/playlists?limit=100&l=en-GB&include=tracks` | **400** |
| `…/playlists?l=en-GB&include=tracks` | **400** |
| `…/playlists?limit=5&l=en-GB&include=tracks` | **400** |
| `…/playlists?limit=100&l=en-GB&include=trackCount` | 200 |
| `…/playlists?limit=100&l=en-GB&extend=tracks` | 200 |
| `…/playlists?limit=100&l=en-GB` | **200, 11 playlists** |

So `include=tracks` is simply not valid here, and neither `include=trackCount` nor
`extend=tracks` adds a usable `trackCount` (it comes back absent).

## Fix

1. **Drop `include=tracks`** from the list URL in `api/client.rs`. Tracks are already fetched
   on demand by `get_playlist_tracks` when a playlist is opened, so the list never needed them.
2. Since the list carries no `track_count`, the card only shows a song count when one is known,
   instead of printing a misleading "0 songs".
3. Surface sync failures in the UI (a toast) instead of only `console.warn`, so the next
   failure is visible.
4. **Auto-sync**: the manual Sync button was removed. The playlist list refreshes on sign-in,
   on opening the Playlists view, and on window focus; the open playlist's tracks re-check on
   focus and after you add a song. (An earlier version polled every 30 s; that repaint flickered
   playback and was removed — see `06-AUTOREFRESH-FLICKER.md`.)

## Evidence

Live log line after the fix, from the app itself (auto-sync on launch):

```
[PLAYLIST] loaded 11 playlists
```

`get_cloud_playlists` now logs its outcome (`[PLAYLIST] loaded N` / `[PLAYLIST] error: …`) so
this can never fail silently again.

## Lesson

The library-playlist write/read endpoints have sharp edges that are only visible against a
live session. A read-only probe with the *exact* production URL is what caught this; the same
discipline is recorded in `docs/02-architecture/HOW-IT-WORKS.md` (Playlists section) and
`AGENTS.md`.
