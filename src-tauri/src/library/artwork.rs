use std::path::{Path, PathBuf};

pub struct ArtworkManager {
    cache_dir: PathBuf,
}

impl ArtworkManager {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Self {
        let dir = cache_dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        Self { cache_dir: dir }
    }

    pub fn save_embedded_art(&self, album_key: &str, data: &[u8], mime: Option<&str>) -> Option<String> {
        if data.is_empty() {
            return None;
        }

        let ext = match mime {
            Some("image/png") => "png",
            Some("image/webp") => "webp",
            _ => "jpg",
        };

        // Deterministic hash based on album key
        let mut hasher = 0xcbf29ce484222325u64;
        for byte in album_key.bytes() {
            hasher ^= byte as u64;
            hasher = hasher.wrapping_mul(0x100000001b3);
        }

        let filename = format!("art_{:016x}.{}", hasher, ext);
        let out_path = self.cache_dir.join(filename);

        // If file already exists in cache, reuse it
        if out_path.exists() {
            return Some(out_path.to_string_lossy().to_string());
        }

        match std::fs::write(&out_path, data) {
            Ok(()) => Some(out_path.to_string_lossy().to_string()),
            Err(e) => {
                log::warn!("Failed to save cached artwork: {}", e);
                None
            }
        }
    }

    pub fn find_folder_art(&self, audio_file_path: &Path) -> Option<String> {
        let parent = audio_file_path.parent()?;
        let candidate_names = [
            "cover.jpg",
            "cover.png",
            "folder.jpg",
            "folder.png",
            "albumart.jpg",
            "albumart.png",
            "front.jpg",
            "front.png",
        ];

        for name in candidate_names {
            let candidate = parent.join(name);
            if candidate.exists() && candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }

        None
    }
}
