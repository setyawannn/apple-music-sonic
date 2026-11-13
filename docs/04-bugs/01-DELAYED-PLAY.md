# Bug 01 — Play click felt 5–15 s delayed

> **Status: partially fixed. The original cause is gone; a different one replaced it.**
> Opened: 2026-11-10 · Updated: 2026-11-13
> Superseded in part by `03-PLAYER-STATE-CHANNEL.md`

---

## What users saw

Click a song → audio starts → the playback bar stays empty (or stale) for 5–15 s → then
artwork, title, and time snap into place.

## The original cause — three stacked timers

| # | Cause | Where |
|---|-------|-------|
| 1 | `player_command` retried 6 × with a **mandatory 1200 ms sleep after every attempt**, including successful ones → 7.2 s floor | old `lib.rs` retry loop |
| 2 | First click had to **build the hidden WebView2 and load music.apple.com** (2–5 s) | old `ensure_player` called only on first play |
| 3 | A 300 ms × 200 poll waited up to **60 s** for MusicKit to exist before commands could run | old bootstrap |

Everything above compounded, and the UI had no optimistic update, so “audio plays but the UI
shows nothing” was the literal truth.

## What was fixed

| Fix | Implementation | Verified |
|-----|----------------|----------|
| Retry loop deleted | Page-side queue (`window.__mkQueue`) + `__mkDrain`; a command issued before MusicKit loads is parked and runs when ready | ✅ code |
| Pre-warm the player | `setup()` calls `ensure_player()` while the user is still browsing | ✅ code |
| Transition-driven reports | listeners on `playbackStateDidChange`, `nowPlayingItemDidChange`, `playbackTimeDidChange`; report tick 250 ms | ⚠️ code exists, channel is dead — see below |
| Optimistic UI | `app.js` writes title/artist/artwork and sets streaming state **before** awaiting the command | ✅ code |
| Repeat clicks | per-click nonce instead of a 10 s same-song lock | ✅ code |

Result: the 7.2 s retry tax and the first-click page build are gone.

## What replaced it — the current delay

`player_command` dispatches the job, then:

```rust
if PLAYER_READY.load(Ordering::SeqCst) { return Ok(()); }
tokio::time::timeout(Duration::from_secs(30), player_ready_signal().notified()).await
```

`PLAYER_READY` is set only by `POST /ready` from the page — and that request **never arrives**
because music.apple.com's CSP blocks loopback `fetch()`. Confirmed across six dev sessions:
zero `/ready`, `/state`, or `/debug` requests.

Consequences:

1. `stream_play` hangs for 30 seconds and then returns `Err`.
2. The frontend `catch` resets `streamingActive` / `streamIsPlaying`, so the icon flips back
   to ▶ roughly 30 s after a click — even though audio is still playing.
3. There is no truthful progress or duration at all.

So the *original* 5–15 s delay is fixed, and the *observed* delay is now a 30-second hang plus
a UI reset. That is worse than what the fix removed, which is why this bug is not closed.

## Correct fix

`03-PLAYER-STATE-CHANNEL.md`, steps 1–2: drop the readiness wait, and report state through
`document.title` polled by Rust. No CSP, no timeout, no reset.

## Lesson

The hidden player needs **no** readiness handshake. The page queue already guarantees
ordering. Any future "wait for ready" logic is a regression.
