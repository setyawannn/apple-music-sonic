# Phase 4: Native FFI & Local Session Bridge — Master Plan

> **Tujuan**: Mengimplementasikan interop FFI native ke Apple Music Windows DLLs (`CoreAudioToolbox.dll`, `CoreMedia.dll`, `CoreFP.dll`) dan pembacaan sesi lokal (`Cookies`, `mspr.hds`) secara modular, cepat, dan aman, dengan target alokasi memori **< 85 MB RAM**.
> **Standar Rekayasa**: *Ponytail Senior Engineering* — minim alokasi heap, zero-bloat, tanpa lock contention, safe error handling dengan `Result<T, E>`, serta **16 skenario Acceptance Tests (8 Positif, 8 Negatif)** yang harus lulus 100%.

---

## 1. Struktur Dokumen Modul

Rencana kerja ini dipecah menjadi file-file spesifikasi atomik berikut:

| No | File Dokumen | Sub-Phase | Ruang Lingkup |
|---|---|---|---|
| 01 | [`01-app-locator.md`](./01-app-locator.md) | Sub-Phase 1 | Deteksi dinamis Appx package path & folder LocalState Windows |
| 02 | [`02-local-session.md`](./02-local-session.md) | Sub-Phase 2 | Ekstraksi sesi SQLite `Cookies` (mode Immutable & Read-Only) |
| 03 | [`03-dll-loader.md`](./03-dll-loader.md) | Sub-Phase 3 | Safe Dynamic DLL Loader (`libloading`) & Memory Benchmark |
| 04 | [`04-coreaudio-ffi.md`](./04-coreaudio-ffi.md) | Sub-Phase 4 | FFI binding CoreAudioToolbox & dekoder ALAC (Ordinal 8) |
| 05 | [`05-coremedia-drm.md`](./05-coremedia-drm.md) | Sub-Phase 5 | Pemetaan simbol CoreMedia (`Fig*`) & FairPlay ordinals |
| 06 | [`06-tauri-ipc-diagnostics.md`](./06-tauri-ipc-diagnostics.md) | Sub-Phase 6 | Integrasi perintah Tauri IPC & dashboard diagnostik UI |
| 07 | [`acceptance-tests-spec.md`](./acceptance-tests-spec.md) | Verifikasi | Rincian lengkap 16 Acceptance Tests (POS-01..08, NEG-01..08) |

---

## 2. Arsitektur Teknis Sub-Sistem FFI

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Apple Music RS Process                          │
│                                                                        │
│  ┌─────────────────────────┐         ┌──────────────────────────────┐  │
│  │   Tauri Frontend UI     │         │   Tauri Commands (IPC)       │  │
│  │  - Status Indicator     │◄───────►│   - get_native_bridge_status │  │
│  │  - Local Auth Import    │         │   - import_local_session     │  │
│  └─────────────────────────┘         └──────────────┬───────────────┘  │
│                                                     │                  │
│  ┌──────────────────────────────────────────────────┴───────────────┐  │
│  │                      src-tauri/src/drm/                          │  │
│  │                                                                  │  │
│  │  ┌─────────────────┐ ┌─────────────────┐ ┌────────────────────┐  │  │
│  │  │   AppLocator    │ │  SessionStore   │ │   NativeLoader     │  │  │
│  │  │   (locator.rs)  │ │(local_session.rs)│ │   (loader.rs)      │  │  │
│  │  └────────┬────────┘ └────────┬────────┘ └─────────┬──────────┘  │  │
│  │           │                   │                    │             │  │
│  │  ┌────────▼───────────────────▼────────────────────▼──────────┐  │  │
│  │  │           FFI Wrappers & Symbol Bindings                   │  │  │
│  │  │  ├── coreaudio.rs (ACAppleLosslessDecoderEntry, etc.)      │  │  │
│  │  │  ├── coremedia.rs (FigContentKeyBossGetClassID, etc.)      │  │  │
│  │  │  └── corefp.rs    (Ordinals 1-6 FairPlay verification)     │  │  │
│  │  └────────────────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
                                 │ Dynamic FFI (Windows OS Paging)
                                 ▼
┌────────────────────────────────────────────────────────────────────────┐
│               Host Operating System (Windows 10/11 x64)                │
│                                                                        │
│  Install Dir (WindowsApps):                                            │
│  ├── CoreAudioToolbox.dll (8.9 MB)                                     │
│  ├── CoreMedia.dll        (23.6 MB)                                    │
│  ├── CoreFP.dll           (37.6 MB)                                    │
│  └── AuthKitWin.dll       (1.0 MB)                                     │
│                                                                        │
│  LocalState Dir (%LOCALAPPDATA%\Packages\...):                         │
│  ├── EBWebView\Default\Network\Cookies (SQLite)                        │
│  └── Apple\CS\AppleMusic\mspr.hds      (Binary session)                │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Target KPI & Batasan (Constraints)

1. **Efisiensi RAM**:
   - Paging DLL native Apple di memori proses tidak boleh melebihi **+30 MB** tambahan.
   - Total Working Set memori aplikasi idle saat FFI bridge aktif: **< 85 MB RAM**.
2. **Keamanan & Stabilitas**:
   - Tidak boleh ada *unhandled panic* atau *access violation crash* (`STATUS_ACCESS_VIOLATION`) saat memuat DLL atau memanggil function pointer.
   - Semua fungsi FFI dibungkus dalam blok `unsafe` terisolasi dengan validasi pointer `is_null()` sebelum dereferensi.
3. **Zero Lock Contention**:
   - Akses file database SQLite sesi lokal dibuka dengan URI `file:path?immutable=1&mode=ro` untuk menjamin tidak ada konflik lock bila aplikasi resmi Apple Music sedang aktif memutar musik di latar belakang.
4. **Verifikasi Otomatis**:
   - Menghasilkan test file baru `tests/ffi_acceptance_tests.rs` dengan 16 test case positif dan negatif yang semuanya berstatus **PASS**.
