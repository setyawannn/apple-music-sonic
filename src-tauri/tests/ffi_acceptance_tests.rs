use app_lib::api::local_session::{LocalSessionReader, SessionError};
use app_lib::drm::{
    get_current_process_memory, AppLocator, CoreAudioBridge, CoreFpBridge, CoreMediaBridge,
    LoaderError, LocatorError, NativeLibrary,
};
use rusqlite::Connection;
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static TEST_MUTEX: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, val: &std::path::Path) -> Self {
        let original = std::env::var(key).ok();
        std::env::set_var(key, val);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(ref val) = self.original {
            std::env::set_var(self.key, val);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn create_mock_cookies_db(entries: &[(&str, &str, &str)]) -> PathBuf {
    let db_path = std::env::temp_dir().join(format!("mock_ffi_cookies_{}.sqlite", uuid::Uuid::new_v4()));
    let conn = Connection::open(&db_path).expect("open mock cookies db");

    conn.execute(
        "CREATE TABLE cookies (
            creation_utc INTEGER NOT NULL,
            host_key TEXT NOT NULL,
            top_frame_site_key TEXT NOT NULL DEFAULT '',
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            encrypted_value BLOB NOT NULL,
            path TEXT NOT NULL
        )",
        [],
    )
    .expect("create cookies table");

    for &(host, name, val) in entries {
        conn.execute(
            "INSERT INTO cookies (creation_utc, host_key, name, value, encrypted_value, path)
             VALUES (1000, ?1, ?2, ?3, X'', '/')",
            [host, name, val],
        )
        .expect("insert cookie");
    }

    db_path
}

// ═══════════════════════════════════════════════════════════════════════════
// POSITIVE ACCEPTANCE TESTS (FFI-POS-01 .. FFI-POS-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_ffi_pos_01_app_package_auto_detection() {
    let locator = AppLocator::new();
    let res = locator.locate();
    if let Ok(install) = res {
        assert!(install.install_dir.exists(), "Installation dir must exist");
        assert!(install.local_state_dir.exists(), "LocalState dir must exist");
        assert!(!install.version.is_empty(), "Version must not be empty");
        assert!(install.available_dlls.contains(&"CoreAudioToolbox.dll".to_string()));
        assert!(install.available_dlls.contains(&"CoreMedia.dll".to_string()));
        assert!(install.available_dlls.contains(&"CoreFP.dll".to_string()));
    }
}

#[test]
fn test_ffi_pos_02_sqlite_cookie_extraction_immutable() {
    let temp_db = create_mock_cookies_db(&[
        (".apple.com", "media-user-token", "mut_sample_token_xyz"),
        (".apple.com", "itctx", "sample_itctx_value"),
    ]);

    let reader = LocalSessionReader::new();
    let session = reader.extract_from_cookies_db(&temp_db).expect("read cookies");
    assert_eq!(session.media_user_token.as_deref(), Some("mut_sample_token_xyz"));
    assert_eq!(session.itctx.as_deref(), Some("sample_itctx_value"));

    let _ = fs::remove_file(&temp_db);
}

#[test]
fn test_ffi_pos_03_native_library_loader_and_memory() {
    let mem_before = get_current_process_memory().expect("get memory before");
    let lib = NativeLibrary::load("kernel32.dll").expect("load kernel32");
    let mem_after = get_current_process_memory().expect("get memory after");

    let sym: libloading::Symbol<unsafe extern "system" fn() -> u32> = unsafe {
        lib.get_symbol(b"GetCurrentProcessId\0").expect("get GetCurrentProcessId")
    };
    let pid = unsafe { sym() };
    assert_eq!(pid, std::process::id());
    assert!(mem_after.working_set_bytes >= mem_before.working_set_bytes);
}

#[test]
fn test_ffi_pos_04_coreaudio_alac_ordinal_resolution() {
    let locator = AppLocator::new();
    if let Ok(install) = locator.locate() {
        let bridge = CoreAudioBridge::load_from_dir(&install.install_dir).expect("load coreaudio");
        assert!(!bridge.alac_decoder_ptr.is_null());
        assert!(bridge.is_operational());
        let status = bridge.decoder_status();
        assert!(status.alac_ready);
        assert!(status.aac_ready);
        assert!(status.mp3_ready);
    }
}

#[test]
fn test_ffi_pos_05_coremedia_real_name_symbol_binding() {
    let locator = AppLocator::new();
    if let Ok(install) = locator.locate() {
        let media = CoreMediaBridge::load_from_dir(&install.install_dir).expect("load coremedia");
        assert!((media.fig_content_key_boss_get_class_id as usize) != 0);
    }
}

#[test]
fn test_ffi_pos_06_coremedia_get_class_id_execution() {
    let locator = AppLocator::new();
    if let Ok(install) = locator.locate() {
        let media = CoreMediaBridge::load_from_dir(&install.install_dir).expect("load coremedia");
        let class_id = media.get_key_boss_class_id();
        assert!(class_id > 0, "FigContentKeyBoss class ID must be non-zero");
    }
}

#[test]
fn test_ffi_pos_07_corefp_obfuscated_ordinals_mapping() {
    let locator = AppLocator::new();
    if let Ok(install) = locator.locate() {
        let fp = CoreFpBridge::load_from_dir(&install.install_dir).expect("load corefp");
        assert_eq!(fp.resolved_count(), 6);
        for (i, &ptr) in fp.fp_ordinals.iter().enumerate() {
            assert!(!ptr.is_null(), "CoreFP ordinal #{} must not be null", i + 1);
        }
    }
}

#[test]
fn test_ffi_pos_08_ram_budget_under_85mb() {
    let locator = AppLocator::new();
    if let Ok(install) = locator.locate() {
        let _ca = CoreAudioBridge::load_from_dir(&install.install_dir).ok();
        let _cm = CoreMediaBridge::load_from_dir(&install.install_dir).ok();
        let _fp = CoreFpBridge::load_from_dir(&install.install_dir).ok();

        let mem = get_current_process_memory().expect("get memory");
        let ram_mb = mem.working_set_bytes as f64 / (1024.0 * 1024.0);
        println!("Working Set RAM with Apple DLLs loaded: {:.2} MB", ram_mb);
        assert!(
            ram_mb < 85.0,
            "Process RAM must remain under 85 MB (actual: {:.2} MB)",
            ram_mb
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NEGATIVE ACCEPTANCE TESTS (FFI-NEG-01 .. FFI-NEG-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_ffi_neg_01_system_not_installed() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let fake_dir = std::env::temp_dir().join(format!("fake_apple_empty_{}", uuid::Uuid::new_v4()));
    let _g = EnvGuard::set("APPLE_MUSIC_INSTALL_DIR", &fake_dir);

    let locator = AppLocator::new();
    let err = locator.locate().unwrap_err();
    assert_eq!(err, LocatorError::NotInstalled);
}

#[test]
fn test_ffi_neg_02_missing_critical_dlls() {
    let temp_dir = std::env::temp_dir().join(format!("partial_apple_dlls_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    // Only create CoreAudioToolbox.dll, miss CoreMedia and CoreFP
    File::create(temp_dir.join("CoreAudioToolbox.dll")).unwrap();

    let locator = AppLocator::new();
    let err = locator.verify_dlls(&temp_dir).unwrap_err();
    match err {
        LocatorError::MissingDlls(missing) => {
            assert!(missing.contains(&"CoreMedia.dll".to_string()));
            assert!(missing.contains(&"CoreFP.dll".to_string()));
        }
        _ => panic!("Expected MissingDlls error"),
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_ffi_neg_03_sqlite_database_not_found() {
    let fake_path = PathBuf::from("non_existent_cookies_file_123.sqlite");
    let reader = LocalSessionReader::new();
    let err = reader.extract_from_cookies_db(&fake_path).unwrap_err();
    match err {
        SessionError::NotFound(p) => assert_eq!(p, fake_path),
        other => panic!("Expected NotFound, got: {:?}", other),
    }
}

#[test]
fn test_ffi_neg_04_sqlite_corrupted_payload() {
    let temp_file = std::env::temp_dir().join(format!("corrupt_cookies_{}.sqlite", uuid::Uuid::new_v4()));
    fs::write(&temp_file, b"THIS IS NOT A VALID SQLITE DATABASE HEADER").unwrap();

    let reader = LocalSessionReader::new();
    let err = reader.extract_from_cookies_db(&temp_file).unwrap_err();
    match err {
        SessionError::Sqlite(_) => {}
        other => panic!("Expected Sqlite error, got: {:?}", other),
    }

    let _ = fs::remove_file(&temp_file);
}

#[test]
fn test_ffi_neg_05_load_non_existent_dll() {
    let err = NativeLibrary::load("fake_library_does_not_exist_9876.dll").unwrap_err();
    match err {
        LoaderError::LoadFailed(_, _) => {}
        other => panic!("Expected LoadFailed, got: {:?}", other),
    }
}

#[test]
fn test_ffi_neg_06_ordinal_out_of_bounds() {
    let lib = NativeLibrary::load("kernel32.dll").expect("load kernel32");
    let err = unsafe { lib.get_ordinal::<()>(0xFFFF) }.unwrap_err();
    match err {
        LoaderError::OrdinalNotFound(0xFFFF) => {}
        other => panic!("Expected OrdinalNotFound, got: {:?}", other),
    }
}

#[test]
fn test_ffi_neg_07_concurrent_cookies_reads_without_lock_fail() {
    let temp_db = create_mock_cookies_db(&[(".apple.com", "itctx", "sample_ctx")]);
    let mut handles = Vec::new();

    for _ in 0..8 {
        let p = temp_db.clone();
        handles.push(std::thread::spawn(move || {
            let reader = LocalSessionReader::new();
            reader.extract_from_cookies_db(&p)
        }));
    }

    for h in handles {
        let res = h.join().unwrap();
        assert!(res.is_ok(), "Concurrent read must not fail with database lock");
    }

    let _ = fs::remove_file(&temp_db);
}

#[test]
fn test_ffi_neg_08_null_pointer_guard() {
    let bridge = CoreAudioBridge {
        _lib: Arc::new(NativeLibrary::load("kernel32.dll").unwrap()),
        alac_decoder_ptr: std::ptr::null(),
        aac_decoder_ptr: std::ptr::null(),
        mp3_decoder_ptr: std::ptr::null(),
    };
    assert!(!bridge.is_operational(), "Bridge with null pointer must not report operational");
    assert!(!bridge.decoder_status().alac_ready);
}
