pub mod queue;
pub mod source;
pub mod stream;

use queue::{QueueManager, RepeatMode};
use serde::{Deserialize, Serialize};
use source::SymphoniaSource;
use stream::StreamCache;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackStatus {
    pub state: PlaybackState,
    pub current_track_id: Option<String>,
    pub current_track_name: Option<String>,
    pub current_artist: Option<String>,
    pub duration_secs: f64,
    pub position_secs: f64,
    pub volume: f32,
    pub repeat_mode: RepeatMode,
    pub is_shuffle: bool,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            current_track_id: None,
            current_track_name: None,
            current_artist: None,
            duration_secs: 0.0,
            position_secs: 0.0,
            volume: 0.8,
            repeat_mode: RepeatMode::Off,
            is_shuffle: false,
        }
    }
}

pub enum AudioCommand {
    Play {
        path: PathBuf,
        track_id: String,
        title: String,
        artist: String,
        duration_secs: f64,
    },
    Pause,
    Resume,
    Stop,
    Seek {
        position_secs: f64,
    },
    SetVolume {
        volume: f32,
    },
}

pub struct Engine {
    pub status: Arc<Mutex<PlaybackStatus>>,
    pub queue: Arc<Mutex<QueueManager>>,
    pub stream_cache: Arc<StreamCache>,
    pub http_client: reqwest::Client,
    command_tx: Sender<AudioCommand>,
}

impl Engine {
    pub fn new() -> Self {
        let status = Arc::new(Mutex::new(PlaybackStatus::default()));
        let queue = Arc::new(Mutex::new(QueueManager::new()));
        let stream_cache = Arc::new(StreamCache::new(StreamCache::default_path()));
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        let (command_tx, command_rx) = channel::<AudioCommand>();

        let thread_status = Arc::clone(&status);
        let thread_queue = Arc::clone(&queue);

        std::thread::Builder::new()
            .name("apple-music-audio-worker".to_string())
            .spawn(move || {
                run_audio_worker(command_rx, thread_status, thread_queue);
            })
            .expect("Failed to spawn audio worker thread");

        Self {
            status,
            queue,
            stream_cache,
            http_client,
            command_tx,
        }
    }

    pub fn send_command(&self, cmd: AudioCommand) -> Result<(), String> {
        self.command_tx.send(cmd).map_err(|e| e.to_string())
    }

    pub fn play_track_file(
        &self,
        path: PathBuf,
        track_id: String,
        title: String,
        artist: String,
        duration_secs: f64,
    ) -> Result<(), String> {
        self.send_command(AudioCommand::Play {
            path,
            track_id,
            title,
            artist,
            duration_secs,
        })
    }

    pub async fn play_stream_url(
        &self,
        track_id: String,
        stream_url: String,
        title: String,
        artist: String,
        duration_secs: f64,
    ) -> Result<(), String> {
        let path = self
            .stream_cache
            .download_to_cache(&self.http_client, &track_id, &stream_url)
            .await?;
        self.play_track_file(path, track_id, title, artist, duration_secs)
    }

    pub fn pause(&self) -> Result<(), String> {
        self.send_command(AudioCommand::Pause)
    }

    pub fn resume(&self) -> Result<(), String> {
        self.send_command(AudioCommand::Resume)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.send_command(AudioCommand::Stop)
    }

    pub fn seek(&self, position_secs: f64) -> Result<(), String> {
        self.send_command(AudioCommand::Seek { position_secs })
    }

    pub fn set_volume(&self, volume: f32) -> Result<(), String> {
        let clamped = if volume.is_nan() {
            0.8
        } else {
            volume.clamp(0.0, 1.0)
        };
        if let Ok(mut s) = self.status.lock() {
            s.volume = clamped;
        }
        self.send_command(AudioCommand::SetVolume { volume: clamped })
    }
}

fn run_audio_worker(
    receiver: Receiver<AudioCommand>,
    status: Arc<Mutex<PlaybackStatus>>,
    queue: Arc<Mutex<QueueManager>>,
) {
    let (_stream, stream_handle) = match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => (Some(stream), Some(handle)),
        Err(e) => {
            log::error!("Audio device initialization failed: {}", e);
            (None, None)
        }
    };

    let mut current_sink: Option<rodio::Sink> = None;
    let mut current_file_path: Option<PathBuf> = None;
    let mut play_start: Option<Instant> = None;
    let mut accumulated_secs: f64 = 0.0;
    let mut track_duration: f64 = 0.0;

    loop {
        let timeout = Duration::from_millis(100);
        let cmd = receiver.recv_timeout(timeout);

        if let Ok(command) = cmd {
            match command {
                AudioCommand::Play {
                    path,
                    track_id,
                    title,
                    artist,
                    duration_secs,
                } => {
                    current_sink = None; // Drop existing sink
                    accumulated_secs = 0.0;
                    play_start = None;

                    if let Some(ref handle) = stream_handle {
                        match SymphoniaSource::new(&path) {
                            Ok(source) => {
                                let calculated_dur = source.duration_secs().unwrap_or(duration_secs);
                                track_duration = calculated_dur;

                                match rodio::Sink::try_new(handle) {
                                    Ok(sink) => {
                                        let vol = status.lock().map(|s| s.volume).unwrap_or(0.8);
                                        sink.set_volume(vol);
                                        sink.append(source);
                                        sink.play();

                                        current_sink = Some(sink);
                                        current_file_path = Some(path);
                                        play_start = Some(Instant::now());

                                        if let Ok(mut s) = status.lock() {
                                            s.state = PlaybackState::Playing;
                                            s.current_track_id = Some(track_id);
                                            s.current_track_name = Some(title);
                                            s.current_artist = Some(artist);
                                            s.duration_secs = calculated_dur;
                                            s.position_secs = 0.0;
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to create Rodio Sink: {}", e);
                                        if let Ok(mut s) = status.lock() {
                                            s.state = PlaybackState::Stopped;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to decode audio file {:?}: {}", path, e);
                                if let Ok(mut s) = status.lock() {
                                    s.state = PlaybackState::Stopped;
                                }
                            }
                        }
                    }
                }
                AudioCommand::Pause => {
                    if let Some(ref sink) = current_sink {
                        if !sink.is_paused() {
                            sink.pause();
                            if let Some(start) = play_start {
                                accumulated_secs += start.elapsed().as_secs_f64();
                                play_start = None;
                            }
                            if let Ok(mut s) = status.lock() {
                                s.state = PlaybackState::Paused;
                                s.position_secs = accumulated_secs.min(track_duration);
                            }
                        }
                    }
                }
                AudioCommand::Resume => {
                    if let Some(ref sink) = current_sink {
                        if sink.is_paused() {
                            sink.play();
                            play_start = Some(Instant::now());
                            if let Ok(mut s) = status.lock() {
                                s.state = PlaybackState::Playing;
                            }
                        }
                    }
                }
                AudioCommand::Stop => {
                    current_sink = None;
                    play_start = None;
                    accumulated_secs = 0.0;
                    if let Ok(mut s) = status.lock() {
                        s.state = PlaybackState::Stopped;
                        s.position_secs = 0.0;
                    }
                }
                AudioCommand::Seek { position_secs } => {
                    let target = position_secs.clamp(0.0, track_duration.max(0.0));
                    if let (Some(ref path), Some(ref handle)) = (&current_file_path, &stream_handle) {
                        match SymphoniaSource::new(path) {
                            Ok(mut source) => {
                                let _ = source.seek(target);
                                if let Ok(sink) = rodio::Sink::try_new(handle) {
                                    let vol = status.lock().map(|s| s.volume).unwrap_or(0.8);
                                    sink.set_volume(vol);
                                    sink.append(source);

                                    let is_paused = status.lock().map(|s| s.state == PlaybackState::Paused).unwrap_or(false);
                                    if is_paused {
                                        sink.pause();
                                        play_start = None;
                                    } else {
                                        sink.play();
                                        play_start = Some(Instant::now());
                                    }

                                    current_sink = Some(sink);
                                    accumulated_secs = target;
                                    if let Ok(mut s) = status.lock() {
                                        s.position_secs = target;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Seek failed for {:?}: {}", path, e);
                            }
                        }
                    }
                }
                AudioCommand::SetVolume { volume } => {
                    let clamped = if volume.is_nan() {
                        0.8
                    } else {
                        volume.clamp(0.0, 1.0)
                    };
                    if let Some(ref sink) = current_sink {
                        sink.set_volume(clamped);
                    }
                    if let Ok(mut s) = status.lock() {
                        s.volume = clamped;
                    }
                }
            }
        }

        // Periodic tick: update position or handle auto-advance
        if let Some(ref sink) = current_sink {
            if sink.empty() {
                // Current track finished naturally -> auto-advance
                current_sink = None;
                play_start = None;
                accumulated_secs = 0.0;

                let next_track = if let Ok(mut q) = queue.lock() {
                    q.next()
                } else {
                    None
                };

                if let Some(track) = next_track {
                    log::info!("Auto-advancing to next track: {}", track.title);
                    if let Some(ref handle) = stream_handle {
                        let path = PathBuf::from(&track.file_path);
                        match SymphoniaSource::new(&path) {
                            Ok(source) => {
                                let calc_dur = source.duration_secs().unwrap_or(track.duration_secs);
                                track_duration = calc_dur;

                                if let Ok(new_sink) = rodio::Sink::try_new(handle) {
                                    let vol = status.lock().map(|s| s.volume).unwrap_or(0.8);
                                    new_sink.set_volume(vol);
                                    new_sink.append(source);
                                    new_sink.play();

                                    current_sink = Some(new_sink);
                                    current_file_path = Some(path);
                                    play_start = Some(Instant::now());

                                    if let Ok(mut s) = status.lock() {
                                        s.state = PlaybackState::Playing;
                                        s.current_track_id = Some(track.id);
                                        s.current_track_name = Some(track.title);
                                        s.current_artist = Some(track.artist);
                                        s.duration_secs = calc_dur;
                                        s.position_secs = 0.0;
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to auto-advance to track {:?}: {}", track.file_path, e);
                                if let Ok(mut s) = status.lock() {
                                    s.state = PlaybackState::Stopped;
                                }
                            }
                        }
                    }
                } else {
                    // End of playlist
                    if let Ok(mut s) = status.lock() {
                        s.state = PlaybackState::Stopped;
                        s.position_secs = 0.0;
                    }
                }
            } else if let Some(start) = play_start {
                let cur = (accumulated_secs + start.elapsed().as_secs_f64()).min(track_duration);
                if let Ok(mut s) = status.lock() {
                    if s.state == PlaybackState::Playing {
                        s.position_secs = cur;
                    }
                }
            }
        }
    }
}
