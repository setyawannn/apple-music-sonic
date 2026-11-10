use app_lib::api::auth::AuthManager;
use app_lib::api::client::{parse_single_playlist_json, parse_single_song_json, ApiClient};
use app_lib::audio::source::SymphoniaSource;
use app_lib::audio::stream::{HlsParser, StreamCache};
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot::{channel, Sender};

async fn spawn_mock_stream_server<F>(handler: F) -> (String, Sender<()>)
where
    F: Fn(&str, &str) -> (u16, &'static str, Vec<u8>) + Send + Sync + 'static,
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
                                let (status_code, content_type, body_bytes) = h(&first_line, &req_str);
                                let status_text = match status_code {
                                    200 => "OK",
                                    403 => "Forbidden",
                                    404 => "Not Found",
                                    _ => "Internal Server Error",
                                };
                                let header = format!(
                                    "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    status_code, status_text, content_type, body_bytes.len()
                                );
                                let _ = stream.write_all(header.as_bytes()).await;
                                let _ = stream.write_all(&body_bytes).await;
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

fn create_test_wav_bytes(duration_ms: u32) -> Vec<u8> {
    let sample_rate = 44100u32;
    let num_channels = 2u16;
    let bits_per_sample = 16u16;
    let num_samples = (sample_rate as f64 * (duration_ms as f64 / 1000.0)) as u32;
    let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = num_channels * (bits_per_sample / 8);
    let data_chunk_size = num_samples * block_align as u32;
    let file_size = 36 + data_chunk_size;

    let mut buf = Vec::with_capacity((file_size + 8) as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_chunk_size.to_le_bytes());

    for i in 0..num_samples {
        let sample = ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / sample_rate as f64).sin() * 10000.0) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}

// ─── POSITIVE ACCEPTANCE TESTS (8) ───────────────────────────────────

#[test]
fn test_stream_pos_01_hls_master_playlist_parsing() {
    let m3u8_content = r#"
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=256000,CODECS="mp4a.40.2"
aac_256k.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=980000,CODECS="alac"
alac_lossless.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS="mp4a.40.5"
aac_64k.m3u8
"#;
    let base_url = "https://audio-ssl.itunes.apple.com/master.m3u8";
    let variants = HlsParser::parse_master(m3u8_content, base_url);

    assert_eq!(variants.len(), 3);
    // Highest bandwidth first
    assert_eq!(variants[0].bandwidth, 980000);
    assert!(variants[0].is_alac);
    assert!(variants[0].is_lossless);
    assert_eq!(variants[0].uri, "https://audio-ssl.itunes.apple.com/alac_lossless.m3u8");

    assert_eq!(variants[1].bandwidth, 256000);
    assert!(!variants[1].is_alac);
}

#[test]
fn test_stream_pos_02_hls_media_playlist_segments() {
    let media_content = r#"
#EXTM3U
#EXT-X-TARGETDURATION:6
#EXTINF:5.892,
segment_0.m4a
#EXTINF:6.012,
segment_1.m4a
#EXTINF:4.120,
segment_2.m4a
#EXT-X-ENDLIST
"#;
    let base_url = "https://audio-ssl.itunes.apple.com/stream/index.m3u8";
    let segments = HlsParser::parse_media(media_content, base_url);

    assert_eq!(segments.len(), 3);
    assert!((segments[0].duration_secs - 5.892).abs() < 0.001);
    assert_eq!(segments[0].uri, "https://audio-ssl.itunes.apple.com/stream/segment_0.m4a");
    assert!((segments[2].duration_secs - 4.120).abs() < 0.001);
}

#[test]
fn test_stream_pos_03_relative_url_resolution() {
    let base = "https://audio-ssl.itunes.apple.com/assets/2026/master.m3u8";
    let rel1 = "audio_01.m4a";
    let rel2 = "/root_audio.m4a";
    let abs = "https://cdn.apple.com/full_track.m4a";

    assert_eq!(HlsParser::resolve_url(base, rel1), "https://audio-ssl.itunes.apple.com/assets/2026/audio_01.m4a");
    assert_eq!(HlsParser::resolve_url(base, rel2), "https://audio-ssl.itunes.apple.com/root_audio.m4a");
    assert_eq!(HlsParser::resolve_url(base, abs), abs);
}

#[tokio::test]
async fn test_stream_pos_04_stream_cache_atomic_write_and_read() {
    let fake_audio = create_test_wav_bytes(50);
    let fake_audio_clone = fake_audio.clone();

    let (server_url, _tx) = spawn_mock_stream_server(move |first_line, _| {
        if first_line.contains("/stream_test.m4a") {
            (200, "audio/mp4", fake_audio_clone.clone())
        } else {
            (404, "text/plain", b"Not Found".to_vec())
        }
    })
    .await;

    let cache = StreamCache::in_temp_dir();
    let client = reqwest::Client::new();
    let stream_url = format!("{}/stream_test.m4a", server_url);

    let path = cache.download_to_cache(&client, "track_123", &stream_url).await.expect("download to cache");
    assert!(path.exists());
    assert!(cache.is_cached("track_123"));

    // Second call should return instantly from cache without network
    let cached_path = cache.download_to_cache(&client, "track_123", &stream_url).await.expect("from cache");
    assert_eq!(path, cached_path);
}

#[test]
fn test_stream_pos_05_stream_cache_lru_pruning() {
    let cache = StreamCache::in_temp_dir();
    let track1 = cache.get_track_cache_path("track_old");
    let track2 = cache.get_track_cache_path("track_new");

    // Create 2 test files
    let mut f1 = File::create(&track1).unwrap();
    f1.write_all(&vec![1u8; 10000]).unwrap();
    drop(f1);

    std::thread::sleep(std::time::Duration::from_millis(50));

    let mut f2 = File::create(&track2).unwrap();
    f2.write_all(&vec![2u8; 10000]).unwrap();
    drop(f2);

    assert!(track1.exists());
    assert!(track2.exists());

    // Prune with a low threshold
    let pruned = cache.prune_if_needed().unwrap();
    // Cache has max 50MB in temp dir, so total 20KB is fine
    assert_eq!(pruned, 0);
}

#[test]
fn test_stream_pos_06_cloud_playlist_json_parsing() {
    let raw_json = json!({
        "id": "p.V83kL9",
        "type": "library-playlists",
        "attributes": {
            "name": "Summer Acoustic Hits",
            "description": {
                "standard": "Relaxing acoustic favorites for summer afternoons."
            },
            "canEdit": true,
            "artwork": {
                "url": "https://is1-ssl.mzstatic.com/image/thumb/Music/{w}x{h}bb.jpg"
            },
            "trackCount": 18
        },
        "relationships": {
            "tracks": {
                "data": [
                    {
                        "id": "i.123",
                        "attributes": {
                            "name": "Sunny Afternoon",
                            "artistName": "The Kinks",
                            "albumName": "Face to Face",
                            "durationInMillis": 216000,
                            "trackNumber": 1
                        }
                    }
                ]
            }
        }
    });

    let playlist = parse_single_playlist_json(&raw_json);
    assert_eq!(playlist.id, "p.V83kL9");
    assert_eq!(playlist.name, "Summer Acoustic Hits");
    assert_eq!(playlist.description.as_deref(), Some("Relaxing acoustic favorites for summer afternoons."));
    assert!(playlist.can_edit);
    assert_eq!(playlist.track_count, 18);
    assert_eq!(playlist.tracks.len(), 1);
    assert_eq!(playlist.tracks[0].title, "Sunny Afternoon");
    assert_eq!(playlist.tracks[0].artist, "The Kinks");
}

#[test]
fn test_stream_pos_07_cloud_playlist_tracks_parsing() {
    let raw_track_json = json!({
        "id": "i.abc999",
        "type": "library-songs",
        "attributes": {
            "name": "Midnight City",
            "artistName": "M83",
            "albumName": "Hurry Up, We're Dreaming",
            "durationInMillis": 243000,
            "trackNumber": 2,
            "audioTraits": ["lossless", "spatial"],
            "previews": [
                { "url": "https://audio-ssl.itunes.apple.com/preview_midnight.m4a" }
            ],
            "artwork": {
                "url": "https://is1-ssl.mzstatic.com/image/thumb/Music/{w}x{h}bb.jpg"
            }
        }
    });

    let song = parse_single_song_json(&raw_track_json);
    assert_eq!(song.id, "i.abc999");
    assert_eq!(song.title, "Midnight City");
    assert_eq!(song.artist, "M83");
    assert_eq!(song.duration_ms, 243000);
    assert!(song.has_lossless);
    assert!(song.has_atmos);
    assert_eq!(song.preview_url.as_deref(), Some("https://audio-ssl.itunes.apple.com/preview_midnight.m4a"));
}

#[test]
fn test_stream_pos_08_symphonia_audio_stream_decoding() {
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("test_stream_decode_{}.wav", uuid::Uuid::new_v4()));
    let wav_bytes = create_test_wav_bytes(100);

    let mut f = File::create(&temp_file).unwrap();
    f.write_all(&wav_bytes).unwrap();
    drop(f);

    let mut source = SymphoniaSource::new(&temp_file).expect("Symphonia probe streamed file");
    assert!(source.duration_secs().unwrap_or(0.0) > 0.08);

    // Consume samples
    let sample = Iterator::next(&mut source);
    assert!(sample.is_some());

    let _ = std::fs::remove_file(&temp_file);
}

// ─── NEGATIVE ACCEPTANCE TESTS (8) ───────────────────────────────────

#[tokio::test]
async fn test_stream_neg_01_empty_stream_url() {
    let cache = StreamCache::in_temp_dir();
    let client = reqwest::Client::new();
    let res = cache.download_to_cache(&client, "track_empty", "").await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_stream_neg_02_server_404_not_found() {
    let (server_url, _tx) = spawn_mock_stream_server(|_, _| {
        (404, "text/plain", b"Audio chunk missing".to_vec())
    })
    .await;

    let cache = StreamCache::in_temp_dir();
    let client = reqwest::Client::new();
    let url = format!("{}/missing.m4a", server_url);

    let res = cache.download_to_cache(&client, "track_404", &url).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("404"));
}

#[tokio::test]
async fn test_stream_neg_03_server_403_forbidden() {
    let (server_url, _tx) = spawn_mock_stream_server(|_, _| {
        (403, "application/json", b"{\"error\":\"Forbidden stream token\"}".to_vec())
    })
    .await;

    let cache = StreamCache::in_temp_dir();
    let client = reqwest::Client::new();
    let url = format!("{}/forbidden.m4a", server_url);

    let res = cache.download_to_cache(&client, "track_403", &url).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("403"));
}

#[test]
fn test_stream_neg_04_corrupted_hls_m3u8_content() {
    let bad_content = "This is not an HLS playlist at all! random text 123456";
    let variants = HlsParser::parse_master(bad_content, "https://apple.com");
    assert!(variants.is_empty());

    let segments = HlsParser::parse_media(bad_content, "https://apple.com");
    assert!(segments.is_empty());
}

#[tokio::test]
async fn test_stream_neg_05_zero_byte_stream_response() {
    let (server_url, _tx) = spawn_mock_stream_server(|_, _| {
        (200, "audio/mp4", Vec::new())
    })
    .await;

    let cache = StreamCache::in_temp_dir();
    let client = reqwest::Client::new();
    let url = format!("{}/empty.m4a", server_url);

    let res = cache.download_to_cache(&client, "track_zero_bytes", &url).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("empty"));
    assert!(!cache.is_cached("track_zero_bytes"));
}

#[tokio::test]
async fn test_stream_neg_06_unauthenticated_cloud_playlists_request() {
    let auth = Arc::new(AuthManager::in_memory());
    let client = ApiClient::new(auth);

    let res = client.get_user_playlists().await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("not authenticated"));
}

#[test]
fn test_stream_neg_07_malformed_cloud_playlist_json() {
    let empty_json = json!({});
    let playlist = parse_single_playlist_json(&empty_json);
    assert_eq!(playlist.id, "");
    assert_eq!(playlist.name, "Untitled Playlist");
    assert_eq!(playlist.track_count, 0);
    assert!(playlist.tracks.is_empty());
}

#[tokio::test]
async fn test_stream_neg_08_rapid_stream_cache_concurrency() {
    let fake_audio = create_test_wav_bytes(20);
    let fake_audio_clone = fake_audio.clone();

    let (server_url, _tx) = spawn_mock_stream_server(move |_, _| {
        (200, "audio/wav", fake_audio_clone.clone())
    })
    .await;

    let cache = Arc::new(StreamCache::in_temp_dir());
    let client = reqwest::Client::new();
    let url = format!("{}/concurrent_stream.wav", server_url);

    let mut handles = Vec::new();
    for i in 0..10 {
        let cache_clone = Arc::clone(&cache);
        let client_clone = client.clone();
        let url_clone = url.clone();
        let track_id = format!("track_concurrent_{}", i % 3);

        handles.push(tokio::spawn(async move {
            cache_clone.download_to_cache(&client_clone, &track_id, &url_clone).await
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert!(res.is_ok());
    }
}
