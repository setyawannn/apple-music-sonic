use app_lib::library::artwork::ArtworkManager;
use app_lib::library::db::Database;
use app_lib::library::metadata::extract_metadata;
use app_lib::library::playlist::PlaylistFileHelper;
use app_lib::library::scanner::Scanner;
use app_lib::library::{Library, Track};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

static COUNTER: AtomicUsize = AtomicUsize::new(100);

fn temp_path(ext: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut dir = std::env::temp_dir();
    dir.push(format!("lib_test_{}_{}.{}", std::process::id(), id, ext));
    dir
}

fn create_wav_file(path: &Path, duration_secs: f32) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    let sample_rate = 44100u32;
    let channels = 2u16;
    let total_frames = (duration_secs * sample_rate as f32) as u32;
    let num_samples = total_frames * channels as u32;
    let subchunk2_size = num_samples * 2;
    let chunk_size = 36 + subchunk2_size;

    file.write_all(b"RIFF")?;
    file.write_all(&chunk_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * channels as u32 * 2).to_le_bytes())?;
    file.write_all(&(channels * 2).to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&subchunk2_size.to_le_bytes())?;

    for _ in 0..num_samples {
        file.write_all(&0i16.to_le_bytes())?;
    }
    file.flush()?;
    Ok(())
}

fn make_track(id: &str, title: &str, artist: &str, album: &str, path: &str) -> Track {
    Track {
        id: id.to_string(),
        title: title.to_string(),
        artist: artist.to_string(),
        album: album.to_string(),
        album_artist: artist.to_string(),
        track_number: 1,
        disc_number: 1,
        duration_secs: 180.0,
        file_path: path.to_string(),
        file_size: 4096,
        genre: "Rock".to_string(),
        year: 2026,
        art_path: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// POSITIVE ACCEPTANCE TESTS (LIB-POS-01 .. LIB-POS-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lib_pos_01_db_initialization_and_schema() {
    let db_file = temp_path("db");
    let db = Database::new(&db_file).expect("initialize sqlite");

    let tracks = db.get_all_tracks().expect("get tracks");
    assert!(tracks.is_empty());

    let playlists = db.get_playlists().expect("get playlists");
    assert!(playlists.is_empty());

    let _ = std::fs::remove_file(&db_file);
}

#[test]
fn test_lib_pos_02_track_crud_lifecycle() {
    let db = Database::in_memory().expect("in-memory db");
    let mut track = make_track("t1", "Bohemian Rhapsody", "Queen", "A Night at the Opera", "C:\\queen.mp3");

    // Insert
    db.insert_or_replace_track(&track, 1000).expect("insert track");
    assert_eq!(db.get_all_tracks().unwrap().len(), 1);

    // Read by ID
    let fetched = db.get_track_by_id("t1").unwrap().expect("found");
    assert_eq!(fetched.title, "Bohemian Rhapsody");
    assert_eq!(fetched.artist, "Queen");

    // Update
    track.genre = "Classic Rock".to_string();
    db.insert_or_replace_track(&track, 1001).expect("update track");
    let updated = db.get_track_by_id("t1").unwrap().unwrap();
    assert_eq!(updated.genre, "Classic Rock");

    // Delete
    db.delete_track_by_path(&track.file_path).expect("delete track");
    assert!(db.get_track_by_id("t1").unwrap().is_none());
}

#[test]
fn test_lib_pos_03_album_and_artist_aggregation() {
    let db = Database::in_memory().expect("in-memory db");
    let t1 = make_track("1", "Song 1", "Artist A", "Album X", "p1.mp3");
    let t2 = make_track("2", "Song 2", "Artist A", "Album X", "p2.mp3");
    let t3 = make_track("3", "Song 3", "Artist B", "Album Y", "p3.mp3");

    db.insert_or_replace_track(&t1, 1).unwrap();
    db.insert_or_replace_track(&t2, 2).unwrap();
    db.insert_or_replace_track(&t3, 3).unwrap();

    let albums = db.get_albums().expect("get albums");
    assert_eq!(albums.len(), 2);
    let album_x = albums.iter().find(|a| a.title == "Album X").unwrap();
    assert_eq!(album_x.track_count, 2);

    let artists = db.get_artists().expect("get artists");
    assert_eq!(artists.len(), 2);
    let artist_a = artists.iter().find(|a| a.name == "Artist A").unwrap();
    assert_eq!(artist_a.track_count, 2);
    assert_eq!(artist_a.album_count, 1);
}

#[test]
fn test_lib_pos_04_incremental_scanner_performance() {
    let temp_dir = std::env::temp_dir().join(format!("scan_test_{}", COUNTER.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wav1 = temp_dir.join("track1.wav");
    let wav2 = temp_dir.join("track2.wav");
    create_wav_file(&wav1, 0.5).unwrap();
    create_wav_file(&wav2, 0.5).unwrap();

    let db = Database::in_memory().unwrap();
    let art_mgr = ArtworkManager::new(temp_dir.join("art_cache"));

    // First scan: parses 2 files
    let count1 = Scanner::scan_directory(&temp_dir, &db, &art_mgr).expect("first scan");
    assert_eq!(count1, 2);

    // Second scan (incremental): no modifications, finishes in <50ms
    let start = std::time::Instant::now();
    let count2 = Scanner::scan_directory(&temp_dir, &db, &art_mgr).expect("second scan");
    let duration = start.elapsed();

    assert_eq!(count2, 2);
    assert!(duration.as_millis() < 100, "Incremental scan should complete under 100ms");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_lib_pos_05_metadata_tag_extraction() {
    let wav = temp_path("wav");
    create_wav_file(&wav, 1.0).unwrap();

    let meta = extract_metadata(&wav).expect("extract metadata");
    // Fallback title should match file stem
    assert_eq!(meta.title, wav.file_stem().unwrap().to_str().unwrap());
    assert!((meta.duration_secs - 1.0).abs() < 0.1);

    let _ = std::fs::remove_file(&wav);
}

#[test]
fn test_lib_pos_06_embedded_artwork_cache() {
    let art_dir = std::env::temp_dir().join(format!("art_test_{}", COUNTER.fetch_add(1, Ordering::SeqCst)));
    let art_mgr = ArtworkManager::new(&art_dir);

    let dummy_image_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46]; // JPEG header
    let cached_path = art_mgr
        .save_embedded_art("Abbey Road - The Beatles", &dummy_image_bytes, Some("image/jpeg"))
        .expect("save art");

    assert!(Path::new(&cached_path).exists());
    let read_bytes = std::fs::read(&cached_path).unwrap();
    assert_eq!(read_bytes, dummy_image_bytes);

    let _ = std::fs::remove_dir_all(&art_dir);
}

#[test]
fn test_lib_pos_07_m3u_import_and_export() {
    let m3u_path = temp_path("m3u8");
    let tracks = vec![
        make_track("1", "Song A", "Artist A", "Album A", "C:\\Music\\songA.mp3"),
        make_track("2", "Song B", "Artist B", "Album B", "C:\\Music\\songB.mp3"),
    ];

    // Export
    PlaylistFileHelper::export_m3u8("My Playlist", &tracks, &m3u_path).expect("export m3u8");
    assert!(m3u_path.exists());

    // Import
    let imported_paths = PlaylistFileHelper::import_m3u8(&m3u_path).expect("import m3u8");
    assert_eq!(imported_paths.len(), 2);
    assert_eq!(imported_paths[0], "C:\\Music\\songA.mp3");
    assert_eq!(imported_paths[1], "C:\\Music\\songB.mp3");

    let _ = std::fs::remove_file(&m3u_path);
}

#[test]
fn test_lib_pos_08_pls_import_and_export() {
    let pls_path = temp_path("pls");
    let tracks = vec![
        make_track("1", "Song A", "Artist A", "Album A", "C:\\Music\\songA.mp3"),
        make_track("2", "Song B", "Artist B", "Album B", "C:\\Music\\songB.mp3"),
    ];

    // Export
    PlaylistFileHelper::export_pls(&tracks, &pls_path).expect("export pls");
    assert!(pls_path.exists());

    // Import
    let imported_paths = PlaylistFileHelper::import_pls(&pls_path).expect("import pls");
    assert_eq!(imported_paths.len(), 2);
    assert_eq!(imported_paths[0], "C:\\Music\\songA.mp3");
    assert_eq!(imported_paths[1], "C:\\Music\\songB.mp3");

    let _ = std::fs::remove_file(&pls_path);
}

// ═══════════════════════════════════════════════════════════════════════════
// NEGATIVE & BOUNDARY ACCEPTANCE TESTS (LIB-NEG-01 .. LIB-NEG-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lib_neg_01_missing_audio_metadata_fallback() {
    let wav = temp_path("wav");
    create_wav_file(&wav, 0.5).unwrap();

    let meta = extract_metadata(&wav).unwrap();
    // Must gracefully fall back to safe defaults without panic
    assert!(!meta.title.is_empty());
    assert_eq!(meta.artist, "Unknown Artist");
    assert_eq!(meta.album, "Unknown Album");
    assert_eq!(meta.track_number, 0);

    let _ = std::fs::remove_file(&wav);
}

#[test]
fn test_lib_neg_02_duplicate_file_path_rescan() {
    let db = Database::in_memory().unwrap();
    let track = make_track("t1", "Title", "Artist", "Album", "C:\\test.mp3");

    // Inserting twice with same file_path must NOT violate UNIQUE constraint (upsert succeeds)
    let res1 = db.insert_or_replace_track(&track, 100);
    assert!(res1.is_ok());

    let res2 = db.insert_or_replace_track(&track, 200);
    assert!(res2.is_ok());

    assert_eq!(db.get_all_tracks().unwrap().len(), 1);
}

#[test]
fn test_lib_neg_03_corrupted_playlist_file_import() {
    let bad_path = temp_path("m3u");
    let mut file = File::create(&bad_path).unwrap();
    file.write_all(b"### NOT A VALID PLAYLIST\n\n#EXTM3U\n").unwrap();

    let paths = PlaylistFileHelper::import_m3u8(&bad_path).unwrap();
    assert!(paths.is_empty(), "Empty/comment-only playlist should yield empty list safely");

    let _ = std::fs::remove_file(&bad_path);
}

#[test]
fn test_lib_neg_04_playlist_with_missing_track_files() {
    let db = Database::in_memory().unwrap();
    let art_dir = temp_path("art");
    let lib = Library::with_db(db, ArtworkManager::new(art_dir));

    let m3u = temp_path("m3u8");
    let mut file = File::create(&m3u).unwrap();
    writeln!(file, "#EXTM3U\nZ:\\missing_track_123.mp3").unwrap();

    // Import playlist referring to missing file: shouldn't panic, creates empty playlist
    let pl = lib.import_playlist_file(&m3u).expect("import missing file playlist");
    assert_eq!(pl.tracks.len(), 0);

    let _ = std::fs::remove_file(&m3u);
}

#[test]
fn test_lib_neg_05_corrupted_artwork_stream() {
    let art_dir = temp_path("art");
    let art_mgr = ArtworkManager::new(&art_dir);

    // Empty data -> returns None gracefully
    let res_empty = art_mgr.save_embedded_art("album", &[], Some("image/jpeg"));
    assert!(res_empty.is_none());

    let _ = std::fs::remove_dir_all(&art_dir);
}

#[test]
fn test_lib_neg_06_prune_deleted_files_on_rescan() {
    let db = Database::in_memory().unwrap();
    let t1 = make_track("1", "Song 1", "Artist", "Album", "C:\\existing.mp3");
    let t2 = make_track("2", "Song 2", "Artist", "Album", "C:\\deleted.mp3");

    db.insert_or_replace_track(&t1, 1).unwrap();
    db.insert_or_replace_track(&t2, 1).unwrap();
    assert_eq!(db.get_all_tracks().unwrap().len(), 2);

    // Prune tracks not present in active list
    let active_paths = vec!["C:\\existing.mp3".to_string()];
    let pruned = db.prune_missing_tracks(&active_paths).unwrap();
    assert_eq!(pruned, 1);

    let remaining = db.get_all_tracks().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "1");
}

#[test]
fn test_lib_neg_07_special_characters_in_metadata() {
    let db = Database::in_memory().unwrap();
    let tricky_title = "夜に駆ける 'YOASOBI' / \"Run into the night\" -- 🎶 (Special & <Tag>)";
    let track = make_track("jp1", tricky_title, "YOASOBI", "THE BOOK", "C:\\yoasobi.mp3");

    db.insert_or_replace_track(&track, 1).expect("insert unicode");
    let fetched = db.get_track_by_id("jp1").unwrap().unwrap();
    assert_eq!(fetched.title, tricky_title);
}

#[test]
fn test_lib_neg_08_concurrent_db_read_write() {
    let db = Arc::new(Database::in_memory().unwrap());
    let mut handles = Vec::new();

    for i in 0..10 {
        let db_clone = Arc::clone(&db);
        handles.push(thread::spawn(move || {
            for j in 0..20 {
                let id = format!("track-{}-{}", i, j);
                let path = format!("C:\\path_{}_{}.mp3", i, j);
                let t = make_track(&id, "Title", "Artist", "Album", &path);
                let _ = db_clone.insert_or_replace_track(&t, 1);
                let _ = db_clone.get_all_tracks();
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    let all = db.get_all_tracks().unwrap();
    assert_eq!(all.len(), 200, "All 200 concurrent tracks should be written without data loss");
}
