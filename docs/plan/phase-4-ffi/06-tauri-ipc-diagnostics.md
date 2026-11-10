# Sub-Phase 6: Tauri Commands & UI Diagnostics (`06-tauri-ipc-diagnostics.md`)

> **Komponen**: `src-tauri/src/lib.rs` & `src/js/app.js`  
> **Status**: Ready for Implementation  
> **Target**: Menghubungkan fungsi-fungsi FFI native dan ekstraksi sesi lokal ke frontend Tauri melalui IPC commands, serta menyajikan status kesehatan bridge secara visual.

---

## 1. Latar Belakang & Kebutuhan

Frontend membutuhkan visibilitas terhadap:
1. Apakah instalasi resmi Apple Music terdeteksi di komputer pengguna.
2. Apakah DLL native Apple (`CoreAudioToolbox`, `CoreMedia`, `CoreFP`) berhasil diikat.
3. Berapa konsumsi RAM proses saat ini (memvalidasi KPI < 85 MB).
4. Tombol/fitur untuk mengimpor sesi otentikasi lokal (`Cookies`) hanya dengan sekali klik tanpa meminta user login manual.

---

## 2. Definisi Tauri IPC Commands

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NativeBridgeStatusPayload {
    pub is_installed: bool,
    pub install_path: Option<String>,
    pub version: Option<String>,
    pub coreaudio_ready: bool,
    pub coremedia_ready: bool,
    pub corefp_ready: bool,
    pub current_ram_mb: f64,
    pub session_detected: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImportSessionResult {
    pub success: bool,
    pub has_user_token: bool,
    pub storefront: Option<String>,
    pub message: String,
}

// ─── IPC Handlers ───────────────────────────────────────────────────

#[tauri::command]
pub fn get_native_bridge_status(state: State<AppState>) -> Result<NativeBridgeStatusPayload, String>;

#[tauri::command]
pub fn import_local_apple_session(state: State<AppState>) -> Result<ImportSessionResult, String>;

#[tauri::command]
pub fn run_native_bridge_diagnostics(state: State<AppState>) -> Result<String, String>;
```

---

## 3. Integrasi Frontend (`src/js/app.js`)

1. **Auto-check saat startup**:
   - Panggil `invoke("get_native_bridge_status")` saat aplikasi selesai dimuat.
   - Jika `session_detected == true` dan pengguna belum login di aplikasi kita, tawarkan prompt:
     *“Apple Music session detected on your PC. Import credentials?”*
2. **Settings Panel Indicator**:
   - Menampilkan badge diagnostik:
     `Native Engine: CoreAudio [OK] | CoreMedia [OK] | RAM: 64.2 MB`

---

## 4. Test Case Specification

### A. Positive Test Cases

#### `TEST-IPC-POS-01: Query Native Bridge Status Returns Valid Payload`
- **Prekondisi**: Aplikasi berjalan di sistem Windows.
- **Aksi**: Panggil `get_native_bridge_status()`.
- **Ekspektasi**: Mengembalikan payload JSON dengan `current_ram_mb > 0.0` dan field boolean yang valid.

#### `TEST-IPC-POS-02: Import Local Session Injects Token to AppState`
- **Prekondisi**: File `Cookies` lokal memiliki sesi Apple valid.
- **Aksi**: Panggil `import_local_apple_session()`.
- **Ekspektasi**:
  - `result.success == true`.
  - `state.api_client.auth.is_authenticated() == true`.

#### `TEST-IPC-POS-03: Diagnostics Command Generates Formatted Log`
- **Prekondisi**: Bridge terpasang.
- **Aksi**: Panggil `run_native_bridge_diagnostics()`.
- **Ekspektasi**: Mengembalikan string laporan multi-baris yang mencakup status setiap DLL dan class ID.

#### `TEST-IPC-POS-04: RAM Measurement Remains Under 85MB KPI`
- **Prekondisi**: Seluruh DLL native di-load ke proses.
- **Aksi**: Panggil `get_native_bridge_status()`.
- **Ekspektasi**: `current_ram_mb < 85.0`.

---

### B. Negative Test Cases

#### `TEST-IPC-NEG-01: Import Local Session When No Cookies Exist`
- **Prekondisi**: File `Cookies` tidak ada atau kosong.
- **Aksi**: Panggil `import_local_apple_session()`.
- **Ekspektasi**: Mengembalikan error ramah: `Err("No valid Apple Music session cookies found on this computer")`.

#### `TEST-IPC-NEG-02: Bridge Status When Apple Music Uninstalled`
- **Prekondisi**: Jalankan pada PC tanpa Apple Music.
- **Aksi**: Panggil `get_native_bridge_status()`.
- **Ekspektasi**: Mengembalikan payload dengan `is_installed: false`, `coreaudio_ready: false`, tanpa panic.

#### `TEST-IPC-NEG-03: Concurrent IPC Import Calls Prevent Race Condition`
- **Prekondisi**: Panggil `import_local_apple_session()` dari 5 thread/request serentak.
- **Aksi**: Pantau state `AppState.api_client`.
- **Ekspektasi**: Semua panggilan selesai secara konsisten tanpa deadlock mutex.

#### `TEST-IPC-NEG-04: Invalid Storefront Recovery`
- **Prekondisi**: Data cookie lokal memiliki format storefront yang rusak/tidak dikenal.
- **Aksi**: Panggil `import_local_apple_session()`.
- **Ekspektasi**: Default ke storefront standar (misal `"id"` atau `"us"`) dan tidak menggagalkan proses login.
