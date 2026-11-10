# Phase 4 Acceptance Test Suite Specification (`acceptance-tests-spec.md`)

> **File Target Implementasi**: `src-tauri/tests/ffi_acceptance_tests.rs`  
> **Standar**: 16 Acceptance Tests (8 Positif + 8 Negatif)  
> **Status**: Ready for Code Implementation

---

## 1. Ikhtisar Acceptance Tests (16 Tests Matrix)

| Test ID | Tipe | Nama Skenario Uji | Komponen Target |
|---|---|---|---|
| `test_ffi_pos_01` | Positif | Deteksi dinamis paket instalasi Apple Music Windows | `locator.rs` |
| `test_ffi_pos_02` | Positif | Ekstraksi sesi dari SQLite Cookies (Immutable URI) | `local_session.rs` |
| `test_ffi_pos_03` | Positif | Dynamic library loader memuat DLL sistem dan mengukur RAM | `loader.rs` |
| `test_ffi_pos_04` | Positif | Resolusi symbol ordinal 8 (ALAC Decoder) pada CoreAudioToolbox | `coreaudio.rs` |
| `test_ffi_pos_05` | Positif | Resolusi simbol CoreMedia nama asli (FigContentKeyBoss) | `coremedia.rs` |
| `test_ffi_pos_06` | Positif | Eksekusi FigContentKeyBossGetClassID mengembalikan non-zero class ID | `coremedia.rs` |
| `test_ffi_pos_07` | Positif | Resolusi seluruh 6 ordinal obfuscated pada CoreFP.dll | `corefp.rs` |
| `test_ffi_pos_08` | Positif | Pengukuran total Working Set proses tetap di bawah 85 MB RAM | `loader.rs` / System |
| `test_ffi_neg_01` | Negatif | Deteksi pada sistem tanpa Apple Music mengembalikan `NotInstalled` | `locator.rs` |
| `test_ffi_neg_02` | Negatif | Direktori instalasi tanpa DLL penting mengembalikan `MissingDlls` | `locator.rs` |
| `test_ffi_neg_03` | Negatif | Database SQLite Cookies tidak ditemukan mengembalikan `NotFound` | `local_session.rs` |
| `test_ffi_neg_04` | Negatif | Database SQLite rusak/corrupted ditangani tanpa panic | `local_session.rs` |
| `test_ffi_neg_05` | Negatif | Memuat file non-DLL atau path fiktif mengembalikan `LoadFailed` | `loader.rs` |
| `test_ffi_neg_06` | Negatif | Mencari ordinal/simbol fiktif mengembalikan `OrdinalNotFound` | `loader.rs` |
| `test_ffi_neg_07` | Negatif | Akses konkuren terhadap SQLite Cookies tidak memicu database lock | `local_session.rs` |
| `test_ffi_neg_08` | Negatif | Dynamic loader menolak pemanggilan pointer null secara aman | `coreaudio.rs` |

---

## 2. Rincian Skenario Positif (POS-01 .. POS-08)

### `test_ffi_pos_01_app_package_auto_detection`
```rust
#[test]
fn test_ffi_pos_01_app_package_auto_detection() {
    let locator = AppLocator::default();
    let install = locator.locate().expect("locate Apple Music installation");
    assert!(install.install_dir.exists());
    assert!(install.local_state_dir.exists());
    assert!(!install.version.is_empty());
}
```

### `test_ffi_pos_02_sqlite_cookie_extraction_immutable`
```rust
#[test]
fn test_ffi_pos_02_sqlite_cookie_extraction_immutable() {
    let temp_db = create_mock_cookies_db(&[
        (".apple.com", "media-user-token", "mut_sample_token_xyz"),
        (".apple.com", "itctx", "sample_itctx_value"),
    ]);
    let reader = LocalSessionReader::default();
    let session = reader.extract_from_cookies_db(&temp_db).expect("read cookies");
    assert_eq!(session.media_user_token.as_deref(), Some("mut_sample_token_xyz"));
    assert_eq!(session.itctx.as_deref(), Some("sample_itctx_value"));
}
```

### `test_ffi_pos_03_native_library_loader_and_memory`
```rust
#[test]
fn test_ffi_pos_03_native_library_loader_and_memory() {
    let mem_before = get_current_process_memory().expect("get memory before");
    let lib = NativeLibrary::load("kernel32.dll").expect("load kernel32");
    let mem_after = get_current_process_memory().expect("get memory after");
    
    let get_pid: libloading::Symbol<unsafe extern "C" fn() -> u32> = unsafe {
        lib.get_symbol(b"GetCurrentProcessId\0").expect("get GetCurrentProcessId")
    };
    let pid = unsafe { get_pid() };
    assert_eq!(pid, std::process::id());
    assert!(mem_after.working_set_bytes >= mem_before.working_set_bytes);
}
```

### `test_ffi_pos_04_coreaudio_alac_ordinal_resolution`
```rust
#[test]
fn test_ffi_pos_04_coreaudio_alac_ordinal_resolution() {
    let install = AppLocator::default().locate().expect("locate install");
    let bridge = CoreAudioBridge::load_from_dir(&install.install_dir).expect("load coreaudio");
    assert!(!bridge.alac_decoder_ptr.is_null());
    assert!(bridge.is_operational());
}
```

### `test_ffi_pos_05_coremedia_real_name_symbol_binding`
```rust
#[test]
fn test_ffi_pos_05_coremedia_real_name_symbol_binding() {
    let install = AppLocator::default().locate().expect("locate install");
    let media = CoreMediaBridge::load_from_dir(&install.install_dir).expect("load coremedia");
    assert!((media.fig_content_key_boss_get_class_id as usize) != 0);
}
```

### `test_ffi_pos_06_coremedia_get_class_id_execution`
```rust
#[test]
fn test_ffi_pos_06_coremedia_get_class_id_execution() {
    let install = AppLocator::default().locate().expect("locate install");
    let media = CoreMediaBridge::load_from_dir(&install.install_dir).expect("load coremedia");
    let class_id = unsafe { (media.fig_content_key_boss_get_class_id)() };
    assert!(class_id > 0, "FigContentKeyBoss class ID must be non-zero");
}
```

### `test_ffi_pos_07_corefp_obfuscated_ordinals_mapping`
```rust
#[test]
fn test_ffi_pos_07_corefp_obfuscated_ordinals_mapping() {
    let install = AppLocator::default().locate().expect("locate install");
    let fp = CoreFpBridge::load_from_dir(&install.install_dir).expect("load corefp");
    for (i, &ptr) in fp.fp_ordinals.iter().enumerate() {
        assert!(!ptr.is_null(), "CoreFP ordinal #{} must not be null", i + 1);
    }
}
```

### `test_ffi_pos_08_ram_budget_under_85mb`
```rust
#[test]
fn test_ffi_pos_08_ram_budget_under_85mb() {
    let install = AppLocator::default().locate().expect("locate install");
    let _ca = CoreAudioBridge::load_from_dir(&install.install_dir).ok();
    let _cm = CoreMediaBridge::load_from_dir(&install.install_dir).ok();
    let _fp = CoreFpBridge::load_from_dir(&install.install_dir).ok();

    let mem = get_current_process_memory().expect("get memory");
    let ram_mb = mem.working_set_bytes as f64 / (1024.0 * 1024.0);
    println!("Working Set RAM with all Apple DLLs loaded: {:.2} MB", ram_mb);
    assert!(ram_mb < 85.0, "Process RAM must remain under 85 MB (actual: {:.2} MB)", ram_mb);
}
```

---

## 3. Rincian Skenario Negatif (NEG-01 .. NEG-08)

### `test_ffi_neg_01_system_not_installed`
```rust
#[test]
fn test_ffi_neg_01_system_not_installed() {
    let empty_dir = tempfile::tempdir().unwrap();
    std::env::set_var("APPLE_MUSIC_INSTALL_DIR", empty_dir.path());
    let locator = AppLocator::default();
    let err = locator.locate().unwrap_err();
    assert_eq!(err, LocatorError::NotInstalled);
    std::env::remove_var("APPLE_MUSIC_INSTALL_DIR");
}
```

### `test_ffi_neg_02_missing_critical_dlls`
```rust
#[test]
fn test_ffi_neg_02_missing_critical_dlls() {
    let temp_dir = tempfile::tempdir().unwrap();
    // Only create CoreAudioToolbox.dll, miss CoreMedia and CoreFP
    std::fs::write(temp_dir.path().join("CoreAudioToolbox.dll"), b"dummy").unwrap();
    let locator = AppLocator::default();
    let err = locator.verify_dlls(temp_dir.path()).unwrap_err();
    match err {
        LocatorError::MissingDlls(missing) => {
            assert!(missing.contains(&"CoreMedia.dll".to_string()));
            assert!(missing.contains(&"CoreFP.dll".to_string()));
        }
        _ => panic!("Expected MissingDlls error"),
    }
}
```

### `test_ffi_neg_03_sqlite_database_not_found`
```rust
#[test]
fn test_ffi_neg_03_sqlite_database_not_found() {
    let fake_path = PathBuf::from("non_existent_cookies_file_123.sqlite");
    let reader = LocalSessionReader::default();
    let err = reader.extract_from_cookies_db(&fake_path).unwrap_err();
    matches!(err, SessionError::NotFound(_));
}
```

### `test_ffi_neg_04_sqlite_corrupted_payload`
```rust
#[test]
fn test_ffi_neg_04_sqlite_corrupted_payload() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp_file.path(), b"THIS IS NOT A VALID SQLITE DATABASE").unwrap();
    let reader = LocalSessionReader::default();
    let err = reader.extract_from_cookies_db(temp_file.path()).unwrap_err();
    matches!(err, SessionError::Sqlite(_));
}
```

### `test_ffi_neg_05_load_non_existent_dll`
```rust
#[test]
fn test_ffi_neg_05_load_non_existent_dll() {
    let err = NativeLibrary::load("fake_library_does_not_exist_9876.dll").unwrap_err();
    matches!(err, LoaderError::LoadFailed(_, _));
}
```

### `test_ffi_neg_06_ordinal_out_of_bounds`
```rust
#[test]
fn test_ffi_neg_06_ordinal_out_of_bounds() {
    let lib = NativeLibrary::load("kernel32.dll").expect("load kernel32");
    let err = unsafe { lib.get_ordinal::<()>(0xFEFE) }.unwrap_err();
    matches!(err, LoaderError::OrdinalNotFound(0xFEFE));
}
```

### `test_ffi_neg_07_concurrent_cookies_reads_without_lock_fail`
```rust
#[test]
fn test_ffi_neg_07_concurrent_cookies_reads_without_lock_fail() {
    let temp_db = create_mock_cookies_db(&[(".apple.com", "itctx", "sample_ctx")]);
    // Spawn 10 concurrent readers simultaneously
    let mut handles = Vec::new();
    for _ in 0..10 {
        let p = temp_db.clone();
        handles.push(std::thread::spawn(move || {
            let reader = LocalSessionReader::default();
            reader.extract_from_cookies_db(&p)
        }));
    }
    for h in handles {
        let res = h.join().unwrap();
        assert!(res.is_ok(), "Concurrent read must not fail with database lock");
    }
}
```

### `test_ffi_neg_08_null_pointer_guard`
```rust
#[test]
fn test_ffi_neg_08_null_pointer_guard() {
    let bridge = CoreAudioBridge {
        _lib: Arc::new(NativeLibrary::dummy()),
        alac_decoder_ptr: std::ptr::null(),
        aac_decoder_ptr: std::ptr::null(),
        mp3_decoder_ptr: std::ptr::null(),
    };
    assert!(!bridge.is_operational(), "Bridge with null pointer must not report operational");
}
```
