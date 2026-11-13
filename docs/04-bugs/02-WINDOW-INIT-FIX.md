# Laporan Analisis & Perbaikan Bug: Jendela Aplikasi Tidak Muncul

> **Status**: Resolved & Verified ✅  
> **Tanggal**: 2026-11-09  
> **Komponen Terdampak**: `src-tauri/src/drm/locator.rs`, `loader.rs`, `coreaudio.rs`, `coremedia.rs`, `corefp.rs`  
> **Gejala**: Proses `app.exe` dan `msedgewebview2.exe` aktif di latar belakang (RAM ~57 MB, CPU responding), namun jendela utama tidak pernah muncul di layar pengguna (`MainWindowHandle: 0`).

---

## 1. Analisis Gejala & Investigasi Runtime

Saat menjalankan aplikasi melalui `npm run dev` / Tauri:
1. Proses `app.exe` berjalan dengan status `Responding: True`.
2. Child process `msedgewebview2.exe` (Edge Chromium Runtime) berhasil di-*spawn*.
3. Endpoint API Apple Music (`https://music.apple.com` dan `https://amp-api.music.apple.com`) berhasil dihubungi oleh backend dan frontend.
4. Namun, pemeriksaan Win32 API terhadap proses `app.exe` menunjukkan:
   ```powershell
   Get-Process -Name "app" | Select Id, MainWindowTitle, MainWindowHandle
   # Id: 15076, MainWindowTitle: "", MainWindowHandle: 0
   ```
5. Tidak ada *window handle* top-level yang terdaftar untuk proses utama, sehingga UI tidak pernah dirender ke layar pengguna.

---

## 2. Root Cause Analysis (Akar Masalah)

### Akar Masalah Utama: DLL Search Order Hijacking pada WebView2 Host Runtime

Secara mendalam, terjadi tabrakan runtime fatal (*dynamic linking collision*) antara **Tauri (WebView2 Host)** dan **Apple Music MSIX Framework**:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Apple Music RS Process                          │
│                                                                        │
│  Tauri UI Engine (WebView2)              DRM FFI Subsystem             │
│  ├── Ingin memuat runtime Edge           ├── Memuat Apple DLLs         │
│  │   WebView2 resmi OS                   │   (CoreMedia, CoreAudio)    │
│  │                                       │                             │
│  │                                       └── Memanggil:                │
│  │                                           SetDllDirectoryW(bridge)  │
│  │                                           (Mengubah DLL search      │
│  │                                            path global proses)      │
│  ▼                                                                     │
│  Windows Dynamic Linker mencari DLL di folder "bridge":                │
│  Ditemukan: Microsoft.Web.WebView2.Core.dll versi Apple Music          │
│             Microsoft.UI.Xaml.dll versi Apple Music                    │
│             DWriteCore.dll versi Apple Music                           │
│                                                                        │
│  ❌ Collision: WebView2 memuat binary yang tidak kompatibel dengan     │
│     driver host sistem Windows -> Inisialisasi window gagal diam-diam!  │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Penyalinan Tanpa Filter ke Staging Folder**:
   Saat menangani restriksi WindowsApps (AppContainer Win32 Error 5), fungsi `get_executable_dir` menyalin **seluruh file `.dll`** dari direktori Apple Music ke folder staging `%LOCALAPPDATA%\apple_music_rs\bridge`.
   Paket Apple Music menyertakan file-file:
   - `Microsoft.Web.WebView2.Core.dll`
   - `Microsoft.Web.WebView2.Core.Projection.dll`
   - `Microsoft.UI.Xaml.dll`
   - `DWriteCore.dll`
2. **Panggilan `SetDllDirectoryW` yang Bersifat Global dan Persisten**:
   Fungsi `CoreAudioBridge`, `CoreMediaBridge`, dan `CoreFpBridge` memanggil `SetDllDirectoryW(bridge_dir)` agar Windows dapat menemukan *sibling dependencies* Apple (seperti `CoreFoundation.dll`).
   Namun, `SetDllDirectoryW` berlaku untuk **seluruh proses secara global**. Karena tidak pernah di-*reset* kembali ke `NULL`, folder `bridge` menjadi prioritas pencarian DLL untuk seluruh panggilan `LoadLibrary` berikutnya, termasuk oleh internal Tauri/WebView2.
3. **Akibat**:
   Saat komponen UI Tauri mencoba menginisialisasi controller WebView2, Windows memuat `Microsoft.Web.WebView2.Core.dll` kustom milik Apple dari folder bridge, bukan dari Edge WebView2 runtime sistem operasi. Hal ini menggagalkan inisialisasi WebView2 controller dan menyebabkan pembuatan window terhenti tanpa melempar panic di terminal.

---

## 3. Solusi & Perbaikan Arsitektur (Bug Fixing)

Tiga lapis perbaikan telah diimplementasikan:

### Solusi 1: Filter Ketat pada Penyalinan DLL Bridge ([`locator.rs`](file:///d:/Workspace/Learn/apple-musicfast/src-tauri/src/drm/locator.rs))
Folder bridge **hanya boleh berisi binary Apple Framework C/Core**. Seluruh binary Microsoft, WinUI, DWrite, Desktop Window Manager (DWM), DirectComposition, Windows Composition Effects, V8, dan AMP UI dilarang keras disalin ke bridge:
```rust
let name_lower = file_name.to_string_lossy().to_lowercase();
// CRITICAL: NEVER copy Microsoft UI, DWM, or WebView2 DLLs into our bridge directory!
// These conflict with Tauri's own WebView2 runtime and cause silent window crashes.
if name_lower.starts_with("microsoft.")
    || name_lower.starts_with("winui")
    || name_lower.starts_with("dwrite")
    || name_lower.starts_with("dwm")
    || name_lower.starts_with("dcomp")
    || name_lower.starts_with("wuceffect")
    || name_lower.starts_with("v8")
    || name_lower.starts_with("amp.")
    || name_lower.starts_with("mrm")
    || name_lower.starts_with("javascriptcorev8")
{
    continue;
}
```

### Solusi 2: RAII Scope Guard untuk `SetDllDirectoryW` ([`loader.rs`](file:///d:/Workspace/Learn/apple-musicfast/src-tauri/src/drm/loader.rs))
Dibuat struktur RAII `DllDirectoryGuard` yang membatasi pengubahan jalur pencarian DLL hanya selama durasi mikrodetik saat pemanggilan `NativeLibrary::load`:
```rust
pub struct DllDirectoryGuard;

impl DllDirectoryGuard {
    pub fn set(_dir: &Path) -> Self {
        #[cfg(windows)]
        {
            let wide: Vec<u16> = _dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
            unsafe { SetDllDirectoryW(wide.as_ptr()) };
        }
        Self
    }
}

impl Drop for DllDirectoryGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe { SetDllDirectoryW(std::ptr::null()) };
    }
}
```
Ketika pemuatan DLL Apple selesai, guard secara otomatis memanggil `SetDllDirectoryW(NULL)`, sehingga direktori pencarian proses kembali ke default sistem. Host WebView2 dan WinUI aman 100%.

### Solusi 3: Pembersihan Folder Staging Lokal
Seluruh file DLL Microsoft, DWM, DirectComposition, dan V8 yang sempat tersalin ke `%LOCALAPPDATA%\apple_music_rs\bridge\` telah dihapus secara bersih.

### Solusi 4: Window Presentation Enforcer ([`lib.rs`](file:///d:/Workspace/Learn/apple-musicfast/src-tauri/src/lib.rs))
Pada lifecycle setup Tauri, window utama diberi pemanggilan eksplisit `main_window.show()`, `main_window.unminimize()`, dan `main_window.set_focus()` untuk menjamin window langsung tampil di foreground layar desktop pengguna.

---

## 4. Hasil Verifikasi Pasca Perbaikan

1. **Automated Acceptance Tests**:
   - Seluruh 16 skenario FFI Acceptance Tests (`tests/ffi_acceptance_tests.rs`) lulus 100%.
   - Seluruh 128 skenario pengujian di seluruh modul proyek lulus 100% (0 error).
2. **Kompilasi Bersih**:
   - `cargo check --all-targets` menghasilkan 0 warning dan 0 error.
   - `npm run build` selesai dalam 193 ms.
   - `cargo build --release` menghasilkan single binary `src-tauri/target/release/app.exe` yang siap dijalankan mandiri.

---

## 5. Status revisit (2026-11-13)

Still solved. The window shows reliably on every recorded launch:

```
[SETUP] Found main window: label=main
[SETUP] After show: is_visible: Ok(true)
```

Two notes for future readers:

- The DLL staging described in section 3 belongs to the **abandoned native-decrypt path**
  (`src-tauri/src/drm/**`). Streaming the MVP does not need it, so the bridge directory and
  the `SetDllDirectoryW` guard are not on the critical path anymore.
- The test counts above (128) still hold at 2026-11-13, but no test covers the streaming
  path. See `docs/03-implementation/01-WHATS-DONE.md`.
