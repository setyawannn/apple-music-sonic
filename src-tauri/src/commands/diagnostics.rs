//! Native-bridge diagnostics for the ABANDONED native decryption path.
//!
//! These only report the state of the parked `drm/**` bridge; nothing in the shipped
//! playback path depends on them.

use crate::api::local_session::LocalSessionReader;
use crate::drm::{
    get_current_process_memory, AppLocator, CoreAudioBridge, CoreFpBridge, CoreMediaBridge,
};
use crate::{AppError, AppResult, AppState};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NativeBridgeStatusPayload {
    pub is_installed: bool,
    pub install_path: Option<String>,
    pub version: Option<String>,
    pub coreaudio_ready: bool,
    pub coremedia_ready: bool,
    pub corefp_ready: bool,
    pub current_ram_mb: f64,
    pub session_detected: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImportSessionResult {
    pub success: bool,
    pub has_user_token: bool,
    pub storefront: Option<String>,
    pub message: String,
}

#[tauri::command]
pub fn get_native_bridge_status(_state: State<AppState>) -> AppResult<NativeBridgeStatusPayload> {
    let locator = AppLocator::new();
    let install_res = locator.locate();

    let mut is_installed = false;
    let mut install_path = None;
    let mut version = None;
    let mut coreaudio_ready = false;
    let mut coremedia_ready = false;
    let mut corefp_ready = false;
    let mut session_detected = false;

    if let Ok(install) = install_res {
        is_installed = true;
        install_path = Some(install.install_dir.to_string_lossy().to_string());
        version = Some(install.version);

        if let Ok(ca) = CoreAudioBridge::load_from_dir(&install.install_dir) {
            coreaudio_ready = ca.is_operational();
        }

        if let Ok(cm) = CoreMediaBridge::load_from_dir(&install.install_dir) {
            coremedia_ready = (cm.fig_content_key_boss_get_class_id as usize) != 0;
        }

        if let Ok(fp) = CoreFpBridge::load_from_dir(&install.install_dir) {
            corefp_ready = fp.resolved_count() == 6;
        }

        let reader = LocalSessionReader::new();
        if let Ok(session) = reader.extract_from_local_state(&install.local_state_dir) {
            session_detected = session.media_user_token.is_some() || session.itctx.is_some();
        }
    }

    let ram_mb = get_current_process_memory()
        .map(|m| m.working_set_bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    Ok(NativeBridgeStatusPayload {
        is_installed,
        install_path,
        version,
        coreaudio_ready,
        coremedia_ready,
        corefp_ready,
        current_ram_mb: ram_mb,
        session_detected,
    })
}

#[tauri::command]
pub fn import_local_apple_session(state: State<AppState>) -> AppResult<ImportSessionResult> {
    let locator = AppLocator::new();
    let install = locator
        .locate()
        .map_err(|e| AppError::Native(e.to_string()))?;

    let reader = LocalSessionReader::new();
    let session = reader
        .extract_from_local_state(&install.local_state_dir)
        .map_err(|e| AppError::Native(e.to_string()))?;

    let mut has_user_token = false;
    if let Some(mut_token) = session.media_user_token.as_ref().or(session.itctx.as_ref()) {
        state
            .api_client
            .auth
            .set_user_token(Some(mut_token.clone()))
            .map_err(|e| AppError::Native(e.to_string()))?;
        has_user_token = true;
    }

    if let Some(storefront) = &session.storefront {
        let _ = state
            .api_client
            .auth
            .set_storefront(storefront.clone(), "en-US".to_string());
    }

    Ok(ImportSessionResult {
        success: true,
        has_user_token,
        storefront: session.storefront,
        message: "Successfully imported Apple Music session from local SQLite cookies".to_string(),
    })
}

#[tauri::command]
pub fn run_native_bridge_diagnostics(_state: State<AppState>) -> AppResult<String> {
    let locator = AppLocator::new();
    let install = locator
        .locate()
        .map_err(|e| AppError::Native(e.to_string()))?;

    let mut lines = Vec::new();
    lines.push(format!("Apple Music Version: {}", install.version));
    lines.push(format!("Install Dir: {}", install.install_dir.display()));
    lines.push(format!(
        "LocalState Dir: {}",
        install.local_state_dir.display()
    ));

    let ca = CoreAudioBridge::load_from_dir(&install.install_dir)
        .map_err(|e| AppError::Native(e.to_string()))?;
    lines.push(format!("CoreAudio Operational: {}", ca.is_operational()));
    lines.push(format!("CoreAudio Decoders: {:?}", ca.decoder_status()));

    let cm = CoreMediaBridge::load_from_dir(&install.install_dir)
        .map_err(|e| AppError::Native(e.to_string()))?;
    lines.push(format!(
        "FigContentKeyBoss Class ID: {}",
        cm.get_key_boss_class_id()
    ));

    let fp = CoreFpBridge::load_from_dir(&install.install_dir)
        .map_err(|e| AppError::Native(e.to_string()))?;
    lines.push(format!(
        "CoreFP Ordinals Resolved: {}/6",
        fp.resolved_count()
    ));

    let mem = get_current_process_memory().map_err(|e| AppError::Native(e.to_string()))?;
    lines.push(format!(
        "Process Working Set: {:.2} MB",
        mem.working_set_bytes as f64 / (1024.0 * 1024.0)
    ));

    Ok(lines.join("\n"))
}
