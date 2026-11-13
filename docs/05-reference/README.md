# Reference Material

Reverse-engineering notes and captured API contracts. Read these for *what Apple's app does*,
not for *what this app does* — the shipped architecture changed after they were written.

| File | Date | Still accurate? |
|------|------|-----------------|
| `01-REA-DECODE.md` | 2026-11-09 | Historical. Describes the installed Apple Music package: DLLs, exports, ordinals, FFI surface. Useful if native decryption is ever revisited. |
| `02-FULL-DECODE-REPORT.md` | 2026-11-09 | Historical. Broadest decode of the app package and its protocols. Same caveat. |
| `03-API-CONTRACTS.md` | 2026-11-09 | **Current.** Live MusicKit/REST endpoints and payload shapes captured from the official client. This is still the reference for `src-tauri/src/api/client.rs`. |

## What changed since 2026-11-09

- Streaming was believed to require decrypting FairPlay in-process by calling Apple's DLLs.
  It does not: a license challenge can only be produced by a real CDM, so the shipped app
  hosts Apple's own MusicKit player in a hidden WebView2 and lets **Windows PlayReady** do the
  decryption. `src-tauri/src/drm/**` is the leftover of the abandoned approach.
- The consequence is an **AAC 256 kbps ceiling**. Every "lossless" claim in these reports
  describes Apple's app, not ours.
- Local file playback (symphonia + rodio + SQLite) was built and then **parked**:
  `docs/03-implementation/04-PARKED-LOCAL-PLAYBACK.md`.

## How to use these documents

- Designing an API call → read `03-API-CONTRACTS.md`.
- Understanding what a specific Apple DLL does → `01-REA-DECODE.md`.
- Planning to revive native decryption → read both decode reports **and**
  `docs/04-bugs/` first; the blocker was the CDM challenge, not the bindings.

Note: these reports were written before the docs were reorganized, so a few of them point at
files under `docs/plan/` that no longer exist. The content they describe is now covered by
`docs/02-architecture/` and `docs/03-implementation/`.
