pub mod locator;
pub mod loader;
pub mod coreaudio;
pub mod coremedia;
pub mod corefp;

pub use locator::{AppLocator, AppleInstallation, LocatorError, CRITICAL_DLLS, OPTIONAL_DLLS};
pub use loader::{NativeLibrary, LoaderError, ProcessMemoryStats, get_current_process_memory};
pub use coreaudio::{CoreAudioBridge, CoreAudioStatus};
pub use coremedia::CoreMediaBridge;
pub use corefp::CoreFpBridge;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DrmDiagnosticReport {
    pub coremedia_loaded: bool,
    pub corefp_loaded: bool,
    pub coreke_loaded: bool,
    pub key_boss_class_id: u64,
    pub fp_ordinals_resolved: usize,
}
