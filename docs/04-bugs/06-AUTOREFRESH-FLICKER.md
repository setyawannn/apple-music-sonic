# Bug 06 — Now-playing art and playlist rows flicker during playback

> Status: **fixed** (2026-11-13)
> Symptom: while a song played, the now-playing artwork (and the playlist's "playing" row)
> flickered, roughly once a second. Reported as "auto refresh makes my song flicker".

## What was happening

Two independent repaints were racing the playback UI.

### 1. The now-playing artwork was rebuilt on every state tick

`player-state` fires on every change — about once a second while a song plays, because the
position changes. The listener did this unconditionally:

```js
if (payload.artwork) {
  artImg.innerHTML = `<img src="${formatted}" ... />`;  // rebuilt every tick
}
```

Re-assigning `innerHTML` destroys and recreates the `<img>` node even when the URL is
identical, so the artwork node flashed on every tick.

### 2. The open playlist was repainted on a timer

The auto-refresh added the same day ran `refreshOpenPlaylist` every 30 s. Any repaint replaced
the `.track-list` HTML, which dropped the `.playing` highlight until the next state tick, so the
currently-playing row blinked. It also had a worse cousin: on window focus, `syncCloudPlaylists`
called `renderPlaylists()` whenever `currentView === "playlists"` — which is *still* true while a
playlist **detail** is open — so focusing the window replaced your open playlist with the list.

(The supposedly-suspect part — the track-id list changing between calls — was measured and is
**not** the cause: three consecutive fetches of a 20-track playlist returned byte-identical id
sequences. The repaint itself was the problem.)

## Fix

1. **Rebuild the artwork `<img>` only when its URL changes.** A new
   `state.streamArtworkUrl` remembers the last painted URL, so steady-state ticks touch nothing.
2. **Removed the periodic playlist refresh.** Refresh of the open playlist is now event-driven:
   on window focus and immediately after you add a song. No timers.
3. **A playlist repaint re-applies the now-playing highlight immediately**, so even a genuine
   refresh can't blank it.
4. **`syncCloudPlaylists` never re-renders the list while a playlist detail is open**
   (`!state.openPlaylistId`), so focus/auto-sync can't yank you out of the playlist.

## Evidence

- Three consecutive `get_cloud_playlist_tracks` calls for the same playlist returned identical
  20-item id sequences (so content-diffing never fired spuriously — the flicker was pure
  repaint).
- `node --check` and `npm run build` clean.
- `setInterval(...)` now appears exactly once in `app.js` — the 250 ms playback clock
  (`syncPlaybackStatus`), which only sets text/width and never rebuilds nodes.

## Lesson

Do not repaint DOM on a timer while media is playing, and do not rebuild a node when only its
contents repeat. Repaint on *user* events, and guard element rebuilds with a "did it actually
change" check.
