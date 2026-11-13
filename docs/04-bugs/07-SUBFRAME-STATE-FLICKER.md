# Bug 07 — Now-playing flicker: the bootstrap ran in every frame

> Status: **fixed** (2026-11-13)
> This is the **real** cause of the playback flicker. Bug 06 fixed genuine repaint
> problems (a timer and an unguarded artwork rebuild) but not this oscillation.

## Symptom

While a song played, the now-playing title/artist/artwork and the highlighted track row
flipped between correct and empty, several times a second.

## How it was found

The Rust state poller logs every `mk_state` it accepts. During playback the log showed the
**same player publishing two different states in alternation**, roughly every 500 ms:

```
FULL pos=44 playing=true
EMPTY
FULL pos=44 playing=true
EMPTY
FULL pos=45 playing=true
EMPTY
```

The `EMPTY` state was `{"isPlaying":false,"position":0,"duration":0,"id":null,…,"queueLength":0}`.
Crucially it carried the **same `volume`** as the real state — so it was not a second window
with its own defaults; it was a second MusicKit instance that shared the persisted volume.

## Root cause

`PLAYER_BOOTSTRAP` is injected with Tauri's `initialization_script`. wry 0.57.1's own docs say:

> **Windows:** scripts are always added to subframes regardless of the `for_main_frame_only`
> option.

So the bootstrap ran in the **main frame and every subframe** of `music.apple.com`. Each
subframe's `MusicKit.getInstance()` is a separate instance with **no queue** — but MusicKit
persists volume in `localStorage`, which frames share, which is why the empty state's volume
matched. Every subframe therefore published an empty `mk_state` on the same cookie, taking turns
with the main frame's real state. The poller faithfully relayed both, and the UI flipped.

## Evidence

- A temporary diagnostic in the guard wrote a `mk_subframe` cookie from any non-top frame; the
  poller logged it once — `[PLAYER] subframe bootstrap detected (guarded)` — proving subframes
  execute the script.
- After the guard, the accepted-state log went from alternating `EMPTY`/`FULL` to a long stable
  run of `FULL` at advancing positions (with a single intentional `EMPTY` at a track change).
- Occasional empties remained. A temporary `top` field showed they came from `top:true` — the
  player frame itself, not a subframe — and only one player window existed
  (`[DIAG] windows=["main","player"]`).

## Fix

### Layer 1 — the subframe guard

One line at the very top of `PLAYER_BOOTSTRAP`, before anything else runs:

```js
if (window.top !== window.self) return;   // only the top document drives playback
```

The main frame still defines `__mkReady`, `__mkQueue`, `__mkDrain`, and `publish`; `player_command`
evals into the top frame, so commands are unaffected. Subframes now do nothing.

### Layer 2 — debounce transient empties

Even the top frame occasionally reports a phantom empty state (MusicKit momentarily returns no
`nowPlayingItem`, or a reset-looking instance with `shuffle:0, queue:0`) while a real song is
loaded. Applying that verbatim still blinks the bar. So the frontend **debounces null-id
reports**: when a real track is loaded or a load is pending, an empty report only clears the UI
after 1.5 s with no real report in between. A single blip is dropped; a genuine stop still clears.

## Lesson

On Windows, a Tauri `initialization_script` runs in **all frames** — the `for_main_frame_only`
flag is not honoured. Any script that assumes "one document" (singletons, cookies, global state)
must first check `window.top === window.self`. And any UI mirroring an external state feed should
treat a momentary empty as a blip, not as truth.
