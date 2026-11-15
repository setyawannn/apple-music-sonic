//! Windows media keys and the System Media Transport Controls (SMTC) flyout.
//!
//! Hardware media keys are owned by the OS, not by the focused window: Windows turns
//! them into SMTC button events, and SMTC is the only supported way for a desktop app
//! to receive them. The WinRT activation path (`GetForCurrentView`) does not exist for
//! Win32, so this goes through `ISystemMediaTransportControlsInterop::GetForWindow`.
//! That interface is absent from the `windows` metadata, hence the small vtable shim:
//! it is the same interface documented in `systemmediatransportcontrolsinterop.h`.
//!
//! Button presses are translated into ordinary player commands, so the hidden
//! MusicKit page stays the single source of playback truth. The flyout is fed from
//! the same `player-state` blob the frontend consumes.

use super::player_command;
use crate::{AppError, AppResult};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::{IInspectable, Interface, GUID, HRESULT, HSTRING};
use windows::Foundation::{TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, MusicDisplayProperties, SystemMediaTransportControls,
    SystemMediaTransportControlsButton as Button,
    SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;

/// WinRT runtime class whose activation factory is the SMTC interop interface.
const SMTC_CLASS: &str = "Windows.Media.SystemMediaTransportControls";

/// `ISystemMediaTransportControlsInterop` from
/// `systemmediatransportcontrolsinterop.h`. It is the SMTC activation factory on
/// desktop, and the only way to bind a control object to a Win32 window.
///
/// The `windows` metadata omits this interface, so it is declared here. Vtable order
/// is `IUnknown` (3 slots), `IInspectable` (3 slots), then the one method below; the
/// GUID is the one shipped in the Windows SDK header.
#[repr(C)]
struct InteropVtbl {
    query_interface: *const (),
    add_ref: *const (),
    release: *const (),
    get_iids: *const (),
    get_runtime_class_name: *const (),
    get_trust_level: *const (),
    get_for_window: unsafe extern "system" fn(
        this: *mut core::ffi::c_void,
        app_window: HWND,
        riid: *const GUID,
        controls: *mut *mut core::ffi::c_void,
    ) -> HRESULT,
}

/// Typed handle to the SMTC interop factory interface.
#[repr(transparent)]
#[derive(Clone)]
struct Interop(IInspectable);

// SAFETY: `Interop` is a transparent wrapper over an `IInspectable` pointer, and the
// GUID below is the IID Windows registers for this factory interface, so the vtable
// behind that pointer really is `InteropVtbl`.
unsafe impl Interface for Interop {
    type Vtable = InteropVtbl;
    const IID: GUID = GUID::from_u128(0xddb0_472d_c911_4a1f_86d9_dc3d_71a9_5f5a);
}

/// A live SMTC session, plus enough memo to avoid redundant COM calls. The OS only
/// hands out one per window, so a process-wide singleton is the honest model.
struct Session {
    controls: SystemMediaTransportControls,
    /// Last artwork URL pushed to the flyout; art must not be rebuilt per tick.
    artwork: String,
    /// Last published transport status (`MediaPlaybackStatus.0`).
    status: i32,
    /// Last published (title, artist, album) triple.
    text: (String, String, String),
}

static SESSION: OnceLock<Mutex<Session>> = OnceLock::new();

fn smtc(error: windows::core::Error) -> AppError {
    AppError::Player(format!("system media controls: {error}"))
}

fn with_session<R>(f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    let mut guard = SESSION.get()?.lock().ok()?;
    Some(f(&mut guard))
}

/// Bind media keys and the now-playing flyout to the main window.
///
/// Called once from setup. A failure is logged and playback still works; media keys
/// are a convenience, not a dependency.
pub(crate) fn register(app: &AppHandle) -> AppResult<()> {
    if SESSION.get().is_some() {
        return Ok(());
    }

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Player("main window was not found".into()))?;
    let hwnd = window
        .hwnd()
        .map_err(|e| AppError::Player(format!("main window has no handle: {e}")))?;

    // SAFETY: `hwnd` belongs to the live main window and is used for the duration
    // of this call. The vtable slot is the documented `GetForWindow` entry point.
    let controls = unsafe { controls_for_window(hwnd) }?;

    controls.SetIsEnabled(true).map_err(smtc)?;
    controls.SetIsPlayEnabled(true).map_err(smtc)?;
    controls.SetIsPauseEnabled(true).map_err(smtc)?;
    controls.SetIsStopEnabled(true).map_err(smtc)?;
    controls.SetIsNextEnabled(true).map_err(smtc)?;
    controls.SetIsPreviousEnabled(true).map_err(smtc)?;

    let handle = app.clone();
    let pressed =
        TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(
            move |_sender: windows::core::Ref<'_, SystemMediaTransportControls>,
                  args: windows::core::Ref<
                '_,
                SystemMediaTransportControlsButtonPressedEventArgs,
            >| {
                if let Some(args) = args.as_ref() {
                    if let Ok(button) = args.Button() {
                        on_button_press(&handle, button);
                    }
                }
                Ok(())
            },
        );
    controls.ButtonPressed(&pressed).map_err(smtc)?;

    let session = Session {
        controls,
        artwork: String::new(),
        status: MediaPlaybackStatus::Closed.0,
        text: (String::new(), String::new(), String::new()),
    };
    // Racing callers both get a session; the loser's handle is dropped, which is safe.
    let _ = SESSION.set(Mutex::new(session));
    Ok(())
}

/// Acquire the per-window control object through the interop activation factory.
///
/// The factory is requested by the interop IID directly. Requesting `IUnknown`
/// instead would hand back the object's primary vtable, and calling a fixed slot on
/// that would be undefined behaviour.
unsafe fn controls_for_window(hwnd: HWND) -> AppResult<SystemMediaTransportControls> {
    let factory: Interop = unsafe {
        windows::Win32::System::WinRT::RoGetActivationFactory(&HSTRING::from(SMTC_CLASS))
    }
    .map_err(smtc)?;

    let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();
    let hr = unsafe {
        (Interface::vtable(&factory).get_for_window)(
            factory.as_raw(),
            hwnd,
            &SystemMediaTransportControls::IID,
            &mut raw,
        )
    };
    hr.ok().map_err(smtc)?;

    if raw.is_null() {
        return Err(AppError::Player(
            "system media controls returned no control object".into(),
        ));
    }
    Ok(unsafe { SystemMediaTransportControls::from_raw(raw) })
}

/// Map an OS button press onto the player's own transport vocabulary.
fn on_button_press(app: &AppHandle, button: Button) {
    let command = if button == Button::Play {
        "resume"
    } else if button == Button::Pause {
        "pause"
    } else if button == Button::Stop {
        "stop"
    } else if button == Button::Next {
        "next"
    } else if button == Button::Previous {
        "previous"
    } else {
        // Record / fast-forward / channel buttons are not part of this app.
        return;
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = player_command(&app, command, None).await {
            eprintln!("[MEDIA] {command} failed: {e}");
        }
    });
}

/// Mirror the hidden player's state into the Windows flyout.
///
/// Runs on every distinct `mk_state` blob (position included), so all work is guarded
/// by a change check. Anything that fails here is cosmetic and deliberately ignored.
pub(crate) fn update_now_playing(state: &serde_json::Value) {
    let text = |key: &str| {
        state
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let title = text("title");
    let artist = text("artist");
    let album = text("album");
    let artwork = text("artwork");
    let playing = state
        .get("isPlaying")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let status = if title.is_empty() {
        MediaPlaybackStatus::Stopped
    } else if playing {
        MediaPlaybackStatus::Playing
    } else {
        MediaPlaybackStatus::Paused
    };

    with_session(|session| {
        if session.status != status.0 && session.controls.SetPlaybackStatus(status).is_ok() {
            session.status = status.0;
        }

        let text_changed = session.text != (title.clone(), artist.clone(), album.clone());
        let art_changed = !artwork.is_empty() && session.artwork != artwork;
        if !text_changed && !art_changed {
            return;
        }

        let result = publish_metadata(
            &session.controls,
            &title,
            &artist,
            &album,
            art_changed.then_some(artwork.as_str()),
        );
        if result.is_ok() {
            session.text = (title, artist, album);
            if art_changed {
                session.artwork = artwork;
            }
        }
    });
}

/// Write title/artist/album/artwork to the flyout and commit the update.
fn publish_metadata(
    controls: &SystemMediaTransportControls,
    title: &str,
    artist: &str,
    album: &str,
    artwork: Option<&str>,
) -> windows::core::Result<()> {
    let updater = controls.DisplayUpdater()?;
    updater.SetType(MediaPlaybackType::Music)?;

    let music: MusicDisplayProperties = updater.MusicProperties()?;
    music.SetTitle(&HSTRING::from(title))?;
    music.SetArtist(&HSTRING::from(artist))?;
    music.SetAlbumTitle(&HSTRING::from(album))?;

    if let Some(url) = artwork {
        let uri = Uri::CreateUri(&HSTRING::from(url))?;
        let thumbnail = RandomAccessStreamReference::CreateFromUri(&uri)?;
        updater.SetThumbnail(&thumbnail)?;
    }

    updater.Update()
}

#[cfg(test)]
mod tests {
    use super::Button;

    /// Button identity is by value; the mapping in `on_button_press` relies on it.
    #[test]
    fn button_constants_are_distinct() {
        assert_ne!(Button::Play, Button::Pause);
        assert_ne!(Button::Next, Button::Previous);
        assert_eq!(Button::Stop.0, 2);
    }
}
