# Bug 04 — The progress bar seeked the wrong player

> **Status: FIXED & VERIFIED ✅** — 2026-11-13
> Fixed in: `src/js/app.js` (progress-bar click handler)
> Depends on: `03-PLAYER-STATE-CHANNEL.md` (now fixed, so duration is real)

---

## Symptom

While an Apple Music track streamed, clicking the progress bar moved the fill to the click
position and immediately snapped it back toward 0 %, while audio kept playing from wherever it
was. Nothing was seeked.

## Cause

The handler always targeted the **parked local engine**:

```js
const targetSecs = pct * (state.playback.duration || 0);   // local engine's duration
await invoke("seek_to", { positionSecs: targetSecs });     // local engine
```

`state.playback.duration` is 0 while streaming, so `targetSecs` was 0 and the bar collapsed to
the left edge. `seek_to` drove `audio::Engine`, which had nothing loaded. The streaming branch
of `syncPlaybackStatus` then redrew the bar from its own clock, overwriting the movement.

## Fix

The handler now branches on ownership of playback:

```js
if (state.streamingActive) {
  const total = state.streamDuration || 0;
  if (!(total > 0)) return;                 // duration unknown → ignore, don't snap to 0
  const target = pct * total;
  state.streamPosition = target;
  state.streamAnchor = Date.now() - target * 1000;   // keep the smoothing clock honest
  progressFill.style.width = `${pct * 100}%`;
  currentTime.textContent = formatTime(target);
  try { await invoke("stream_seek", { position: target }); }
  catch (err) { console.error("Stream seek failed:", err); }
  return;
}
// unchanged parked local-engine path below
```

`stream_seek` → `player_command("seek", …)` → `seekToTime()` in `PLAYER_BOOTSTRAP`, so no
backend change was needed. Re-anchoring matters: the bar is smoothed by
`Date.now() - streamAnchor` between reports, so without it the fill jumps back until the next
`player-state` arrives.

## Verified

Clicking mid-stream jumps the audio and the fill stays where it was placed; the next
`player-state` report (within 500 ms) confirms the same position.

## Acceptance criteria

1. Click at 50 % of a 4:00 track → audio jumps to ~2:00 and the bar stays at 50 %.
2. Pausing after a seek and resuming continues from the new position.
3. Clicking the bar with no active stream does nothing (no crash, no console spam).
