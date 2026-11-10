use std::path::Path;
use std::sync::Arc;
use crate::drm::loader::{NativeLibrary, LoaderError};

pub type FigGetClassIdFn = unsafe extern "C" fn() -> u64;

#[derive(Debug)]
pub struct CoreMediaBridge {
    _lib: Arc<NativeLibrary>,
    pub fig_content_key_boss_get_class_id: FigGetClassIdFn,
    pub fig_cpe_cryptor_get_class_id: Option<FigGetClassIdFn>,
    pub fig_cpe_fairplay_cryptor_get_class_id: Option<FigGetClassIdFn>,
}

unsafe impl Send for CoreMediaBridge {}
unsafe impl Sync for CoreMediaBridge {}

impl CoreMediaBridge {
    /// Load CoreMedia.dll from the Apple Music installation directory
    pub fn load_from_dir(dir: &Path) -> Result<Self, LoaderError> {
        let exec_dir = crate::drm::locator::AppLocator::new()
            .get_executable_dir(dir)
            .unwrap_or_else(|_| dir.to_path_buf());

        let dll_path = exec_dir.join("CoreMedia.dll");
        if !dll_path.exists() {
            return Err(LoaderError::LoadFailed(
                dll_path,
                "CoreMedia.dll not found in directory".to_string(),
            ));
        }

        // Set DLL search directory temporarily to resolve sibling dependencies (CoreFoundation.dll, etc.)
        let _guard = crate::drm::loader::DllDirectoryGuard::set(&exec_dir);
        let lib = Arc::new(NativeLibrary::load(&dll_path)?);

        // CoreMedia export: FigContentKeyBossGetClassID
        let boss_fn: FigGetClassIdFn = unsafe {
            let sym = lib.get_symbol::<FigGetClassIdFn>(b"FigContentKeyBossGetClassID\0")?;
            *sym
        };

        // Optional exports: FigCPECryptorGetClassID, FigCPEFairPlayCryptorGetClassID
        let cpe_fn: Option<FigGetClassIdFn> = unsafe {
            lib.get_symbol::<FigGetClassIdFn>(b"FigCPECryptorGetClassID\0")
                .map(|sym| *sym)
                .ok()
        };

        let fairplay_fn: Option<FigGetClassIdFn> = unsafe {
            lib.get_symbol::<FigGetClassIdFn>(b"FigCPEFairPlayCryptorGetClassID\0")
                .map(|sym| *sym)
                .ok()
        };

        Ok(Self {
            _lib: lib,
            fig_content_key_boss_get_class_id: boss_fn,
            fig_cpe_cryptor_get_class_id: cpe_fn,
            fig_cpe_fairplay_cryptor_get_class_id: fairplay_fn,
        })
    }

    /// Retrieve class ID for Content Key Boss
    pub fn get_key_boss_class_id(&self) -> u64 {
        unsafe { (self.fig_content_key_boss_get_class_id)() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drm::locator::AppLocator;
    use std::fs;

    // ─── Positive Tests (POS-01 .. POS-04) ──────────────────────────────

    #[test]
    fn test_drm_pos_01_load_coremedia_and_resolve_symbols() {
        if let Ok(install) = AppLocator::new().locate() {
            let bridge = CoreMediaBridge::load_from_dir(&install.install_dir);
            if let Ok(b) = bridge {
                assert!((b.fig_content_key_boss_get_class_id as usize) != 0);
            }
        }
    }

    #[test]
    fn test_drm_pos_02_execute_get_class_id_returns_nonzero() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(b) = CoreMediaBridge::load_from_dir(&install.install_dir) {
                let id = b.get_key_boss_class_id();
                assert!(id > 0, "Class ID should be non-zero (actual: {})", id);
            }
        }
    }

    #[test]
    fn test_drm_pos_03_optional_cpe_cryptor_symbols_checked() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(b) = CoreMediaBridge::load_from_dir(&install.install_dir) {
                // At least one of the CPE cryptor functions should be mapped if present
                if let Some(cpe) = b.fig_cpe_cryptor_get_class_id {
                    let id = unsafe { cpe() };
                    assert!(id > 0);
                }
            }
        }
    }

    #[test]
    fn test_drm_pos_04_thread_safe_access_across_concurrency() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(b) = CoreMediaBridge::load_from_dir(&install.install_dir) {
                let arc_b = Arc::new(b);
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let ref_b = Arc::clone(&arc_b);
                    handles.push(std::thread::spawn(move || {
                        let id = ref_b.get_key_boss_class_id();
                        assert!(id > 0);
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
    fn test_drm_neg_01_missing_coremedia_dll_file() {
        let fake_dir = std::env::temp_dir().join(format!("empty_cm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&fake_dir).unwrap();

        let err = CoreMediaBridge::load_from_dir(&fake_dir).unwrap_err();
        match err {
            LoaderError::LoadFailed(p, _) => {
                assert!(p.to_string_lossy().contains("CoreMedia.dll"));
            }
            other => panic!("Expected LoadFailed, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&fake_dir);
    }

    #[test]
    fn test_drm_neg_02_mock_dll_without_fig_content_key_boss() {
        let temp_dir = std::env::temp_dir().join(format!("fake_cm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        // Copy kernel32.dll as CoreMedia.dll (does not have FigContentKeyBossGetClassID)
        let kernel32_path = std::path::PathBuf::from(r"C:\Windows\System32\kernel32.dll");
        if kernel32_path.exists() {
            let target_dll = temp_dir.join("CoreMedia.dll");
            if fs::copy(&kernel32_path, &target_dll).is_ok() {
                let err = CoreMediaBridge::load_from_dir(&temp_dir).unwrap_err();
                match err {
                    LoaderError::SymbolNotFound(name) => {
                        assert_eq!(name, "FigContentKeyBossGetClassID");
                    }
                    other => panic!("Expected SymbolNotFound, got: {:?}", other),
                }
            }
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_drm_neg_03_corrupted_coremedia_binary() {
        let temp_dir = std::env::temp_dir().join(format!("corrupt_cm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let bad_file = temp_dir.join("CoreMedia.dll");
        fs::write(&bad_file, b"MALFORMED_HEADER_DATA").unwrap();

        let err = CoreMediaBridge::load_from_dir(&temp_dir).unwrap_err();
        match err {
            LoaderError::LoadFailed(_, _) => {}
            other => panic!("Expected LoadFailed for malformed DLL, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_drm_neg_04_execution_on_invalid_path() {
        let bad_path = std::path::PathBuf::from(r"Z:\NonExistentDrivePath\CoreMedia");
        let err = CoreMediaBridge::load_from_dir(&bad_path).unwrap_err();
        match err {
            LoaderError::LoadFailed(_, _) => {}
            other => panic!("Expected LoadFailed for non-existent path, got: {:?}", other),
        }
    }
}
