# Sub-Phase 3: Safe Native DLL Loader Primitive (`03-dll-loader.md`)

> **Komponen**: `src-tauri/src/drm/loader.rs`  
> **Status**: Ready for Implementation  
> **Target**: Menyediakan abstraksi *dynamic library loader* yang aman (thread-safe, RAII), mendukung pemanggilan fungsi by-name dan by-ordinal, serta memonitor konsumsi memori fisik proses (RAM Working Set).

---

## 1. Latar Belakang & Kebutuhan

Apple DLLs pada Windows memiliki karakteristik:
1. `CoreMedia.dll` (12.934 ekspor) dipanggil berdasarkan **nama asli** (`b"FigContentKeyBossGetClassID\0"`).
2. `CoreFP.dll` dan `CoreAudioToolbox.dll` memiliki fungsi-fungsi penting yang dipanggil berdasarkan **ordinal** (contoh: ordinal 8 untuk ALAC decoder).

Diperlukan loader yang:
- Menerapkan RAII (`Drop` menutup handle DLL).
- Menyediakan fungsi pemanggilan aman by-name dan by-ordinal.
- Memantau penambahan RAM (Working Set) proses agar terbukti **< 85 MB**.

---

## 2. Struktur Data & Interface Rust

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoaderError {
    #[error("Failed to load DLL from {0}: {1}")]
    LoadFailed(PathBuf, String),
    #[error("Symbol '{0}' not found in DLL")]
    SymbolNotFound(String),
    #[error("Ordinal #{0} not found in DLL")]
    OrdinalNotFound(u16),
    #[error("Memory monitoring error: {0}")]
    MemoryError(String),
}

pub struct NativeLibrary {
    path: PathBuf,
    lib: libloading::Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessMemoryStats {
    pub working_set_bytes: usize,
    pub peak_working_set_bytes: usize,
}

impl NativeLibrary {
    /// Memuat DLL dari path tertentu
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, LoaderError>;
    
    /// Mengambil function pointer berdasarkan nama
    pub unsafe fn get_symbol<T>(&self, symbol_name: &[u8]) -> Result<libloading::Symbol<T>, LoaderError>;
    
    /// Mengambil function pointer berdasarkan ordinal (Windows PE export table)
    pub unsafe fn get_ordinal<T>(&self, ordinal: u16) -> Result<*const (), LoaderError>;
}

/// Utility untuk membaca memori proses saat ini di Windows
pub fn get_current_process_memory() -> Result<ProcessMemoryStats, LoaderError>;
```

---

## 3. Langkah Implementasi (Implementation Flow)

1. **Step 1 - Crate Dependency**: Tambahkan `libloading = "0.8"` ke `Cargo.toml`.
2. **Step 2 - Safe Wrapper Construction**:
   - Bungkus `libloading::Library::new(path)` dengan penanganan error yang jelas.
3. **Step 3 - Ordinal Resolution**:
   - Di Windows, `GetProcAddress(hModule, MAKEINTRESOURCEA(ordinal))` digunakan untuk mencari fungsi berdasarkan nomor ordinal.
4. **Step 4 - Memory Monitoring Integration**:
   - Gunakan Windows API `K32GetProcessMemoryInfo` (atau `GetProcessMemoryInfo`) untuk mengambil `PROCESS_MEMORY_COUNTERS.WorkingSetSize`.
   - Mengukur `delta_ram = ram_setelah - ram_sebelum`.

---

## 4. Test Case Specification

### A. Positive Test Cases

#### `TEST-LDR-POS-01: Load Standard System DLL`
- **Prekondisi**: `kernel32.dll` selalu ada di Windows.
- **Aksi**: Panggil `NativeLibrary::load("kernel32.dll")`.
- **Ekspektasi**: `result.is_ok()` bernilai `true`.

#### `TEST-LDR-POS-02: Resolve Symbol by Name`
- **Prekondisi**: `kernel32.dll` dimuat.
- **Aksi**: Ambil simbol `GetCurrentProcessId`.
- **Ekspektasi**: Mengembalikan function pointer valid. Ketika dipanggil, menghasilkan PID proses yang sama dengan `std::process::id()`.

#### `TEST-LDR-POS-03: Resolve Symbol by Ordinal`
- **Prekondisi**: DLL dengan ordinal terdefinisi dimuat.
- **Aksi**: Panggil `get_ordinal(ordinal_num)`.
- **Ekspektasi**: Mengembalikan raw pointer yang tidak null.

#### `TEST-LDR-POS-04: Process Memory Tracking Returns Valid Bytes`
- **Prekondisi**: Proses berjalan normal.
- **Aksi**: Panggil `get_current_process_memory()`.
- **Ekspektasi**: `working_set_bytes > 0` dan `working_set_bytes < 150_000_000` (sesuai target performa).

---

### B. Negative Test Cases

#### `TEST-LDR-NEG-01: Non-Existent DLL Path`
- **Prekondisi**: Path `C:\fake_non_existent_library_12345.dll`.
- **Aksi**: Panggil `NativeLibrary::load()`.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::LoadFailed(_, _))`. Tidak boleh crash.

#### `TEST-LDR-NEG-02: Missing Symbol by Name`
- **Prekondisi**: `kernel32.dll` dimuat.
- **Aksi**: Ambil simbol `ThisSymbolDoesNotExistDefinitely1234`.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::SymbolNotFound(_))`.

#### `TEST-LDR-NEG-03: Invalid Ordinal Number`
- **Prekondisi**: DLL dimuat.
- **Aksi**: Ambil ordinal yang tidak ada (misal `0xFFFF`).
- **Ekspektasi**: Mengembalikan `Err(LoaderError::OrdinalNotFound(0xFFFF))`.

#### `TEST-LDR-NEG-04: Corrupted Non-PE File as DLL`
- **Prekondisi**: File teks biasa dinamai `.dll`.
- **Aksi**: Coba muat dengan `NativeLibrary::load()`.
- **Ekspektasi**: Mengembalikan `Err(LoaderError::LoadFailed(_, _))` tanpa fatal unhandled exception.
