# Sub-Phase 5: CoreMedia & DRM Symbol Mapping (`05-coremedia-drm.md`)

> **Komponen**: `src-tauri/src/drm/coremedia.rs` & `src-tauri/src/drm/corefp.rs`  
> **Status**: Ready for Implementation  
> **Target**: Memetakan fungsi media pipeline dari `CoreMedia.dll` dan entry points FairPlay dari `CoreFP.dll` serta `CoreKE.dll` untuk verifikasi orkestrasi DRM secara safe.

---

## 1. Latar Belakang & Kebutuhan

Apple Media Framework pada Windows mengandalkan:
1. `CoreMedia.dll` (23.6 MB) — memuat 12.934 ekspor dengan **nama asli tidak ter-obfuscate**. Berisi kelas orkestrasi konten seperti `FigContentKeyBoss`, `FigCPEFairPlayCryptor`, dan `FigSampleBufferSetDecryptor`.
2. `CoreFP.dll` (37.6 MB) — DRM FairPlay inti dengan **6 ekspor ter-obfuscate** yang dipanggil via ordinal 1 s/d 6.
3. `CoreKE.dll` (45.7 MB) — Key Exchange dengan 2 ekspor ter-obfuscate (ordinal 1 dan 2).

Sub-phase ini memvalidasi keterhubungan modul-modul ini, memastikan bahwa runtime Apple Framework dapat diinisialisasi tanpa mengalami *side-by-side dependency error* atau access violation.

---

## 2. Struktur Data & Interface Rust

```rust
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use crate::drm::loader::{NativeLibrary, LoaderError};

pub struct CoreMediaBridge {
    _lib: Arc<NativeLibrary>,
    pub fig_content_key_boss_get_class_id: unsafe extern "C" fn() -> u64,
    pub fig_cpe_cryptor_get_class_id: unsafe extern "C" fn() -> u64,
    pub fig_cpe_fairplay_cryptor_get_class_id: unsafe extern "C" fn() -> u64,
}

pub struct CoreFpBridge {
    _lib: Arc<NativeLibrary>,
    pub fp_ordinals: [*const c_void; 6],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrmDiagnosticReport {
    pub coremedia_loaded: bool,
    pub corefp_loaded: bool,
    pub coreke_loaded: bool,
    pub key_boss_class_id: u64,
    pub fp_ordinals_resolved: usize,
}
```

---

## 3. Langkah Implementasi (Implementation Flow)

1. **Step 1 - Dependency Resolution**:
   - `CoreMedia.dll` memerlukan `CoreFoundation.dll` dan dependensi C runtime Apple yang berada di direktori yang sama.
   - Gunakan `SetDllDirectoryW` pada direktori instalasi sebelum memuat agar dependensi sibling ter-resolve secara otomatis oleh dynamic linker Windows.
2. **Step 2 - Muat CoreMedia**:
   - Ambil function pointer untuk:
     - `b"FigContentKeyBossGetClassID\0"`
     - `b"FigCPECryptorGetClassID\0"`
     - `b"FigCPEFairPlayCryptorGetClassID\0"`
3. **Step 3 - Panggil GetClassID**:
   - Panggil `FigContentKeyBossGetClassID()` dalam blok `unsafe`.
   - Apple CoreMedia mengembalikan class ID integer unik (non-zero). Ini menjadi bukti tak terbantahkan bahwa DLL aktif dan terinisialisasi.
4. **Step 4 - Muat CoreFP**:
   - Ambil ordinal 1 sampai 6 (`WIn9UJ86JKdV4dM`, dll.).
   - Pastikan seluruh 6 ordinal mengembalikan pointer valid.

---

## 4. Test Case Specification

### A. Positive Test Cases

#### `TEST-DRM-POS-01: Load CoreMedia and Resolve Real Function Names`
- **Prekondisi**: Host memiliki instalasi resmi Apple Music.
- **Aksi**: Muat `CoreMedia.dll` dan ambil simbol `FigContentKeyBossGetClassID`.
- **Ekspektasi**: Pointer fungsi berhasil diperoleh tanpa error.

#### `TEST-DRM-POS-02: Execute FigContentKeyBossGetClassID Returns Valid Non-Zero ID`
- **Prekondisi**: `CoreMediaBridge` terinisialisasi.
- **Aksi**: Panggil fungsi `(bridge.fig_content_key_boss_get_class_id)()`.
- **Ekspektasi**: Mengembalikan nilai ID > 0 tanpa exception.

#### `TEST-DRM-POS-03: Resolve All 6 CoreFP Ordinals`
- **Prekondisi**: `CoreFP.dll` dimuat dari host.
- **Aksi**: Panggil pengambilan ordinal 1..=6.
- **Ekspektasi**: Seluruh 6 pointer tidak bernilai NULL (`fp_ordinals_resolved == 6`).

#### `TEST-DRM-POS-04: Sibling DLL Directory Resolution`
- **Prekondisi**: Pasang direktori Apple Music ke `SetDllDirectoryW`.
- **Aksi**: Muat `CoreMedia.dll` yang bergantung pada `CoreFoundation.dll`.
- **Ekspektasi**: Dynamic linker berhasil menyelesaikan dependensi otomatis tanpa missing DLL error.

---

### B. Negative Test Cases

#### `TEST-DRM-NEG-01: CoreMedia Missing Export Symbol`
- **Prekondisi**: Muat DLL tiruan tanpa simbol `FigContentKeyBossGetClassID`.
- **Aksi**: Inisialisasi bridge.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::SymbolNotFound(_))`.

#### `TEST-DRM-NEG-02: CoreFP Incomplete Ordinal Set`
- **Prekondisi**: Muat DLL yang hanya memiliki ordinal 1..3 tetapi tidak memiliki ordinal 4..6.
- **Aksi**: Inisialisasi `CoreFpBridge`.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::OrdinalNotFound(4))` pada pemeriksaan ordinal pertama yang hilang.

#### `TEST-DRM-NEG-03: Execution With Missing Sibling Path`
- **Prekondisi**: Hapus path instalasi dari DLL search directory.
- **Aksi**: Coba muat DLL dependen secara terisolasi jika dependensinya hilang.
- **Ekspektasi**: Mengembalikan error terkelola yang menangkap kode error Windows (misal Win32 Error 126 `ERROR_MOD_NOT_FOUND`).

#### `TEST-DRM-NEG-04: Non-Windows Build Compatibility`
- **Prekondisi**: Kompilasi pada target non-Windows.
- **Aksi**: Jalankan `cargo check`.
- **Ekspektasi**: Kode di-stub dengan `#[cfg(windows)]` sehingga platform lain tidak mengalami compile error.
