# Bug 03 — The player state channel is dead, and every command hangs 30 s

> **Status: OPEN — P0, blocking all playback UI work**
> Found: 2026-11-13 · Affects: `stream_play`, `stream_pause`, `stream_resume`, `stream_seek`,
> the playback bar, the transfer of now-playing metadata

---

## Summary

`music.apple.com`'s Content-Security-Policy blocks the page's `fetch()` calls to
`http://127.0.0.1:38471`. The loopback listener therefore **never receives a single request**,
which means:

- `PLAYER_READY` is permanently `false`
- every `player_command` waits 30 s and then returns `Err`
- the frontend resets its optimistic playback state when that error lands
- position, duration, title, artist, and artwork from the player are unavailable

Audio still plays, because the page-side command queue executes independently of the channel.

## Evidence

Server binds on every launch:

```
[PLAYER] state listener on 127.0.0.1:38471        (× 6 launches)
```

Grep for any received request across the recorded session log:

```
$ Select-String dev-run.log -Pattern '\[PLAYER\] (ready|state|debug)'
(no matches)
```

Six launches, three request types, zero packets. The bootstrap's `fetch(..., { mode: 'no-cors' })`
does not help: `mode` governs CORS, not CSP. `connect-src` rejects the request before the
network stack sees it.

Command log shows the hang in practice:

```
[PLAYER] cmd=play   has_arg=true nonce=1794310356397   ← 11:32:33
[PLAYER] cmd=pause  ...                                ← 11:32:47  (user clicked)
```

`stream_play` never logged completion; the promise was still pending when the user paused.

## Consequences, precisely

| # | Effect | User-visible |
|---|--------|--------------|
| 1 | `stream_play` rejects after 30 s | Console error only, then… |
| 2 | frontend `catch` sets `streamingActive = false` | icon flips to ▶, badge/progress revert |
| 3 | `player-state` listener never fires | progress is `Date.now()` arithmetic |
| 4 | duration comes from catalog `duration_ms` | wrong for tracks with a different master length |
| 5 | no real `nowPlayingItem` | title/artist/artwork are whatever was clicked, not what Apple is playing |

## Fix

### Step 1 — delete the readiness wait (immediate, independent)

```rust
// player_command, after a successful dispatch
return Ok(());               // the page queue guarantees ordering
```

Remove the `PLAYER_READY` load/timeout branch. Keep the flag only if you want it for logs.
This alone removes the 30 s hang and the icon reset.

### Step 2 — a channel CSP cannot block

Write state into the document title and let Rust read it:

```js
// PLAYER_BOOTSTRAP
function publish() {
  try {
    var m = window.MusicKit.getInstance();
    var item = m.nowPlayingItem || null, a = (item && item.attributes) || null;
    document.title = JSON.stringify({
      p: m.isPlaying ? 1 : 0, t: m.currentPlaybackTime || 0,
      d: m.currentPlaybackDuration || 0, id: item && item.id,
      n: a && a.name, ar: a && a.artistName,
      w: a && a.artwork ? a.artwork.url : null
    });
  } catch (e) {}
}
window.__mkReady.then(function (m) {
  ['playbackStateDidChange','nowPlayingItemDidChange','playbackTimeDidChange']
    .forEach(function (n) { try { m.addEventListener(n, publish); } catch (e) {} });
  publish();
  setInterval(publish, 500);
});
```

```rust
// Rust: poll the window title, dedupe, emit
tauri::async_runtime::spawn(async move {
    let mut last = String::new();
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let Some(w) = app.get_webview_window("player") else { continue };
        let Ok(t) = w.title() else { continue };
        if t != last && t.starts_with('{') { last = t.clone(); let _ = app.emit("player-state", t); }
    }
});
```

Notes:
- `document.title` is not subject to CSP and is a native WebView2 property — no bridge needed.
- WebView2 reflects title changes to `WebViewWindow::title()`.
- Keep the JSON compact (title strings are short).
- The existing `app.js` listener already parses JSON payloads, so no frontend change is
  required if the field names are mapped to `payload.isPlaying`, `position`, `duration`,
  `id`, `title`, `artist`, `artwork`.

**Alternative considered:** `window.ipc.postMessage` into Tauri's IPC internals. Works, but
depends on internal message formats and needs capability changes; the title channel is
smaller and has no version risk.

### Step 3 — remove the corpse

Once step 2 is verified for a full listening session, delete:
- the TCP listener (`start_state_listener`) or keep it local-only for debugging,
- the `/debug` probe loop and `probe_report`,
- the `setInterval(report, 250)` `fetch` path in the bootstrap.

## Secondary issue in the same area

`start_apple_sign_in` deletes the shared `auth_profile` directory
(`lib.rs:591-597`) while the hidden player is running on it, and never closes or reloads the
player. After signing in, the player can hold a wiped profile.

**Fix:** before wiping, close the `player` window; after a successful sign-in, let
`ensure_player()` rebuild it on demand. One line each, but it must not be forgotten.

## Acceptance criteria

1. `stream_play` resolves in < 100 ms and never rejects on a timeout.
2. Play/pause updates the icon without a click, within ~500 ms.
3. Position advances from the player's own clock; pausing freezes it.
4. Resume from a paused state continues at the correct offset.
5. Nothing regresses when the player is closed and reopened.
