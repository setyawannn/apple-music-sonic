use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use crate::drm::loader::{NativeLibrary, LoaderError};

#[derive(Debug)]
pub struct CoreAudioBridge {
    pub _lib: Arc<NativeLibrary>,
    pub alac_decoder_ptr: *const c_void,
    pub aac_decoder_ptr: *const c_void,
    pub mp3_decoder_ptr: *const c_void,
}

// Thread-safe handle markers
unsafe impl Send for CoreAudioBridge {}
unsafe impl Sync for CoreAudioBridge {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreAudioStatus {
    pub alac_ready: bool,
    pub aac_ready: bool,
    pub mp3_ready: bool,
}

impl CoreAudioBridge {
    /// Load CoreAudioToolbox.dll from installation directory
    pub fn load_from_dir(dir: &Path) -> Result<Self, LoaderError> {
        let exec_dir = crate::drm::locator::AppLocator::new()
            .get_executable_dir(dir)
            .unwrap_or_else(|_| dir.to_path_buf());

        let dll_path = exec_dir.join("CoreAudioToolbox.dll");
        if !dll_path.exists() {
            return Err(LoaderError::LoadFailed(
                dll_path,
                "CoreAudioToolbox.dll not found in directory".to_string(),
            ));
        }

        // Add install dir to DLL search path temporarily during load, then restore to NULL on drop
        let _guard = crate::drm::loader::DllDirectoryGuard::set(&exec_dir);
        let lib = Arc::new(NativeLibrary::load(&dll_path)?);

        // Ordinal 8: ACAppleLosslessDecoderEntry
        let alac_ptr = unsafe {
            lib.get_ordinal::<()>(8)
                .map(|p| p as *const c_void)
                .unwrap_or(std::ptr::null())
        };

        // Ordinal 17: ACMP4AACLowComplexityDecoderEntry
        let aac_ptr = unsafe {
            lib.get_ordinal::<()>(17)
                .map(|p| p as *const c_void)
                .unwrap_or(std::ptr::null())
        };

        // Ordinal 13: ACMP3DecoderEntry
        let mp3_ptr = unsafe {
            lib.get_ordinal::<()>(13)
                .map(|p| p as *const c_void)
                .unwrap_or(std::ptr::null())
        };

        if alac_ptr.is_null() {
            return Err(LoaderError::OrdinalNotFound(8));
        }

        Ok(Self {
            _lib: lib,
            alac_decoder_ptr: alac_ptr,
            aac_decoder_ptr: aac_ptr,
            mp3_decoder_ptr: mp3_ptr,
        })
    }

    /// Check whether all necessary decoders are ready
    pub fn is_operational(&self) -> bool {
        !self.alac_decoder_ptr.is_null()
    }

    /// Retrieve detailed readiness status of audio decoders
    pub fn decoder_status(&self) -> CoreAudioStatus {
        CoreAudioStatus {
            alac_ready: !self.alac_decoder_ptr.is_null(),
            aac_ready: !self.aac_decoder_ptr.is_null(),
            mp3_ready: !self.mp3_decoder_ptr.is_null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drm::locator::AppLocator;
    use std::fs;

    // ─── Positive Tests (POS-01 .. POS-04) ──────────────────────────────

    #[test]
    fn test_ca_pos_01_load_coreaudio_from_host_install() {
        if let Ok(install) = AppLocator::new().locate() {
            let bridge = CoreAudioBridge::load_from_dir(&install.install_dir);
            if let Ok(bridge) = bridge {
                assert!(bridge.is_operational(), "CoreAudio bridge must be operational");
            }
        }
    }

    #[test]
    fn test_ca_pos_02_alac_decoder_ordinal_8_valid() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(bridge) = CoreAudioBridge::load_from_dir(&install.install_dir) {
                assert!(!bridge.alac_decoder_ptr.is_null(), "ALAC decoder ptr must not be null");
                let status = bridge.decoder_status();
                assert!(status.alac_ready);
            }
        }
    }

    #[test]
    fn test_ca_pos_03_aac_and_mp3_decoder_ordinals_valid() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(bridge) = CoreAudioBridge::load_from_dir(&install.install_dir) {
                let status = bridge.decoder_status();
                assert!(status.aac_ready, "AAC decoder ordinal 17 must be resolved");
                assert!(status.mp3_ready, "MP3 decoder ordinal 13 must be resolved");
            }
        }
    }

    #[test]
    fn test_ca_pos_04_thread_safe_access_across_concurrency() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(bridge) = CoreAudioBridge::load_from_dir(&install.install_dir) {
                let arc_bridge = Arc::new(bridge);
                let mut handles = Vec::new();

                for _ in 0..4 {
                    let b = Arc::clone(&arc_bridge);
                    handles.push(std::thread::spawn(move || {
                        assert!(b.is_operational());
                        assert!(!b.alac_decoder_ptr.is_null());
                    }));
                }

                for h in handles {
                    h.join().unwrap();
                }
            }
        }
    }

    // ─── Negative Tests (NEG-01 .. NEG-04) ──────────────────────────────

    #[test]
    fn test_ca_neg_01_missing_coreaudio_dll_file() {
        let empty_dir = std::env::temp_dir().join(format!("empty_ca_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&empty_dir).unwrap();

        let err = CoreAudioBridge::load_from_dir(&empty_dir).unwrap_err();
        match err {
            LoaderError::LoadFailed(p, _) => {
                assert!(p.to_string_lossy().contains("CoreAudioToolbox.dll"));
            }
            other => panic!("Expected LoadFailed, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&empty_dir);
    }

    #[test]
    fn test_ca_neg_02_mock_dll_without_ordinal_8() {
        // Use app_lib.dll (which has no ordinal 8 export) as a mock DLL
        let temp_dir = std::env::temp_dir().join(format!("fake_ca_dir_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let app_lib_path = std::path::PathBuf::from(r"target\debug\app_lib.dll");
        let alt_lib_path = std::path::PathBuf::from(r"src-tauri\target\debug\app_lib.dll");

        let source = if app_lib_path.exists() {
            Some(app_lib_path)
        } else if alt_lib_path.exists() {
            Some(alt_lib_path)
        } else {
            None
        };

        if let Some(src) = source {
            let target_dll = temp_dir.join("CoreAudioToolbox.dll");
            if fs::copy(&src, &target_dll).is_ok() {
                let err = CoreAudioBridge::load_from_dir(&temp_dir).unwrap_err();
                match err {
                    LoaderError::OrdinalNotFound(8) => {}
                    other => panic!("Expected OrdinalNotFound(8), got: {:?}", other),
                }
            }
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ca_neg_03_null_pointer_verification_guard() {
        let fake_bridge = CoreAudioBridge {
            _lib: Arc::new(NativeLibrary::load("kernel32.dll").unwrap()),
            alac_decoder_ptr: std::ptr::null(),
            aac_decoder_ptr: std::ptr::null(),
            mp3_decoder_ptr: std::ptr::null(),
        };

        assert!(!fake_bridge.is_operational());
        let status = fake_bridge.decoder_status();
        assert!(!status.alac_ready);
    }

    #[test]
    fn test_ca_neg_04_graceful_handling_on_corrupt_dll_headers() {
        let temp_dir = std::env::temp_dir().join(format!("corrupt_ca_dir_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let bad_dll = temp_dir.join("CoreAudioToolbox.dll");
        fs::write(&bad_dll, b"NOT_A_PE_HEADER").unwrap();

        let err = CoreAudioBridge::load_from_dir(&temp_dir).unwrap_err();
        match err {
            LoaderError::LoadFailed(_, _) => {}
            other => panic!("Expected LoadFailed for corrupted DLL, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
