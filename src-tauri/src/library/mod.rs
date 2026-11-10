pub mod artwork;
pub mod db;
pub mod metadata;
pub mod playlist;
pub mod scanner;

pub use artwork::ArtworkManager;
pub use db::Database;
pub use playlist::PlaylistFileHelper;
pub use scanner::Scanner;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: u32,
    pub disc_number: u32,
    pub duration_secs: f64,
    pub file_path: String,
    pub file_size: u64,
    pub genre: String,
    pub year: i32,
    pub art_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: i32,
    pub track_count: u32,
    pub art_path: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Artist {
    pub name: String,
    pub track_count: u32,
    pub album_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tracks: Vec<String>,
    pub is_folder: bool,
    pub parent_id: Option<String>,
}

pub struct Library {
    pub db: Arc<Database>,
    pub artwork_manager: Arc<ArtworkManager>,
}

impl Library {
    pub fn new() -> Self {
        let base_dir = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("AppleMusicRS");

        let db_path = base_dir.join("library.db");
        let art_dir = base_dir.join("art_cache");

        let db = Database::new(db_path).unwrap_or_else(|e| {
            log::error!("Falling back to in-memory DB: {}", e);
            Database::in_memory().expect("in-memory db")
        });

        Self {
            db: Arc::new(db),
            artwork_manager: Arc::new(ArtworkManager::new(art_dir)),
        }
    }

    pub fn with_db(db: Database, artwork_manager: ArtworkManager) -> Self {
        Self {
            db: Arc::new(db),
            artwork_manager: Arc::new(artwork_manager),
        }
    }

    pub fn scan_music_folder(&self) -> anyhow::Result<usize> {
        let music_path = std::env::var("USERPROFILE")
            .map(|p| PathBuf::from(p).join("Music"))
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Default\\Music"));

        self.scan_custom_folder(music_path)
    }

    pub fn scan_custom_folder<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<usize> {
        Scanner::scan_directory(path, &self.db, &self.artwork_manager)
    }

    pub fn track_count(&self) -> usize {
        self.db.get_all_tracks().map(|t| t.len()).unwrap_or(0)
    }

    pub fn tracks(&self) -> Vec<Track> {
        self.db.get_all_tracks().unwrap_or_default()
    }

    pub fn albums(&self) -> Vec<Album> {
        self.db.get_albums().unwrap_or_default()
    }

    pub fn artists(&self) -> Vec<Artist> {
        self.db.get_artists().unwrap_or_default()
    }

    pub fn playlists(&self) -> Vec<Playlist> {
        self.db.get_playlists().unwrap_or_default()
    }

    pub fn create_playlist(&self, name: &str, description: &str) -> anyhow::Result<Playlist> {
        let id = Uuid::new_v4().to_string();
        self.db.create_playlist(&id, name, description)
    }

    pub fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> anyhow::Result<()> {
        self.db.add_track_to_playlist(playlist_id, track_id)
    }

    pub fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> anyhow::Result<()> {
        self.db.remove_track_from_playlist(playlist_id, track_id)
    }

    pub fn delete_playlist(&self, playlist_id: &str) -> anyhow::Result<()> {
        self.db.delete_playlist(playlist_id)
    }

    pub fn import_playlist_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<Playlist> {
        let path = path.as_ref();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Imported Playlist");

        let file_paths = match ext.as_str() {
            "pls" => PlaylistFileHelper::import_pls(path)?,
            _ => PlaylistFileHelper::import_m3u8(path)?,
        };

        let playlist = self.create_playlist(name, "Imported from file")?;
        let all_tracks = self.tracks();

        for fp in file_paths {
            if let Some(matching) = all_tracks.iter().find(|t| t.file_path == fp) {
                let _ = self.add_track_to_playlist(&playlist.id, &matching.id);
            }
        }

        Ok(playlist)
    }

    pub fn export_playlist_file<P: AsRef<Path>>(&self, playlist_id: &str, out_path: P) -> anyhow::Result<()> {
        let out_path = out_path.as_ref();
        let ext = out_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

        let playlists = self.playlists();
        let playlist = playlists
            .iter()
            .find(|p| p.id == playlist_id)
            .ok_or_else(|| anyhow::anyhow!("Playlist not found"))?;

        let all_tracks = self.tracks();
        let playlist_tracks: Vec<Track> = playlist
            .tracks
            .iter()
            .filter_map(|tid| all_tracks.iter().find(|t| &t.id == tid).cloned())
            .collect();

        match ext.as_str() {
            "pls" => PlaylistFileHelper::export_pls(&playlist_tracks, out_path)?,
            _ => PlaylistFileHelper::export_m3u8(&playlist.name, &playlist_tracks, out_path)?,
        }

        Ok(())
    }
}
