use app_lib::api::auth::AuthManager;
use app_lib::api::client::{parse_catalog_search_json, ApiClient};
use app_lib::api::models::StoreSong;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot::{channel, Sender};

async fn spawn_mock_server<F>(handler: F) -> (String, Sender<()>)
where
    F: Fn(&str, &str) -> (u16, String) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    let (tx, mut rx) = channel();
    let handler = Arc::new(handler);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut rx => break,
                Ok((mut stream, _)) = listener.accept() => {
                    let h = Arc::clone(&handler);
                    tokio::spawn(async move {
                        let mut buf = [0u8; 4096];
                        if let Ok(n) = stream.read(&mut buf).await {
                            if n > 0 {
                                let req_str = String::from_utf8_lossy(&buf[..n]);
                                let first_line = req_str.lines().next().unwrap_or("").to_string();
                                let (status_code, body) = h(&first_line, &req_str);
                                let status_text = match status_code {
                                    200 => "OK",
                                    401 => "Unauthorized",
                                    404 => "Not Found",
                                    429 => "Too Many Requests",
                                    _ => "Internal Server Error",
                                };
                                let resp = format!(
                                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                    status_code, status_text, body.as_bytes().len(), body
                                );
                                let _ = stream.write_all(resp.as_bytes()).await;
                                let _ = stream.flush().await;
                            }
                        }
                    });
                }
            }
        }
    });

    (format!("http://{}", addr), tx)
}

const MOCK_SEARCH_JSON: &str = r#"{
  "results": {
    "songs": {
      "data": [
        {
          "id": "1440857781",
          "type": "songs",
          "attributes": {
            "name": "Yellow",
            "artistName": "Coldplay",
            "albumName": "Parachutes",
            "durationInMillis": 266773,
            "trackNumber": 5,
            "releaseDate": "2000-06-26",
            "genreNames": ["Alternative", "Rock"],
            "audioTraits": ["lossless", "spatial", "atmos"],
            "artwork": {
              "url": "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/xx/{w}x{h}bb.{f}"
            },
            "previews": [
              { "url": "https://audio-ssl.itunes.apple.com/preview.m4a" }
            ]
          }
        }
      ]
    },
    "albums": {
      "data": [
        {
          "id": "1440857780",
          "type": "albums",
          "attributes": {
            "name": "Parachutes",
            "artistName": "Coldplay",
            "trackCount": 10,
            "releaseDate": "2000-07-10",
            "artwork": {
              "url": "https://is1-ssl.mzstatic.com/image/thumb/Music115/v4/album/{w}x{h}bb.{f}"
            }
          }
        }
      ]
    },
    "artists": {
      "data": [
        {
          "id": "471744",
          "type": "artists",
          "attributes": {
            "name": "Coldplay",
            "url": "https://music.apple.com/artist/coldplay/471744"
          }
        }
      ]
    }
  }
}"#;

// ═══════════════════════════════════════════════════════════════════════════
// POSITIVE ACCEPTANCE TESTS (API-POS-01 .. API-POS-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_api_pos_01_auth_session_persistence() {
    let temp_session = std::env::temp_dir().join(format!("session_persist_{}.json", uuid::Uuid::new_v4()));
    let auth = AuthManager::new(&temp_session);

    assert!(!auth.is_authenticated());

    auth.set_user_token(Some("valid_mut_token_abc123456789".to_string()))
        .expect("save token");
    assert!(auth.is_authenticated());
    assert_eq!(auth.user_token().as_deref(), Some("valid_mut_token_abc123456789"));

    let auth2 = AuthManager::new(&temp_session);
    assert!(auth2.is_authenticated());
    assert_eq!(auth2.user_token().as_deref(), Some("valid_mut_token_abc123456789"));

    let _ = std::fs::remove_file(&temp_session);
}

#[tokio::test]
async fn test_api_pos_02_authenticated_headers_injection() {
    let (server_url, shutdown_tx) = spawn_mock_server(|_line, req| {
        assert!(req.contains("authorization: Bearer"));
        assert!(req.contains("music-user-token: test_user_session_token_xyz"));
        assert!(req.contains("origin: https://music.apple.com"));
        (200, MOCK_SEARCH_JSON.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    auth.set_user_token(Some("test_user_session_token_xyz".to_string())).unwrap();

    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);
    let res = client.search_catalog("coldplay", None, 10).await.unwrap();

    assert_eq!(res.songs.len(), 1);
    assert_eq!(res.songs[0].title, "Yellow");
    let _ = shutdown_tx.send(());
}

#[test]
fn test_api_pos_03_catalog_search_parsing() {
    let json_val: serde_json::Value = serde_json::from_str(MOCK_SEARCH_JSON).unwrap();
    let result = parse_catalog_search_json(&json_val);

    assert_eq!(result.songs.len(), 1);
    assert_eq!(result.songs[0].title, "Yellow");
    assert_eq!(result.songs[0].artist, "Coldplay");
    assert!(result.songs[0].has_lossless);
    assert!(result.songs[0].has_atmos);
    assert_eq!(
        result.songs[0].preview_url.as_deref(),
        Some("https://audio-ssl.itunes.apple.com/preview.m4a")
    );

    assert_eq!(result.albums.len(), 1);
    assert_eq!(result.albums[0].title, "Parachutes");

    assert_eq!(result.artists.len(), 1);
    assert_eq!(result.artists[0].name, "Coldplay");
}

#[tokio::test]
async fn test_api_pos_04_search_suggestions_parsing() {
    let mock_suggestions = r#"{
      "results": {
        "suggestions": [
          { "searchTerm": "taylor swift" },
          { "searchTerm": "taylor swift songs" },
          { "searchTerm": "tay k" }
        ]
      }
    }"#;

    let (server_url, shutdown_tx) = spawn_mock_server(move |_line, _req| {
        (200, mock_suggestions.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);

    let suggestions = client.get_search_suggestions("tay").await.unwrap();
    assert_eq!(suggestions.terms.len(), 3);
    assert_eq!(suggestions.terms[0], "taylor swift");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_pos_05_personal_library_tracks_parsing() {
    let mock_personal = r#"{
      "data": [
        {
          "id": "i.12345",
          "type": "library-songs",
          "attributes": {
            "name": "Cruel Summer",
            "artistName": "Taylor Swift",
            "albumName": "Lover",
            "durationInMillis": 178426,
            "trackNumber": 2,
            "genreNames": ["Pop"],
            "audioTraits": ["lossless"],
            "artwork": { "url": "https://cover.jpg" }
          }
        }
      ]
    }"#;

    let (server_url, shutdown_tx) = spawn_mock_server(move |_line, req| {
        assert!(req.contains("music-user-token: active_user_token"));
        (200, mock_personal.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    auth.set_user_token(Some("active_user_token".to_string())).unwrap();

    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);
    let library_songs = client.get_personal_library_songs().await.unwrap();

    assert_eq!(library_songs.len(), 1);
    assert_eq!(library_songs[0].title, "Cruel Summer");
    assert_eq!(library_songs[0].artist, "Taylor Swift");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_pos_06_personal_recommendations_parsing() {
    let mock_recs = r#"{
      "data": [
        {
          "id": "rec.01",
          "type": "personal-recommendation",
          "relationships": {
            "contents": {
              "data": [
                {
                  "id": "alb.999",
                  "type": "albums",
                  "attributes": {
                    "name": "Midnights",
                    "artistName": "Taylor Swift",
                    "trackCount": 13
                  }
                }
              ]
            }
          }
        }
      ]
    }"#;

    let (server_url, shutdown_tx) = spawn_mock_server(move |_line, _req| {
        (200, mock_recs.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    auth.set_user_token(Some("token_for_recs".to_string())).unwrap();

    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);
    let recs = client.get_personal_recommendations().await.unwrap();

    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].title, "Midnights");

    let _ = shutdown_tx.send(());
}

#[test]
fn test_api_pos_07_artwork_template_formatter() {
    let song = StoreSong {
        id: "1".to_string(),
        title: "Song".to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        duration_ms: 1000,
        track_number: 1,
        artwork_url: Some("https://mzstatic.com/image/{w}x{h}bb.{f}".to_string()),
        preview_url: None,
        has_lossless: false,
        has_atmos: false,
        release_date: None,
        genre: None,
    };

    let formatted = song.formatted_artwork_url(600, 600, "webp").unwrap();
    assert_eq!(formatted, "https://mzstatic.com/image/600x600bb.webp");
}

#[test]
fn test_api_pos_08_user_sign_out() {
    let auth = AuthManager::in_memory();
    auth.set_user_token(Some("active_token".to_string())).unwrap();
    assert!(auth.is_authenticated());

    auth.sign_out().expect("sign out");
    assert!(!auth.is_authenticated());
    assert!(auth.user_token().is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// NEGATIVE & BOUNDARY ACCEPTANCE TESTS (API-NEG-01 .. API-NEG-08)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_api_neg_01_expired_session_handling() {
    let (server_url, shutdown_tx) = spawn_mock_server(|_line, _req| {
        (401, r#"{"error":"401 Unauthorized token expired"}"#.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);

    let res = client.search_catalog("test", None, 10).await;
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("401 Unauthorized"), "Error: {}", err_msg);

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_neg_02_network_offline_graceful_error() {
    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, "http://127.0.0.1:1".to_string(), "http://127.0.0.1:1".to_string());

    let res = client.search_catalog("test", None, 5).await;
    assert!(res.is_err(), "Must return Err on network failure");
}

#[tokio::test]
async fn test_api_neg_03_resource_not_found_404() {
    let (server_url, shutdown_tx) = spawn_mock_server(|_line, _req| {
        (404, r#"{"error":"Album not found"}"#.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);

    let res = client.get_catalog_album("non_existent_album_id").await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("404"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_neg_04_rate_limiting_429_backoff() {
    let (server_url, shutdown_tx) = spawn_mock_server(|_line, _req| {
        (429, r#"{"error":"Rate limited"}"#.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);

    let res = client.search_catalog("spam", None, 5).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("429"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_neg_05_empty_or_whitespace_search() {
    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::new(auth);

    let res1 = client.search_catalog("", None, 25).await.unwrap();
    assert_eq!(res1.songs.len(), 0);

    let res2 = client.search_catalog("     ", None, 25).await.unwrap();
    assert_eq!(res2.songs.len(), 0);

    let sug = client.get_search_suggestions("").await.unwrap();
    assert_eq!(sug.terms.len(), 0);
}

#[tokio::test]
async fn test_api_neg_06_malformed_json_api_response() {
    let (server_url, shutdown_tx) = spawn_mock_server(|_line, _req| {
        (200, "{THIS IS NOT VALID JSON AT ALL!".to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);

    let res = client.search_catalog("test", None, 5).await;
    assert!(res.is_err(), "Must fail gracefully on malformed JSON");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_neg_07_special_characters_in_search() {
    let (server_url, shutdown_tx) = spawn_mock_server(|line, _req| {
        assert!(line.contains("term=") || line.contains("term"));
        (200, MOCK_SEARCH_JSON.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::with_base_url(auth, server_url.clone(), server_url);

    let special_query = "AC/DC & Guns N' Roses (夜に駆ける) #1 🎶";
    let res = client.search_catalog(special_query, None, 10).await;
    assert!(res.is_ok(), "Query with special characters must succeed");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_api_neg_08_concurrent_search_requests() {
    let (server_url, shutdown_tx) = spawn_mock_server(|_line, _req| {
        (200, MOCK_SEARCH_JSON.to_string())
    }).await;

    let auth = Arc::new(AuthManager::in_memory());
    let client = Arc::new(ApiClient::with_base_url(auth, server_url.clone(), server_url));

    let mut handles = Vec::new();
    for i in 0..15 {
        let client_clone = Arc::clone(&client);
        handles.push(tokio::spawn(async move {
            let res = client_clone.search_catalog(&format!("query-{}", i), None, 5).await;
            assert!(res.is_ok());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let _ = shutdown_tx.send(());
}
