# Bug 04 — The progress bar seeks the wrong player

> **Status: OPEN — P0, small fix**
> Found: 2026-11-13 · Affects: seeking while streaming

---

## Symptom

While an Apple Music track is streaming, clicking anywhere on the progress bar **moves the
bar to the click position and immediately snaps it back toward 0 %**, while the audio keeps
playing from wherever it was. Nothing is seeked.

## Cause

`src/js/app.js:1205-1218`:

```js
progressBar?.addEventListener("click", async (e) => {
  const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
  progressFill.style.width = `${pct * 100}%`;
  const targetSecs = pct * (state.playback.duration || 0);   // ← parked local engine
  await invoke("seek_to", { positionSecs: targetSecs });     // ← parked local engine
  state.playback.position = targetSecs;
  currentTime.textContent = formatTime(targetSecs);
});
```

Two problems:

1. `state.playback.duration` is the **local engine's** duration, which is 0 while streaming.
   So `targetSecs = 0` and the bar collapses to the left edge.
2. `seek_to` targets `audio::Engine`, which has nothing loaded. The streamed player is never
   told anything.

The streaming branch of `syncPlaybackStatus` (`app.js:1247-1265`) then redraws the bar from
the local clock, which is why the movement is immediately overwritten.

## Fix

```js
progressBar?.addEventListener("click", async (e) => {
  const rect = progressBar.getBoundingClientRect();
  const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));

  if (state.streamingActive) {
    const total = state.streamDuration || 0;
    if (!(total > 0)) return;                       // unknown duration → ignore the click
    const target = pct * total;
    state.streamPosition = target;
    state.streamAnchor = Date.now() - target * 1000; // keep the local clock honest
    progressFill.style.width = `${pct * 100}%`;
    currentTime.textContent = formatTime(target);
    try {
      await invoke("stream_seek", { position: target });
    } catch (err) {
      console.error("Stream seek failed:", err);
    }
    return;
  }

  // parked local engine path (unchanged, dormant)
  const targetSecs = pct * (state.playback.duration || 0);
  ...
});
```

`stream_seek` already maps to `seekToTime()` in `PLAYER_BOOTSTRAP`, so no backend change is
needed.

## Dependency

Real duration comes from the player, so this fix is only *correct* after
`03-PLAYER-STATE-CHANNEL.md` lands. Until then, seeking will work against the catalog's
`duration_ms`, which is usually close but not authoritative — acceptable as a stopgap.

## Acceptance criteria

1. Click at 50 % of a 4:00 track → audio jumps to ~2:00 and the bar stays at 50 %.
2. Pausing after a seek and resuming continues from the new position.
3. Clicking the bar with no active stream does nothing (no crash, no console spam).
