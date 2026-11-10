# Sub-Phase 1: App Environment Discovery (`01-app-locator.md`)

> **Komponen**: `src-tauri/src/drm/locator.rs`  
> **Status**: Ready for Implementation  
> **Target**: Menemukan lokasi folder instalasi paket Apple Music Windows dan folder `LocalState` secara dinamis dan aman tanpa *hardcoded absolute path*.

---

## 1. Latar Belakang & Kebutuhan

Paket aplikasi Windows Store (MSIX/Appx) menggunakan penamaan folder ber-versi dinamis, contohnya:
`C:\Program Files\WindowsApps\AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa`

Sedangkan data sesi pengguna berada pada:
`%LOCALAPPDATA%\Packages\AppleInc.AppleMusicWin_nzyj5cx40ttqa\LocalState`

Komponen `AppLocator` bertugas:
1. Mendeteksi kedua direktori tersebut secara otomatis.
2. Memverifikasi kelengkapan DLL utama (`CoreAudioToolbox.dll`, `CoreMedia.dll`, `CoreFP.dll`).
3. Mendukung environment variable override (`APPLE_MUSIC_INSTALL_DIR`, `APPLE_MUSIC_DATA_DIR`) untuk keperluan automated testing & CI.
4. Menangani kasus ketika Apple Music belum terpasang dengan mengembalikan error yang elegan (`DrmError::NotInstalled`) tanpa membuat aplikasi crash.

---

## 2. Struktur Data & Interface Rust

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum LocatorError {
    #[error("Apple Music is not installed on this system")]
    NotInstalled,
    #[error("LocalState directory not found at: {0}")]
    LocalStateNotFound(PathBuf),
    #[error("Missing critical DLLs: {0:?}")]
    MissingDlls(Vec<String>),
    #[error("Environment error: {0}")]
    EnvError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleInstallation {
    /// Direktori tempat DLL berada (misal di WindowsApps)
    pub install_dir: PathBuf,
    /// Direktori tempat cookies & data sesi berada
    pub local_state_dir: PathBuf,
    /// Versi terdeteksi (misal "1.1792.39682.0")
    pub version: String,
    /// Daftar DLL penting yang berhasil ditemukan
    pub available_dlls: Vec<String>,
}

pub trait PackageLocator: Send + Sync {
    fn locate(&self) -> Result<AppleInstallation, LocatorError>;
    fn verify_dlls(&self, install_dir: &std::path::Path) -> Result<Vec<String>, LocatorError>;
}
```

---

## 3. Langkah Implementasi (Implementation Flow)

1. **Step 1 - Environment Override Check**: Periksa apakah `APPLE_MUSIC_INSTALL_DIR` diset. Jika ya, gunakan path tersebut (krusial untuk unit test isolasi).
2. **Step 2 - Scan WindowsApps (Registry / Filesystem Walk)**:
   - Akses `C:\Program Files\WindowsApps` mencari entri yang cocok dengan prefix `AppleInc.AppleMusicWin_`.
   - Ambil entri dengan versi tertinggi jika terdapat lebih dari satu folder.
3. **Step 3 - Scan LocalAppData**:
   - Ambil `%LOCALAPPDATA%\Packages\AppleInc.AppleMusicWin_nzyj5cx40ttqa\LocalState`.
4. **Step 4 - Verifikasi Keberadaan DLL**:
   - Cek keberadaan `CoreAudioToolbox.dll`, `CoreMedia.dll`, `CoreFP.dll`, `AuthKitWin.dll`.
   - Kumpulkan daftar DLL yang ada ke dalam `available_dlls`.
5. **Step 5 - Return Hasil Terstruktur**:
   - Kembalikan `AppleInstallation` jika semua prasyarat terpenuhi.

---

## 4. Test Case Specification

### A. Positive Test Cases

#### `TEST-LOC-POS-01: Auto-detect Existing Apple Music Installation`
- **Prekondisi**: Apple Music terpasang di sistem host Windows.
- **Aksi**: Panggil `AppLocator::default().locate()`.
- **Ekspektasi**:
  - `result.is_ok()` bernilai `true`.
  - `install_dir.exists()` bernilai `true`.
  - `local_state_dir.exists()` bernilai `true`.
  - `available_dlls` memuat `"CoreAudioToolbox.dll"`, `"CoreMedia.dll"`, `"CoreFP.dll"`.

#### `TEST-LOC-POS-02: Environment Variable Override Takes Precedence`
- **Prekondisi**: Siapkan direktori tiruan (temp dir) yang berisi dummy `CoreAudioToolbox.dll`. Set `env::set_var("APPLE_MUSIC_INSTALL_DIR", temp_dir)`.
- **Aksi**: Panggil `AppLocator::default().locate()`.
- **Ekspektasi**:
  - `install_dir` mengarah persis ke `temp_dir` bukan ke `C:\Program Files\WindowsApps`.

#### `TEST-LOC-POS-03: Version String Extraction from Folder Name`
- **Prekondisi**: Folder bernama `AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa`.
- **Aksi**: Jalankan fungsi ekstraksi versi `parse_package_version()`.
- **Ekspektasi**: Mengembalikan `Some("1.1792.39682.0")`.

#### `TEST-LOC-POS-04: Multiple Versions Selects Newest`
- **Prekondisi**: Terdapat dua folder: `AppleInc.AppleMusicWin_1.1000.0.0_...` dan `AppleInc.AppleMusicWin_1.1792.39682.0_...`.
- **Aksi**: Jalankan deteksi direktori.
- **Ekspektasi**: Folder dengan versi lebih baru (`1.1792.39682.0`) yang terpilih.

---

### B. Negative Test Cases

#### `TEST-LOC-NEG-01: System Without Apple Music Returns NotInstalled`
- **Prekondisi**: Set environment override mengarah ke folder kosong yang bukan instalasi Apple Music.
- **Aksi**: Panggil `locate()`.
- **Ekspektasi**: Mengembalikan `Err(LocatorError::NotInstalled)`. Tidak boleh panic.

#### `TEST-LOC-NEG-02: Missing Critical DLL Returns MissingDlls`
- **Prekondisi**: Siapkan folder tiruan yang hanya memiliki `CoreAudioToolbox.dll` tanpa `CoreMedia.dll` dan `CoreFP.dll`.
- **Aksi**: Panggil `verify_dlls(mock_path)`.
- **Ekspektasi**: Mengembalikan `Err(LocatorError::MissingDlls(missing))` di mana list missing berisi `"CoreMedia.dll"` dan `"CoreFP.dll"`.

#### `TEST-LOC-NEG-03: Corrupted or Non-Existent LocalState Directory`
- **Prekondisi**: Install dir valid, tapi `%LOCALAPPDATA%\Packages\...` dihapus/diubah ke path fiktif via env override.
- **Aksi**: Panggil `locate()`.
- **Ekspektasi**: Mengembalikan `Err(LocatorError::LocalStateNotFound(_))`.

#### `TEST-LOC-NEG-04: Non-Windows Platform Handling`
- **Prekondisi**: Compile target non-Windows (`#[cfg(not(windows))]`).
- **Aksi**: Panggil `locate()`.
- **Ekspektasi**: Mengembalikan error informatif atau dummy uninstalled state dengan kompilasi tetap sukses (zero compile break).
