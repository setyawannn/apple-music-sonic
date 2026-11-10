use app_lib::audio::queue::{QueueManager, RepeatMode};
use app_lib::audio::source::SymphoniaSource;
use app_lib::audio::{Engine, PlaybackState};
use app_lib::library::Track;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_file_path(ext: &str) -> PathBuf {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut dir = std::env::temp_dir();
    dir.push(format!("app_music_test_{}_{}.{}", std::process::id(), id, ext));
    dir
}

fn create_synthetic_wav(
    path: &Path,
    duration_secs: f32,
    sample_rate: u32,
    channels: u16,
) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    let total_frames = (duration_secs * sample_rate as f32) as u32;
    let num_samples = total_frames * channels as u32;
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;
    let subchunk2_size = num_samples * 2;
    let chunk_size = 36 + subchunk2_size;

    // RIFF chunk
    file.write_all(b"RIFF")?;
    file.write_all(&chunk_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    // fmt subchunk
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?; // Subchunk1Size (16 for PCM)
    file.write_all(&1u16.to_le_bytes())?; // AudioFormat (1 = PCM)
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?; // BitsPerSample

    // data subchunk
    file.write_all(b"data")?;
    file.write_all(&subchunk2_size.to_le_bytes())?;

    // 440 Hz Sine wave
    let freq = 440.0f32;
    for frame in 0..total_frames {
        let t = frame as f32 / sample_rate as f32;
        let sample_val = (t * freq * 2.0 * std::f32::consts::PI).sin();
        let pcm = (sample_val * 30000.0) as i16;
        for _ in 0..channels {
            file.write_all(&pcm.to_le_bytes())?;
        }
    }

    file.flush()?;
    Ok(())
}

fn make_dummy_track(id: &str, title: &str, path: &str, duration: f64) -> Track {
    Track {
        id: id.to_string(),
        title: title.to_string(),
        artist: "Test Artist".to_string(),
        album: "Test Album".to_string(),
        album_artist: "Test Artist".to_string(),
        track_number: 1,
        disc_number: 1,
        duration_secs: duration,
        file_path: path.to_string(),
        file_size: 1024,
        genre: "Test".to_string(),
        year: 2026,
        art_path: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// POSITIVE ACCEPTANCE TESTS (POS-01 .. POS-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_pos_01_wav_probe_and_metadata() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 2.0, 44100, 2).expect("create wav");

    let source = SymphoniaSource::new(&path).expect("probe source");
    assert_eq!(rodio::Source::sample_rate(&source), 44100);
    assert_eq!(rodio::Source::channels(&source), 2);

    let dur = source.duration_secs().expect("duration exists");
    assert!((dur - 2.0).abs() < 0.1, "Duration expected ~2.0s, got {}", dur);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_pos_02_sample_iteration_bounds() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 0.5, 44100, 1).expect("create wav");

    let source = SymphoniaSource::new(&path).expect("probe source");
    let mut count = 0;
    for sample in source {
        assert!(!sample.is_nan(), "Sample should not be NaN");
        assert!(!sample.is_infinite(), "Sample should not be Infinite");
        assert!(
            sample >= -1.0 && sample <= 1.0,
            "Sample value {} out of [-1.0, 1.0] bounds",
            sample
        );
        count += 1;
    }

    assert!(count > 0, "Iterator should yield samples");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_pos_03_playback_lifecycle() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 1.0, 44100, 2).expect("create wav");

    let engine = Engine::new();

    // 1. Initial State
    {
        let status = engine.status.lock().unwrap();
        assert_eq!(status.state, PlaybackState::Stopped);
    }

    // 2. Play
    engine
        .play_track_file(
            path.clone(),
            "track-1".to_string(),
            "Title 1".to_string(),
            "Artist 1".to_string(),
            1.0,
        )
        .expect("play");

    thread::sleep(Duration::from_millis(250));
    {
        let status = engine.status.lock().unwrap();
        assert_eq!(status.state, PlaybackState::Playing);
        assert_eq!(status.current_track_id.as_deref(), Some("track-1"));
    }

    // 3. Pause
    engine.pause().expect("pause");
    thread::sleep(Duration::from_millis(150));
    {
        let status = engine.status.lock().unwrap();
        assert_eq!(status.state, PlaybackState::Paused);
    }

    // 4. Resume
    engine.resume().expect("resume");
    thread::sleep(Duration::from_millis(150));
    {
        let status = engine.status.lock().unwrap();
        assert_eq!(status.state, PlaybackState::Playing);
    }

    // 5. Stop
    engine.stop().expect("stop");
    thread::sleep(Duration::from_millis(150));
    {
        let status = engine.status.lock().unwrap();
        assert_eq!(status.state, PlaybackState::Stopped);
        assert_eq!(status.position_secs, 0.0);
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_pos_04_volume_control() {
    let engine = Engine::new();

    for vol in [0.0f32, 0.25, 0.5, 0.85, 1.0] {
        engine.set_volume(vol).expect("set_volume");
        thread::sleep(Duration::from_millis(50));
        let status = engine.status.lock().unwrap();
        assert!((status.volume - vol).abs() < 1e-4);
    }
}

#[test]
fn test_pos_05_accurate_seeking() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 5.0, 44100, 2).expect("create wav");

    let engine = Engine::new();
    engine
        .play_track_file(
            path.clone(),
            "seek-track".to_string(),
            "Seek Track".to_string(),
            "Artist".to_string(),
            5.0,
        )
        .expect("play");

    thread::sleep(Duration::from_millis(200));

    // Seek to 2.5s
    engine.seek(2.5).expect("seek");
    thread::sleep(Duration::from_millis(200));

    {
        let status = engine.status.lock().unwrap();
        assert!(
            status.position_secs >= 2.4,
            "Expected position >= 2.4s after seek, got {}",
            status.position_secs
        );
    }

    engine.stop().expect("stop");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_pos_06_queue_navigation() {
    let mut q = QueueManager::new();
    let t1 = make_dummy_track("1", "Song 1", "p1.mp3", 100.0);
    let t2 = make_dummy_track("2", "Song 2", "p2.mp3", 120.0);
    let t3 = make_dummy_track("3", "Song 3", "p3.mp3", 140.0);

    q.set_queue(vec![t1.clone(), t2.clone(), t3.clone()], Some(0));

    assert_eq!(q.current_track().map(|t| t.id.as_str()), Some("1"));

    // Next -> 2
    let next1 = q.next().expect("next");
    assert_eq!(next1.id, "2");

    // Next -> 3
    let next2 = q.next().expect("next");
    assert_eq!(next2.id, "3");

    // Prev -> 2
    let prev1 = q.prev().expect("prev");
    assert_eq!(prev1.id, "2");
}

#[test]
fn test_pos_07_queue_repeat_modes() {
    let mut q = QueueManager::new();
    let t1 = make_dummy_track("1", "Song 1", "p1.mp3", 100.0);
    let t2 = make_dummy_track("2", "Song 2", "p2.mp3", 120.0);
    q.set_queue(vec![t1.clone(), t2.clone()], Some(1)); // at last track

    // 1. RepeatMode::Off
    q.set_repeat_mode(RepeatMode::Off);
    assert!(q.next().is_none(), "Should be None at end of queue in Off mode");

    // 2. RepeatMode::All
    q.set_queue(vec![t1.clone(), t2.clone()], Some(1));
    q.set_repeat_mode(RepeatMode::All);
    let looped = q.next().expect("looped track");
    assert_eq!(looped.id, "1", "Should loop back to start in All mode");

    // 3. RepeatMode::One
    q.set_repeat_mode(RepeatMode::One);
    let repeated = q.next().expect("same track");
    assert_eq!(repeated.id, "1", "Should stay on same track in One mode");
}

#[test]
fn test_pos_08_queue_shuffle() {
    let mut q = QueueManager::new();
    let tracks: Vec<Track> = (0..10)
        .map(|i| make_dummy_track(&i.to_string(), &format!("Song {}", i), "path.mp3", 100.0))
        .collect();

    q.set_queue(tracks.clone(), Some(0));
    q.set_shuffle(true);

    let mut visited_ids = Vec::new();
    if let Some(cur) = q.current_track() {
        visited_ids.push(cur.id.clone());
    }

    while let Some(next) = q.next() {
        visited_ids.push(next.id);
    }

    assert_eq!(visited_ids.len(), 10, "All tracks should be visited");
    for t in &tracks {
        assert!(
            visited_ids.contains(&t.id),
            "Track {} missing from shuffled queue",
            t.id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NEGATIVE & BOUNDARY ACCEPTANCE TESTS (NEG-01 .. NEG-08)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_neg_01_non_existent_file() {
    let non_existent = PathBuf::from("Z:\\does_not_exist_xyz123.mp3");
    let res = SymphoniaSource::new(&non_existent);
    assert!(res.is_err(), "Opening non-existent file must return Err");

    // Engine should also handle non-existent file gracefully without crashing
    let engine = Engine::new();
    let play_res = engine.play_track_file(
        non_existent,
        "bad-id".to_string(),
        "Bad Track".to_string(),
        "Bad Artist".to_string(),
        100.0,
    );
    assert!(play_res.is_ok(), "Command should be dispatched safely");

    thread::sleep(Duration::from_millis(200));
    let status = engine.status.lock().unwrap();
    assert_ne!(status.state, PlaybackState::Playing);
}

#[test]
fn test_neg_02_corrupted_audio_file() {
    let path = temp_file_path("mp3");
    let mut file = File::create(&path).expect("create corrupt file");
    file.write_all(b"THIS IS NOT A VALID MP3 FILE OR HEADER AT ALL! RANDOM TEXT")
        .expect("write text");
    file.flush().unwrap();

    let res = SymphoniaSource::new(&path);
    assert!(res.is_err(), "Decoding plain text as mp3 must fail safely");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_neg_03_zero_byte_file() {
    let path = temp_file_path("wav");
    let _ = File::create(&path).expect("create 0-byte file");

    let res = SymphoniaSource::new(&path);
    assert!(res.is_err(), "0-byte audio file must fail probing gracefully");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_neg_04_seek_out_of_bounds_high() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 2.0, 44100, 2).expect("create wav");

    let engine = Engine::new();
    engine
        .play_track_file(
            path.clone(),
            "oob-seek".to_string(),
            "Seek Track".to_string(),
            "Artist".to_string(),
            2.0,
        )
        .expect("play");

    thread::sleep(Duration::from_millis(200));

    // Seek way beyond duration (e.g. 9999s)
    let res = engine.seek(9999.0);
    assert!(res.is_ok(), "Seek dispatch should succeed");

    thread::sleep(Duration::from_millis(200));
    {
        let status = engine.status.lock().unwrap();
        // Clamped to max duration or finished
        assert!(status.position_secs <= 2.1);
    }

    engine.stop().expect("stop");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_neg_05_seek_out_of_bounds_negative() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 3.0, 44100, 2).expect("create wav");

    let engine = Engine::new();
    engine
        .play_track_file(
            path.clone(),
            "neg-seek".to_string(),
            "Seek Track".to_string(),
            "Artist".to_string(),
            3.0,
        )
        .expect("play");

    thread::sleep(Duration::from_millis(200));

    // Seek to -50.0s
    engine.seek(-50.0).expect("seek");
    thread::sleep(Duration::from_millis(200));

    {
        let status = engine.status.lock().unwrap();
        assert!(status.position_secs >= 0.0);
    }

    engine.stop().expect("stop");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_neg_06_volume_out_of_bounds() {
    let engine = Engine::new();

    // Negative volume -> clamped to 0.0
    engine.set_volume(-2.5).expect("set_volume");
    thread::sleep(Duration::from_millis(50));
    assert_eq!(engine.status.lock().unwrap().volume, 0.0);

    // Over 1.0 -> clamped to 1.0
    engine.set_volume(50.0).expect("set_volume");
    thread::sleep(Duration::from_millis(50));
    assert_eq!(engine.status.lock().unwrap().volume, 1.0);

    // NaN -> falls back to default 0.8
    engine.set_volume(f32::NAN).expect("set_volume");
    thread::sleep(Duration::from_millis(50));
    assert_eq!(engine.status.lock().unwrap().volume, 0.8);
}

#[test]
fn test_neg_07_empty_queue_operations() {
    let mut q = QueueManager::new();
    assert!(q.is_empty());
    assert!(q.current_track().is_none());
    assert!(q.next().is_none(), "next() on empty queue must be None");
    assert!(q.prev().is_none(), "prev() on empty queue must be None");

    q.set_shuffle(true);
    assert!(q.next().is_none());

    q.set_repeat_mode(RepeatMode::All);
    assert!(q.next().is_none());
}

#[test]
fn test_neg_08_rapid_concurrent_commands() {
    let path = temp_file_path("wav");
    create_synthetic_wav(&path, 5.0, 44100, 2).expect("create wav");

    let engine = Arc::new(Engine::new());
    let mut handles = Vec::new();

    for i in 0..10 {
        let eng = Arc::clone(&engine);
        let p = path.clone();
        handles.push(thread::spawn(move || {
            for j in 0..20 {
                match (i + j) % 5 {
                    0 => {
                        let _ = eng.play_track_file(
                            p.clone(),
                            format!("concurrent-{}", i),
                            "Title".to_string(),
                            "Artist".to_string(),
                            5.0,
                        );
                    }
                    1 => {
                        let _ = eng.pause();
                    }
                    2 => {
                        let _ = eng.resume();
                    }
                    3 => {
                        let _ = eng.seek((j as f64) * 0.2);
                    }
                    _ => {
                        let _ = eng.set_volume(((j % 10) as f32) / 10.0);
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    // Engine must still be healthy and lockable without deadlocks
    let status = engine.status.lock().unwrap();
    assert!(status.volume >= 0.0 && status.volume <= 1.0);

    let _ = engine.stop();
    let _ = std::fs::remove_file(&path);
}
