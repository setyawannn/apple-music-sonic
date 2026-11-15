//! Apple ID sign-in, session persistence and auth state.

use crate::api::AuthState;
use crate::player::{ensure_player, PLAYER_LABEL};
use crate::{AppError, AppResult, AppState};
use tauri::{Emitter, Manager, State};

// ─── Apple Music API & Auth Commands ────────────────────────────────

/// Ask Apple whether a Music-User-Token is actually live.
/// A token can be non-empty and well-formed yet already revoked, which is exactly
/// what MusicKit hands back from a cached (expired) web session.
async fn validate_music_user_token(dev_token: &str, user_token: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    else {
        return false;
    };
    match client
        .get("https://amp-api.music.apple.com/v1/me/library/songs?limit=1")
        .header("Authorization", format!("Bearer {dev_token}"))
        .header("Music-User-Token", user_token)
        .header("Origin", "https://music.apple.com")
        .header("Referer", "https://music.apple.com/")
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

#[tauri::command]
pub async fn start_apple_sign_in(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if let Some(existing) = app.get_webview_window("apple_auth") {
        let _ = existing.set_focus();
        return Ok(());
    }

    let url = "https://music.apple.com/login"
        .parse::<tauri::Url>()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;

    let auth_manager = state.api_client.auth.clone();
    let app_handle = app.clone();
    let dev_token = auth_manager.developer_token();

    // The sign-in window gets its own WebView2 profile. The app's main profile keeps a
    // cached Apple session, and MusicKit happily replays that dead token — which made
    // sign-in "succeed" instantly with an expired token and never prompt for credentials.
    let auth_profile = app.path().app_local_data_dir()?.join("auth_profile");

    // The hidden player runs on this same profile. Wiping it underneath a live
    // webview leaves the player holding a destroyed session, so close it first;
    // ensure_player() rebuilds it against the fresh profile when needed.
    if let Some(player) = app.get_webview_window(PLAYER_LABEL) {
        let _ = player.close();
        // Give WebView2 a moment to release its lock on the profile directory.
        for _ in 0..20 {
            if app.get_webview_window(PLAYER_LABEL).is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    let _ = std::fs::remove_dir_all(&auth_profile);
    std::fs::create_dir_all(&auth_profile)?;

    let window =
        tauri::WebviewWindowBuilder::new(&app, "apple_auth", tauri::WebviewUrl::External(url))
            .title("Sign in with Apple ID")
            .inner_size(520.0, 720.0)
            .center()
            .resizable(true)
            .data_directory(auth_profile)
            .build()?;

    let _ = window.show();

    // `media-user-token` is HttpOnly, so injected JavaScript can never read it — that is
    // why the old `document.cookie` poller only ever picked up MusicKit's cached token.
    // Reading the WebView2 cookie jar directly gets the real, freshly minted cookie.
    let _ = window.set_focus();

    tauri::async_runtime::spawn(async move {
        // ~10 minutes: long enough for 2FA and a slow Apple ID round trip.
        for _ in 0..600u32 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            // The user closed the window — nothing left to watch.
            if app_handle.get_webview_window("apple_auth").is_none() {
                return;
            }

            let Ok(jar) = window.cookies() else { continue };
            let Some(cookie) = jar.iter().find(|c| c.name() == "media-user-token") else {
                continue;
            };
            let token = cookie.value().trim().to_string();
            if token.len() < 20 {
                continue;
            }

            // Apple is the only authority on whether this token is alive. A dead token
            // looks perfectly valid locally, so never persist one that isn't confirmed.
            if !validate_music_user_token(&dev_token, &token).await {
                continue;
            }

            if let Ok(jar) = window.cookies() {
                if let Some(sf) = jar
                    .iter()
                    .find(|c| matches!(c.name(), "geo" | "x-apple-store-front"))
                    .map(|c| c.value().trim().to_string())
                    .filter(|v| v.len() == 2)
                {
                    let _ = auth_manager.set_storefront(sf, auth_manager.language());
                }
            }

            if auth_manager.set_user_token(Some(token)).is_err() {
                continue;
            }

            let auth_state = AuthState {
                is_authenticated: true,
                storefront: auth_manager.storefront(),
                language: auth_manager.language(),
                user_token_masked: auth_manager.masked_user_token(),
            };
            let _ = app_handle.emit("auth-success", auth_state);
            if let Some(w) = app_handle.get_webview_window("apple_auth") {
                let _ = w.close();
            }

            // Pre-warm the player on the fresh profile so the next click streams.
            if let Err(e) = ensure_player(&app_handle) {
                eprintln!("[AUTH] player re-warm failed: {}", e);
            }
            return;
        }
    });

    Ok(())
}

#[tauri::command]
pub fn set_user_session(state: State<AppState>, token: String) -> AppResult<AuthState> {
    state.api_client.auth.set_user_token(Some(token))?;
    get_auth_state(state)
}

#[tauri::command]
pub fn sign_out(state: State<AppState>) -> AppResult<()> {
    Ok(state.api_client.auth.sign_out()?)
}

#[tauri::command]
pub fn get_auth_state(state: State<AppState>) -> AppResult<AuthState> {
    let auth = &state.api_client.auth;
    Ok(AuthState {
        is_authenticated: auth.is_authenticated(),
        storefront: auth.storefront(),
        language: auth.language(),
        user_token_masked: auth.masked_user_token(),
    })
}

#[tauri::command]
pub fn set_storefront(
    state: State<AppState>,
    storefront: String,
    language: String,
) -> AppResult<()> {
    Ok(state.api_client.auth.set_storefront(storefront, language)?)
}
