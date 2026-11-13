# Bug 03 — The player state channel was dead, and every command hung 30 s

> **Status: FIXED & VERIFIED ✅** — 2026-11-13
> Affected: `stream_play`, `stream_pause`, `stream_resume`, `stream_seek`, the playback bar,
> now-playing metadata.
> Fixed in: `src-tauri/src/lib.rs` (`player_command`, `PLAYER_BOOTSTRAP`,
> `start_state_poller`), `src/js/app.js` (progress-bar seek).

---

## What was broken

Two separate faults, stacked:

1. **The state channel could never work.** `PLAYER_BOOTSTRAP` POSTed player state to
   `http://127.0.0.1:38471`. `music.apple.com`'s CSP `connect-src` blocks that before it
   leaves the page, so the listener received **zero** packets across every recorded run.
2. **Every command waited on that channel.** `player_command` blocked up to 30 s on a
   `/ready` signal the page could not send, then returned `Err`. The frontend's `catch`
   reset its optimistic state, so the play icon flipped back even though audio was playing.

## Evidence it was dead (pre-fix)

Server bound on every launch, and no request ever arrived:

```
[PLAYER] state listener on 127.0.0.1:38471        (× 6 launches)
$ Select-String dev-run.log -Pattern '\[PLAYER\] (ready|state|debug)'
(no matches)
```

`mode: 'no-cors'` does not help: `mode` governs CORS, not CSP.

## Fix, part 1 — delete the readiness wait

`player_command` now dispatches the job and returns immediately:

```rust
window.eval(&js).map_err(|e| e.to_string())?;
println!("[PLAYER] cmd={} nonce={}", cmd, nonce);
Ok(())
```

The page owns ordering (`window.__mkQueue` + `__mkDrain`), so a command issued before
MusicKit is live is parked and executed when it loads. There is no handshake, no timeout, no
retry, and re-introducing one would be a regression.

Removed with it: `PLAYER_READY`, `PLAYER_READY_SIGNAL`, `player_ready_signal()`,
`PLAYER_STATE_PORT`, `LAST_PLAYER_STATE`, the TCP listener, the `/debug` handler, the 5 s
probe loop, and the `probe_report` command (48 → 47 commands).

## Fix, part 2 — a channel CSP cannot block

**Title channel was tried first and failed.** The plan was to write JSON into
`document.title` and poll `WebviewWindow::title()`. Measured result:

```
[TITLE] "Apple Music Player"      ← never changed, over a full song
```

WebView2 does not mirror `document.title` to the native window title. Do not retry it.

**Cookie channel works.** `document.cookie` is first-party, CSP-immune, and
`window.cookies()` is the same API the sign-in flow already uses to read
`media-user-token` — proven on this exact webview.

Page side (`PLAYER_BOOTSTRAP`):

```js
function publish() {
  var m = window.MusicKit.getInstance();
  var item = m.nowPlayingItem || null, attrs = (item && item.attributes) || null;
  var json = JSON.stringify({
    isPlaying: !!m.isPlaying,
    position: m.currentPlaybackTime || 0,
    duration: m.currentPlaybackDuration || 0,
    id: item && item.id ? item.id : null,
    title: attrs ? attrs.name : null,
    artist: attrs ? attrs.artistName : null,
    artwork: attrs && attrs.artwork ? attrs.artwork.url : null
  });
  document.cookie = 'mk_state=' + encodeURIComponent(json) +
                    '; path=/; max-age=300; Secure; SameSite=Lax';
}
window.__mkReady.then(function (m) {
  ['playbackStateDidChange','nowPlayingItemDidChange','playbackTimeDidChange']
    .forEach(function (n) { try { m.addEventListener(n, publish); } catch (e) {} });
  publish();
  setInterval(publish, 500);
});
```

Rust side (`start_state_poller`, 250 ms):

```rust
let jar = window.cookies()?;
let raw = jar.iter().find(|c| c.name() == "mk_state").map(|c| c.value())?;
let json = if raw.starts_with('{') { raw.to_string() } else { percent_decode(raw) };
if json == last || serde_json::from_str::<Value>(&json).is_err() { continue; }
last = json.clone();
app.emit("player-state", json);
```

Notes that matter:

- WebView2 may return the cookie already decoded or still percent-encoded; both are accepted,
  because encoding a `%` again would corrupt valid JSON.
- Dedupe is by full string, so position advances emit ~2/s and a paused player emits nothing.
- The payload keys are unchanged, so `app.js`'s existing listener needed no edit.

## Verified

Live from a dev run after the fix:

```
[PLAYER] state {"isPlaying":true,"position":17,"duration":155,"id":"6812102720",
                "title":"hang up","artist":"Kendis","artwork":"https://is1-ssl.mzstatic.com/..."}
[PLAYER] state {"isPlaying":true,"position":18,...}
```

- Real position, real duration (155 s), title/artist/artwork from `nowPlayingItem`.
- `stream_play` resolves in milliseconds; the icon no longer snaps back after 30 s.

## Secondary issue fixed in the same pass

`start_apple_sign_in` deleted the shared `auth_profile` directory while the hidden player was
running on it, and never closed or reloaded the player — so signing in left the player holding
a wiped session until restart. It now:

1. closes the `player` window before wiping,
2. waits up to ~1 s for WebView2 to release the profile lock,
3. calls `ensure_player()` after a successful sign-in to re-warm it.

## Regression guards

- `stream_play` must resolve fast and never reject on a timeout.
- Play/pause updates the icon without a click, within ~500 ms.
- Position advances from the player's own clock; pausing freezes it.
- Nothing breaks when the player is closed and rebuilt.
- Do not add a readiness handshake. Do not use `document.title`. Do not use loopback `fetch`.
