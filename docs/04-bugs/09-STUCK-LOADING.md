# Bug 09 — Play icon spins forever, and a false "this song didn't start" toast

> Status: **fixed** (2026-11-15)
> Reported as: "the song I click keeps loading while the song plays, and sometimes I get a
> toast saying it didn't start even though it is playing."

## Symptom

Two faces of the same state machine bug:

1. Click a song → the play button switches to the spinner → **the song starts and is audible,
   but the spinner never goes away**.
2. Sometimes, after ~12 s of that, a red toast appears: *"This song didn't start. It may be
   unavailable, or you may need to sign in again."* — over a song that is clearly playing.

The loading state therefore never told the truth, and the error state actively lied.

## Root cause

`beginStreamLoad(songId)` remembered only the **id** of the clicked row, and
`applyPlayerState` cleared the spinner only on exact equality:

```js
// before
if (!state.streamPendingId || (payload.id && payload.id === state.streamPendingId)) {
  settleStreamPhase(state.streamIsPlaying);
}
```

That assumption is wrong, for three separate reasons:

- **Catalog and library are different id namespaces.** Ask for a catalog id (`1440857781`) and
  MusicKit can report the user's library twin (`i.ldDQxglt4EvkLo1`) — same song, different id.
  Ask for a library id and the converse happens.
- **`setQueue({ songs })` is the catalog API.** A library-playlist row carries `i.…` ids, so
  MusicKit resolves them and may hand back a re-derived item.
- **Shuffle can change which item starts.** The bootstrap sets `m.shuffleMode` *before*
  `setQueue`, so a "play this song" click with shuffle already on can legitimately begin on a
  different track.

In every one of those cases the player was fine; only the id comparison failed. The watchdog
then fired `error` unconditionally, without checking whether audio was actually playing.

A second, smaller lie: the row highlight had the same strict comparison
(`el.dataset.catalogId === payload.id`), so the playing row could stay un-highlighted.

## Fix

Identity is now decided by evidence, not by one id:

```js
function isPendingTrack(payload) {
  if (!state.streamPendingId) return true;
  if (payload.id && payload.id === state.streamPendingId) return true;

  const title = normalizeTrackText(payload.title);
  if (!title || title !== state.streamPendingTitle) return false;

  const artist = normalizeTrackText(payload.artist);
  return !artist || !state.streamPendingArtist || artist === state.streamPendingArtist;
}
```

`beginStreamLoad(song)` now takes the whole row, so the pending title and artist are available
for that comparison. `normalizeTrackText` lowercases, trims, and collapses whitespace, so
`"Hang Up"` matches `"hang up"`.

Four changes in total:

1. **Settle on id OR title + artist** (`isPendingTrack`), not on id alone.
2. **Grace fallback:** if the player is audibly playing a different item for 3 s, the load
   settles as *playing* — the bar mirrors the player instead of the click, and a
   `console.info("[stream] …")` line records the remap.
3. **The watchdog never fires over audio.** At 12 s it settles to `playing` when
   `streamIsPlaying && streamCurrentId`, and only reports failure when nothing is playing.
4. **The row highlight accepts the same title/artist fallback**, so the playing row is
   highlighted even when the ids disagree.

The full-screen Now Playing play button also shows the spinner now (`.np-play-btn.loading`),
because both bars must mirror one phase.

## Why not just compare titles always?

Titles repeat across covers, remixes and live versions, so id is still checked first. Title +
artist is only the fallback for the namespace gap, and the 3 s grace exists for the case where
even that does not line up (for example a re-recorded track in the library).

## Verification

No automated test covers `app.js` (that is unchanged and documented in `01-WHATS-DONE.md`), so
this one is verified live:

- the spinner must clear on the first real `player-state` for the row that was clicked, whether
  the id matched or not;
- playing a catalog song that is also in the library must clear the spinner and highlight the
  row;
- no error toast may appear while `[PLAYER] state` reports `isPlaying:true`.

## Rule

**`loading` ends when the player proves it is playing, not when an id matches.** Any strict
identity check across the catalog/library boundary is a bug waiting to happen — compare ids,
then titles, and always trust audible state over a click.
