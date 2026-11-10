# Sub-Phase 4: CoreAudioToolbox & ALAC Decoder Binding (`04-coreaudio-ffi.md`)

> **Komponen**: `src-tauri/src/drm/coreaudio.rs`  
> **Status**: Ready for Implementation  
> **Target**: Memetakan dan memvalidasi pemanggilan fungsi native dari `CoreAudioToolbox.dll`, khususnya dekoder ALAC (Lossless) dan AAC melalui FFI yang aman.

---

## 1. Latar Belakang & Kebutuhan

Berdasarkan laporan decode [docs/FULL-DECODE-REPORT.md](../../FULL-DECODE-REPORT.md):
- `CoreAudioToolbox.dll` (8.9 MB) memiliki **183 fungsi ekspor**.
- Dekoder ALAC berada pada **Ordinal 8**: `ACAppleLosslessDecoderEntry` (di alamat dasar `0x180438800`).
- Dekoder AAC-LC (256kbps) berada pada **Ordinal 17**: `ACMP4AACLowComplexityDecoderEntry`.
- Terdapat fungsi konversi buffer audio standar CoreAudio seperti `AudioConverterConvertBuffer`.

Komponen ini bertanggung jawab menyediakan jembatan FFI Rust yang membungkus pemanggilan pointer tersebut dengan validasi batas, konversi tipe aman, dan zero memory leak.

---

## 2. Struktur Data & Interface Rust

```rust
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use crate::drm::loader::{NativeLibrary, LoaderError};

pub struct CoreAudioBridge {
    _lib: Arc<NativeLibrary>,
    pub alac_decoder_ptr: *const c_void,
    pub aac_decoder_ptr: *const c_void,
    pub mp3_decoder_ptr: *const c_void,
}

// Thread-safe handle marker
unsafe impl Send for CoreAudioBridge {}
unsafe impl Sync for CoreAudioBridge {}

impl CoreAudioBridge {
    /// Memuat CoreAudioToolbox.dll dari direktori instalasi
    pub fn load_from_dir(dir: &Path) -> Result<Self, LoaderError>;

    /// Memeriksa apakah seluruh decoder penting berhasil terikat
    pub fn is_operational(&self) -> bool;

    /// Mengembalikan ringkasan status pointer
    pub fn decoder_status(&self) -> CoreAudioStatus;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreAudioStatus {
    pub alac_ready: bool,
    pub aac_ready: bool,
    pub mp3_ready: bool,
}
```

---

## 3. Langkah Implementasi (Implementation Flow)

1. **Step 1 - Muat DLL**: Panggil `NativeLibrary::load(dir.join("CoreAudioToolbox.dll"))`.
2. **Step 2 - Ikat Ordinal ALAC (Ordinal 8)**:
   - Ambil pointer fungsi ordinal 8 via `lib.get_ordinal(8)`.
   - Pastikan pointer tidak bernilai null (`!ptr.is_null()`).
3. **Step 3 - Ikat Ordinal AAC & MP3 (Ordinal 17 & 13)**:
   - Ambil ordinal 17 (`ACMP4AACLowComplexityDecoderEntry`) dan ordinal 13 (`ACMP3DecoderEntry`).
4. **Step 4 - Validasi Keselamatan Pointer**:
   - Jika salah satu pointer null, kembalikan `LoaderError::OrdinalNotFound`.
5. **Step 5 - Struktur Bungkus Safe**:
   - Simpan dalam `CoreAudioBridge`.

---

## 4. Test Case Specification

### A. Positive Test Cases

#### `TEST-CA-POS-01: Load CoreAudioToolbox from Host Install`
- **Prekondisi**: Host memiliki instalasi resmi Apple Music.
- **Aksi**: Panggil `CoreAudioBridge::load_from_dir(&install_dir)`.
- **Ekspektasi**: `result.is_ok()` bernilai `true`, `is_operational()` bernilai `true`.

#### `TEST-CA-POS-02: ALAC Decoder Entry Point (Ordinal 8) Valid Non-Null`
- **Prekondisi**: `CoreAudioBridge` terinisialisasi.
- **Aksi**: Periksa `bridge.alac_decoder_ptr`.
- **Ekspektasi**: Pointer tidak bernilai null (`ptr as usize != 0`).

#### `TEST-CA-POS-03: AAC and MP3 Decoder Ordinals Valid`
- **Prekondisi**: `CoreAudioBridge` terinisialisasi.
- **Aksi**: Periksa `bridge.decoder_status()`.
- **Ekspektasi**: `status.aac_ready == true` dan `status.mp3_ready == true`.

#### `TEST-CA-POS-04: Thread-Safe Access Across Concurrency`
- **Prekondisi**: `Arc<CoreAudioBridge>` dibagikan ke 4 thread Tokio/OS.
- **Aksi**: Setiap thread membaca pointer decoder dan menjalankan inspeksi.
- **Ekspektasi**: Tidak terjadi data race, deadlock, atau memory corruption.

---

### B. Negative Test Cases

#### `TEST-CA-NEG-01: Missing CoreAudioToolbox.dll File`
- **Prekondisi**: Direktori kosong tanpa `CoreAudioToolbox.dll`.
- **Aksi**: Panggil `CoreAudioBridge::load_from_dir(&empty_dir)`.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::LoadFailed(_, _))`.

#### `TEST-CA-NEG-02: Mock DLL Without Ordinal 8`
- **Prekondisi**: Muat DLL tiruan yang tidak mengekspor ordinal 8.
- **Aksi**: Panggil pemuatan bridge.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::OrdinalNotFound(8))`.

#### `TEST-CA-NEG-03: Null Pointer Verification Guard`
- **Prekondisi**: Simulasi kondisi di mana `GetProcAddress` menghasilkan NULL.
- **Aksi**: Verifikasi logic `is_operational()`.
- **Ekspektasi**: Mengembalikan `false` dan menolak inisialisasi decoder pipeline.

#### `TEST-CA-NEG-04: Graceful Handling on Corrupt DLL Headers`
- **Prekondisi**: File `CoreAudioToolbox.dll` dengan header PE rusak.
- **Aksi**: Panggil `load_from_dir`.
- **Ekspektasi**: Mengembalikan error loader terkelola tanpa process termination (exit code != crash).
