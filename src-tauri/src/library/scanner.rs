use super::artwork::ArtworkManager;
use super::db::Database;
use super::metadata::extract_metadata;
use super::Track;
use std::path::Path;
use std::time::UNIX_EPOCH;
use uuid::Uuid;
use walkdir::WalkDir;

pub struct Scanner;

impl Scanner {
    pub fn scan_directory<P: AsRef<Path>>(
        root_dir: P,
        db: &Database,
        art_mgr: &ArtworkManager,
    ) -> anyhow::Result<usize> {
        let root = root_dir.as_ref();
        if !root.exists() {
            return Ok(db.get_all_tracks()?.len());
        }

        let cached_stats = db.get_cached_file_stats()?;
        let mut seen_paths = Vec::new();

        for entry in WalkDir::new(root)
            .max_depth(8)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().map(|ext| {
                    matches!(
                        ext.to_str().unwrap_or("").to_lowercase().as_str(),
                        "mp3" | "m4a" | "flac" | "wav" | "ogg" | "alac"
                    )
                }).unwrap_or(false)
            })
        {
            let path = entry.path();
            let path_str = path.to_string_lossy().to_string();
            seen_paths.push(path_str.clone());

            let fs_meta = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let file_size = fs_meta.len();
            let mtime = fs_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Fast incremental check: if file size and modified timestamp match DB, skip parsing
            if let Some(&(cached_size, cached_mtime)) = cached_stats.get(&path_str) {
                if cached_size == file_size && cached_mtime == mtime {
                    continue;
                }
            }

            // File is new or changed: probe metadata
            let meta = match extract_metadata(path) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Failed to read metadata for {:?}: {}", path, e);
                    continue;
                }
            };

            let album_key = format!("{} - {}", meta.album, meta.artist);

            // Check embedded artwork, fallback to sibling folder art
            let art_path = if let Some(ref art_data) = meta.artwork_data {
                art_mgr.save_embedded_art(&album_key, art_data, meta.artwork_mime.as_deref())
            } else {
                art_mgr.find_folder_art(path)
            };

            let track = Track {
                id: Uuid::new_v4().to_string(),
                title: meta.title,
                artist: meta.artist,
                album: meta.album,
                album_artist: meta.album_artist,
                track_number: meta.track_number,
                disc_number: meta.disc_number,
                duration_secs: meta.duration_secs,
                file_path: path_str,
                file_size,
                genre: meta.genre,
                year: meta.year,
                art_path,
            };

            if let Err(e) = db.insert_or_replace_track(&track, mtime) {
                log::error!("Failed to store track in database: {}", e);
            }
        }

        // Prune any tracks that no longer exist in the scanned directory
        let _ = db.prune_missing_tracks(&seen_paths);

        Ok(db.get_all_tracks()?.len())
    }
}
