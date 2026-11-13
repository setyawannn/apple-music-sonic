# Bug 01 — Play click felt 5–15 s delayed

> **Status: FIXED ✅** — the original timers are gone and the follow-on 30 s hang is fixed too.
> Opened: 2026-11-10 · Closed: 2026-11-13
> See `03-PLAYER-STATE-CHANNEL.md` for the channel that replaced the retry loop.

---

## What users saw

Click a song → audio starts → the playback bar stays empty (or stale) for 5–15 s → then
artwork, title, and time snap into place.

## Cause — three stacked timers

| # | Cause | Where |
|---|-------|-------|
| 1 | `player_command` retried 6 × with a **mandatory 1200 ms sleep after every attempt**, including successful ones → 7.2 s floor | old `lib.rs` retry loop |
| 2 | First click had to **build the hidden WebView2 and load music.apple.com** (2–5 s) | old `ensure_player` called only on first play |
| 3 | A 300 ms × 200 poll waited up to **60 s** for MusicKit to exist before commands could run | old bootstrap |

Everything compounded, and the UI had no optimistic update, so "audio plays but the UI shows
nothing" was the literal truth.

## Fixes

| Fix | Implementation | Verified |
|-----|----------------|----------|
| Retry loop deleted | Page-side queue (`window.__mkQueue`) + `__mkDrain`; a command issued before MusicKit loads is parked and runs when ready | ✅ code |
| Readiness wait deleted | `player_command` dispatches and returns `Ok(())`; no 30 s timeout, no `Err` | ✅ runtime log |
| Pre-warm the player | `setup()` calls `ensure_player()` while the user is still browsing | ✅ code |
| Optimistic UI | `app.js` writes title/artist/artwork and sets streaming state **before** awaiting the command | ✅ code |
| Repeat clicks | per-click nonce instead of a 10 s same-song lock | ✅ code |
| Truthful state | cookie channel (`mk_state`) → `start_state_poller` → `player-state` | ✅ runtime log |

Measured after the fix: `stream_play` returns in milliseconds and `[PLAYER] state {...}` shows
the player's own position advancing 1 s per second — no timeout, no icon reset.

## Why this stayed open for a while

Deleting the retry loop was not enough: the replacement handshake (`PLAYER_READY` + `/ready`
POST + 30 s timeout) could never be satisfied, because music.apple.com's CSP blocks loopback
`fetch()`. So the visible symptom changed from a 5–15 s delay to a 30 s hang plus a UI reset.
Both are now gone — see `03-PLAYER-STATE-CHANNEL.md`.

## Lesson

The hidden player needs **no** readiness handshake. The page queue already guarantees ordering.
Any future "wait for ready" logic is a regression.
