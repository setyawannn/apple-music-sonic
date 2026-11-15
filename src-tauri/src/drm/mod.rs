pub mod coreaudio;
pub mod corefp;
pub mod coremedia;
pub mod loader;
pub mod locator;

pub use coreaudio::{CoreAudioBridge, CoreAudioStatus};
pub use corefp::CoreFpBridge;
pub use coremedia::CoreMediaBridge;
pub use loader::{get_current_process_memory, LoaderError, NativeLibrary, ProcessMemoryStats};
pub use locator::{AppLocator, AppleInstallation, LocatorError, CRITICAL_DLLS, OPTIONAL_DLLS};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DrmDiagnosticReport {
    pub coremedia_loaded: bool,
    pub corefp_loaded: bool,
    pub coreke_loaded: bool,
    pub key_boss_class_id: u64,
    pub fp_ordinals_resolved: usize,
}
