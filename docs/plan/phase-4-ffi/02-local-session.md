# Sub-Phase 2: Local Session & SQLite Cookie Extractor (`02-local-session.md`)

> **Komponen**: `src-tauri/src/api/local_session.rs`  
> **Status**: Ready for Implementation  
> **Target**: Membaca sesi otentikasi Apple Music dari database SQLite WebView2 yang tersimpan di sistem lokal tanpa perlu login manual dan tanpa memicu lock collision database.

---

## 1. Latar Belakang & Kebutuhan

Aplikasi resmi Apple Music Windows menyimpan sesi pengguna di WebView2 (Edge Chromium embedded). Sesi ini tersimpan pada:
`%LOCALAPPDATA%\Packages\AppleInc.AppleMusicWin_nzyj5cx40ttqa\LocalState\EBWebView\Default\Network\Cookies`

Terdapat risiko kritis:
Jika aplikasi resmi sedang berjalan, mengakses file `Cookies` secara langsung dengan koneksi write atau lock normal akan memicu error:
`SqliteFailure: database is locked (code 5)`.

**Solusi Senior Dev**:
Buka SQLite dengan mode Read-Only dan flag URI **`?immutable=1`**:
`file:///path/to/Cookies?immutable=1&mode=ro`
Ini menginstruksikan engine SQLite bahwa file tersebut tidak akan dimodifikasi oleh proses kita, sehingga SQLite tidak memerlukan lock file POSIX/Windows (`.shm` / `.wal`), sehingga dapat dibaca dengan aman 100% tanpa mengganggu aplikasi resmi.

---

## 2. Struktur Data & Interface Rust

```rust
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Cookies database not found at: {0}")]
    NotFound(PathBuf),
    #[error("SQLite query error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("No valid Apple Music session cookies found in database")]
    NoSessionFound,
    #[error("Decryption error: {0}")]
    DecryptionFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalAppleSession {
    pub media_user_token: Option<String>,
    pub storefront: Option<String>,
    pub itctx: Option<String>,
    pub dsid: Option<String>,
    pub all_cookies: Vec<(String, String)>,
}

pub trait LocalSessionReader: Send + Sync {
    fn extract_from_cookies_db(&self, db_path: &Path) -> Result<LocalAppleSession, SessionError>;
    fn extract_from_local_state(&self, local_state_dir: &Path) -> Result<LocalAppleSession, SessionError>;
}
```

---

## 3. Skema Database SQLite Target

Tabel `cookies` pada Chromium / WebView2:
```sql
SELECT host_key, name, path, value, encrypted_value 
FROM cookies 
WHERE host_key LIKE '%apple.com%';
```

Jika `value` tidak kosong, gunakan nilai string langsung.  
Jika terenkripsi (`encrypted_value` dengan prefix `v10` / DPAPI), panggil Windows API `CryptUnprotectData` (via `windows-sys` atau helper ringan) untuk mendekripsi data byte ke string UTF-8.

---

## 4. Test Case Specification

### A. Positive Test Cases

#### `TEST-SES-POS-01: Read Cookies from Isolated SQLite Mock`
- **Prekondisi**: Buat SQLite database temporer dalam file dengan tabel `cookies` standar dan isi entri:
  - `host_key`: `".apple.com"`
  - `name`: `"media-user-token"`
  - `value`: `"test_mut_sample_token_12345"`
- **Aksi**: Jalankan `extract_from_cookies_db(&mock_db_path)`.
- **Ekspektasi**:
  - `result.is_ok()` bernilai `true`.
  - `session.media_user_token` bernilai `Some("test_mut_sample_token_12345")`.

#### `TEST-SES-POS-02: Immutable URI Mode Prevents Lock Failure`
- **Prekondisi**: Buka file database mock dengan koneksi SQLite biasa (memegang open handle).
- **Aksi**: Panggil `extract_from_cookies_db` pada file yang sama secara bersamaan.
- **Ekspektasi**: Berhasil membaca tanpa error `database is locked`.

#### `TEST-SES-POS-03: Multiple Apple Domain Filtering`
- **Prekondisi**: Database mock memiliki cookie untuk domain `.google.com`, `.apple.com`, dan `music.apple.com`.
- **Aksi**: Jalankan ekstraksi.
- **Ekspektasi**: Hanya cookie yang berakhir dengan `apple.com` yang diambil; cookie pihak ketiga diabaikan.

#### `TEST-SES-POS-04: Auto-detection from LocalState Directory`
- **Prekondisi**: Struktur folder mock dibuat sesuai hierarki `%LOCALAPPDATA%` resmi:
  `temp_root/EBWebView/Default/Network/Cookies`.
- **Aksi**: Panggil `extract_from_local_state(&temp_root)`.
- **Ekspektasi**: Menemukan file `Cookies` di sub-folder `Network/` secara otomatis.

---

### B. Negative Test Cases

#### `TEST-SES-NEG-01: Non-Existent SQLite Database File`
- **Prekondisi**: Path mengarah ke file fiktif `non_existent_cookies.sqlite`.
- **Aksi**: Panggil `extract_from_cookies_db()`.
- **Ekspektasi**: Mengembalikan `Err(SessionError::NotFound(_))`.

#### `TEST-SES-NEG-02: Corrupted SQLite Database File`
- **Prekondisi**: Buat file dummy berisi byte acak (bukan database SQLite valid).
- **Aksi**: Panggil `extract_from_cookies_db()`.
- **Ekspektasi**: Mengembalikan `Err(SessionError::Sqlite(_))`. Tidak boleh panic.

#### `TEST-SES-NEG-03: Empty Table or No Apple Cookies Found`
- **Prekondisi**: Database SQLite valid dengan tabel `cookies`, tetapi hanya berisi domain lain (misal `example.com`).
- **Aksi**: Panggil `extract_from_cookies_db()`.
- **Ekspektasi**: Mengembalikan `Err(SessionError::NoSessionFound)`.

#### `TEST-SES-NEG-04: Missing Cookies Table Schema`
- **Prekondisi**: Database SQLite valid tetapi tidak memiliki tabel `cookies` (misal skema tabel lain).
- **Aksi**: Panggil `extract_from_cookies_db()`.
- **Ekspektasi**: Mengembalikan `Err(SessionError::Sqlite(_))` dengan error deskriptif `no such table: cookies`.
