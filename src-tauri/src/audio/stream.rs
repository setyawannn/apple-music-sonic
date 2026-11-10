use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct HlsStreamVariant {
    pub bandwidth: u64,
    pub codecs: Option<String>,
    pub uri: String,
    pub is_alac: bool,
    pub is_lossless: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsSegment {
    pub uri: String,
    pub duration_secs: f64,
}

pub struct HlsParser;

impl HlsParser {
    pub fn is_hls(content_or_url: &str) -> bool {
        content_or_url.contains("#EXTM3U") || content_or_url.ends_with(".m3u8") || content_or_url.contains(".m3u8?")
    }

    pub fn resolve_url(base_url: &str, relative: &str) -> String {
        let trimmed = relative.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return trimmed.to_string();
        }

        if let Ok(base) = reqwest::Url::parse(base_url) {
            if let Ok(joined) = base.join(trimmed) {
                return joined.to_string();
            }
        }

        // Simple fallback
        if let Some(pos) = base_url.rfind('/') {
            format!("{}/{}", &base_url[..pos], trimmed.trim_start_matches('/'))
        } else {
            trimmed.to_string()
        }
    }

    pub fn parse_master(content: &str, base_url: &str) -> Vec<HlsStreamVariant> {
        if !content.contains("#EXTM3U") {
            return Vec::new();
        }

        let mut variants = Vec::new();
        let mut has_stream_inf = false;
        let mut current_bandwidth = 0u64;
        let mut current_codecs = None;
        let mut is_alac = false;
        let mut is_lossless = false;

        for line in content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            if line.starts_with("#EXT-X-STREAM-INF:") {
                has_stream_inf = true;
                let info = &line["#EXT-X-STREAM-INF:".len()..];
                for part in info.split(',') {
                    let mut kv = part.splitn(2, '=');
                    if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                        let key = k.trim().to_uppercase();
                        let val = v.trim().trim_matches('"');
                        if key == "BANDWIDTH" {
                            current_bandwidth = val.parse::<u64>().unwrap_or(0);
                        } else if key == "CODECS" {
                            current_codecs = Some(val.to_string());
                            let val_lower = val.to_lowercase();
                            if val_lower.contains("alac") {
                                is_alac = true;
                                is_lossless = true;
                            } else if val_lower.contains("flac") {
                                is_lossless = true;
                            }
                        }
                    }
                }
            } else if !line.starts_with('#') && has_stream_inf {
                let uri = Self::resolve_url(base_url, line);
                variants.push(HlsStreamVariant {
                    bandwidth: current_bandwidth,
                    codecs: current_codecs.take(),
                    uri,
                    is_alac,
                    is_lossless,
                });
                has_stream_inf = false;
                current_bandwidth = 0;
                is_alac = false;
                is_lossless = false;
            }
        }

        // Sort by highest bandwidth (best quality) first
        variants.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
        variants
    }

    pub fn parse_media(content: &str, base_url: &str) -> Vec<HlsSegment> {
        if !content.contains("#EXTM3U") {
            return Vec::new();
        }

        let mut segments = Vec::new();
        let mut has_inf = false;
        let mut current_duration = 0.0f64;

        for line in content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            if line.starts_with("#EXTINF:") {
                has_inf = true;
                let info = &line["#EXTINF:".len()..];
                let dur_str = info.split(',').next().unwrap_or("0");
                current_duration = dur_str.trim().parse::<f64>().unwrap_or(0.0);
            } else if !line.starts_with('#') && has_inf {
                let uri = Self::resolve_url(base_url, line);
                segments.push(HlsSegment {
                    uri,
                    duration_secs: current_duration,
                });
                has_inf = false;
                current_duration = 0.0;
            }
        }

        segments
    }
}

pub struct StreamCache {
    cache_dir: PathBuf,
    max_cache_bytes: u64,
}

impl StreamCache {
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Self {
        let path = cache_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&path);
        Self {
            cache_dir: path,
            max_cache_bytes: 200 * 1024 * 1024, // 200 MB default max cache
        }
    }

    pub fn default_path() -> PathBuf {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("AppleMusicRS");
        base.join("stream_cache")
    }

    pub fn in_temp_dir() -> Self {
        let path = std::env::temp_dir().join(format!("apple_music_stream_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&path);
        Self {
            cache_dir: path,
            max_cache_bytes: 50 * 1024 * 1024,
        }
    }

    pub fn get_track_cache_path(&self, track_id: &str) -> PathBuf {
        let sanitized: String = track_id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.cache_dir.join(format!("{}.m4a", sanitized))
    }

    pub fn is_cached(&self, track_id: &str) -> bool {
        let path = self.get_track_cache_path(track_id);
        if let Ok(meta) = fs::metadata(&path) {
            meta.len() > 1024 // At least 1KB
        } else {
            false
        }
    }

    pub async fn download_to_cache(
        &self,
        client: &reqwest::Client,
        track_id: &str,
        url: &str,
    ) -> Result<PathBuf, String> {
        let target_path = self.get_track_cache_path(track_id);
        if self.is_cached(track_id) {
            return Ok(target_path);
        }

        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to initiate stream request: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Stream server returned error HTTP {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read stream bytes: {}", e))?;

        if bytes.is_empty() {
            return Err("Received empty stream audio bytes from server".to_string());
        }

        // Atomic write via temp file
        let temp_path = target_path.with_extension("tmp");
        {
            let mut file = File::create(&temp_path).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
        }

        fs::rename(&temp_path, &target_path).map_err(|e| e.to_string())?;
        let _ = self.prune_if_needed();

        Ok(target_path)
    }

    pub fn prune_if_needed(&self) -> Result<usize, String> {
        let mut entries = Vec::new();
        let mut total_bytes = 0u64;

        if let Ok(read_dir) = fs::read_dir(&self.cache_dir) {
            for entry in read_dir.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let len = meta.len();
                        total_bytes += len;
                        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        entries.push((entry.path(), len, modified));
                    }
                }
            }
        }

        if total_bytes <= self.max_cache_bytes {
            return Ok(0);
        }

        // Sort oldest modified first
        entries.sort_by_key(|e| e.2);
        let mut removed_count = 0;

        for (path, len, _) in entries {
            if total_bytes <= self.max_cache_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total_bytes = total_bytes.saturating_sub(len);
                removed_count += 1;
            }
        }

        Ok(removed_count)
    }
}
