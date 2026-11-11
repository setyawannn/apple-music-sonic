# Bug: Play Click Has 5-15 Second Delay

## Symptoms
- User clicks a song to play
- Audio starts immediately (music plays, timer runs)
- UI shows nothing for 5-15 seconds
- Then playback bar, artwork, title, artist all snap to current state

## Root Cause

The delay is caused by **three stacked timers**:

### 1. `player_command` retry loop (up to 7.2s delay)
File: `src-tauri/src/lib.rs:410-420`
```rust
for attempt in 0..6 {
    // eval...
    tokio::time::sleep(Duration::from_millis(1200)).await;
}
```
**Bug 1a:** This ALWAYS waits 1200ms between iterations even if the first succeeds.
**Bug 1b:** 6 retries × 1200ms = 7.2 seconds minimum even when everything works.

### 2. Hidden WebView2 first load (2-5s)
File: `src-tauri/src/lib.rs:369-386`
```rust
let url = "https://music.apple.com/id/";
// Builds hidden WebView2, navigates to apple.com
```
On first play, the hidden WebView2 needs to:
- Create the window
- Navigate to music.apple.com/id/
- Wait for page + MusicKit JS bundle to load (2-5 seconds)

### 3. MusicKit ready poll (up to 60s)
File: `src-tauri/src/lib.rs:309-314`
```javascript
var tries = 0;
var t = setInterval(function () {
    tries += 1;
    if (got() || tries > 200) clearInterval(t);
}, 300);
```
Polls every 300ms up to 200 times (60 seconds) for MusicKit to be ready.
`window.__mkDo` just silently returns if not ready yet (the `if (!window.__mkDo) return;` path isn't in the bootstrap, but `__mkReady` promise doesn't resolve until MusicKit loads).

### The Audio Timing Mismatch
The `player-state` events POST to `127.0.0.1:38471/state` once per second:
```javascript
setInterval(report, 1000);
```
So even after playback starts, the UI can lag up to 1 second behind. And the `streamAnchor` calculation:
```javascript
state.streamAnchor = Date.now() - state.streamPosition * 1000;
```
Assume state arrives 8 seconds into the song, the position jumps immediately to 0:08.

## Why Audio Plays But UI Doesn't Update

The hidden WebView2 successfully calls `MusicKit.play()` which starts audio immediately through the WebView2 CDM. But the state reporting loopback (`report()` function via `fetch()`) doesn't fire for up to 1 second after playback starts, and the retry loop in `player_command` blocks the Rust side from processing incoming state events.

## Fix Plan

### Immediate (Reduces delay from ~10s to ~2s)
1. **Break early on success** — return from `player_command` immediately after first successful `eval()`, not after 6 retries
2. **Reduce retry interval** — change 1200ms to 200ms
3. **Reduce initial guesses** — 6 → 3 attempts

### Medium (Reduces delay from ~2s to ~500ms)
4. **Pre-load the WebView2 on app startup** — `ensure_player()` at setup, not on first play click
5. **Pre-warm MusicKit** — after page loads, wait for MusicKit to be ready before user clicks play

### Long-term
6. **Decrease `report` interval** — 1000ms → 250ms for smoother progress
7. **Send state immediately on play** — attach a callback to `musicdidplay` event
8. **Show "Loading..." in playback bar** — don't show stale "No Track Selected" while waiting
