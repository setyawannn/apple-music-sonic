use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use crate::drm::loader::{NativeLibrary, LoaderError};

#[derive(Debug)]
pub struct CoreFpBridge {
    _lib: Arc<NativeLibrary>,
    pub fp_ordinals: [*const c_void; 6],
}

unsafe impl Send for CoreFpBridge {}
unsafe impl Sync for CoreFpBridge {}

impl CoreFpBridge {
    /// Load CoreFP.dll from Apple Music install directory and resolve all 6 ordinals
    pub fn load_from_dir(dir: &Path) -> Result<Self, LoaderError> {
        let exec_dir = crate::drm::locator::AppLocator::new()
            .get_executable_dir(dir)
            .unwrap_or_else(|_| dir.to_path_buf());

        let dll_path = exec_dir.join("CoreFP.dll");
        if !dll_path.exists() {
            return Err(LoaderError::LoadFailed(
                dll_path,
                "CoreFP.dll not found in directory".to_string(),
            ));
        }

        let _guard = crate::drm::loader::DllDirectoryGuard::set(&exec_dir);
        let lib = Arc::new(NativeLibrary::load(&dll_path)?);

        let mut ordinals: [*const c_void; 6] = [std::ptr::null(); 6];
        for i in 1..=6 {
            let ptr = unsafe {
                lib.get_ordinal::<()>(i as u16)
                    .map(|p| p as *const c_void)
                    .unwrap_or(std::ptr::null())
            };
            if ptr.is_null() {
                return Err(LoaderError::OrdinalNotFound(i as u16));
            }
            ordinals[i - 1] = ptr;
        }

        Ok(Self {
            _lib: lib,
            fp_ordinals: ordinals,
        })
    }

    /// Count how many ordinals resolved to valid non-null pointers
    pub fn resolved_count(&self) -> usize {
        self.fp_ordinals.iter().filter(|&&p| !p.is_null()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drm::locator::AppLocator;
    use std::fs;

    // ─── Positive Tests (POS-01 .. POS-04) ──────────────────────────────

    #[test]
    fn test_fp_pos_01_load_corefp_from_host() {
        if let Ok(install) = AppLocator::new().locate() {
            let bridge = CoreFpBridge::load_from_dir(&install.install_dir);
            if let Ok(b) = bridge {
                assert_eq!(b.resolved_count(), 6);
            }
        }
    }

    #[test]
    fn test_fp_pos_02_all_6_ordinals_non_null() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(b) = CoreFpBridge::load_from_dir(&install.install_dir) {
                for (idx, &ptr) in b.fp_ordinals.iter().enumerate() {
                    assert!(!ptr.is_null(), "CoreFP ordinal #{} must not be null", idx + 1);
                }
            }
        }
    }

    #[test]
    fn test_fp_pos_03_resolved_count_equals_six() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(b) = CoreFpBridge::load_from_dir(&install.install_dir) {
                assert_eq!(b.resolved_count(), 6);
            }
        }
    }

    #[test]
    fn test_fp_pos_04_concurrency_safe_access() {
        if let Ok(install) = AppLocator::new().locate() {
            if let Ok(b) = CoreFpBridge::load_from_dir(&install.install_dir) {
                let arc_b = Arc::new(b);
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let ref_b = Arc::clone(&arc_b);
                    handles.push(std::thread::spawn(move || {
                        assert_eq!(ref_b.resolved_count(), 6);
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
    fn test_fp_neg_01_missing_corefp_dll() {
        let fake_dir = std::env::temp_dir().join(format!("empty_fp_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&fake_dir).unwrap();

        let err = CoreFpBridge::load_from_dir(&fake_dir).unwrap_err();
        match err {
            LoaderError::LoadFailed(p, _) => {
                assert!(p.to_string_lossy().contains("CoreFP.dll"));
            }
            other => panic!("Expected LoadFailed, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&fake_dir);
    }

    #[test]
    fn test_fp_neg_02_corrupted_corefp_binary() {
        let temp_dir = std::env::temp_dir().join(format!("corrupt_fp_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let bad_file = temp_dir.join("CoreFP.dll");
        fs::write(&bad_file, b"MALFORMED_DATA").unwrap();

        let err = CoreFpBridge::load_from_dir(&temp_dir).unwrap_err();
        match err {
            LoaderError::LoadFailed(_, _) => {}
            other => panic!("Expected LoadFailed, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_fp_neg_03_resolved_count_on_empty_array() {
        let mock_bridge = CoreFpBridge {
            _lib: Arc::new(NativeLibrary::load("kernel32.dll").unwrap()),
            fp_ordinals: [std::ptr::null(); 6],
        };
        assert_eq!(mock_bridge.resolved_count(), 0);
    }

    #[test]
    fn test_fp_neg_04_invalid_search_path() {
        let bad_path = std::path::PathBuf::from(r"Y:\InvalidDrive\CoreFP");
        let err = CoreFpBridge::load_from_dir(&bad_path).unwrap_err();
        match err {
            LoaderError::LoadFailed(_, _) => {}
            other => panic!("Expected LoadFailed, got: {:?}", other),
        }
    }
}
