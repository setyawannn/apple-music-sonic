use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
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
    pub install_dir: PathBuf,
    pub local_state_dir: PathBuf,
    pub version: String,
    pub available_dlls: Vec<String>,
}

pub const CRITICAL_DLLS: &[&str] = &[
    "CoreAudioToolbox.dll",
    "CoreMedia.dll",
    "CoreFP.dll",
];

pub const OPTIONAL_DLLS: &[&str] = &[
    "AuthKitWin.dll",
    "CoreKE.dll",
    "CoreLSKD.dll",
    "CFNetwork.dll",
];

#[derive(Debug, Clone, Default)]
pub struct AppLocator;

impl AppLocator {
    pub fn new() -> Self {
        Self
    }

    /// Locate Apple Music installation directory and LocalState directory
    pub fn locate(&self) -> Result<AppleInstallation, LocatorError> {
        let install_dir = self.find_install_dir()?;
        let local_state_dir = self.find_local_state_dir().ok_or_else(|| {
            if let Ok(dir) = std::env::var("APPLE_MUSIC_LOCAL_STATE_DIR") {
                return LocatorError::LocalStateNotFound(PathBuf::from(dir));
            }
            if let Ok(dir) = std::env::var("APPLE_MUSIC_DATA_DIR") {
                return LocatorError::LocalStateNotFound(PathBuf::from(dir));
            }
            let fallback = match std::env::var("LOCALAPPDATA") {
                Ok(appdata) => PathBuf::from(appdata)
                    .join("Packages")
                    .join("AppleInc.AppleMusicWin_nzyj5cx40ttqa")
                    .join("LocalState"),
                Err(_) => PathBuf::from("LocalState"),
            };
            LocatorError::LocalStateNotFound(fallback)
        })?;

        let available_dlls = self.verify_dlls(&install_dir)?;
        let version = Self::parse_version_from_path(&install_dir)
            .unwrap_or_else(|| "unknown".to_string());

        Ok(AppleInstallation {
            install_dir,
            local_state_dir,
            version,
            available_dlls,
        })
    }

    /// Verify that critical Apple DLLs exist in the specified directory
    pub fn verify_dlls(&self, install_dir: &Path) -> Result<Vec<String>, LocatorError> {
        if !install_dir.exists() {
            return Err(LocatorError::NotInstalled);
        }

        let mut available = Vec::new();
        let mut missing_critical = Vec::new();

        for &dll in CRITICAL_DLLS {
            if install_dir.join(dll).exists() {
                available.push(dll.to_string());
            } else {
                missing_critical.push(dll.to_string());
            }
        }

        if !missing_critical.is_empty() {
            return Err(LocatorError::MissingDlls(missing_critical));
        }

        for &dll in OPTIONAL_DLLS {
            if install_dir.join(dll).exists() {
                available.push(dll.to_string());
            }
        }

        Ok(available)
    }

    /// Ensure Apple DLLs are in an executable location.
    /// When located in WindowsApps, Win32 desktop apps face AppContainer execution restriction (Win32 Error 5).
    /// This stages the required DLLs to %LOCALAPPDATA%\apple_music_rs\bridge for safe FFI loading.
    pub fn get_executable_dir(&self, install_dir: &Path) -> Result<PathBuf, LocatorError> {
        let path_str = install_dir.to_string_lossy();
        if !path_str.contains("WindowsApps") {
            return Ok(install_dir.to_path_buf());
        }

        let stage_dir = match std::env::var("LOCALAPPDATA") {
            Ok(appdata) => PathBuf::from(appdata).join("apple_music_rs").join("bridge"),
            Err(_) => std::env::temp_dir().join("apple_music_bridge"),
        };

        let _ = std::fs::create_dir_all(&stage_dir);

        if let Ok(entries) = std::fs::read_dir(install_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().map(|e| e.eq_ignore_ascii_case("dll")).unwrap_or(false) {
                    if let Some(file_name) = p.file_name() {
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

                        let dst = stage_dir.join(file_name);
                        let should_copy = if dst.exists() {
                            let src_len = p.metadata().map(|m| m.len()).unwrap_or(0);
                            let dst_len = dst.metadata().map(|m| m.len()).unwrap_or(0);
                            src_len != dst_len
                        } else {
                            true
                        };

                        if should_copy {
                            let _ = std::fs::copy(&p, &dst);
                        }
                    }
                }
            }
        }

        Ok(stage_dir)
    }

    /// Extract version string from directory name (e.g. "AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa")
    pub fn parse_version_from_path(path: &Path) -> Option<String> {
        let folder_name = path.file_name()?.to_str()?;
        if let Some(start_idx) = folder_name.find("AppleInc.AppleMusicWin_") {
            let remainder = &folder_name[start_idx + "AppleInc.AppleMusicWin_".len()..];
            if let Some(end_idx) = remainder.find('_') {
                return Some(remainder[..end_idx].to_string());
            }
            return Some(remainder.to_string());
        }
        None
    }

    /// Discover installation directory
    fn find_install_dir(&self) -> Result<PathBuf, LocatorError> {
        // Priority 1: Environment override (for testing and custom installs)
        if let Ok(custom_dir) = std::env::var("APPLE_MUSIC_INSTALL_DIR") {
            let path = PathBuf::from(custom_dir);
            if path.exists() {
                return Ok(path);
            } else {
                return Err(LocatorError::NotInstalled);
            }
        }

        // Priority 2: Windows Registry query (Standard AppModel repository in HKCU)
        #[cfg(windows)]
        {
            if let Some(path) = self.find_install_dir_from_registry() {
                if path.exists() {
                    return Ok(path);
                }
            }
        }

        // Priority 3: Known standard install location
        let standard_path = PathBuf::from(
            r"C:\Program Files\WindowsApps\AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa",
        );
        if standard_path.exists() {
            return Ok(standard_path);
        }

        Err(LocatorError::NotInstalled)
    }

    /// Query HKCU AppModel repository for Apple Music PackageRootFolder
    #[cfg(windows)]
    fn find_install_dir_from_registry(&self) -> Option<PathBuf> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        #[link(name = "advapi32")]
        extern "system" {
            fn RegOpenKeyExW(
                hKey: isize,
                lpSubKey: *const u16,
                ulOptions: u32,
                samDesired: u32,
                phkResult: *mut isize,
            ) -> i32;
            fn RegEnumKeyExW(
                hKey: isize,
                dwIndex: u32,
                lpName: *mut u16,
                lpcchName: *mut u32,
                lpReserved: *mut u32,
                lpClass: *mut u16,
                lpcchClass: *mut u32,
                lpftLastWriteTime: *mut u64,
            ) -> i32;
            fn RegQueryValueExW(
                hKey: isize,
                lpValueName: *const u16,
                lpReserved: *mut u32,
                lpType: *mut u32,
                lpData: *mut u8,
                lpcbData: *mut u32,
            ) -> i32;
            fn RegCloseKey(hKey: isize) -> i32;
        }

        const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
        const KEY_READ: u32 = 0x20019;
        const ERROR_SUCCESS: i32 = 0;

        let subkey_path = "Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\AppModel\\Repository\\Packages\0"
            .encode_utf16()
            .collect::<Vec<u16>>();

        let mut h_packages: isize = 0;
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey_path.as_ptr(),
                0,
                KEY_READ,
                &mut h_packages,
            )
        };

        if status != ERROR_SUCCESS || h_packages == 0 {
            return None;
        }

        let mut found_dir = None;
        let mut index = 0u32;
        let mut name_buf = [0u16; 256];

        loop {
            let mut name_len = name_buf.len() as u32;
            let enum_status = unsafe {
                RegEnumKeyExW(
                    h_packages,
                    index,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };

            if enum_status != ERROR_SUCCESS {
                break;
            }

            let key_name = OsString::from_wide(&name_buf[..name_len as usize])
                .to_string_lossy()
                .into_owned();

            if key_name.starts_with("AppleInc.AppleMusicWin_") {
                let package_subkey_str = format!("{}\0", key_name);
                let package_subkey_utf16 = package_subkey_str.encode_utf16().collect::<Vec<u16>>();
                let mut h_pkg: isize = 0;

                let open_pkg = unsafe {
                    RegOpenKeyExW(
                        h_packages,
                        package_subkey_utf16.as_ptr(),
                        0,
                        KEY_READ,
                        &mut h_pkg,
                    )
                };

                if open_pkg == ERROR_SUCCESS && h_pkg != 0 {
                    let val_name = "PackageRootFolder\0".encode_utf16().collect::<Vec<u16>>();
                    let mut data_buf = [0u8; 1024];
                    let mut data_len = data_buf.len() as u32;
                    let mut val_type = 0u32;

                    let query_status = unsafe {
                        RegQueryValueExW(
                            h_pkg,
                            val_name.as_ptr(),
                            std::ptr::null_mut(),
                            &mut val_type,
                            data_buf.as_mut_ptr(),
                            &mut data_len,
                        )
                    };

                    unsafe { RegCloseKey(h_pkg) };

                    if query_status == ERROR_SUCCESS && data_len > 1 {
                        let utf16_slice = unsafe {
                            std::slice::from_raw_parts(
                                data_buf.as_ptr() as *const u16,
                                (data_len / 2) as usize,
                            )
                        };
                        let mut folder_path = OsString::from_wide(utf16_slice)
                            .to_string_lossy()
                            .into_owned();
                        // Strip trailing null characters
                        if let Some(idx) = folder_path.find('\0') {
                            folder_path.truncate(idx);
                        }

                        let p = PathBuf::from(folder_path);
                        if p.exists() {
                            found_dir = Some(p);
                            break;
                        }
                    }
                }
            }

            index += 1;
        }

        unsafe { RegCloseKey(h_packages) };
        found_dir
    }

    /// Discover LocalState directory
    pub fn find_local_state_dir(&self) -> Option<PathBuf> {
        // Priority 1: Environment override
        if let Ok(dir) = std::env::var("APPLE_MUSIC_LOCAL_STATE_DIR") {
            let p = PathBuf::from(dir);
            if p.exists() {
                return Some(p);
            }
        }
        if let Ok(dir) = std::env::var("APPLE_MUSIC_DATA_DIR") {
            let p = PathBuf::from(dir);
            if p.exists() {
                return Some(p);
            }
        }

        // Priority 2: Standard AppData Packages folder
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            let packages_dir = PathBuf::from(appdata).join("Packages");
            if packages_dir.exists() {
                // Check direct known package name first
                let direct_path = packages_dir
                    .join("AppleInc.AppleMusicWin_nzyj5cx40ttqa")
                    .join("LocalState");
                if direct_path.exists() {
                    return Some(direct_path);
                }

                // If not found, scan subdirectories matching AppleInc.AppleMusicWin_
                if let Ok(entries) = std::fs::read_dir(&packages_dir) {
                    for entry in entries.flatten() {
                        let file_name = entry.file_name();
                        let name_str = file_name.to_string_lossy();
                        if name_str.starts_with("AppleInc.AppleMusicWin_") {
                            let candidate = entry.path().join("LocalState");
                            if candidate.exists() {
                                return Some(candidate);
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: &Path) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, val);
            Self { key, original }
        }

        fn unset(key: &'static str) -> Self {
            let original = std::env::var(key).ok();
            std::env::remove_var(key);
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

    fn create_mock_install_dir() -> (PathBuf, PathBuf) {
        let unique = uuid::Uuid::new_v4();
        let install_dir = std::env::temp_dir().join(format!("AppleInc.AppleMusicWin_1.1792.39682.0_mock_{}", unique));
        let local_state = std::env::temp_dir().join(format!("LocalState_mock_{}", unique));

        fs::create_dir_all(&install_dir).expect("create mock install dir");
        fs::create_dir_all(&local_state).expect("create mock local state");

        for &dll in CRITICAL_DLLS {
            File::create(install_dir.join(dll)).expect("create mock dll");
        }
        File::create(install_dir.join("AuthKitWin.dll")).expect("create mock authkit");

        (install_dir, local_state)
    }

    // ─── Positive Tests (POS-01 .. POS-04) ──────────────────────────────

    #[test]
    fn test_loc_pos_01_auto_detect_existing_installation() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let locator = AppLocator::new();
        let result = locator.locate();
        // If Apple Music is installed on the host, it should succeed
        if let Ok(install) = result {
            assert!(install.install_dir.exists(), "install_dir must exist");
            assert!(install.local_state_dir.exists(), "local_state_dir must exist");
            assert!(!install.available_dlls.is_empty(), "critical DLLs must be present");
            assert!(install.available_dlls.contains(&"CoreAudioToolbox.dll".to_string()));
            assert!(install.available_dlls.contains(&"CoreMedia.dll".to_string()));
            assert!(install.available_dlls.contains(&"CoreFP.dll".to_string()));
        }
    }

    #[test]
    fn test_loc_pos_02_environment_override_takes_precedence() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let (mock_install, mock_local) = create_mock_install_dir();
        let _g1 = EnvGuard::set("APPLE_MUSIC_INSTALL_DIR", &mock_install);
        let _g2 = EnvGuard::set("APPLE_MUSIC_LOCAL_STATE_DIR", &mock_local);

        let locator = AppLocator::new();
        let install = locator.locate().expect("locate with overrides");

        assert_eq!(install.install_dir, mock_install);
        assert_eq!(install.local_state_dir, mock_local);
        assert_eq!(install.version, "1.1792.39682.0");

        let _ = fs::remove_dir_all(&mock_install);
        let _ = fs::remove_dir_all(&mock_local);
    }

    #[test]
    fn test_loc_pos_03_version_string_extraction() {
        let p1 = PathBuf::from(r"C:\Program Files\WindowsApps\AppleInc.AppleMusicWin_1.1792.39682.0_x64__nzyj5cx40ttqa");
        let v1 = AppLocator::parse_version_from_path(&p1);
        assert_eq!(v1.as_deref(), Some("1.1792.39682.0"));

        let p2 = PathBuf::from(r"D:\Apps\AppleInc.AppleMusicWin_2.0.1_arm64");
        let v2 = AppLocator::parse_version_from_path(&p2);
        assert_eq!(v2.as_deref(), Some("2.0.1"));

        let p3 = PathBuf::from(r"C:\SomeOtherFolder\NotApple");
        let v3 = AppLocator::parse_version_from_path(&p3);
        assert_eq!(v3, None);
    }

    #[test]
    fn test_loc_pos_04_verify_dlls_success() {
        let (mock_install, mock_local) = create_mock_install_dir();
        let locator = AppLocator::new();

        let dlls = locator.verify_dlls(&mock_install).expect("verify dlls");
        assert!(dlls.contains(&"CoreAudioToolbox.dll".to_string()));
        assert!(dlls.contains(&"CoreMedia.dll".to_string()));
        assert!(dlls.contains(&"CoreFP.dll".to_string()));
        assert!(dlls.contains(&"AuthKitWin.dll".to_string()));

        let _ = fs::remove_dir_all(&mock_install);
        let _ = fs::remove_dir_all(&mock_local);
    }

    // ─── Negative Tests (NEG-01 .. NEG-04) ──────────────────────────────

    #[test]
    fn test_loc_neg_01_system_without_apple_music_returns_not_installed() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let fake_path = std::env::temp_dir().join(format!("fake_apple_music_{}", uuid::Uuid::new_v4()));
        let _g = EnvGuard::set("APPLE_MUSIC_INSTALL_DIR", &fake_path);

        let locator = AppLocator::new();
        let err = locator.locate().unwrap_err();
        assert_eq!(err, LocatorError::NotInstalled);
    }

    #[test]
    fn test_loc_neg_02_missing_critical_dll_returns_missing_dlls() {
        let unique = uuid::Uuid::new_v4();
        let partial_dir = std::env::temp_dir().join(format!("partial_apple_music_{}", unique));
        fs::create_dir_all(&partial_dir).expect("create partial dir");

        // Only create CoreAudioToolbox.dll, miss CoreMedia.dll and CoreFP.dll
        File::create(partial_dir.join("CoreAudioToolbox.dll")).expect("create single dll");

        let locator = AppLocator::new();
        let err = locator.verify_dlls(&partial_dir).unwrap_err();

        match err {
            LocatorError::MissingDlls(missing) => {
                assert!(missing.contains(&"CoreMedia.dll".to_string()));
                assert!(missing.contains(&"CoreFP.dll".to_string()));
                assert!(!missing.contains(&"CoreAudioToolbox.dll".to_string()));
            }
            other => panic!("Expected MissingDlls error, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&partial_dir);
    }

    #[test]
    fn test_loc_neg_03_non_existent_local_state_returns_error() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let (mock_install, _) = create_mock_install_dir();
        let fake_local = std::env::temp_dir().join(format!("fake_local_state_{}", uuid::Uuid::new_v4()));

        let _g1 = EnvGuard::set("APPLE_MUSIC_INSTALL_DIR", &mock_install);
        let _g2 = EnvGuard::set("APPLE_MUSIC_LOCAL_STATE_DIR", &fake_local);
        let _g3 = EnvGuard::unset("APPLE_MUSIC_DATA_DIR");
        let _g4 = EnvGuard::unset("LOCALAPPDATA");

        let locator = AppLocator::new();
        let err = locator.locate().unwrap_err();

        match err {
            LocatorError::LocalStateNotFound(path) => {
                assert_eq!(path, fake_local);
            }
            other => panic!("Expected LocalStateNotFound, got: {:?}", other),
        }

        let _ = fs::remove_dir_all(&mock_install);
    }

    #[test]
    fn test_loc_neg_04_verify_non_existent_folder_returns_not_installed() {
        let non_existent = PathBuf::from(r"C:\fake_never_existent_directory_987654");
        let locator = AppLocator::new();
        let err = locator.verify_dlls(&non_existent).unwrap_err();
        assert_eq!(err, LocatorError::NotInstalled);
    }
}

